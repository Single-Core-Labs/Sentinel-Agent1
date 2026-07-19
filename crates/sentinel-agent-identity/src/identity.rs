use serde::{Deserialize, Serialize};
use jsonwebtoken::{encode, decode, EncodingKey, DecodingKey, Header, Validation, Algorithm};
use chrono::Utc;
use uuid::Uuid;
use crate::crypto::{KeyPair, generate_agent_key_material};
use crate::bom::{AgentBillOfMaterials, resolve_backend_url};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Claims embedded in an agent's JWT for authentication/authorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClaims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Represents a registered agent identity with cryptographic keys.
///
/// Handles key generation, JWT creation/validation, registration
/// with retry, and task-specific authorization headers.
#[derive(Debug)]
pub struct AgentIdentity {
    pub agent_id: String,
    pub keypair: KeyPair,
    pub bom: AgentBillOfMaterials,
    registration: Arc<Mutex<Option<AgentRegistration>>>,
}

/// Response from the backend after successful agent registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub agent_id: String,
    pub public_key: Vec<u8>,
    pub registered_at: String,
    pub backend_url: String,
}

impl AgentIdentity {
    /// Create a new agent identity with freshly generated keys.
    pub fn new() -> Self {
        let agent_id = Uuid::new_v4().to_string();
        let keypair = generate_agent_key_material();
        let bom = AgentBillOfMaterials::new(&agent_id, &keypair);
        Self {
            agent_id,
            keypair,
            bom,
            registration: Arc::new(Mutex::new(None)),
        }
    }

    /// Create an agent identity from an existing key pair.
    pub fn from_keypair(agent_id: impl Into<String>, keypair: KeyPair) -> Self {
        let agent_id = agent_id.into();
        let bom = AgentBillOfMaterials::new(&agent_id, &keypair);
        Self {
            agent_id,
            keypair,
            bom,
            registration: Arc::new(Mutex::new(None)),
        }
    }

    /// Sign an assertion payload with the agent's private key.
    pub fn sign_assertion(&self, payload: &[u8]) -> Vec<u8> {
        self.keypair.sign(payload)
    }

    /// Create a JWT for this agent, optionally scoped to a task.
    pub fn create_jwt(&self, _audience: &str, task_id: Option<&str>) -> Result<String, IdentityError> {
        let now = Utc::now().timestamp() as usize;
        let claims = AgentClaims {
            sub: self.agent_id.clone(),
            exp: now + 3600,
            iat: now,
            agent_id: self.agent_id.clone(),
            task_id: task_id.map(String::from),
            session_id: None,
        };

        let key = EncodingKey::from_ed_der(&pkcs8_private_key_der(&self.keypair));
        let header = Header {
            alg: Algorithm::EdDSA,
            ..Default::default()
        };
        encode(&header, &claims, &key)
            .map_err(|e| IdentityError::JwtError(e.to_string()))
    }

    /// Verify a JWT against this agent's public key.
    pub fn verify_jwt(&self, token: &str) -> Result<AgentClaims, IdentityError> {
        let key = DecodingKey::from_ed_der(&self.keypair.verifying_key.to_bytes());
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_exp = true;
        let token_data = decode::<AgentClaims>(token, &key, &validation)
            .map_err(|e| IdentityError::JwtError(e.to_string()))?;
        Ok(token_data.claims)
    }

    /// Register this agent with the backend.
    /// Uses env-resolved backend URL and retries on failure.
    pub async fn register(&self) -> Result<AgentRegistration, IdentityError> {
        let backend_url = resolve_backend_url();
        self.register_with_backend(&backend_url).await
    }

    /// Register with a specific backend URL, with retry logic.
    pub async fn register_with_backend(&self, backend_url: &str) -> Result<AgentRegistration, IdentityError> {
        let reg = AgentRegistration {
            agent_id: self.agent_id.clone(),
            public_key: self.keypair.public_key_bytes(),
            registered_at: Utc::now().to_rfc3339(),
            backend_url: backend_url.to_string(),
        };

        let register_url = format!("{}/api/v1/agents/register", backend_url);
        let client = reqwest::Client::new();

        // Retry up to 3 times with exponential backoff
        let mut last_err = None;
        for attempt in 0..3 {
            match client.post(&register_url)
                .json(&self.bom)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        tracing::info!(agent_id = %self.agent_id, backend = %backend_url, "agent registered");
                        let mut lock = self.registration.lock().await;
                        *lock = Some(reg.clone());
                        return Ok(reg);
                    }
                    last_err = Some(IdentityError::RegistrationError(
                        format!("HTTP {}", resp.status())
                    ));
                }
                Err(e) => {
                    last_err = Some(IdentityError::RegistrationError(e.to_string()));
                }
            }
            // Exponential backoff: 1s, 2s, 4s
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
            }
        }

        Err(last_err.unwrap_or_else(|| IdentityError::RegistrationError("Unknown".into())))
    }

    /// Build an `Authorization` header value for a task-specific assertion.
    /// Format: `Bearer <agent-jwt>`
    pub fn authorization_header_for_agent_task(&self, task_id: &str) -> Result<String, IdentityError> {
        let jwt = self.create_jwt("task", Some(task_id))?;
        Ok(format!("Bearer {}", jwt))
    }

    /// Check if this agent has been registered with the backend.
    pub async fn is_registered(&self) -> bool {
        self.registration.lock().await.is_some()
    }

    /// Get the registration info if registered.
    pub async fn registration(&self) -> Option<AgentRegistration> {
        self.registration.lock().await.clone()
    }
}

/// Build a PKCS#8 v2 DER-encoded Ed25519 private key (Ring-compatible).
///
/// Ring expects the PKCS#8 v2 (OneAsymmetricKey) format with the public
/// key embedded in a `[1] EXPLICIT` context-tagged field.
///
/// Format:
///   SEQUENCE {
///     INTEGER 1,               -- version v2
///     SEQUENCE { OID 1.3.101.112 },
///     OCTET STRING { OCTET STRING { <seed> } },
///     [1] { BIT STRING { <pubkey> } }
///   }
fn pkcs8_private_key_der(keypair: &KeyPair) -> Vec<u8> {
    let seed = keypair.signing_key.to_bytes();
    let pubkey = keypair.verifying_key.to_bytes();
    let mut der = Vec::with_capacity(85);
    der.extend_from_slice(&[
        0x30, 0x53,             // SEQUENCE (83 bytes)
        0x02, 0x01, 0x01,       // INTEGER 1 (v2)
        0x30, 0x05,             // SEQUENCE (5 bytes)
        0x06, 0x03,             // OID (3 bytes)
        0x2b, 0x65, 0x70,       // 1.3.101.112 (Ed25519)
        0x04, 0x22,             // OCTET STRING (34 bytes)
        0x04, 0x20,             // OCTET STRING (32 bytes) — seed
    ]);
    der.extend_from_slice(&seed);
    der.extend_from_slice(&[
        0xa1, 0x23,             // [1] EXPLICIT (35 bytes)
        0x03, 0x21,             // BIT STRING (33 bytes)
        0x00,                   // 0 unused bits
    ]);
    der.extend_from_slice(&pubkey);
    der
}

impl Default for AgentIdentity {
    fn default() -> Self {
        Self::new()
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("JWT error: {0}")]
    JwtError(String),
    #[error("Registration failed: {0}")]
    RegistrationError(String),
    #[error("Not registered")]
    NotRegistered,
}

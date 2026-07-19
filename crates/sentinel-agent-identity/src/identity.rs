use serde::{Deserialize, Serialize};
use jsonwebtoken::{encode, decode, EncodingKey, DecodingKey, Header, Validation};
use chrono::Utc;
use uuid::Uuid;
use crate::crypto::KeyPair;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClaims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub agent_id: String,
    pub task_id: Option<String>,
}

#[derive(Debug)]
pub struct AgentIdentity {
    pub agent_id: String,
    pub keypair: KeyPair,
    registration: Arc<Mutex<Option<AgentRegistration>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub agent_id: String,
    pub public_key: Vec<u8>,
    pub registered_at: String,
}

impl AgentIdentity {
    pub fn new() -> Self {
        let keypair = KeyPair::generate();
        Self {
            agent_id: Uuid::new_v4().to_string(),
            keypair,
            registration: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_keypair(agent_id: impl Into<String>, keypair: KeyPair) -> Self {
        Self {
            agent_id: agent_id.into(),
            keypair,
            registration: Arc::new(Mutex::new(None)),
        }
    }

    pub fn sign_assertion(&self, payload: &[u8]) -> Vec<u8> {
        self.keypair.sign(payload)
    }

    pub fn create_jwt(&self, _audience: &str) -> Result<String, IdentityError> {
        let now = Utc::now().timestamp() as usize;
        let claims = AgentClaims {
            sub: self.agent_id.clone(),
            exp: now + 3600,
            iat: now,
            agent_id: self.agent_id.clone(),
            task_id: None,
        };

        let key = EncodingKey::from_ed_der(
            &self.keypair.secret_key_bytes()
        );
        encode(&Header::default(), &claims, &key)
            .map_err(|e| IdentityError::JwtError(e.to_string()))
    }

    pub fn verify_jwt(&self, token: &str) -> Result<AgentClaims, IdentityError> {
        let key = DecodingKey::from_ed_der(
            &self.keypair.public_key_bytes()
        );
        let token_data = decode::<AgentClaims>(token, &key, &Validation::default())
            .map_err(|e| IdentityError::JwtError(e.to_string()))?;
        Ok(token_data.claims)
    }

    pub async fn register(&self, backend_url: &str) -> Result<AgentRegistration, IdentityError> {
        let reg = AgentRegistration {
            agent_id: self.agent_id.clone(),
            public_key: self.keypair.public_key_bytes(),
            registered_at: Utc::now().to_rfc3339(),
        };

        // Send registration to backend (future)
        tracing::info!(agent_id = %self.agent_id, backend = %backend_url, "registering agent");

        let mut lock = self.registration.lock().await;
        *lock = Some(reg.clone());
        Ok(reg)
    }

    pub async fn is_registered(&self) -> bool {
        self.registration.lock().await.is_some()
    }
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
}

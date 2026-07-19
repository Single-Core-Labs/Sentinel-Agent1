use serde::{Deserialize, Serialize};
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation, Algorithm};
use crate::identity::AgentClaims;

/// A JWKS key entry (minimal subset for JWT validation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwkKey {
    pub kty: String,
    #[serde(default)]
    pub alg: Option<String>,
    #[serde(default)]
    pub kid: Option<String>,
    #[serde(default)]
    pub n: Option<String>,
    #[serde(default)]
    pub e: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
    #[serde(default)]
    pub crv: Option<String>,
    #[serde(default)]
    pub use_: Option<String>,
}

/// A JWKS key set, fetched from the backend for token validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwks {
    pub keys: Vec<JwkKey>,
}

impl Jwks {
    pub fn empty() -> Self {
        Self { keys: Vec::new() }
    }

    pub fn key_by_kid(&self, kid: &str) -> Option<&JwkKey> {
        self.keys.iter().find(|k| k.kid.as_deref() == Some(kid))
    }
}

/// Decode and validate a JWT using the agent's own public key (Ed25519).
pub fn decode_agent_identity_jwt(
    token: &str,
    public_key_bytes: &[u8],
) -> Result<AgentClaims, JwksError> {
    let key = DecodingKey::from_ed_der(public_key_bytes);
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;

    let token_data = decode::<AgentClaims>(token, &key, &validation)
        .map_err(|e| JwksError::ValidationFailed(e.to_string()))?;
    Ok(token_data.claims)
}

/// Decode and validate a JWT against a JWKS key set.
/// Resolves the key by `kid` from the token header.
pub fn decode_jwt_with_jwks(
    token: &str,
    jwks: &Jwks,
) -> Result<AgentClaims, JwksError> {
    let header = decode_header(token)
        .map_err(|e| JwksError::DecodeError(e.to_string()))?;

    let kid = header.kid
        .ok_or(JwksError::MissingKid)?;

    let jwk = jwks.key_by_kid(&kid)
        .ok_or(JwksError::KeyNotFound(kid))?;

    let alg = header.alg;
    let key = key_from_jwk(jwk, alg)?;

    let mut validation = Validation::new(alg);
    validation.validate_exp = true;
    validation.set_required_spec_claims(&["exp", "sub", "iat"]);

    let token_data = decode::<AgentClaims>(token, &key, &validation)
        .map_err(|e| JwksError::ValidationFailed(e.to_string()))?;
    Ok(token_data.claims)
}

fn key_from_jwk(jwk: &JwkKey, alg: Algorithm) -> Result<DecodingKey, JwksError> {
    match alg {
        Algorithm::EdDSA => {
            let der = jwk.x.as_ref()
                .and_then(|x| base64_url_decode(x))
                .ok_or_else(|| JwksError::KeyFormatError("Missing Ed25519 public key".into()))?;
            Ok(DecodingKey::from_ed_der(&der))
        }
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            let n = jwk.n.as_ref()
                .and_then(|n| base64_url_decode(n))
                .ok_or_else(|| JwksError::KeyFormatError("Missing RSA modulus".into()))?;
            let e = jwk.e.as_ref()
                .and_then(|e| base64_url_decode(e))
                .ok_or_else(|| JwksError::KeyFormatError("Missing RSA exponent".into()))?;
            Ok(DecodingKey::from_rsa_raw_components(&n, &e))
        }
        _ => Err(JwksError::KeyFormatError(format!(
            "Unsupported algorithm: {:?}", alg
        ))),
    }
}

fn base64_url_decode(input: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input)
        .ok()
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum JwksError {
    #[error("Failed to decode JWT header: {0}")]
    DecodeError(String),
    #[error("Missing key ID (kid) in token header")]
    MissingKid,
    #[error("Key not found in JWKS: {0}")]
    KeyNotFound(String),
    #[error("Key format error: {0}")]
    KeyFormatError(String),
    #[error("JWT validation failed: {0}")]
    ValidationFailed(String),
    #[error("JWKS fetch failed: {0}")]
    FetchError(String),
}

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub agent_id: Option<String>,
}

pub struct Authenticator {
    secret: String,
}

impl Authenticator {
    pub fn new(secret: impl Into<String>) -> Self {
        Self { secret: secret.into() }
    }

    pub fn create_token(&self, subject: &str, agent_id: Option<&str>) -> Result<String, AuthError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AuthError::TimeError)?;

        let claims = Claims {
            sub: subject.to_string(),
            iat: now.as_secs() as usize,
            exp: now.as_secs() as usize + 86400,
            agent_id: agent_id.map(String::from),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| AuthError::JwtError(e.to_string()))
    }

    pub fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| AuthError::JwtError(e.to_string()))?;
        Ok(token_data.claims)
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("JWT error: {0}")]
    JwtError(String),
    #[error("System time error")]
    TimeError,
    #[error("Not authenticated")]
    NotAuthenticated,
}

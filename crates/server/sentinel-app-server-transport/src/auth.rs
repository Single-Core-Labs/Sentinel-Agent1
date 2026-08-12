use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
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
        Self {
            secret: secret.into(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_round_trip_returns_claims() {
        let auth = Authenticator::new("test-secret");
        let token = auth
            .create_token("user-1", Some("agent-7"))
            .expect("token creation failed");
        let claims = auth.validate_token(&token).expect("validation failed");
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.agent_id.as_deref(), Some("agent-7"));
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn token_round_trip_without_agent_id() {
        let auth = Authenticator::new("test-secret");
        let token = auth
            .create_token("user-2", None)
            .expect("token creation failed");
        let claims = auth.validate_token(&token).expect("validation failed");
        assert_eq!(claims.sub, "user-2");
        assert_eq!(claims.agent_id, None);
    }

    #[test]
    fn token_rejected_with_wrong_secret() {
        let auth = Authenticator::new("secret-a");
        let other = Authenticator::new("secret-b");
        let token = auth
            .create_token("user-1", None)
            .expect("token creation failed");
        assert!(other.validate_token(&token).is_err());
    }

    #[test]
    fn tampered_token_rejected() {
        let auth = Authenticator::new("test-secret");
        let token = auth
            .create_token("user-1", None)
            .expect("token creation failed");
        let tampered = format!("{}x", token);
        assert!(auth.validate_token(&tampered).is_err());
    }

    #[test]
    fn tokens_are_unique_across_subjects() {
        let auth = Authenticator::new("test-secret");
        let a = auth.create_token("user-a", None).expect("token a failed");
        let b = auth.create_token("user-b", None).expect("token b failed");
        assert_ne!(a, b);
    }
}

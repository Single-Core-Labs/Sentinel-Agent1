use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::RngCore;

/// Ed25519 key pair for an agent's cryptographic identity.
///
/// Used for signing assertions, JWTs, and verifying signatures
/// from other agents or backend services.
#[derive(Debug)]
pub struct KeyPair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

impl KeyPair {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let signing_key = SigningKey::from_bytes(&bytes);
        let verifying_key = signing_key.verifying_key();
        Self { signing_key, verifying_key }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        let bytes: [u8; 32] = bytes.try_into()
            .map_err(|_| CryptoError::InvalidKeyLength)?;
        let signing_key = SigningKey::from_bytes(&bytes);
        let verifying_key = signing_key.verifying_key();
        Ok(Self { signing_key, verifying_key })
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.signing_key.sign(message).to_bytes().to_vec()
    }

    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
        let sig_bytes: [u8; 64] = signature.try_into()
            .map_err(|_| CryptoError::InvalidSignature)?;
        let signature = Signature::from_bytes(&sig_bytes);
        self.verifying_key.verify_strict(message, &signature)
            .map_err(|_| CryptoError::SignatureMismatch)
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.verifying_key.to_bytes().to_vec()
    }

    pub fn secret_key_bytes(&self) -> Vec<u8> {
        self.signing_key.to_bytes().to_vec()
    }
}

/// Generate fresh Ed25519 key material for a new agent.
/// This is the primary entry point for creating agent identities.
pub fn generate_agent_key_material() -> KeyPair {
    KeyPair::generate()
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Invalid key length")]
    InvalidKeyLength,
    #[error("Invalid signature format")]
    InvalidSignature,
    #[error("Signature verification failed")]
    SignatureMismatch,
}

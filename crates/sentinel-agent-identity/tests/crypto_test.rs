use sentinel_agent_identity::crypto::{KeyPair, CryptoError, generate_agent_key_material};

#[test]
fn test_keypair_generate_sign_verify_roundtrip() {
    let keypair = KeyPair::generate();
    let message = b"hello world";
    let sig = keypair.sign(message);
    assert_eq!(sig.len(), 64);
    assert!(keypair.verify(message, &sig).is_ok());
}

#[test]
fn test_keypair_verify_wrong_message_fails() {
    let keypair = KeyPair::generate();
    let sig = keypair.sign(b"original message");
    let result = keypair.verify(b"wrong message", &sig);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CryptoError::SignatureMismatch));
}

#[test]
fn test_keypair_verify_wrong_key_fails() {
    let kp1 = KeyPair::generate();
    let kp2 = KeyPair::generate();
    let sig = kp1.sign(b"message");
    let result = kp2.verify(b"message", &sig);
    assert!(result.is_err());
}

#[test]
fn test_keypair_from_bytes_deterministic() {
    let seed = b"01234567890123456789012345678901"; // 32 bytes
    let kp1 = KeyPair::from_bytes(seed).unwrap();
    let kp2 = KeyPair::from_bytes(seed).unwrap();
    assert_eq!(kp1.public_key_bytes(), kp2.public_key_bytes());
    assert_eq!(kp1.secret_key_bytes(), kp2.secret_key_bytes());

    let sig1 = kp1.sign(b"test");
    let sig2 = kp2.sign(b"test");
    assert_eq!(sig1, sig2);
}

#[test]
fn test_keypair_invalid_key_length_error() {
    let short = b"too short"; // 9 bytes
    let result = KeyPair::from_bytes(short);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CryptoError::InvalidKeyLength));
}

#[test]
fn test_keypair_invalid_signature_length_error() {
    let keypair = KeyPair::generate();
    let short_sig = b"short"; // 5 bytes
    let result = keypair.verify(b"msg", short_sig);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CryptoError::InvalidSignature));
}

#[test]
fn test_keypair_public_key_bytes_32_bytes() {
    let keypair = KeyPair::generate();
    assert_eq!(keypair.public_key_bytes().len(), 32);
}

#[test]
fn test_keypair_secret_key_bytes_32_bytes() {
    let keypair = KeyPair::generate();
    assert_eq!(keypair.secret_key_bytes().len(), 32);
}

#[test]
fn test_generate_agent_key_material_returns_valid_keypair() {
    let kp = generate_agent_key_material();
    let sig = kp.sign(b"verify me");
    assert!(kp.verify(b"verify me", &sig).is_ok());
}

#[test]
fn test_crypto_error_display() {
    assert_eq!(CryptoError::InvalidKeyLength.to_string(), "Invalid key length");
    assert_eq!(CryptoError::InvalidSignature.to_string(), "Invalid signature format");
    assert_eq!(CryptoError::SignatureMismatch.to_string(), "Signature verification failed");
}

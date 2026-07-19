use sentinel_agent_identity::identity::{AgentIdentity, IdentityError};
use sentinel_agent_identity::crypto::KeyPair;

#[test]
fn test_agent_identity_new_creates_unique_id_and_keypair() {
    let id1 = AgentIdentity::new();
    let id2 = AgentIdentity::new();
    assert_ne!(id1.agent_id, id2.agent_id);
    assert_eq!(id1.keypair.public_key_bytes().len(), 32);
    assert_eq!(id2.keypair.public_key_bytes().len(), 32);
    assert_ne!(id1.keypair.public_key_bytes(), id2.keypair.public_key_bytes());
}

#[test]
fn test_agent_identity_new_creates_bom() {
    let identity = AgentIdentity::new();
    assert_eq!(identity.bom.agent_id, identity.agent_id);
    assert_eq!(identity.bom.public_key, identity.keypair.public_key_bytes());
    assert!(identity.bom.capabilities.contains(&"agent.chat".to_string()));
}

#[test]
fn test_agent_identity_from_keypair() {
    let kp = KeyPair::generate();
    let identity = AgentIdentity::from_keypair("custom-agent", kp);
    assert_eq!(identity.agent_id, "custom-agent");
    let sig = identity.keypair.sign(b"msg");
    assert!(identity.keypair.verify(b"msg", &sig).is_ok());
}

#[test]
fn test_create_and_verify_jwt_roundtrip() {
    let identity = AgentIdentity::new();
    let jwt = identity.create_jwt("sentinel-backend", Some("task-42")).unwrap();
    assert!(jwt.starts_with("ey")); // JWT header starts with base64

    let claims = identity.verify_jwt(&jwt).unwrap();
    assert_eq!(claims.sub, identity.agent_id);
    assert_eq!(claims.agent_id, identity.agent_id);
    assert_eq!(claims.task_id.as_deref(), Some("task-42"));
    assert!(claims.exp > claims.iat);
}

#[test]
fn test_create_jwt_without_task_id() {
    let identity = AgentIdentity::new();
    let jwt = identity.create_jwt("audience", None).unwrap();
    let claims = identity.verify_jwt(&jwt).unwrap();
    assert!(claims.task_id.is_none());
}

#[test]
fn test_verify_jwt_with_wrong_key_fails() {
    let identity1 = AgentIdentity::new();
    let identity2 = AgentIdentity::new();
    let jwt = identity1.create_jwt("test", None).unwrap();
    let result = identity2.verify_jwt(&jwt);
    assert!(result.is_err());
}

#[test]
fn test_verify_jwt_invalid_token_fails() {
    let identity = AgentIdentity::new();
    let result = identity.verify_jwt("not.a.jwt");
    assert!(result.is_err());
}

#[test]
fn test_authorization_header_format() {
    let identity = AgentIdentity::new();
    let header = identity.authorization_header_for_agent_task("task-1").unwrap();
    assert!(header.starts_with("Bearer "));
    let jwt = header.trim_start_matches("Bearer ");
    let claims = identity.verify_jwt(jwt).unwrap();
    assert_eq!(claims.task_id.as_deref(), Some("task-1"));
}

#[test]
fn test_sign_assertion() {
    let identity = AgentIdentity::new();
    let payload = b"agent-assertion-data";
    let sig = identity.sign_assertion(payload);
    assert!(identity.keypair.verify(payload, &sig).is_ok());
}

#[tokio::test]
async fn test_not_registered_initially() {
    let identity = AgentIdentity::new();
    assert!(!identity.is_registered().await);
    assert!(identity.registration().await.is_none());
}

#[test]
fn test_identity_error_display() {
    let jwt_err = IdentityError::JwtError("bad sig".into());
    assert!(jwt_err.to_string().contains("bad sig"));

    let reg_err = IdentityError::RegistrationError("timeout".into());
    assert!(reg_err.to_string().contains("timeout"));

    let not_reg = IdentityError::NotRegistered;
    assert_eq!(not_reg.to_string(), "Not registered");
}

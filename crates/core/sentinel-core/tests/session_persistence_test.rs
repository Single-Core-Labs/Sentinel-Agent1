use sentinel_core::*;

/// E2E test: create a session, persist it, reload it, verify state integrity.
#[tokio::test]
async fn test_session_persistence_roundtrip() {
    let dir = std::env::temp_dir().join(format!("sentinel_session_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");

    let store = JsonFileThreadStore::new(dir.clone());

    // Create thread with realistic state
    let yolo_mode = false;
    let mut thread = AgentThread::new(10, 20, yolo_mode);
    thread.conversation.add_user_message("Hello, what can you do?");
    thread.conversation.add_assistant_text("I can help you with various tasks like writing code, searching files, and running commands.");

    // Save
    store.save_thread(&thread).await.expect("save_thread failed");

    // Load into a new thread
    let loaded = store.load_thread(&thread.id.to_string()).await.expect("load_thread failed");

    // Verify state integrity
    assert_eq!(thread.id, loaded.id, "thread id should match");
    assert_eq!(thread.max_turns, loaded.max_turns, "max_turns should match");
    assert_eq!(thread.max_iterations, loaded.max_iterations, "max_iterations should match");
    assert_eq!(thread.yolo_mode, loaded.yolo_mode, "yolo_mode should match");
    assert_eq!(thread.turn, loaded.turn, "turn count should match");
    assert_eq!(thread.iterations, loaded.iterations, "iterations should match");
    assert_eq!(thread.parent_thread_id, loaded.parent_thread_id, "parent_thread_id should match");
    assert_eq!(thread.conversation.total_items(), loaded.conversation.total_items(), "conversation items should match");

    // Verify conversation content
    let thread_text = thread.conversation.turns.iter()
        .flat_map(|t| t.items.iter())
        .filter_map(|i| match i {
            sentinel_core::Item::UserMessage { text, .. } => Some(text.as_str()),
            sentinel_core::Item::AssistantText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    let loaded_text = loaded.conversation.turns.iter()
        .flat_map(|t| t.items.iter())
        .filter_map(|i| match i {
            sentinel_core::Item::UserMessage { text, .. } => Some(text.as_str()),
            sentinel_core::Item::AssistantText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert_eq!(thread_text, loaded_text, "conversation text should match");
    assert!(thread_text.contains("Hello"), "should contain original content");

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}

/// Test that sanitized persistence doesn't leak secrets.
#[tokio::test]
async fn test_session_persistence_sanitizes_secrets() {
    let dir = std::env::temp_dir().join(format!("sentinel_sanitize_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");

    let store = JsonFileThreadStore::new(dir.clone());

    let mut thread = AgentThread::new(10, 20, true);
    thread.conversation.add_user_message("My API key is sk-abc123def456ghi789jkl012mnopqrs and my token is Bearer xyz.token.here.longer.example.value");

    // Save (sanitization happens during save)
    store.save_thread(&thread).await.expect("save_thread failed");

    // Read raw JSON file to verify secrets were redacted
    let path = dir.join(format!("{}.json", thread.id));
    let json_content = std::fs::read_to_string(&path).expect("failed to read persisted file");

    // The original secret should NOT appear in the file
    assert!(!json_content.contains("sk-abc123def456ghi789jkl012mnopqrs"),
        "File should NOT contain the raw API key");
    assert!(!json_content.contains("Bearer xyz.token.here.longer.example.value"),
        "File should NOT contain the raw bearer token");

    // But the [REDACTED] placeholder should exist
    assert!(json_content.contains("[REDACTED]"),
        "File should contain [REDACTED] placeholder");

    // The loaded thread should still be usable
    let loaded = store.load_thread(&thread.id.to_string()).await.expect("load_thread failed after sanitization");
    assert_eq!(thread.id, loaded.id, "thread id should survive sanitization");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Test fork preserves parent relationship.
#[tokio::test]
async fn test_session_fork() {
    let dir = std::env::temp_dir().join(format!("sentinel_fork_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");

    let store = JsonFileThreadStore::new(dir.clone());

    let mut parent = AgentThread::new(10, 20, false);
    parent.conversation.add_user_message("Write a Python script");
    store.save_thread(&parent).await.expect("save parent failed");

    // Fork
    let forked = store.fork_thread(&parent.id.to_string()).await.expect("fork failed");

    assert_eq!(forked.parent_thread_id, Some(parent.id.to_string()),
        "forked thread should reference parent id");
    assert_ne!(forked.id, parent.id, "fork should have a different id");
    assert_eq!(forked.conversation.total_items(), parent.conversation.total_items(),
        "fork should inherit conversation");

    let _ = std::fs::remove_dir_all(&dir);
}

use sentinel_tools::{ToolRegistry, ToolContext};

#[tokio::test]
async fn test_read_write_roundtrip() {
    let registry = ToolRegistry::new();
    let ctx = ToolContext::new();
    let tmp = std::env::temp_dir().join("sentinel-test-tools.txt");

    // Write
    let write_args = serde_json::json!({
        "file_path": tmp.to_str().unwrap(),
        "content": "hello world\nsecond line\n"
    });
    let result = registry.execute("write", write_args, &ctx).await;
    assert!(!result.is_error, "write failed: {}", result.text);

    // Read
    let read_args = serde_json::json!({
        "file_path": tmp.to_str().unwrap()
    });
    let result = registry.execute("read", read_args, &ctx).await;
    assert!(!result.is_error, "read failed: {}", result.text);
    assert!(result.text.contains("hello world"), "content mismatch: {}", result.text);

    // Edit
    let edit_args = serde_json::json!({
        "file_path": tmp.to_str().unwrap(),
        "old_string": "hello world",
        "new_string": "hi there"
    });
    let result = registry.execute("edit", edit_args, &ctx).await;
    assert!(!result.is_error, "edit failed: {}", result.text);

    // Verify edit
    let verify_args = serde_json::json!({
        "file_path": tmp.to_str().unwrap()
    });
    let result = registry.execute("read", verify_args, &ctx).await;
    assert!(result.text.contains("hi there"), "edit not applied: {}", result.text);

    // Cleanup
    let _ = std::fs::remove_file(&tmp);
}

#[tokio::test]
async fn test_tool_not_found() {
    let registry = ToolRegistry::new();
    let ctx = ToolContext::new();
    let result = registry.execute("nonexistent_tool", serde_json::json!({}), &ctx).await;
    assert!(result.is_error);
    assert!(result.text.contains("not found"));
}

#[tokio::test]
async fn test_bash_echo() {
    let registry = ToolRegistry::new();
    let ctx = ToolContext::new();

    let args = serde_json::json!({ "command": "echo hello" });

    let result = registry.execute("bash", args, &ctx).await;
    assert!(result.text.contains("hello"), "bash echo failed: {}", result.text);
}

#[tokio::test]
async fn test_glob_pattern() {
    let registry = ToolRegistry::new();
    let ctx = ToolContext::new();

    // Create a temp dir with test files
    let tmp_dir = std::env::temp_dir().join("sentinel-glob-test");
    let _ = std::fs::create_dir_all(&tmp_dir);
    std::fs::write(tmp_dir.join("a.txt"), "a").unwrap();
    std::fs::write(tmp_dir.join("b.rs"), "b").unwrap();
    std::fs::write(tmp_dir.join("c.txt"), "c").unwrap();

    let glob_args = serde_json::json!({
        "pattern": "*.txt",
        "path": tmp_dir.to_str().unwrap()
    });
    let result = registry.execute("glob", glob_args, &ctx).await;
    assert!(!result.is_error, "glob failed: {}", result.text);
    assert!(result.text.contains("a.txt"), "should find a.txt: {}", result.text);
    assert!(!result.text.contains("b.rs"), "should not find b.rs: {}", result.text);

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_tool_defs() {
    let registry = ToolRegistry::new();
    let defs = registry.list();
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"read"), "read tool missing");
    assert!(names.contains(&"write"), "write tool missing");
    assert!(names.contains(&"edit"), "edit tool missing");
    assert!(names.contains(&"bash"), "bash tool missing");
    assert!(names.contains(&"glob"), "glob tool missing");
    assert!(names.contains(&"grep"), "grep tool missing");
    assert_eq!(defs.len(), 11, "expected 11 built-in tools, got {}", defs.len());
}

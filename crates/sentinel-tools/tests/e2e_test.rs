use std::sync::Arc;
use sentinel_tools::{ToolRegistry, ToolContext};
use sentinel_sandbox::SandboxPolicy;
use serde_json::json;

#[tokio::test]
async fn test_e2e_sandbox_and_edit() {
    let mut policy = SandboxPolicy::strict();
    // Allow edit in current directory, but no bash commands
    policy.write_paths = vec![std::env::current_dir().unwrap().display().to_string()];
    policy.allowed_commands = vec!["dummy_cmd".into()];

    let mut ctx = ToolContext::new();
    ctx.sandbox = Some(Arc::new(policy));
    
    let tools = ToolRegistry::new();

    println!("--- Test 1: Legitimate file edit (apply_patch) ---");
    let target_file = "dummy_edit.txt";
    std::fs::write(target_file, "line 1\nline 2\nline 3\n").unwrap();
    let patch = "@@ -1,3 +1,3 @@\n line 1\n-line 2\n+line 2 modified\n line 3\n";

    let edit_res = sentinel_ai_core::apply_patch::apply_patch(std::env::current_dir().unwrap().as_path(), std::path::Path::new(target_file), patch);
    println!("Edit Result: {:?}", edit_res);
    let after_content = std::fs::read_to_string(target_file).unwrap();
    println!("File Content after edit:\n{}", after_content);
    std::fs::remove_file(target_file).unwrap();
    
    assert!(edit_res.is_ok());
    assert!(after_content.contains("line 2 modified"));

    println!("\n--- Test 2: Sandbox Policy Violation (exec rm) ---");
    let bash_args = json!({
        "command": "rm -rf /",
    });

    let bash_res = tools.execute("bash", bash_args, &ctx).await;
    println!("Bash Result: {:?}", bash_res);
    assert!(bash_res.is_error);
    assert!(bash_res.text.contains("Execution denied"));
}

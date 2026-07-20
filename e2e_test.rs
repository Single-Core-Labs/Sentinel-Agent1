use std::sync::Arc;
use sentinel_tools::{ToolRegistry, ToolContext};
use sentinel_sandbox::SandboxPolicy;
use serde_json::json;

#[tokio::main]
async fn main() {
    let mut policy = SandboxPolicy::strict();
    // Allow edit in current directory, but no bash commands
    policy.write_paths = vec![std::env::current_dir().unwrap().display().to_string()];
    policy.allowed_commands = vec![];

    let mut ctx = ToolContext::new();
    ctx.sandbox = Some(policy);
    
    let tools = ToolRegistry::new(); // loads builtin tools

    println!("--- Test 1: Legitimate file edit (apply_patch) ---");
    let target_file = "dummy_edit.txt";
    std::fs::write(target_file, "line 1\nline 2\nline 3\n").unwrap();

    let edit_args = json!({
        "path": target_file,
        "patch": "@@ -1,3 +1,3 @@\n line 1\n-line 2\n+line 2 modified\n line 3\n"
    });

    let edit_res = tools.execute("edit", edit_args, &ctx).await;
    println!("Edit Result: {:?}", edit_res);
    println!("File Content after edit:\n{}", std::fs::read_to_string(target_file).unwrap());
    std::fs::remove_file(target_file).unwrap();

    println!("\n--- Test 2: Sandbox Policy Violation (exec rm) ---");
    let bash_args = json!({
        "command": "rm -rf /",
    });

    let bash_res = tools.execute("bash", bash_args, &ctx).await;
    println!("Bash Result: {:?}", bash_res);
}

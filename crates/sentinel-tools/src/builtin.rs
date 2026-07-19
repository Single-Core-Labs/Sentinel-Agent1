use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use crate::tool::{Tool, ToolContext, ToolOutput};

pub fn builtin_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ReadTool),
        Arc::new(WriteTool),
        Arc::new(EditTool),
        Arc::new(GlobTool),
        Arc::new(GrepTool),
        Arc::new(BashTool),
        Arc::new(WebSearchTool),
        Arc::new(GitStatusTool),
        Arc::new(GitDiffTool),
        Arc::new(GitCommitTool),
        Arc::new(GitLogTool),
    ]
}

// ── Read ─────────────────────────────────────────────────────────
pub struct ReadTool;
#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str { "read" }
    fn description(&self) -> &str { "Read the contents of a file" }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Absolute path to the file" },
                "offset": { "type": "integer", "description": "Line number to start from (1-indexed)" },
                "limit": { "type": "integer", "description": "Maximum number of lines to read" }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let path = args["file_path"].as_str().unwrap_or("");
        if path.is_empty() {
            return ToolOutput::err("file_path is required");
        }
        match std::fs::read_to_string(path) {
            Ok(content) => ToolOutput::ok(content),
            Err(e) => ToolOutput::err(format!("Failed to read {}: {}", path, e)),
        }
    }
}

// ── Write ────────────────────────────────────────────────────────
pub struct WriteTool;
#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str { "write" }
    fn description(&self) -> &str { "Write content to a file, creating it if necessary" }
    fn is_mutating(&self) -> bool { true }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Absolute path to the file" },
                "content": { "type": "string", "description": "Content to write" }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let path = args["file_path"].as_str().unwrap_or("");
        let content = args["content"].as_str().unwrap_or("");
        if path.is_empty() {
            return ToolOutput::err("file_path is required");
        }
        match std::fs::write(path, content) {
            Ok(_) => ToolOutput::ok(format!("Wrote {} bytes to {}", content.len(), path)),
            Err(e) => ToolOutput::err(format!("Failed to write {}: {}", path, e)),
        }
    }
}

// ── Edit ─────────────────────────────────────────────────────────
pub struct EditTool;
#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str { "edit" }
    fn description(&self) -> &str { "Replace text in a file using exact string match" }
    fn is_mutating(&self) -> bool { true }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Absolute path to the file" },
                "old_string": { "type": "string", "description": "Text to replace (must match exactly)" },
                "new_string": { "type": "string", "description": "Replacement text" },
                "replace_all": { "type": "boolean", "description": "Replace all occurrences" }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let path = args["file_path"].as_str().unwrap_or("");
        let old = args["old_string"].as_str().unwrap_or("");
        let new = args["new_string"].as_str().unwrap_or("");
        let replace_all = args["replace_all"].as_bool().unwrap_or(false);

        if path.is_empty() { return ToolOutput::err("file_path is required"); }
        if old.is_empty() { return ToolOutput::err("old_string is required"); }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return ToolOutput::err(format!("Failed to read {}: {}", path, e)),
        };

        if !content.contains(old) {
            return ToolOutput::err("old_string not found in file content");
        }

        let new_content = if replace_all {
            content.replace(old, new)
        } else {
            match content.find(old) {
                Some(pos) => {
                    let mut result = content.clone();
                    result.replace_range(pos..pos + old.len(), new);
                    result
                }
                None => return ToolOutput::err("old_string not found in file content"),
            }
        };

        match std::fs::write(path, &new_content) {
            Ok(_) => ToolOutput::ok(format!("Edited {}", path)),
            Err(e) => ToolOutput::err(format!("Failed to write {}: {}", path, e)),
        }
    }
}

// ── Glob ─────────────────────────────────────────────────────────
pub struct GlobTool;
#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }
    fn description(&self) -> &str { "Find files matching a glob pattern" }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. **/*.rs)" },
                "path": { "type": "string", "description": "Directory to search in" }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let pattern = args["pattern"].as_str().unwrap_or("");
        if pattern.is_empty() { return ToolOutput::err("pattern is required"); }
        let base_dir = args["path"].as_str().map(|p| p.to_string());
        let full_pattern = match &base_dir {
            Some(dir) => format!("{}/{}", dir.trim_end_matches('/'), pattern),
            None => pattern.to_string(),
        };
        match glob::glob(&full_pattern) {
            Ok(entries) => {
                let results: Vec<String> = entries.filter_map(|e| e.ok().map(|p| p.display().to_string())).collect();
                ToolOutput::ok(serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string()))
            }
            Err(e) => ToolOutput::err(format!("Glob error: {}", e)),
        }
    }
}

// ── Grep ─────────────────────────────────────────────────────────
pub struct GrepTool;
#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str { "Search file contents using a regex pattern" }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search for" },
                "path": { "type": "string", "description": "Directory to search in" },
                "include": { "type": "string", "description": "File pattern to include (e.g. *.rs)" }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let pattern = args["pattern"].as_str().unwrap_or("");
        if pattern.is_empty() { return ToolOutput::err("pattern is required"); }
        let path = args["path"].as_str().unwrap_or(".");
        let include = args["include"].as_str();

        // Simple recursive grep without external deps
        let mut results = Vec::new();
        if let Ok(entries) = walk_dir(path, include) {
            for entry in entries {
                if let Ok(content) = std::fs::read_to_string(&entry) {
                    for (i, line) in content.lines().enumerate() {
                        if line.contains(pattern) {
                            results.push(format!("{}:{}: {}", entry, i + 1, line));
                        }
                    }
                }
            }
        }
        ToolOutput::ok(results.join("\n"))
    }
}

fn walk_dir(dir: &str, include: Option<&str>) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    let dir = std::path::Path::new(dir);
    if !dir.is_dir() {
        return Ok(vec![dir.to_string_lossy().to_string()]);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_dir(&path.to_string_lossy(), include)?);
        } else if let Some(ext) = include {
            if path.to_string_lossy().ends_with(ext.trim_start_matches('*')) {
                files.push(path.to_string_lossy().to_string());
            }
        } else {
            files.push(path.to_string_lossy().to_string());
        }
    }
    Ok(files)
}

// ── Bash ─────────────────────────────────────────────────────────
pub struct BashTool;
#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }
    fn description(&self) -> &str { "Execute a shell command and capture output" }
    fn is_mutating(&self) -> bool { true }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command to execute" },
                "timeout": { "type": "integer", "description": "Timeout in milliseconds" },
                "workdir": { "type": "string", "description": "Working directory" }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let command = args["command"].as_str().unwrap_or("");
        if command.is_empty() { return ToolOutput::err("command is required"); }

        let _timeout = args["timeout"].as_u64().unwrap_or(120_000);
        let workdir = args["workdir"].as_str()
            .or(ctx.workspace_dir.as_deref())
            .unwrap_or(".");

        #[cfg(target_os = "windows")]
        let shell = "powershell";
        #[cfg(not(target_os = "windows"))]
        let shell = "sh";

        let result = tokio::process::Command::new(shell)
            .arg("-c")
            .arg(command)
            .current_dir(workdir)
            .output()
            .await;

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let mut text = String::new();
                if !stdout.is_empty() { text.push_str(&stdout); }
                if !stderr.is_empty() {
                    if !text.is_empty() { text.push('\n'); }
                    text.push_str(&stderr);
                }
                if output.status.success() {
                    ToolOutput::ok(text)
                } else {
                    ToolOutput::err(text)
                }
            }
            Err(e) => ToolOutput::err(format!("Command failed: {}", e)),
        }
    }
}

// ── WebSearch ────────────────────────────────────────────────────
pub struct WebSearchTool;
#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str { "web_search" }
    fn description(&self) -> &str { "Search the web for information" }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "max_results": { "type": "integer", "description": "Maximum number of results" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let query = args["query"].as_str().unwrap_or("");
        if query.is_empty() { return ToolOutput::err("query is required"); }
        let max_results = args["max_results"].as_u64().unwrap_or(5);

        // Simple web search via a public API (can be replaced with any search backend)
        let client = reqwest::Client::new();
        let url = format!("https://en.wikipedia.org/w/api.php?action=opensearch&search={}&limit={}&format=json",
            urlencoding(query), max_results);

        match client.get(&url).send().await {
            Ok(resp) => {
                match resp.text().await {
                    Ok(body) => ToolOutput::ok(body),
                    Err(e) => ToolOutput::err(format!("Search failed: {}", e)),
                }
            }
            Err(e) => ToolOutput::err(format!("Search request failed: {}", e)),
        }
    }
}

// ── Git Status ─────────────────────────────────────────────────
pub struct GitStatusTool;
#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str { "git_status" }
    fn description(&self) -> &str { "Show the working tree status" }
    fn is_mutating(&self) -> bool { false }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to git repo" }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let path = args["path"].as_str().unwrap_or(".");
        run_git(path, &["status", "--short"])
    }
}

// ── Git Diff ───────────────────────────────────────────────────
pub struct GitDiffTool;
#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str { "git_diff" }
    fn description(&self) -> &str { "Show changes in the working tree" }
    fn is_mutating(&self) -> bool { false }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to git repo" },
                "staged": { "type": "boolean", "description": "Show staged changes only" }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let path = args["path"].as_str().unwrap_or(".");
        let staged = args["staged"].as_bool().unwrap_or(false);
        if staged {
            run_git(path, &["diff", "--cached"])
        } else {
            run_git(path, &["diff"])
        }
    }
}

// ── Git Commit ─────────────────────────────────────────────────
pub struct GitCommitTool;
#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str { "git_commit" }
    fn description(&self) -> &str { "Create a git commit with staged changes" }
    fn is_mutating(&self) -> bool { true }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to git repo" },
                "message": { "type": "string", "description": "Commit message" }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let path = args["path"].as_str().unwrap_or(".");
        let message = args["message"].as_str().unwrap_or("");
        if message.is_empty() {
            return ToolOutput::err("commit message is required");
        }
        run_git(path, &["commit", "-m", message])
    }
}

// ── Git Log ────────────────────────────────────────────────────
pub struct GitLogTool;
#[async_trait]
impl Tool for GitLogTool {
    fn name(&self) -> &str { "git_log" }
    fn description(&self) -> &str { "Show commit logs" }
    fn is_mutating(&self) -> bool { false }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to git repo" },
                "max_count": { "type": "integer", "description": "Number of commits to show" }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let path = args["path"].as_str().unwrap_or(".");
        let max_count = args["max_count"].as_u64().unwrap_or(10);
        run_git(path, &["log", "--oneline", &format!("-{}", max_count)])
    }
}

fn run_git(path: &str, args: &[&str]) -> ToolOutput {
    let result = std::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .output();
    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut text = String::new();
            if !stdout.is_empty() { text.push_str(&stdout); }
            if !stderr.is_empty() {
                if !text.is_empty() { text.push('\n'); }
                text.push_str(&stderr);
            }
            if output.status.success() {
                ToolOutput::ok(text.trim())
            } else {
                ToolOutput::err(text.trim())
            }
        }
        Err(e) => ToolOutput::err(format!("git command failed: {}", e)),
    }
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "%20".into(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}

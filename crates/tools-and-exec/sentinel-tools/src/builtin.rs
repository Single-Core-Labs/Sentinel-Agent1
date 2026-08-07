use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub fn builtin_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ReadTool),
        Arc::new(ViewTool),
        Arc::new(WriteTool),
        Arc::new(EditTool),
        Arc::new(ApplyPatchTool),
        Arc::new(PatchTool),
        Arc::new(LsTool),
        Arc::new(GlobTool),
        Arc::new(GrepTool),
        Arc::new(RunShellCommandTool),
        Arc::new(WebSearchTool),
        Arc::new(WebFetchTool),
        Arc::new(SourcegraphTool),
        Arc::new(PlanTool),
        Arc::new(GitHubTool),
        Arc::new(GitStatusTool),
        Arc::new(GitDiffTool),
        Arc::new(GitCommitTool),
        Arc::new(GitLogTool),
        Arc::new(NotifyTool),
        Arc::new(ExploreDocsTool),
        Arc::new(FetchDocsTool),
        Arc::new(FindApiTool),
    ]
}

// ── Read ─────────────────────────────────────────────────────────
pub struct ReadTool;
#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "Read the contents of a file"
    }
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
        let offset = args["offset"].as_u64().map(|v| v as usize);
        let limit = args["limit"].as_u64().map(|v| v as usize);

        match std::fs::read_to_string(path) {
            Ok(content) => {
                if offset.is_none() && limit.is_none() {
                    return ToolOutput::ok(content);
                }
                let lines: Vec<&str> = content.lines().collect();
                let start = offset.unwrap_or(1).saturating_sub(1);
                if start >= lines.len() {
                    return ToolOutput::ok("");
                }
                let end = match limit {
                    Some(l) => (start + l).min(lines.len()),
                    None => lines.len(),
                };
                let sliced = lines[start..end].join("\n");
                ToolOutput::ok(sliced)
            }
            Err(e) => ToolOutput::err(format!("Failed to read {}: {}", path, e)),
        }
    }
}

// ── Write ────────────────────────────────────────────────────────
pub struct WriteTool;
#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn description(&self) -> &str {
        "Write content to a file, creating it if necessary"
    }
    fn is_mutating(&self) -> bool {
        true
    }
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
        let p = std::path::Path::new(path);
        if let Some(parent) = p.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return ToolOutput::err(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        e
                    ));
                }
            }
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
    fn name(&self) -> &str {
        "edit"
    }
    fn description(&self) -> &str {
        "Replace text in a file using exact string match"
    }
    fn is_mutating(&self) -> bool {
        true
    }
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

        if path.is_empty() {
            return ToolOutput::err("file_path is required");
        }
        if old.is_empty() {
            return ToolOutput::err("old_string is required");
        }

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

// ── ApplyPatch ──────────────────────────────────────────────────────
pub struct ApplyPatchTool;
#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }
    fn description(&self) -> &str {
        "Apply a git-style unified diff to one or more files. Supports multi-file diffs, \
         new-file creation (--- /dev/null) and file deletion (+++ /dev/null). All paths \
         must resolve inside the workspace root. The whole diff is validated before any \
         file is written."
    }
    fn is_mutating(&self) -> bool {
        true
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "diff": { "type": "string", "description": "Unified diff text (git diff format)" },
                "base_path": { "type": "string", "description": "Workspace root to apply within (defaults to the agent workspace)" }
            },
            "required": ["diff"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        execute_patch(args, ctx).await
    }
}

/// `patch` is an alias for `apply_patch`, exposed under the name models trained on
/// Claude Code / opencode tool definitions expect. Both share the same executor.
pub struct PatchTool;
#[async_trait]
impl Tool for PatchTool {
    fn name(&self) -> &str {
        "patch"
    }
    fn description(&self) -> &str {
        "Alias of `apply_patch`. Apply a git-style unified diff to one or more files. Supports \
         multi-file diffs, new-file creation (--- /dev/null) and file deletion (+++ /dev/null). \
         All paths must resolve inside the workspace root. The whole diff is validated before \
         any file is written."
    }
    fn is_mutating(&self) -> bool {
        true
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "diff": { "type": "string", "description": "Unified diff text (git diff format)" },
                "base_path": { "type": "string", "description": "Workspace root to apply within (defaults to the agent workspace)" }
            },
            "required": ["diff"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        execute_patch(args, ctx).await
    }
}

async fn execute_patch(args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
    let diff = args["diff"].as_str().unwrap_or("");
    if diff.is_empty() {
        return ToolOutput::err("diff is required");
    }
    let base = args["base_path"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or(ctx.workspace_dir.as_deref())
        .unwrap_or(".");
    let base_path = std::path::Path::new(base);
    match sentinel_ai_core::apply_patch::apply_patch_multi(base_path, diff) {
        Ok(changed) => {
            let mut out = format!("Applied patch to {} file(s):", changed.len());
            for path in changed {
                out.push_str(&format!("\n- {}", path));
            }
            ToolOutput::ok(out)
        }
        Err(e) => ToolOutput::err(format!("Patch failed: {}", e)),
    }
}

// ── Glob ─────────────────────────────────────────────────────────
pub struct GlobTool;
#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }
    fn description(&self) -> &str {
        "Find files matching a glob pattern (supports ** doublestar). When a limit is set, returns the most recently modified matches"
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. **/*.rs)" },
                "path": { "type": "string", "description": "Directory to search in" },
                "dot_files": { "type": "boolean", "description": "Include hidden files (default false)" },
                "limit": { "type": "integer", "description": "Max results; the most recently modified matches are kept (default: no limit)" }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let pattern = args["pattern"].as_str().unwrap_or("");
        if pattern.is_empty() {
            return ToolOutput::err("pattern is required");
        }
        let base_dir = args["path"].as_str().map(|p| p.to_string());
        let dot_files = args["dot_files"].as_bool().unwrap_or(false);
        let limit = args["limit"].as_u64().map(|v| v as usize);
        let base_dir = base_dir.unwrap_or_else(|| ".".to_string());
        let full_pattern = format!(
            "{}/{}",
            base_dir.trim_end_matches(['/', '\\']),
            pattern
        );
        let base_path = std::path::Path::new(&base_dir);
        let filter = crate::filter::FileFilter::new(base_path, dot_files);
        match glob::glob(&full_pattern) {
            Ok(entries) => {
                let mut results: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter(|p| !filter.should_skip(p))
                    .map(|p| p.display().to_string())
                    .collect();
                // With a limit, collect the full match set, keep the most
                // recently modified first, then truncate and say so.
                if let Some(max) = limit {
                    if results.len() > max {
                        results.sort_by(|a, b| {
                            let mtime = |p: &str| {
                                std::fs::metadata(p)
                                    .and_then(|m| m.modified())
                                    .ok()
                            };
                            let (ta, tb) = (mtime(a), mtime(b));
                            tb.cmp(&ta).then_with(|| a.cmp(b))
                        });
                        let total = results.len();
                        results.truncate(max);
                        let json = serde_json::to_string_pretty(&results)
                            .unwrap_or_else(|_| "[]".to_string());
                        return ToolOutput::ok(format!(
                            "{} match(es), showing the latest {} (most recently modified first):\n{}",
                            total, max, json
                        ));
                    }
                }
                results.sort();
                ToolOutput::ok(
                    serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string()),
                )
            }
            Err(e) => ToolOutput::err(format!("Glob error: {}", e)),
        }
    }
}

// ── Grep ─────────────────────────────────────────────────────────
pub struct GrepTool;
#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Search file contents using a regex pattern"
    }
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
        if pattern.is_empty() {
            return ToolOutput::err("pattern is required");
        }
        let path = args["path"].as_str().unwrap_or(".");
        let include = args["include"].as_str();

        let regex = regex::Regex::new(pattern).ok();
        let base = std::path::Path::new(path);
        let filter = crate::filter::FileFilter::new(base, false);

        let mut results = Vec::new();
        for entry in walk_filtered(base, &filter) {
            if let Some(inc) = include {
                let name = entry
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                if !match_include(&name, inc) {
                    continue;
                }
            }
            if let Ok(content) = std::fs::read_to_string(&entry) {
                for (i, line) in content.lines().enumerate() {
                    let hit = match &regex {
                        Some(re) => re.is_match(line),
                        None => line.contains(pattern),
                    };
                    if hit {
                        results.push(format!("{}:{}: {}", entry.display(), i + 1, line));
                    }
                }
            }
        }
        ToolOutput::ok(results.join("\n"))
    }
}

/// Depth-first walk pruning hidden/ignored directories upfront.
fn walk_filtered(dir: &std::path::Path, filter: &crate::filter::FileFilter) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if !filter.should_skip(&p) {
                    stack.push(p);
                }
            } else if !filter.should_skip(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Match a file name against an `include` filter (`*.rs` glob or exact name).
fn match_include(name: &str, include: &str) -> bool {
    let inc = include.trim();
    if inc.is_empty() {
        return true;
    }
    if inc.contains('*') || inc.contains('?') {
        crate::filter::glob_match(inc, name)
    } else {
        name == inc
    }
}

// ── RunShellCommand (sandboxed) ─────────────────────────────────
pub struct RunShellCommandTool;
#[async_trait]
impl Tool for RunShellCommandTool {
    fn name(&self) -> &str {
        "run_shell_command"
    }
    fn description(&self) -> &str {
        "Execute a shell command inside an OS-level sandbox jail and capture output. On Windows commands run under a Job Object (kill-on-close, process limits); on Linux under bubblewrap when available."
    }
    fn is_mutating(&self) -> bool {
        true
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "timeout": { "type": "integer", "description": "Timeout in milliseconds" },
                "workdir": { "type": "string", "description": "Working directory (defaults to workspace)" }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let command = args["command"].as_str().unwrap_or("");
        if command.is_empty() {
            return ToolOutput::err("command is required");
        }

        let timeout_ms = args["timeout"].as_u64().unwrap_or(120_000);
        let workdir = args["workdir"]
            .as_str()
            .or(ctx.sandbox_dir.as_deref())
            .or(ctx.workspace_dir.as_deref())
            .unwrap_or(".");

        let workdir = if !workdir.is_empty() && !std::path::Path::new(workdir).is_dir() {
            std::env::current_dir()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_else(|_| ".".to_string())
        } else {
            workdir.to_string()
        };

        let jail =
            sentinel_exec::OSJailSandbox::new(&workdir).with_mode(sentinel_exec::JailMode::Auto);

        #[cfg(target_os = "windows")]
        let (shell, shell_arg) = ("cmd", "/C");
        #[cfg(not(target_os = "windows"))]
        let (shell, shell_arg) = ("sh", "-c");

        let run = async { jail.run(shell, &[shell_arg, command], None).await };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), run).await {
            Ok(Ok(output)) => {
                let mut text = String::new();
                if !output.stdout.is_empty() {
                    text.push_str(&output.stdout);
                }
                if !output.stderr.is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&output.stderr);
                }
                if output.success() {
                    ToolOutput::ok_sandboxed(text)
                } else {
                    ToolOutput::err_sandboxed(format!("exit code {}: {}", output.exit_code, text))
                }
            }
            Ok(Err(e)) => ToolOutput::err_sandboxed(format!("Command failed: {}", e)),
            Err(_) => {
                ToolOutput::err_sandboxed(format!("Command timed out after {} ms", timeout_ms))
            }
        }
    }
}

// ── WebSearch ────────────────────────────────────────────────────
pub struct WebSearchTool;
#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "Search the web for information"
    }
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
        if query.is_empty() {
            return ToolOutput::err("query is required");
        }
        let max_results = args["max_results"].as_u64().unwrap_or(5);

        // Simple web search via a public API (can be replaced with any search backend)
        let client = reqwest::Client::new();
        let url = format!(
            "https://en.wikipedia.org/w/api.php?action=opensearch&search={}&limit={}&format=json",
            urlencoding(query),
            max_results
        );

        match client.get(&url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(body) => ToolOutput::ok(body),
                Err(e) => ToolOutput::err(format!("Search failed: {}", e)),
            },
            Err(e) => ToolOutput::err(format!("Search request failed: {}", e)),
        }
    }
}

// ── Git Status ─────────────────────────────────────────────────
pub struct GitStatusTool;
#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }
    fn description(&self) -> &str {
        "Show the working tree status"
    }
    fn is_mutating(&self) -> bool {
        false
    }
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
        run_git(path, &["status", "--short"]).await
    }
}

// ── Git Diff ───────────────────────────────────────────────────
pub struct GitDiffTool;
#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }
    fn description(&self) -> &str {
        "Show changes in the working tree"
    }
    fn is_mutating(&self) -> bool {
        false
    }
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
            run_git(path, &["diff", "--cached"]).await
        } else {
            run_git(path, &["diff"]).await
        }
    }
}

// ── Git Commit ─────────────────────────────────────────────────
pub struct GitCommitTool;
#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }
    fn description(&self) -> &str {
        "Create a git commit with staged changes"
    }
    fn is_mutating(&self) -> bool {
        true
    }
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
        run_git(path, &["commit", "-m", message]).await
    }
}

// ── Git Log ────────────────────────────────────────────────────
pub struct GitLogTool;
#[async_trait]
impl Tool for GitLogTool {
    fn name(&self) -> &str {
        "git_log"
    }
    fn description(&self) -> &str {
        "Show commit logs"
    }
    fn is_mutating(&self) -> bool {
        false
    }
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
        run_git(path, &["log", "--oneline", &format!("-{}", max_count)]).await
    }
}

async fn run_git(path: &str, args: &[&str]) -> ToolOutput {
    let result = tokio::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .await;
    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut text = String::new();
            if !stdout.is_empty() {
                text.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
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

// ── WebFetch ──────────────────────────────────────────────────────
pub struct WebFetchTool;
#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }
    fn description(&self) -> &str {
        "Fetch content from a URL and return it as text"
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" },
                "format": { "type": "string", "enum": ["text", "markdown", "html"], "description": "Output format" }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let url = args["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return ToolOutput::err("url is required");
        }

        let client = reqwest::Client::builder()
            .user_agent("SentinelAI/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status();
                match resp.text().await {
                    Ok(body) => {
                        if status.is_success() {
                            ToolOutput::ok(format!("Status: {}\n\n{}", status.as_u16(), body))
                        } else {
                            ToolOutput::err(format!("Status: {}\n\n{}", status.as_u16(), body))
                        }
                    }
                    Err(e) => ToolOutput::err(format!("Failed to read response body: {}", e)),
                }
            }
            Err(e) => ToolOutput::err(format!("Request failed: {}", e)),
        }
    }
}

// ── Plan ──────────────────────────────────────────────────────────
pub struct PlanTool;
#[async_trait]
impl Tool for PlanTool {
    fn name(&self) -> &str {
        "plan"
    }
    fn description(&self) -> &str {
        "Create a structured task plan for multi-step work"
    }
    fn is_mutating(&self) -> bool {
        false
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Plan title" },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" },
                            "priority": { "type": "string", "enum": ["high", "medium", "low"] }
                        },
                        "required": ["description"]
                    },
                    "description": "Ordered list of steps"
                }
            },
            "required": ["title", "steps"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let title = args["title"].as_str().unwrap_or("Plan");
        let steps = args["steps"].as_array();
        if steps.is_none() || steps.unwrap().is_empty() {
            return ToolOutput::err("steps must be a non-empty array");
        }
        let steps = steps.unwrap();
        let mut output = format!("# {}\n\n", title);
        for (i, step) in steps.iter().enumerate() {
            let desc = step["description"].as_str().unwrap_or("(no description)");
            let priority = step["priority"].as_str().unwrap_or("medium");
            output.push_str(&format!("{}. [{}] {}\n", i + 1, priority, desc));
        }
        ToolOutput::ok(output)
    }
}

// ── GitHub ────────────────────────────────────────────────────────
pub struct GitHubTool;
#[async_trait]
impl Tool for GitHubTool {
    fn name(&self) -> &str {
        "github"
    }
    fn description(&self) -> &str {
        "Interact with GitHub API (issues, PRs, repos)"
    }
    fn is_mutating(&self) -> bool {
        true
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create_issue", "create_pr", "list_issues", "get_repo"],
                    "description": "GitHub action to perform"
                },
                "owner": { "type": "string", "description": "Repository owner" },
                "repo": { "type": "string", "description": "Repository name" },
                "title": { "type": "string", "description": "Issue/PR title" },
                "body": { "type": "string", "description": "Issue/PR body" },
                "head": { "type": "string", "description": "Head branch (for PRs)" },
                "base": { "type": "string", "description": "Base branch (for PRs)" }
            },
            "required": ["action", "owner", "repo"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let action = args["action"].as_str().unwrap_or("");
        let owner = args["owner"].as_str().unwrap_or("");
        let repo = args["repo"].as_str().unwrap_or("");
        if action.is_empty() || owner.is_empty() || repo.is_empty() {
            return ToolOutput::err("action, owner, and repo are required");
        }

        let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
        let client = reqwest::Client::new();
        let api_base = format!("https://api.github.com/repos/{}/{}", owner, repo);

        match action {
            "get_repo" => {
                match client
                    .get(&api_base)
                    .header("User-Agent", "SentinelAI")
                    .bearer_auth(&token)
                    .send()
                    .await
                {
                    Ok(resp) => ToolOutput::ok(resp.text().await.unwrap_or_default()),
                    Err(e) => ToolOutput::err(format!("GitHub API error: {}", e)),
                }
            }
            "list_issues" => {
                let url = format!("{}/issues?state=open&per_page=10", api_base);
                match client
                    .get(&url)
                    .header("User-Agent", "SentinelAI")
                    .bearer_auth(&token)
                    .send()
                    .await
                {
                    Ok(resp) => ToolOutput::ok(resp.text().await.unwrap_or_default()),
                    Err(e) => ToolOutput::err(format!("GitHub API error: {}", e)),
                }
            }
            "create_issue" => {
                let title = args["title"].as_str().unwrap_or("");
                let body = args["body"].as_str().unwrap_or("");
                if title.is_empty() {
                    return ToolOutput::err("title is required for create_issue");
                }
                let payload = json!({ "title": title, "body": body });
                match client
                    .post(format!("{}/issues", api_base))
                    .header("User-Agent", "SentinelAI")
                    .bearer_auth(&token)
                    .json(&payload)
                    .send()
                    .await
                {
                    Ok(resp) => ToolOutput::ok(resp.text().await.unwrap_or_default()),
                    Err(e) => ToolOutput::err(format!("GitHub API error: {}", e)),
                }
            }
            "create_pr" => {
                let title = args["title"].as_str().unwrap_or("");
                let body = args["body"].as_str().unwrap_or("");
                let head = args["head"].as_str().unwrap_or("");
                let base = args["base"].as_str().unwrap_or("main");
                if title.is_empty() {
                    return ToolOutput::err("title is required for create_pr");
                }
                if head.is_empty() {
                    return ToolOutput::err("head branch is required for create_pr");
                }
                let payload = json!({ "title": title, "body": body, "head": head, "base": base });
                match client
                    .post(format!("{}/pulls", api_base))
                    .header("User-Agent", "SentinelAI")
                    .bearer_auth(&token)
                    .json(&payload)
                    .send()
                    .await
                {
                    Ok(resp) => ToolOutput::ok(resp.text().await.unwrap_or_default()),
                    Err(e) => ToolOutput::err(format!("GitHub API error: {}", e)),
                }
            }
            _ => ToolOutput::err(format!("Unknown action: {}", action)),
        }
    }
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".into(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

// ── Notify ─────────────────────────────────────────────────────
pub struct NotifyTool;
#[async_trait]
impl Tool for NotifyTool {
    fn name(&self) -> &str {
        "notify"
    }
    fn description(&self) -> &str {
        "Send a notification to configured messaging destinations (webhook, Slack)."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Notification body" },
                "title": { "type": "string", "description": "Optional title" },
                "severity": { "type": "string", "enum": ["info", "success", "warning", "error"], "description": "Severity level" },
                "webhook_url": { "type": "string", "description": "Optional custom webhook URL (defaults to SENTINEL_NOTIFY_WEBHOOK env)" }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let message = args["message"].as_str().unwrap_or("");
        if message.is_empty() {
            return ToolOutput::err("message is required");
        }
        let title = args["title"].as_str().unwrap_or("Notification");
        let severity = args["severity"].as_str().unwrap_or("info");
        let env_webhook = std::env::var("SENTINEL_NOTIFY_WEBHOOK").ok();
        let webhook = args["webhook_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or(env_webhook);

        let payload = json!({
            "text": format!("[{}] {}: {}", severity, title, message)
        });

        match webhook {
            Some(url) => {
                let client = reqwest::Client::new();
                match client.post(&url).json(&payload).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_success() {
                            ToolOutput::ok(format!("Notification sent (status {})", status))
                        } else {
                            let body = resp.text().await.unwrap_or_default();
                            ToolOutput::err(format!("Webhook returned {}: {}", status, body))
                        }
                    }
                    Err(e) => ToolOutput::err(format!("Failed to send notification: {}", e)),
                }
            }
            None => {
                // Fallback: log to stdout
                tracing::info!("NOTIFY [{}] {}: {}", severity, title, message);
                ToolOutput::ok("Notification logged (no webhook configured)")
            }
        }
    }
}

// ── Explore Docs ─────────────────────────────────────────────
pub struct ExploreDocsTool;
#[async_trait]
impl Tool for ExploreDocsTool {
    fn name(&self) -> &str {
        "explore_docs"
    }
    fn description(&self) -> &str {
        "Browse Sentinel AI documentation structure. Use this to discover available docs, then use fetch_docs to get full content."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "endpoint": { "type": "string", "description": "Documentation endpoint (e.g. transformers, datasets, trl, peft, diffusers, hub, gradio)" },
                "query": { "type": "string", "description": "Optional search query" },
                "max_results": { "type": "integer", "description": "Max results (default 20, max 50)" }
            },
            "required": ["endpoint"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let endpoint = args["endpoint"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches('/');
        if endpoint.is_empty() {
            return ToolOutput::err("endpoint is required");
        }
        let query = args["query"].as_str().filter(|s| !s.is_empty());
        let max_results = args["max_results"].as_u64().unwrap_or(20).min(50);

        let client = reqwest::Client::builder()
            .user_agent("SentinelAI/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        if endpoint == "gradio" {
            let url = "https://gradio.app/llms.txt";
            return match client.get(url).send().await {
                Ok(resp) => match resp.text().await {
                    Ok(body) => ToolOutput::ok(format!(
                        "# Gradio Documentation\n\nSource: https://gradio.app/docs\n\n---\n\n{}",
                        body
                    )),
                    Err(e) => ToolOutput::err(format!("Failed to read Gradio docs: {}", e)),
                },
                Err(e) => ToolOutput::err(format!("Failed to fetch Gradio docs: {}", e)),
            };
        }

        let docs_url = format!("https://huggingface.co/docs/{}/en/index.md", endpoint);
        match client.get(&docs_url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(body) => {
                    let lines: Vec<&str> = body.lines().collect();
                    let shown: Vec<&str> =
                        lines.iter().take(max_results as usize).copied().collect();
                    let total = lines.len();
                    let mut out = format!("Documentation for: {}\n\n", endpoint);
                    if let Some(q) = query {
                        out.push_str(&format!(
                            "Query: '{}' — showing up to {} results out of {} pages\n\n",
                            q, max_results, total
                        ));
                    } else {
                        out.push_str(&format!(
                            "Found {} pages (showing first {})\n\n",
                            total, max_results
                        ));
                    }
                    for (i, line) in shown.iter().enumerate() {
                        out.push_str(&format!("{}. {}\n", i + 1, line));
                    }
                    ToolOutput::ok(out)
                }
                Err(e) => ToolOutput::err(format!("Failed to read docs: {}", e)),
            },
            Ok(resp) => ToolOutput::err(format!("Docs endpoint returned {}", resp.status())),
            Err(e) => ToolOutput::err(format!("Failed to fetch docs: {}", e)),
        }
    }
}

// ── Fetch Docs ──────────────────────────────────────────────
pub struct FetchDocsTool;
#[async_trait]
impl Tool for FetchDocsTool {
    fn name(&self) -> &str {
        "fetch_docs"
    }
    fn description(&self) -> &str {
        "Fetch full markdown content of a documentation page. Use after explore_docs to get complete page content."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "Full URL to the documentation page. The .md extension is added automatically if missing." }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let mut url = args["url"].as_str().unwrap_or("").to_string();
        if url.is_empty() {
            return ToolOutput::err("url is required");
        }

        if !url.ends_with(".md") {
            url.push_str(".md");
        }

        let client = reqwest::Client::builder()
            .user_agent("SentinelAI/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(body) => ToolOutput::ok(format!("Documentation from: {}\n\n{}", url, body)),
                Err(e) => ToolOutput::err(format!("Failed to read response: {}", e)),
            },
            Ok(resp) => ToolOutput::err(format!("HTTP {} fetching {}", resp.status(), url)),
            Err(e) => ToolOutput::err(format!("Request failed: {}", e)),
        }
    }
}

// ── Find API ────────────────────────────────────────────────
pub struct FindApiTool;
#[async_trait]
impl Tool for FindApiTool {
    fn name(&self) -> &str {
        "find_api"
    }
    fn description(&self) -> &str {
        "Search Sentinel AI OpenAPI specification to find REST API endpoints. Returns curl examples with auth."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Keyword search across endpoint summaries and descriptions" },
                "tag": { "type": "string", "description": "Filter by API category tag" }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let query = args["query"].as_str().filter(|s| !s.is_empty());
        let tag = args["tag"].as_str().filter(|s| !s.is_empty());

        if query.is_none() && tag.is_none() {
            return ToolOutput::err("Provide either 'query' or 'tag' (or both)");
        }

        let client = reqwest::Client::builder()
            .user_agent("SentinelAI/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        let spec_url = "https://huggingface.co/.well-known/openapi.json";
        match client.get(spec_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                    Ok(spec) => {
                        let mut results = Vec::new();
                        if let Some(paths) = spec["paths"].as_object() {
                            for (path, path_item) in paths {
                                if let Some(obj) = path_item.as_object() {
                                    for (method, op) in obj {
                                        if !matches!(
                                            method.as_str(),
                                            "get" | "post" | "put" | "delete" | "patch"
                                        ) {
                                            continue;
                                        }
                                        let summary = op["summary"].as_str().unwrap_or("");
                                        let desc = op["description"].as_str().unwrap_or("");
                                        let tags_arr = op["tags"]
                                            .as_array()
                                            .map(|a| {
                                                a.iter()
                                                    .filter_map(|t| t.as_str())
                                                    .collect::<Vec<_>>()
                                                    .join(", ")
                                            })
                                            .unwrap_or_default();

                                        let mut matched = true;
                                        if let Some(q) = query {
                                            let ql = q.to_lowercase();
                                            matched = summary.to_lowercase().contains(&ql)
                                                || desc.to_lowercase().contains(&ql);
                                        }
                                        if let Some(t) = tag {
                                            matched = matched
                                                && tags_arr
                                                    .to_lowercase()
                                                    .contains(&t.to_lowercase());
                                        }
                                        if matched {
                                            results.push(format!(
                                                "{} {} — {}\n   Tags: {}",
                                                method.to_uppercase(),
                                                path,
                                                summary,
                                                tags_arr
                                            ));
                                        }
                                    }
                                }
                            }
                        }

                        if results.is_empty() {
                            ToolOutput::ok("No matching API endpoints found.")
                        } else {
                            let mut out = format!("Found {} API endpoint(s)\n\n", results.len());
                            for (i, r) in results.iter().enumerate() {
                                out.push_str(&format!("{}. {}\n\n", i + 1, r));
                            }
                            ToolOutput::ok(out)
                        }
                    }
                    Err(e) => ToolOutput::err(format!("Failed to parse OpenAPI spec: {}", e)),
                }
            }
            Ok(resp) => ToolOutput::err(format!("HTTP {} fetching OpenAPI spec", resp.status())),
            Err(e) => ToolOutput::err(format!("Failed to fetch OpenAPI spec: {}", e)),
        }
    }
}

// ── View ───────────────────────────────────────────────────────
/// Reads a file with line numbers, supporting offset/limit for large
/// files. Suggests nearby files when the requested path is mistyped.
pub struct ViewTool;
#[async_trait]
impl Tool for ViewTool {
    fn name(&self) -> &str {
        "view"
    }
    fn description(&self) -> &str {
        "Read a file with line numbers (offset/limit supported). Suggests nearby files on mistyped paths."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the file to view" },
                "offset": { "type": "integer", "description": "1-based line to start from" },
                "limit": { "type": "integer", "description": "Maximum number of lines to return" }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let path = args["file_path"].as_str().unwrap_or("");
        if path.is_empty() {
            return ToolOutput::err("file_path is required");
        }
        let offset = args["offset"].as_u64().map(|v| v as usize);
        let limit = args["limit"].as_u64().map(|v| v as usize);

        let content = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => {
                return ToolOutput::err(suggest_similar_path(path, ctx));
            }
        };

        if content.iter().take(8192).any(|b| *b == 0) {
            return ToolOutput::err(format!("{} appears to be a binary file", path));
        }

        let text = match String::from_utf8(content) {
            Ok(t) => t,
            Err(_) => {
                return ToolOutput::err(format!("{} is not valid UTF-8 text", path));
            }
        };

        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();
        let start = offset.unwrap_or(1).saturating_sub(1);
        if start >= total {
            return ToolOutput::ok(format!(
                "{} has {} lines; requested offset {} is past the end.",
                path, total, start + 1
            ));
        }
        let end = match limit {
            Some(l) => (start + l).min(total),
            None => total,
        };

        let mut out = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            out.push_str(&format!("{:>6} | {}\n", start + i + 1, line));
        }
        if end < total {
            out.push_str(&format!(
                "... ({} of {} lines shown; use offset/limit to page)\n",
                end - start,
                total
            ));
        }
        ToolOutput::ok(out)
    }
}

/// Best-effort nearest-filename suggestion for a mistyped path, scoped to
/// the parent directory (or the workspace root when the parent is missing).
fn suggest_similar_path(path: &str, ctx: &ToolContext) -> String {
    let p = std::path::Path::new(path);
    let parent = match p.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => ctx
            .workspace_dir
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::path::PathBuf::new),
    };
    let wanted = p
        .file_name()
        .map(|f| f.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut scored: Vec<(usize, String)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.to_lowercase() == wanted {
                return format!("File not found: {}", path);
            }
            scored.push((edit_distance(&wanted, &name.to_lowercase()), name));
        }
    }
    scored.sort_by_key(|(d, _)| *d);
    let mut out = format!("File not found: {}", path);
    if let Some((dist, name)) = scored.first() {
        if *dist <= 4 && *dist < wanted.len().max(2) {
            out.push_str(&format!("\nDid you mean: {}", name));
            if scored.len() > 1 && scored[1].0 <= *dist + 1 {
                out.push_str(&format!(" or {}", scored[1].1));
            }
        }
    }
    out
}

/// Classic Levenshtein edit distance between two ASCII/lowercased strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur.push((prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}

// ── Ls ─────────────────────────────────────────────────────────
/// Lists files and subdirectories in a tree structure, skipping hidden
/// and common system/build directories.
pub struct LsTool;
#[async_trait]
impl Tool for LsTool {
    fn name(&self) -> &str {
        "ls"
    }
    fn description(&self) -> &str {
        "List and search files. Without `fuzzy`: prints a tree (skips hidden and common build/system dirs). With `fuzzy <query>`: ranks matching paths by subsequence similarity (fzf-style), best first"
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory to explore (default: workspace root)" },
                "depth": { "type": "integer", "description": "Maximum recursion depth (default 2, max 6)" },
                "max_entries": { "type": "integer", "description": "Maximum entries to print (default 200)" },
                "fuzzy": { "type": "string", "description": "Fuzzy query; returns ranked flat matches instead of a tree" }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let root = args["path"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| ctx.workspace_dir.as_ref().map(std::path::PathBuf::from))
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        if !root.is_dir() {
            return ToolOutput::err(format!("Not a directory: {}", root.display()));
        }

        let depth = args["depth"].as_u64().map(|v| v as usize).unwrap_or(2).min(6);
        let max_entries = args["max_entries"].as_u64().map(|v| v as usize).unwrap_or(200);
        let fuzzy_query = args["fuzzy"]
            .as_str()
            .map(str::to_string)
            .filter(|s| !s.is_empty());

        if let Some(query) = fuzzy_query {
            let mut matches: Vec<(i64, String)> = Vec::new();
            collect_fuzzy(&root, &query, depth, &mut matches);
            if matches.is_empty() {
                return ToolOutput::ok(format!("no matches for {:?}\n", query));
            }
            matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            if matches.len() > max_entries {
                matches.truncate(max_entries);
            }
            let mut out = format!("{} match(es) for {:?}:\n", matches.len(), query);
            for (_, path) in matches {
                out.push_str(&format!("  {}\n", path));
            }
            return ToolOutput::ok(out);
        }

        let mut out = String::new();
        out.push_str(&format!("{}\n", root.display()));
        let mut count = 0usize;
        let mut truncated = false;

        fn walk(
            dir: &std::path::Path,
            prefix: &str,
            depth_left: usize,
            max: usize,
            count: &mut usize,
            truncated: &mut bool,
            out: &mut String,
        ) {
            if *count >= max {
                *truncated = true;
                return;
            }
            let mut entries: Vec<_> = std::fs::read_dir(dir)
                .map(|rd| rd.flatten().collect())
                .unwrap_or_default();
            entries.sort_by_key(|e| {
                let is_dir = e.path().is_dir();
                (!is_dir, e.file_name().to_string_lossy().to_lowercase())
            });

            for (i, entry) in entries.iter().enumerate() {
                if *count >= max {
                    *truncated = true;
                    return;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if is_skipped_entry(&name) {
                    continue;
                }
                let is_dir = entry.path().is_dir();
                let last = i == entries.len() - 1;
                let branch = if last { "└── " } else { "├── " };
                out.push_str(&format!(
                    "{}{}{}{}\n",
                    prefix,
                    branch,
                    name,
                    if is_dir { "/" } else { "" }
                ));
                *count += 1;
                if is_dir && depth_left > 0 {
                    let child_prefix = format!("{}{}", prefix, if last { "    " } else { "│   " });
                    walk(
                        &entry.path(),
                        &child_prefix,
                        depth_left - 1,
                        max,
                        count,
                        truncated,
                        out,
                    );
                }
            }
        }

        walk(&root, "", depth, max_entries, &mut count, &mut truncated, &mut out);
        if truncated {
            out.push_str(&format!("... (truncated after {} entries)\n", count));
        }
        ToolOutput::ok(out)
    }
}

/// Entries skipped by `ls`: hidden files and common build/system dirs.
fn is_skipped_entry(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    matches!(
        name,
        "node_modules"
            | "target"
            | "dist"
            | "build"
            | "out"
            | ".venv"
            | "venv"
            | "vendor"
            | "__pycache__"
            | ".next"
            | ".cache"
            | ".idea"
            | ".vscode"
            | ".git"
    )
}

/// Subsequence fuzzy match with scoring (fzf-style). Returns `Some(score)`
/// where lower is a better match; `None` when `query` is not a subsequence
/// of `text` (case-insensitive). Intended for the `ls --fuzzy` ranking:
/// contiguous runs and a leading-position hit score better than gaps.
fn fuzzy_score(query: &str, text: &str) -> Option<i64> {
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let mut qi = 0usize;
    let mut prev: Option<usize> = None;
    let mut score: i64 = 0;
    for (ti, tc) in t.iter().enumerate() {
        if qi < q.len() && *tc == q[qi] {
            match prev {
                None => {
                    score -= 1;
                    if ti == 0 {
                        score -= 3; // prefix bonus
                    }
                }
                Some(p) => {
                    if ti == p + 1 {
                        score -= 3; // contiguity bonus
                    } else {
                        score += 2; // gap penalty between matched chars
                    }
                }
            }
            prev = Some(ti);
            qi += 1;
            if qi == q.len() {
                return Some(score);
            }
        } else {
            score += 1; // gap penalty for skipped text
        }
    }
    None
}

/// Flat walk for `ls --fuzzy`: emit every non-skipped entry whose relative
/// path is a subsequence match, keeping the score. Descends into non-matching
/// directories so deep matches are found.
fn collect_fuzzy(root: &std::path::Path, query: &str, max_depth: usize, out: &mut Vec<(i64, String)>) {
    fn walk(
        dir: &std::path::Path,
        root: &std::path::Path,
        query: &str,
        depth_left: usize,
        out: &mut Vec<(i64, String)>,
    ) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_skipped_entry(&name) {
                continue;
            }
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());
            if let Some(score) = fuzzy_score(query, &rel) {
                out.push((score, rel));
            }
            if path.is_dir() && depth_left > 0 {
                walk(&path, root, query, depth_left - 1, out);
            }
        }
    }
    walk(root, root, query, max_depth, out);
}

// ── Sourcegraph ────────────────────────────────────────────────
/// Searches code across public repositories via Sourcegraph's GraphQL API.
pub struct SourcegraphTool;
#[async_trait]
impl Tool for SourcegraphTool {
    fn name(&self) -> &str {
        "sourcegraph"
    }
    fn description(&self) -> &str {
        "Search public code on Sourcegraph. Returns repository, file path, and matching line snippets."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Sourcegraph search query, e.g. 'lang:go regexp' or 'repo:facebook/react useSyncExternalStore'" },
                "first": { "type": "integer", "description": "Maximum file matches to return (default 10, max 25)" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let query = args["query"].as_str().unwrap_or("").trim();
        if query.is_empty() {
            return ToolOutput::err("query is required");
        }
        let first = args["first"].as_u64().unwrap_or(10).min(25) as u32;

        let endpoint = std::env::var("SOURCEGRAPH_ENDPOINT")
            .unwrap_or_else(|_| "https://sourcegraph.com/.api/graphql".to_string());
        let token = std::env::var("SOURCEGRAPH_TOKEN")
            .or_else(|_| std::env::var("SRC_ACCESS_TOKEN"))
            .ok();

        let client = reqwest::Client::builder()
            .user_agent("SentinelAI/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let body = serde_json::json!({
            "query": r#"query($q: String!, $n: Int!) {
  search(query: $q, version: V3) {
    results {
      matchCount
      results {
        __typename
        ... on FileMatch {
          repository { name }
          file { path }
          lineMatches { lineNumber preview }
        }
      }
    }
  }
}"#,
            "variables": { "q": query, "n": first }
        });

        let mut req = client.post(&endpoint).json(&body);
        if let Some(t) = &token {
            req = req.header("Authorization", format!("Bearer {}", t));
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                let parsed: serde_json::Value = match resp.json().await {
                    Ok(v) => v,
                    Err(e) => return ToolOutput::err(format!("Failed to parse response: {}", e)),
                };
                if let Some(errors) = parsed["errors"].as_array() {
                    let msg = errors
                        .iter()
                        .filter_map(|e| e["message"].as_str())
                        .collect::<Vec<_>>()
                        .join("; ");
                    return ToolOutput::err(format!("Sourcegraph error: {}", msg));
                }
                let results = parsed["data"]["search"]["results"]["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let match_count = parsed["data"]["search"]["results"]["matchCount"]
                    .as_u64()
                    .unwrap_or(results.len() as u64);

                if results.is_empty() {
                    return ToolOutput::ok("No results found.");
                }
                let mut out = format!("{} result(s) for: {}\n\n", match_count, query);
                for (i, r) in results.iter().enumerate() {
                    let repo = r["repository"]["name"].as_str().unwrap_or("?");
                    let path = r["file"]["path"].as_str().unwrap_or("?");
                    out.push_str(&format!("{}. {}/{}", i + 1, repo, path));
                    let lines = r["lineMatches"].as_array().cloned().unwrap_or_default();
                    for lm in lines.iter().take(5) {
                        let ln = lm["lineNumber"].as_u64().unwrap_or(0) + 1;
                        let preview = lm["preview"].as_str().unwrap_or("");
                        out.push_str(&format!("\n    {} | {}", ln, preview.trim_end()));
                    }
                    out.push('\n');
                }
                ToolOutput::ok(out)
            }
            Ok(resp) => ToolOutput::err(format!(
                "HTTP {} from Sourcegraph ({}). Set SOURCEGRAPH_TOKEN or SOURCEGRAPH_ENDPOINT to configure.",
                resp.status(),
                endpoint
            )),
            Err(e) => ToolOutput::err(format!("Request failed: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ToolContext {
        ToolContext::new()
    }

    #[tokio::test]
    async fn view_numbers_lines_and_pages() {
        let dir = std::env::temp_dir().join(format!("sentinel-view-test-{}", std::process::id()));
        let file = dir.join("demo.txt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "one\ntwo\nthree\nfour\n").unwrap();

        let full = ViewTool
            .execute(json!({ "file_path": file.to_string_lossy() }), &ctx())
            .await;
        assert!(!full.is_error, "{}", full.text);
        assert!(full.text.contains("1 | one"), "{}", full.text);
        assert!(full.text.contains("4 | four"), "{}", full.text);

        let paged = ViewTool
            .execute(
                json!({ "file_path": file.to_string_lossy(), "offset": 2, "limit": 2 }),
                &ctx(),
            )
            .await;
        assert!(!paged.is_error, "{}", paged.text);
        assert!(paged.text.contains("2 | two"), "{}", paged.text);
        assert!(paged.text.contains("3 | three"), "{}", paged.text);
        assert!(!paged.text.contains("4 | four"), "{}", paged.text);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn view_suggests_mistyped_paths() {
        let dir = std::env::temp_dir().join(format!("sentinel-view-sug-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app_router.rs"), "// x\n").unwrap();

        let out = ViewTool
            .execute(
                json!({ "file_path": format!("{}/app_rouer.rs", dir.display()) }),
                &ctx(),
            )
            .await;
        assert!(out.is_error, "{}", out.text);
        assert!(out.text.contains("Did you mean"), "{}", out.text);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn view_rejects_binary_files() {
        let dir = std::env::temp_dir().join(format!("sentinel-view-bin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("blob.bin"), [0u8, 1, 2, 3, 0]).unwrap();

        let out = ViewTool
            .execute(
                json!({ "file_path": format!("{}/blob.bin", dir.display()) }),
                &ctx(),
            )
            .await;
        assert!(out.is_error, "{}", out.text);
        assert!(out.text.contains("binary"), "{}", out.text);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn ls_renders_tree_and_skips_hidden() {
        let dir = std::env::temp_dir().join(format!("sentinel-ls-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::write(dir.join("src").join("main.rs"), "// m\n").unwrap();
        std::fs::write(dir.join(".hidden"), "h\n").unwrap();
        std::fs::write(dir.join("node_modules").join("pkg.js"), "// p\n").unwrap();

        let out = LsTool
            .execute(json!({ "path": dir.to_string_lossy(), "depth": 2 }), &ctx())
            .await;
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.contains("src/"), "{}", out.text);
        assert!(out.text.contains("main.rs"), "{}", out.text);
        assert!(!out.text.contains(".hidden"), "{}", out.text);
        assert!(!out.text.contains("node_modules"), "{}", out.text);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn edit_distance_is_sane() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("abc", "abc"), 0);
        assert_eq!(edit_distance("abc", "abd"), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn fuzzy_score_ranks_better_matches_lower() {
        // exact prefix beats gapped subsequence
        assert!(fuzzy_score("ab", "ab").unwrap() < fuzzy_score("ab", "xab").unwrap());
        // contiguous beats gapped
        assert!(fuzzy_score("ab", "xxab").unwrap() < fuzzy_score("ab", "axb").unwrap());
        // matching always beats non-match
        assert!(fuzzy_score("abc", "axbycz").is_some());
        assert!(fuzzy_score("abc", "ab").is_none());
        assert!(fuzzy_score("abc", "ABC").is_some());
        // empty query matches everything at zero
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[tokio::test]
    async fn sourcegraph_requires_query() {
        let out = SourcegraphTool
            .execute(json!({ "query": "   " }), &ctx())
            .await;
        assert!(out.is_error, "{}", out.text);
    }
}

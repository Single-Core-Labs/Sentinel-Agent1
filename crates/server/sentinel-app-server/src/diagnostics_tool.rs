//! `diagnostics` tool: exposes the LSP `DiagnosticsStore` to the agent as a
//! regular tool so it can surface errors/warnings for a file or the project.
//!
//! The store is populated by the live LSP clients (`crate::lsp::LspManager`);
//! when no language server has published anything yet the tool reports an
//! empty result rather than failing, and when no store is attached it
//! explains that LSP is not available.

use async_trait::async_trait;
use sentinel_tools::tool::{Tool, ToolContext, ToolOutput};
use std::sync::Arc;

pub struct DiagnosticsTool {
    store: Arc<crate::lsp::DiagnosticsStore>,
}

impl DiagnosticsTool {
    pub fn new(store: Arc<crate::lsp::DiagnosticsStore>) -> Self {
        Self { store }
    }
}

fn severity_name(severity: Option<u32>) -> &'static str {
    match severity {
        Some(1) => "error",
        Some(2) => "warning",
        Some(3) => "info",
        Some(4) => "hint",
        _ => "unknown",
    }
}

fn severity_filter(sev: &str) -> Option<u32> {
    match sev.to_lowercase().as_str() {
        "error" | "1" => Some(1),
        "warning" | "warn" | "2" => Some(2),
        "info" | "information" | "3" => Some(3),
        "hint" | "4" => Some(4),
        "all" | "" => None,
        _ => None,
    }
}

#[async_trait]
impl Tool for DiagnosticsTool {
    fn name(&self) -> &str {
        "diagnostics"
    }
    fn description(&self) -> &str {
        "Retrieve code diagnostics (errors, warnings) for a file or the whole project from LSP clients"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Optional path to a single file. Omit to list all files with diagnostics." },
                "severity": { "type": "string", "description": "Filter: error, warning, info, hint, or all (default all)" }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let file_path = args["file_path"].as_str().filter(|s| !s.is_empty());
        let severity = args["severity"].as_str().and_then(severity_filter);

        let mut out = String::new();
        let mut total = 0usize;

        match file_path {
            Some(path) => {
                let diags = self
                    .store
                    .snapshot_for_path(std::path::Path::new(path))
                    .await;
                out.push_str(&format!("{} — {} diagnostic(s)\n", path, diags.len()));
                for d in diags {
                    if let Some(s) = severity
                        && d.severity != Some(s)
                    {
                        continue;
                    }
                    let line = d.start_line.map(|l| l + 1).unwrap_or(0);
                    let col = d.start_char.map(|c| c + 1).unwrap_or(0);
                    out.push_str(&format!(
                        "  {}:{} [{}] {}{}\n",
                        line,
                        col,
                        severity_name(d.severity),
                        d.message,
                        d.code
                            .as_ref()
                            .map(|c| format!(" ({})", c))
                            .unwrap_or_default(),
                    ));
                    total += 1;
                }
            }
            None => {
                let per_file = self.store.per_file().await;
                if per_file.is_empty() {
                    return ToolOutput::ok(
                        "No diagnostics available. LSP clients may not be configured (sentinel.toml `[lsp_servers]`), or no files have been checked yet.",
                    );
                }
                let total_diags = self.store.total().await;
                out.push_str(&format!(
                    "{} file(s) with diagnostics ({} total)\n",
                    per_file.len(),
                    total_diags
                ));
                for (uri, count) in &per_file {
                    let path = uri
                        .strip_prefix("file://")
                        .unwrap_or(uri)
                        .replace("%20", " ");
                    out.push_str(&format!("  {} — {} diagnostic(s)\n", path, count));
                }
                total = per_file.len();
            }
        }

        if total == 0 {
            return ToolOutput::ok("No diagnostics for the requested scope.");
        }
        ToolOutput::ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_store_returns_helpful_message() {
        let tool = DiagnosticsTool::new(Arc::new(crate::lsp::DiagnosticsStore::new()));
        let out = tool
            .execute(serde_json::json!({}), &ToolContext::new())
            .await;
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.contains("LSP"), "{}", out.text);
    }

    #[tokio::test]
    async fn lists_file_diagnostics_with_severity_filter() {
        let store = Arc::new(crate::lsp::DiagnosticsStore::new());
        let path = "C:/project/src/main.rs";
        let uri = crate::lsp::path_to_file_uri(std::path::Path::new(path));
        store
            .record(
                &uri,
                vec![
                    crate::lsp::LspDiagnostic {
                        uri: uri.clone(),
                        message: "unused variable".into(),
                        severity: Some(2),
                        code: Some("unused_vars".into()),
                        source: Some("rust-analyzer".into()),
                        start_line: Some(10),
                        start_char: Some(4),
                    },
                    crate::lsp::LspDiagnostic {
                        uri: uri.clone(),
                        message: "mismatched types".into(),
                        severity: Some(1),
                        code: None,
                        source: Some("rust-analyzer".into()),
                        start_line: Some(12),
                        start_char: Some(0),
                    },
                ],
            )
            .await;

        let tool = DiagnosticsTool::new(store.clone());
        let all = tool
            .execute(
                serde_json::json!({ "file_path": path }),
                &ToolContext::new(),
            )
            .await;
        assert!(!all.is_error, "{}", all.text);
        assert!(all.text.contains("unused variable"), "{}", all.text);
        assert!(all.text.contains("mismatched types"), "{}", all.text);

        let errors_only = tool
            .execute(
                serde_json::json!({ "file_path": path, "severity": "error" }),
                &ToolContext::new(),
            )
            .await;
        assert!(
            errors_only.text.contains("mismatched types"),
            "{}",
            errors_only.text
        );
        assert!(
            !errors_only.text.contains("unused variable"),
            "{}",
            errors_only.text
        );
    }
}

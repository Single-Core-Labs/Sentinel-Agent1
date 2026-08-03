use std::collections::HashMap;
use tower_lsp::lsp_types::*;

use sentinel_core::{Agent, AgentThread, AutoApprovalGate};
use std::sync::Arc;

/// Tracks open documents and provides agent-powered LSP features.
pub struct LspSession {
    /// Open documents: URI -> content
    documents: HashMap<Url, String>,
    pub agent: Option<Arc<Agent>>,
}

impl LspSession {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            agent: None,
        }
    }

    pub fn with_agent(mut self, agent: Arc<Agent>) -> Self {
        self.agent = Some(agent);
        self
    }

    pub fn open_document(&mut self, uri: Url, content: String) {
        tracing::debug!("Document opened: {}", uri);
        self.documents.insert(uri, content);
    }

    pub fn update_document(&mut self, uri: &Url, content: String) {
        tracing::debug!("Document updated: {}", uri);
        self.documents.insert(uri.clone(), content);
    }

    pub fn close_document(&mut self, uri: &Url) {
        tracing::debug!("Document closed: {}", uri);
        self.documents.remove(uri);
    }

    pub fn get_hover(&self, params: &TextDocumentPositionParams) -> Option<Hover> {
        let _uri = &params.text_document.uri;
        let _line = params.position.line;
        let _col = params.position.character;

        // Future: use agent to generate contextual hover info
        // For now, return None to fall back to the language's native hover
        None
    }

    pub fn get_completions(
        &self,
        _params: &TextDocumentPositionParams,
    ) -> Option<CompletionResponse> {
        // Future: provide agent-powered completions
        None
    }

    pub fn get_code_actions(&self, _params: &CodeActionParams) -> Option<CodeActionResponse> {
        let actions = vec![
            CodeActionOrCommand::CodeAction(CodeAction {
                title: "Explain with Sentinel".into(),
                kind: Some(CodeActionKind::REFACTOR),
                command: Some(Command {
                    title: "Explain".into(),
                    command: "sentinel.explain".into(),
                    arguments: None,
                }),
                ..Default::default()
            }),
            CodeActionOrCommand::CodeAction(CodeAction {
                title: "Refactor with Sentinel".into(),
                kind: Some(CodeActionKind::REFACTOR),
                command: Some(Command {
                    title: "Refactor".into(),
                    command: "sentinel.refactor".into(),
                    arguments: None,
                }),
                ..Default::default()
            }),
        ];
        Some(actions)
    }

    pub async fn explain_selection(&self, args: &[serde_json::Value]) -> Option<String> {
        if let Some(ref agent) = self.agent {
            let prompt = args
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or("Explain the selected code");
            let mut thread = AgentThread::new(5, 10, true);
            let gate = AutoApprovalGate;
            if let Ok(output) = agent
                .run_with_approval(
                    &mut thread,
                    &format!("Explain this code:\n{}", prompt),
                    &gate,
                    &None,
                )
                .await
            {
                return match output {
                    sentinel_core::AgentOutput::Success { text } => Some(text),
                    sentinel_core::AgentOutput::Error { message } => {
                        Some(format!("Error: {}", message))
                    }
                };
            }
        }
        Some("Sentinel LSP: Connect an agent to enable explanations.".into())
    }

    pub async fn refactor_code(&self, args: &[serde_json::Value]) -> Option<serde_json::Value> {
        if let Some(ref agent) = self.agent {
            let code = args.first().and_then(|v| v.as_str()).unwrap_or("");
            let mut thread = AgentThread::new(5, 10, true);
            let gate = AutoApprovalGate;
            if let Ok(output) = agent
                .run_with_approval(
                    &mut thread,
                    &format!(
                        "Refactor the following code to make it cleaner and more efficient:\n{}",
                        code
                    ),
                    &gate,
                    &None,
                )
                .await
            {
                let text = match output {
                    sentinel_core::AgentOutput::Success { text } => text,
                    sentinel_core::AgentOutput::Error { message } => format!("Error: {}", message),
                };
                return Some(serde_json::json!({ "refactored": text }));
            }
        }
        None
    }

    pub async fn generate_code(&self, args: &[serde_json::Value]) -> Option<serde_json::Value> {
        if let Some(ref agent) = self.agent {
            let spec = args
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or("Generate code");
            let mut thread = AgentThread::new(5, 10, true);
            let gate = AutoApprovalGate;
            if let Ok(output) = agent
                .run_with_approval(&mut thread, spec, &gate, &None)
                .await
            {
                let text = match output {
                    sentinel_core::AgentOutput::Success { text } => text,
                    sentinel_core::AgentOutput::Error { message } => format!("Error: {}", message),
                };
                return Some(serde_json::json!({ "generated": text }));
            }
        }
        None
    }

    pub async fn analyze_document(&self, uri: &Url) -> Vec<Diagnostic> {
        if let Some(content) = self.documents.get(uri) {
            tracing::debug!("Analyzing document {} ({} chars)", uri, content.len());
        }
        Vec::new()
    }
}

impl Default for LspSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_and_close_document() {
        let mut session = LspSession::new();
        let uri = Url::parse("file:///test.rs").unwrap();
        session.open_document(uri.clone(), "fn main() {}".into());
        assert!(session.documents.contains_key(&uri));

        session.close_document(&uri);
        assert!(!session.documents.contains_key(&uri));
    }

    #[test]
    fn test_code_actions_available() {
        let session = LspSession::new();
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///test.rs").unwrap(),
            },
            range: Range::new(Position::new(0, 0), Position::new(1, 0)),
            context: CodeActionContext {
                diagnostics: vec![],
                only: None,
                trigger_kind: Some(CodeActionTriggerKind::INVOKED),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let actions = session.get_code_actions(&params);
        assert!(actions.is_some());
    }
}

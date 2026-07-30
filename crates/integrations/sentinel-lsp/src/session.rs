use std::collections::HashMap;
use tower_lsp::lsp_types::*;

/// Tracks open documents and provides agent-powered LSP features.
pub struct LspSession {
    /// Open documents: URI -> content
    documents: HashMap<Url, String>,
}

impl LspSession {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
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

    pub fn get_completions(&self, _params: &TextDocumentPositionParams) -> Option<CompletionResponse> {
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
        Some(actions.into())
    }

    pub async fn explain_selection(&self, _args: &[serde_json::Value]) -> Option<String> {
        // Future: use sentinel-core agent to explain selected code
        Some("Sentinel LSP explanation — agent integration pending (post-launch)".into())
    }

    pub async fn refactor_code(&self, _args: &[serde_json::Value]) -> Option<serde_json::Value> {
        // Future: use sentinel-core agent to refactor code
        None
    }

    pub async fn generate_code(&self, _args: &[serde_json::Value]) -> Option<serde_json::Value> {
        // Future: use sentinel-core agent to generate code from prompt
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
            text_document: TextDocumentIdentifier { uri: Url::parse("file:///test.rs").unwrap() },
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

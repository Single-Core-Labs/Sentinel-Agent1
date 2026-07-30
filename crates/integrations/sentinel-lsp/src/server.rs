use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::{LspService, Server};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::session::LspSession;

/// LSP server that exposes the Sentinel agent as a language server.
///
/// Provides code actions, diagnostics, and inline editing powered by
/// the Sentinel agent — allowing IDEs to call on the agent for
/// refactoring, code generation, and explanation.
pub struct SentinelLspServer {
    session: Arc<RwLock<LspSession>>,
    capabilities: ServerCapabilities,
    client: Option<tower_lsp::Client>,
}

impl SentinelLspServer {
    pub fn new(session: LspSession) -> Self {
        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::INCREMENTAL)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: vec![
                    "sentinel.explain".into(),
                    "sentinel.refactor".into(),
                    "sentinel.generate".into(),
                    "sentinel.review".into(),
                ],
                work_done_progress_options: Default::default(),
            }),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some("sentinel".into()),
                inter_file_dependencies: true,
                work_done_progress_options: Default::default(),
                workspace_diagnostics: true,
            })),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![".".into()]),
                ..Default::default()
            }),
            ..Default::default()
        };

        Self {
            session: Arc::new(RwLock::new(session)),
            capabilities,
            client: None,
        }
    }
}

#[tower_lsp::async_trait]
impl tower_lsp::LanguageServer for SentinelLspServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!(
            "LSP server initialized (process_id={:?}, root_uri={:?})",
            params.process_id,
            params.root_uri,
        );

        Ok(InitializeResult {
            capabilities: self.capabilities.clone(),
            server_info: Some(ServerInfo {
                name: "sentinel-lsp".into(),
                version: Some("0.1.0".into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("LSP client initialized");
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("LSP server shutting down");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let mut session = self.session.write().await;
        session.open_document(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let mut session = self.session.write().await;
        for change in params.content_changes {
            session.update_document(&params.text_document.uri, change.text);
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        tracing::debug!("Document saved: {:?}", params.text_document.uri);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut session = self.session.write().await;
        session.close_document(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let session = self.session.read().await;
        Ok(session.get_hover(&params.text_document_position_params))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let session = self.session.read().await;
        Ok(session.get_completions(&params.text_document_position))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let session = self.session.read().await;
        Ok(session.get_code_actions(&params))
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            "sentinel.explain" => {
                let session = self.session.read().await;
                let explanation = session.explain_selection(&params.arguments).await;
                Ok(explanation.map(|e| serde_json::json!({ "explanation": e })))
            }
            "sentinel.refactor" => {
                let session = self.session.read().await;
                let result = session.refactor_code(&params.arguments).await;
                Ok(result)
            }
            "sentinel.generate" => {
                let session = self.session.read().await;
                let result = session.generate_code(&params.arguments).await;
                Ok(result)
            }
            _ => Ok(None),
        }
    }

    async fn diagnostic(&self, params: DocumentDiagnosticParams) -> Result<DocumentDiagnosticReportResult> {
        let session = self.session.read().await;
        let diagnostics = session.analyze_document(&params.text_document.uri).await;
        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: diagnostics,
                },
            }),
        ))
    }
}

/// Run the LSP server over stdio.
pub async fn run_lsp_server(session: LspSession) {
    let server = SentinelLspServer::new(session);
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|_client| server);
    Server::new(stdin, stdout, socket).serve(service).await;
}

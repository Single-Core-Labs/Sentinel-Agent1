//! Application core and state management.
//!
//! [`App`] is the single orchestrating struct for the CLI crate. It owns the
//! services the central agent needs — the session/message/history store, the
//! permission gate and the theme from config — together with the agent itself.
//! Both the interactive and the non-interactive entry points construct one `App`
//! and tear it down through [`shutdown`](App::shutdown).

use colored::*;
use futures::FutureExt;
use sentinel_config::{SentinelConfig, ThemeSettings};
use sentinel_core::thread_store::{JsonFileThreadStore, ThreadStore, ThreadStoreError};
use sentinel_core::{
    Agent, AgentOutput, AgentThread, ApprovalGate, AutoApprovalGate, PolicyEngine,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;

/// Recover from a panic so the user gets a friendly message and a non-zero
/// exit instead of a raw unwind.
pub(crate) fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown internal error".to_string()
    }
}

/// Central application state: services + agent + background clients.
pub struct App {
    config: Arc<SentinelConfig>,
    store: Option<Arc<dyn ThreadStore>>,
    permissions: Box<dyn ApprovalGate>,
    #[allow(dead_code)]
    theme: ThemeSettings,
    agent: Option<Agent>,
    shutdown_tx: watch::Sender<bool>,
    #[allow(dead_code)]
    shutdown_rx: watch::Receiver<bool>,
}

impl App {
    /// Construct the app core from config: session/history store,
    /// permission gate, and theme.
    pub fn new(config: SentinelConfig) -> Self {
        let theme = config.theme.clone();
        let store = thread_store_from_config(&config);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            config: Arc::new(config),
            store,
            permissions: Box::new(crate::approval::CliApprovalGate),
            theme,
            agent: None,
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Override the session/history store (used by tests).
    #[allow(dead_code)]
    pub fn with_store(mut self, store: Option<Arc<dyn ThreadStore>>) -> Self {
        self.store = store;
        self
    }

    /// Attach the central agent that orchestrates tasks through its tools.
    pub fn attach_agent(&mut self, agent: Agent) {
        self.agent = Some(agent);
    }

    /// Take the agent back out (e.g. to wrap it in a `PipelineAgent`).
    pub fn take_agent(&mut self) -> Option<Agent> {
        self.agent.take()
    }

    #[allow(dead_code)]
    pub fn agent(&self) -> Option<&Agent> {
        self.agent.as_ref()
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &SentinelConfig {
        &self.config
    }

    #[allow(dead_code)]
    pub fn theme(&self) -> &ThemeSettings {
        &self.theme
    }

    #[allow(dead_code)]
    pub fn store(&self) -> Option<&Arc<dyn ThreadStore>> {
        self.store.as_ref()
    }

    /// Replace the permission gate used for interactive runs.
    pub fn set_permissions(&mut self, gate: Box<dyn ApprovalGate>) {
        self.permissions = gate;
    }

    pub fn permissions(&self) -> &dyn ApprovalGate {
        self.permissions.as_ref()
    }

    /// New session thread using config limits and the given auto-approval flag.
    pub fn new_session(&self, yolo_mode: bool) -> AgentThread {
        AgentThread::new(
            self.config.agent.max_turns,
            self.config.agent.max_iterations,
            yolo_mode,
        )
    }

    pub async fn resume_session(&self, id: &str) -> Result<AgentThread, ThreadStoreError> {
        match &self.store {
            Some(s) => s.load_thread(id).await,
            None => Err(ThreadStoreError::NotFound(id.to_string())),
        }
    }

    pub async fn save_session(&self, thread: &AgentThread) -> Result<(), ThreadStoreError> {
        match &self.store {
            Some(s) => s.save_thread(thread).await,
            None => Err(ThreadStoreError::Store(
                "no session store configured".into(),
            )),
        }
    }

    #[allow(dead_code)]
    pub async fn list_sessions(&self) -> Result<Vec<String>, ThreadStoreError> {
        match &self.store {
            Some(s) => s.list_threads().await,
            None => Ok(Vec::new()),
        }
    }

    /// Non-interactive command execution: run the agent on a fresh (or
    /// resumed) session with every permission request auto-approved to
    /// streamline automated operations, then save the session, print the
    /// final output and a token summary.
    pub async fn run_non_interactive(
        &self,
        thread: &mut AgentThread,
        prompt: &str,
        policy: Option<Arc<dyn PolicyEngine>>,
    ) -> anyhow::Result<()> {
        let agent = self
            .agent
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no agent attached to app"))?;

        let result = match std::panic::AssertUnwindSafe(agent.run_with_approval(
            thread,
            prompt,
            &AutoApprovalGate,
            &policy,
        ))
        .catch_unwind()
        .await
        {
            Ok(r) => r,
            Err(payload) => {
                crate::display::print_error(&panic_message(payload));
                return Err(anyhow::anyhow!("agent panicked during run"));
            }
        };

        if let Some(store) = &self.store
            && let Err(e) = store.save_thread(thread).await
        {
            eprintln!("{} Failed to save session: {}", "W".yellow(), e);
        }

        match result {
            Ok(AgentOutput::Success { text }) => {
                if !text.is_empty() {
                    println!("\n{}", text);
                }
                let (p, c) = (agent.prompt_tokens(), agent.completion_tokens());
                println!(
                    "\n[sentinel] session summary: prompt_tokens={} completion_tokens={} total_tokens={}",
                    p,
                    c,
                    p + c
                );
                println!();
                Ok(())
            }
            Ok(AgentOutput::Error { message }) => {
                crate::display::print_error(&message);
                Err(anyhow::anyhow!(message))
            }
            Err(e) => {
                crate::display::print_error(&e.to_string());
                Err(anyhow::anyhow!("{}", e))
            }
        }
    }

    /// Interactive REPL: read prompts from stdin, run the agent with the
    /// configured permission gate, print the result, and save the session
    /// after every turn so history survives crashes and resumable sessions
    /// stay current.
    pub async fn run_interactive(
        &self,
        thread: &mut AgentThread,
        policy: Option<Arc<dyn PolicyEngine>>,
    ) -> anyhow::Result<()> {
        use std::io::Write;
        let agent = self
            .agent
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no agent attached to app"))?;

        loop {
            print!("{} ", ">".yellow().bold());
            std::io::stdout().flush()?;

            let mut input = String::new();
            let bytes = std::io::stdin().read_line(&mut input)?;
            if bytes == 0 {
                println!();
                break;
            }
            let input = input.trim().to_string();
            if input.is_empty() {
                continue;
            }
            if matches!(input.as_str(), "exit" | "quit") {
                break;
            }
            if matches!(input.as_str(), "/help" | "/h") {
                println!();
                println!("  /help, /h   Show this help");
                println!("  exit, quit  End the session");
                println!("  (session id: {} — resume later with `sentinel ai --resume {}`)", thread.id, thread.id);
                println!();
                continue;
            }

            let result = match std::panic::AssertUnwindSafe(agent.run_with_approval(
                thread,
                &input,
                self.permissions.as_ref(),
                &policy,
            ))
            .catch_unwind()
            .await
            {
                Ok(r) => r,
                Err(payload) => {
                    crate::display::print_error(&panic_message(payload));
                    Ok(AgentOutput::Error {
                        message: "agent panicked during run".to_string(),
                    })
                }
            };

            if let Some(store) = &self.store
                && let Err(e) = store.save_thread(thread).await
            {
                eprintln!("{} Failed to save session: {}", "W".yellow(), e);
            }

            match result {
                Ok(AgentOutput::Success { text }) => {
                    if !text.is_empty() {
                        println!("\n{}", text);
                    }
                }
                Ok(AgentOutput::Error { message }) => {
                    crate::display::print_error(&message)
                }
                Err(e) => crate::display::print_error(&e.to_string()),
            }
            println!();
        }

        Ok(())
    }

    /// Graceful shutdown: signal every background task.
    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Whether a shutdown has been requested.
    #[allow(dead_code)]
    pub fn shutdown_requested(&self) -> bool {
        *self.shutdown_rx.borrow()
    }
}

/// Pick the session/history store from `config.thread_store`:
/// `none`/empty disables persistence, `sqlite` uses the SQLite store (when the
/// `sqlite` feature is enabled), anything else falls back to JSON files.
fn thread_store_from_config(config: &SentinelConfig) -> Option<Arc<dyn ThreadStore>> {
    match config.thread_store.as_str() {
        "none" | "" => None,
        "sqlite" => {
            #[cfg(feature = "sqlite")]
            {
                use sentinel_core::thread_store::SqliteThreadStore;
                let db_path = std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join("sentinel_threads.db");
                match SqliteThreadStore::new(&db_path) {
                    Ok(s) => {
                        tracing::info!("Session store: SQLite at {}", db_path.display());
                        Some(Arc::new(s) as Arc<dyn ThreadStore>)
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to open SQLite thread store: {} — sessions will not persist",
                            e
                        );
                        None
                    }
                }
            }
            #[cfg(not(feature = "sqlite"))]
            {
                tracing::warn!("sqlite feature not enabled — sessions will not persist");
                None
            }
        }
        _ => {
            let dir = default_session_dir();
            tracing::info!("Session store: JSON files at {}", dir.display());
            Some(Arc::new(JsonFileThreadStore::new(dir)) as Arc<dyn ThreadStore>)
        }
    }
}

fn default_session_dir() -> PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return PathBuf::from(home).join("threads");
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|h| PathBuf::from(h).join(".sentinel").join("threads"))
        .unwrap_or_else(|_| PathBuf::from("sentinel_threads"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use sentinel_protocol::{
        Choice, CompletionRequest, CompletionResponse, ContentBlock, Delta, Message, Role,
        StreamChoice, StreamChunk,
    };
    use sentinel_provider::{ModelProvider, ProviderError};
    use sentinel_provider_info::ProviderInfo;
    use sentinel_tools::ToolRegistry;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            id: "r0".into(),
            model: "mock-model".into(),
            choices: vec![Choice {
                index: 0,
                message: Message::new(
                    Role::Assistant,
                    vec![ContentBlock::Text { text: text.into() }],
                ),
                finish_reason: Some("stop".into()),
            }],
            usage: None,
        }
    }

    struct ScriptedProvider {
        info: ProviderInfo,
        responses: Vec<CompletionResponse>,
        cursor: AtomicUsize,
    }

    #[async_trait]
    impl ModelProvider for ScriptedProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn complete(
            &self,
            _req: &CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
            let pick = idx.min(self.responses.len().saturating_sub(1));
            self.responses
                .get(pick)
                .cloned()
                .ok_or_else(|| ProviderError::RequestError("no responses scripted".into()))
        }

        async fn complete_stream(
            &self,
            req: &CompletionRequest,
        ) -> Result<
            Box<dyn tokio_stream::Stream<Item = Result<StreamChunk, ProviderError>> + Send + Unpin>,
            ProviderError,
        > {
            let resp = self.complete(req).await?;
            let chunks: Vec<Result<StreamChunk, ProviderError>> = resp
                .choices
                .into_iter()
                .map(|c| {
                    Ok(StreamChunk {
                        id: "mock-0".into(),
                        model: "mock-model".into(),
                        choices: vec![StreamChoice {
                            index: 0,
                            delta: Delta {
                                role: Some("assistant".into()),
                                content: Some(c.message.extract_text()),
                                tool_calls: None,
                            },
                            finish_reason: c.finish_reason,
                        }],
                    })
                })
                .collect();
            Ok(Box::new(tokio_stream::iter(chunks)))
        }
    }

    fn scripted_provider(responses: Vec<CompletionResponse>) -> Arc<dyn ModelProvider> {
        Arc::new(ScriptedProvider {
            info: ProviderInfo::default(),
            responses,
            cursor: AtomicUsize::new(0),
        })
    }

    fn test_agent() -> Agent {
        Agent::new(
            scripted_provider(vec![text_response("hello")]),
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
        )
    }

    #[test]
    fn new_sets_theme_from_config() {
        let mut cfg = SentinelConfig::default();
        cfg.theme.name = "paper".into();
        let app = App::new(cfg);
        assert_eq!(app.theme().name, "paper");
        assert!(app.agent().is_none());
    }

    #[test]
    fn store_none_when_disabled() {
        let cfg = SentinelConfig {
            thread_store: "none".into(),
            ..SentinelConfig::default()
        };
        assert!(App::new(cfg).store().is_none());
    }

    #[test]
    fn store_json_by_default() {
        assert!(App::new(SentinelConfig::default()).store().is_some());
    }

    #[test]
    fn attach_and_take_agent() {
        let mut app = App::new(SentinelConfig::default());
        assert!(app.agent().is_none());
        app.attach_agent(test_agent());
        assert!(app.agent().is_some());
        let taken = app.take_agent();
        assert!(taken.is_some());
        assert!(app.agent().is_none());
    }

    #[test]
    fn new_session_uses_config_limits() {
        let mut cfg = SentinelConfig::default();
        cfg.agent.max_turns = 3;
        cfg.agent.max_iterations = 7;
        let app = App::new(cfg);
        let thread = app.new_session(true);
        assert_eq!(thread.max_turns, 3);
        assert_eq!(thread.max_iterations, 7);
        assert!(thread.yolo_mode);
    }

    #[tokio::test]
    async fn run_non_interactive_runs_agent_and_saves_session() {
        let dir = std::env::temp_dir().join(format!(
            "sentinel_app_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = App::new(SentinelConfig::default())
            .with_store(Some(
                Arc::new(JsonFileThreadStore::new(&dir)) as Arc<dyn ThreadStore>
            ));
        app.attach_agent(test_agent());

        let mut thread = app.new_session(true);
        app.run_non_interactive(&mut thread, "hi", None)
            .await
            .expect("non-interactive run must succeed");

        let sessions = app.list_sessions().await.expect("list must succeed");
        assert_eq!(sessions.len(), 1, "session must be persisted after the run");
        let loaded = app
            .resume_session(&thread.id.to_string())
            .await
            .expect("saved session must load");
        assert_eq!(loaded.id, thread.id);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn shutdown_signals_flag() {
        let app = App::new(SentinelConfig::default());
        assert!(!app.shutdown_requested());
        app.shutdown().await;
        assert!(app.shutdown_requested(), "shutdown must flip the signal");
    }
}

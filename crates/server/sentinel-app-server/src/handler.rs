use sentinel_analytics::{AnalyticsEvent, AnalyticsPipeline, EventKind};
use sentinel_app_server_protocol::api::{self, methods, ServerEvent, SessionSummary};
use sentinel_app_server_protocol::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use sentinel_config::SentinelConfig;
use sentinel_core::conversation::Item;
#[cfg(feature = "sqlite")]
use sentinel_core::thread_store::SqliteThreadStore;
use sentinel_core::thread_store::ThreadStore;
use sentinel_provider::{ModelProvider, ProviderKind};
use sentinel_provider_info::ProviderInfo;
use sentinel_tools::ToolRegistry;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

pub struct RequestHandler {
    sessions: Arc<tokio::sync::Mutex<HashMap<String, Arc<crate::session::AppSession>>>>,
    config: Arc<SentinelConfig>,
    analytics: Arc<AnalyticsPipeline>,
    tools: Arc<ToolRegistry>,
    headroom_compressor: Option<Arc<dyn sentinel_core::ContentCompressor>>,
    #[allow(dead_code)]
    thread_store: Option<Arc<dyn ThreadStore>>,
    /// Runtime overrides applied via `config/set` (merged on top of the
    /// file-based config for `config/get` responses).
    config_overrides: RwLock<serde_json::Map<String, Value>>,
    /// Pending `dialog/askUser` requests, keyed by request id.
    pending_dialogs: Mutex<HashMap<String, oneshot::Sender<String>>>,
    /// Currently authenticated agent token (None = logged out).
    auth_token: RwLock<Option<String>>,
    /// Latest IDE context per active file.
    ide_context: RwLock<HashMap<String, Value>>,
    /// Server start time (for diagnostics uptime).
    started_at: Instant,
    /// Lifetime token counters (approximate, chars/4).
    tokens_in: AtomicU64,
    tokens_out: AtomicU64,
}

impl RequestHandler {
    /// Look up a session by ID (used by streaming clients).
    pub async fn get_session(&self, session_id: &str) -> Option<Arc<crate::session::AppSession>> {
        let sessions = self.sessions.lock().await;
        sessions.get(session_id).cloned()
    }

    async fn get_or_load_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<crate::session::AppSession>, JsonRpcError> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_id) {
            return Ok(session.clone());
        }

        if let Some(ref store) = self.thread_store {
            match store.load_thread(session_id).await {
                Ok(thread) => {
                    let model_id = self.config.agent.default_model.clone();
                    let provider_info =
                        self.find_provider_for_model(&model_id).ok_or_else(|| {
                            JsonRpcError::internal_error(format!(
                                "No provider info found for default model: {}",
                                model_id
                            ))
                        })?;
                    let provider = ProviderKind::from_info(provider_info).map_err(|e| {
                        JsonRpcError::internal_error(format!("Failed to create provider: {}", e))
                    })?;
                    let provider: Arc<dyn ModelProvider> = Arc::new(provider);

                    let session = Arc::new(crate::session::AppSession::new_with_thread(
                        session_id.to_string(),
                        thread,
                        provider,
                        self.tools.clone(),
                        self.config.clone(),
                        self.analytics.clone(),
                        self.headroom_compressor.clone(),
                    ));

                    sessions.insert(session_id.to_string(), session.clone());
                    Ok(session)
                }
                Err(e) => Err(JsonRpcError::invalid_params(format!(
                    "Session not found or failed to load from database: {}",
                    e
                ))),
            }
        } else {
            Err(JsonRpcError::invalid_params(format!(
                "Session not found: {}",
                session_id
            )))
        }
    }

    pub fn new(
        config: Arc<SentinelConfig>,
        analytics: Arc<AnalyticsPipeline>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self::new_with_headroom(config, analytics, tools, None)
    }

    /// Create with an explicit thread store for session persistence (Gap 5).
    pub fn new_with_store(
        config: Arc<SentinelConfig>,
        analytics: Arc<AnalyticsPipeline>,
        tools: Arc<ToolRegistry>,
        thread_store: Option<Arc<dyn ThreadStore>>,
    ) -> Self {
        let mut handler = Self::new_with_headroom(config, analytics, tools, None);
        handler.thread_store = thread_store;
        handler
    }

    pub fn new_with_headroom(
        config: Arc<SentinelConfig>,
        analytics: Arc<AnalyticsPipeline>,
        tools: Arc<ToolRegistry>,
        headroom_compressor: Option<Arc<dyn sentinel_core::ContentCompressor>>,
    ) -> Self {
        let thread_store: Option<Arc<dyn ThreadStore>> = match config.thread_store.as_str() {
            "sqlite" => {
                #[cfg(feature = "sqlite")]
                {
                    let db_path = std::env::current_dir()
                        .expect("Failed to get current directory")
                        .join("sentinel_threads.db");
                    match SqliteThreadStore::new(db_path) {
                        Ok(store) => Some(Arc::new(store)),
                        Err(e) => {
                            panic!("Failed to initialize SQLite thread store: {}", e);
                        }
                    }
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    panic!("sqlite feature not enabled for sentinel-app-server");
                }
            }
            _ => None,
        };
        let handler = Self {
            sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            config,
            analytics,
            tools,
            headroom_compressor,
            thread_store,
            config_overrides: RwLock::new(serde_json::Map::new()),
            pending_dialogs: Mutex::new(HashMap::new()),
            auth_token: RwLock::new(None),
            ide_context: RwLock::new(HashMap::new()),
            started_at: Instant::now(),
            tokens_in: AtomicU64::new(0),
            tokens_out: AtomicU64::new(0),
        };
        handler.spawn_log_pump();
        handler
    }

    /// Pump the process-wide log bus (Gap 8a) into every active session's
    /// event channel. Filters to WARN/ERROR by default, DEBUG+ when the
    /// `[debug] enabled` flag is set in config.
    fn spawn_log_pump(&self) {
        let sessions = self.sessions.clone();
        let debug_enabled = self.config.debug.enabled;
        // Subscribe synchronously so events emitted right after construction
        // are never missed (broadcast receivers do not replay).
        let mut rx = crate::logs::subscribe_logs();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(line) => {
                        let level = crate::logs::level_from_str(&line.level);
                        if !crate::logs::visible_at_min_level(&level, debug_enabled) {
                            continue;
                        }
                        let sessions = sessions.lock().await;
                        for session in sessions.values() {
                            let _ = session.events.send(ServerEvent::Log {
                                level: line.level.clone(),
                                message: line.message.clone(),
                            });
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    fn find_provider_for_model(&self, model_id: &str) -> Option<ProviderInfo> {
        for p in self.config.providers() {
            if p.models.iter().any(|m| m.id == model_id) {
                return Some(p.clone());
            }
        }
        None
    }

    pub async fn handle(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let id = req.id.clone();
        let result = match req.method.as_str() {
            methods::PING => self.handle_ping(),
            methods::CREATE_SESSION => self.handle_create_session(req.params).await,
            methods::DESTROY_SESSION => self.handle_destroy_session(req.params).await,
            methods::GET_SESSION => self.handle_get_session(req.params).await,
            methods::CHAT => self.handle_chat(req.params).await,
            methods::CHAT_STREAM => self.handle_chat_stream(req.params).await,
            methods::GET_HISTORY => self.handle_get_history(req.params).await,
            methods::TOOLS_LIST => {
                let tool_defs = self.tools.list();
                Ok(serde_json::to_value(tool_defs).unwrap_or_default())
            }
            methods::TOOLS_CALL => self.handle_tools_call(req.params).await,
            methods::FS_READ_FILE => self.handle_fs_read_file(req.params).await,
            methods::FS_WRITE_FILE => self.handle_fs_write_file(req.params).await,
            methods::FS_GLOB => self.handle_fs_glob(req.params).await,
            methods::FS_GREP => self.handle_fs_grep(req.params).await,
            methods::COMMAND_EXEC => self.handle_command_exec(req.params).await,
            methods::COMMAND_EXEC_SANDBOXED => self.handle_command_exec_sandboxed(req.params).await,
            methods::CONFIG_GET => self.handle_config_get(),
            methods::CONFIG_SET => self.handle_config_set(req.params).await,
            methods::EVENT_SUBSCRIBE => self.handle_event_subscribe(req.params).await,
            methods::EVENT_UNSUBSCRIBE => Ok(serde_json::json!({ "unsubscribed": true })),
            methods::DIALOG_ASK_USER => self.handle_dialog_ask_user(req.params).await,
            methods::DIALOG_SUBMIT_RESPONSE => self.handle_dialog_submit_response(req.params).await,
            methods::SESSION_BROWSER_LIST => self.handle_session_browser_list().await,
            methods::IDE_CONTEXT_SYNC => self.handle_ide_context_sync(req.params).await,
            methods::IDE_DIFF_PREVIEW => self.handle_ide_diff_preview(req.params).await,
            methods::AUTH_LOGIN => self.handle_auth_login(req.params).await,
            methods::AUTH_LOGOUT => self.handle_auth_logout().await,
            methods::AUTH_STATUS => self.handle_auth_status().await,
            methods::DIAGNOSTICS => self.handle_diagnostics().await,
            _ => Err(JsonRpcError::method_not_found(format!(
                "Unknown method: {}",
                req.method
            ))),
        };

        match result {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(result),
                error: None,
            },
            Err(err) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(err),
            },
        }
    }

    fn handle_ping(&self) -> Result<Value, JsonRpcError> {
        Ok(serde_json::json!({ "pong": true }))
    }

    async fn handle_create_session(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::CreateSessionParams = parse_params(params)?;
        let model_id = p
            .model
            .unwrap_or_else(|| self.config.agent.default_model.clone());

        let provider_info = self.find_provider_for_model(&model_id).ok_or_else(|| {
            JsonRpcError::invalid_params(format!(
                "No configured provider found for model '{}'. Available providers: {}",
                model_id,
                self.config
                    .providers()
                    .iter()
                    .flat_map(|p| p.models.iter().map(|m| m.id.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;

        let provider = ProviderKind::from_info(provider_info).map_err(|e| {
            JsonRpcError::internal_error(format!("Failed to create provider: {}", e))
        })?;
        let provider: Arc<dyn ModelProvider> = Arc::new(provider);

        let session = match &self.headroom_compressor {
            Some(compressor) => Arc::new(crate::session::AppSession::new_with_compressor(
                Some(model_id.clone()),
                provider,
                self.tools.clone(),
                self.config.clone(),
                self.analytics.clone(),
                compressor.clone(),
            )),
            None => Arc::new(crate::session::AppSession::new(
                Some(model_id.clone()),
                provider,
                self.tools.clone(),
                self.config.clone(),
                self.analytics.clone(),
            )),
        };

        let session_id = session.id.clone();
        let _ = session.events.send(ServerEvent::SessionCreated {
            session_id: session_id.clone(),
            model: model_id.clone(),
        });
        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), session.clone());

        if let Some(ref store) = self.thread_store {
            let thread = session.thread.lock().await;
            if let Err(e) = store.save_thread(&thread).await {
                tracing::error!("Failed to save new thread to store: {:?}", e);
            }
        }

        self.analytics.emit(AnalyticsEvent::new(
            EventKind::SessionCreated,
            Some(session_id.clone()),
        ));

        Ok(serde_json::json!({
            "session_id": session_id,
            "model": model_id,
        }))
    }

    async fn handle_destroy_session(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let session_id: String = parse_params::<serde_json::Value>(params)?
            .get("session_id")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| JsonRpcError::invalid_params("Missing session_id"))?;
        let mut sessions = self.sessions.lock().await;
        let removed = sessions.remove(&session_id);
        if let Some(ref session) = removed {
            let _ = session.events.send(ServerEvent::SessionEnded {
                session_id: session_id.clone(),
                reason: "destroyed".into(),
            });
        }
        drop(removed);

        if let Some(ref store) = self.thread_store {
            if let Err(e) = store.delete_thread(&session_id).await {
                tracing::warn!("Failed to delete thread from store: {:?}", e);
            }
        }

        self.analytics.emit(AnalyticsEvent::new(
            EventKind::SessionEnded,
            Some(session_id.clone()),
        ));

        Ok(serde_json::json!({ "destroyed": true, "session_id": session_id }))
    }

    async fn handle_get_session(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let session_id: String = parse_params::<serde_json::Value>(params)?
            .get("session_id")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| JsonRpcError::invalid_params("Missing session_id"))?;

        let session = self.get_or_load_session(&session_id).await?;

        let thread = session.thread.lock().await;
        Ok(serde_json::json!({
            "session_id": session_id,
            "turn": thread.turn,
            "iterations": thread.iterations,
            "status": format!("{:?}", thread.status),
            "turn_count": thread.conversation.turn_count(),
            "total_items": thread.conversation.total_items(),
        }))
    }

    async fn handle_chat(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::ChatParams = parse_params(params)?;
        let session = self.get_or_load_session(&p.session_id).await?;

        self.tokens_in
            .fetch_add(estimate_tokens(&p.message), Ordering::Relaxed);
        self.analytics.emit(
            AnalyticsEvent::new(EventKind::MessageSent, Some(p.session_id.clone()))
                .with_metadata(serde_json::json!({ "len": p.message.len() })),
        );

        let chat_result = session.chat(&p.message).await;

        if let Some(ref store) = self.thread_store {
            let thread = session.thread.lock().await;
            if let Err(e) = store.save_thread(&thread).await {
                tracing::error!("Failed to save thread to store: {:?}", e);
            }
        }

        match chat_result {
            Ok(response) => {
                self.tokens_out
                    .fetch_add(estimate_tokens(&response), Ordering::Relaxed);
                self.analytics.emit(
                    AnalyticsEvent::new(EventKind::MessageReceived, Some(p.session_id))
                        .with_metadata(serde_json::json!({ "len": response.len() })),
                );
                Ok(serde_json::json!({ "response": response }))
            }
            Err(e) => Err(JsonRpcError::internal_error(e)),
        }
    }

    async fn handle_chat_stream(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::ChatStreamParams = parse_params(params)?;
        let session = self.get_or_load_session(&p.session_id).await?;

        let (tx, rx) = mpsc::channel(64);
        let msg = p.message.clone();
        tokio::spawn({
            let session = session.clone();
            let store = self.thread_store.clone();
            async move {
                session.chat_stream(&msg, tx).await;
                if let Some(ref store) = store {
                    let thread = session.thread.lock().await;
                    if let Err(e) = store.save_thread(&thread).await {
                        tracing::error!("Failed to save thread to store: {:?}", e);
                    }
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        let chunks: Vec<serde_json::Value> = stream
            .filter_map(|r| match r {
                Ok(chunk) => Some(serde_json::to_value(chunk).unwrap_or_default()),
                Err(e) => Some(serde_json::json!({ "error": e })),
            })
            .collect()
            .await;

        Ok(serde_json::json!({ "chunks": chunks }))
    }

    async fn handle_get_history(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let session_id: String = parse_params::<serde_json::Value>(params)?
            .get("session_id")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| JsonRpcError::invalid_params("Missing session_id"))?;

        let session = self.get_or_load_session(&session_id).await?;

        let thread = session.thread.lock().await;
        let conversation = &thread.conversation;

        Ok(serde_json::json!({
            "session_id": session_id,
            "conversation": serde_json::to_value(conversation).unwrap_or_default(),
        }))
    }

    async fn handle_tools_call(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::ToolCallParams = parse_params(params)?;
        let ctx = sentinel_tools::ToolContext::new();
        let output = self.tools.execute(&p.tool_name, p.arguments, &ctx).await;

        self.analytics.emit(
            AnalyticsEvent::new(EventKind::ToolCalled, None)
                .with_metadata(serde_json::json!({ "tool": p.tool_name })),
        );

        Ok(serde_json::json!({
            "output": output.text,
            "is_error": output.is_error,
        }))
    }

    async fn handle_fs_read_file(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::FsReadParams = parse_params(params)?;
        let ctx = sentinel_tools::ToolContext::new();
        let args = serde_json::json!({ "file_path": p.path });
        let output = self.tools.execute("read", args, &ctx).await;
        if output.is_error {
            Err(JsonRpcError::internal_error(output.text))
        } else {
            Ok(serde_json::json!({ "content": output.text }))
        }
    }

    async fn handle_fs_write_file(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::FsWriteParams = parse_params(params)?;
        let ctx = sentinel_tools::ToolContext::new();
        let args = serde_json::json!({ "file_path": p.path, "content": p.content });
        let output = self.tools.execute("write", args, &ctx).await;
        if output.is_error {
            Err(JsonRpcError::internal_error(output.text))
        } else {
            Ok(serde_json::json!({ "message": output.text }))
        }
    }

    async fn handle_fs_glob(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::FsGlobParams = parse_params(params)?;
        let ctx = sentinel_tools::ToolContext::new();
        let args = serde_json::json!({ "pattern": p.pattern });
        let output = self.tools.execute("glob", args, &ctx).await;
        if output.is_error {
            Err(JsonRpcError::internal_error(output.text))
        } else {
            let files: Vec<String> = serde_json::from_str(&output.text).unwrap_or_default();
            Ok(serde_json::json!({ "files": files }))
        }
    }

    async fn handle_fs_grep(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let ctx = sentinel_tools::ToolContext::new();
        let args = params.clone().unwrap_or_default();
        let output = self.tools.execute("grep", args, &ctx).await;
        if output.is_error {
            Err(JsonRpcError::internal_error(output.text))
        } else {
            Ok(serde_json::json!({ "matches": output.text }))
        }
    }

    async fn handle_command_exec(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::CommandExecParams = parse_params(params)?;
        let ctx = sentinel_tools::ToolContext::new();
        let full_cmd = if p.args.is_empty() {
            p.command.clone()
        } else {
            format!("{} {}", p.command, p.args.join(" "))
        };
        let args = serde_json::json!({
            "command": full_cmd,
            "workdir": p.cwd.unwrap_or_default(),
            "timeout": 120_000,
        });
        let output = self.tools.execute("bash", args, &ctx).await;
        let exit_code = if output.is_error { 1 } else { 0 };
        Ok(serde_json::json!({
            "exit_code": exit_code,
            "stdout": if output.is_error { "" } else { &output.text },
            "stderr": if output.is_error { &output.text } else { "" },
        }))
    }

    async fn handle_command_exec_sandboxed(
        &self,
        params: Option<Value>,
    ) -> Result<Value, JsonRpcError> {
        let p: api::CommandExecParams = parse_params(params)?;
        if p.command.is_empty() {
            return Err(JsonRpcError::invalid_params("command is required"));
        }
        let cwd = p.cwd.clone().unwrap_or_else(|| ".".to_string());
        let jail = sentinel_exec::jail::OSJailSandbox::new(&cwd)
            .with_mode(sentinel_exec::jail::JailMode::Auto);
        let env: Option<Vec<(String, String)>> = p.env.clone().map(|m| m.into_iter().collect());
        let args: Vec<&str> = p.args.iter().map(|s| s.as_str()).collect();

        match jail.run(&p.command, &args, env).await {
            Ok(out) => Ok(serde_json::json!({
                "exit_code": out.exit_code,
                "stdout": out.stdout,
                "stderr": out.stderr,
            })),
            Err(e) => Err(JsonRpcError::internal_error(format!(
                "Sandboxed exec failed: {}",
                e
            ))),
        }
    }

    fn config_with_overrides(&self) -> Result<Value, JsonRpcError> {
        let mut base = self.handle_config_get()?;
        let overrides = self
            .config_overrides
            .try_read()
            .map_err(|_| JsonRpcError::internal_error("config overrides lock poisoned"))?;
        if let Some(obj) = base.as_object_mut() {
            for (k, v) in overrides.iter() {
                obj.insert(k.clone(), v.clone());
            }
        }
        Ok(base)
    }

    async fn handle_config_set(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;
        let obj = params
            .as_object()
            .ok_or_else(|| JsonRpcError::invalid_params("Params must be an object"))?;

        // Validate the supported keys before applying anything.
        const VALID_KEYS: [&str; 4] = ["default_model", "max_turns", "max_iterations", "yolo_mode"];
        for (key, value) in obj {
            if !VALID_KEYS.contains(&key.as_str()) {
                return Err(JsonRpcError::invalid_params(format!(
                    "Unknown config key '{}'. Supported keys: {}",
                    key,
                    VALID_KEYS.join(", ")
                )));
            }
            let ok = match key.as_str() {
                "default_model" => value.is_string() && !value.as_str().unwrap_or("").is_empty(),
                "max_turns" | "max_iterations" => value.as_u64().is_some_and(|n| n > 0),
                "yolo_mode" => value.is_boolean(),
                _ => false,
            };
            if !ok {
                return Err(JsonRpcError::invalid_params(format!(
                    "Invalid value for '{}': {}",
                    key, value
                )));
            }
        }

        let mut overrides = self.config_overrides.write().await;
        for (key, value) in obj {
            overrides.insert(key.clone(), value.clone());
        }
        drop(overrides);

        self.config_with_overrides()
    }

    /// Register a client for server->client events on a session.
    /// Returns the session id; the caller (transport loop) subscribes to the
    /// session's broadcast channel.
    pub async fn subscribe_events(&self, params: Option<Value>) -> Result<String, JsonRpcError> {
        let session_id: String = parse_params::<serde_json::Value>(params)?
            .get("session_id")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| JsonRpcError::invalid_params("Missing session_id"))?;
        let _ = self.get_or_load_session(&session_id).await?;
        Ok(session_id)
    }

    async fn handle_event_subscribe(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let session_id = self.subscribe_events(params).await?;
        Ok(serde_json::json!({
            "subscribed": true,
            "session_id": session_id,
        }))
    }

    /// Register a pending user dialog. If `session_id` is present in the
    /// params, an `ask_user` event is broadcast on that session's channel so
    /// subscribed clients can display the dialog.
    async fn handle_dialog_ask_user(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let session_id = params
            .as_ref()
            .and_then(|v| v.get("session_id"))
            .and_then(|v| v.as_str().map(String::from));
        let p: api::AskUserParams = parse_params(params)?;

        let (tx, _rx) = oneshot::channel::<String>();
        self.pending_dialogs
            .lock()
            .await
            .insert(p.request_id.clone(), tx);

        if let Some(sid) = &session_id {
            if let Some(session) = self.get_session(sid).await {
                let _ = session.events.send(ServerEvent::AskUserDialog {
                    request_id: p.request_id.clone(),
                    prompt: p.prompt.clone(),
                    options: p.options.clone(),
                    allow_custom: p.allow_custom,
                });
            }
        }

        Ok(serde_json::json!({
            "request_id": p.request_id,
            "prompt": p.prompt,
            "options": p.options,
            "allow_custom": p.allow_custom,
            "pending": true,
        }))
    }

    async fn handle_dialog_submit_response(
        &self,
        params: Option<Value>,
    ) -> Result<Value, JsonRpcError> {
        let p: api::SubmitResponseParams = parse_params(params)?;
        let mut pending = self.pending_dialogs.lock().await;
        match pending.remove(&p.request_id) {
            Some(tx) => {
                let _ = tx.send(p.response);
                Ok(serde_json::json!({ "resolved": true, "request_id": p.request_id }))
            }
            None => Err(JsonRpcError::invalid_params(format!(
                "No pending dialog with request_id '{}'",
                p.request_id
            ))),
        }
    }

    /// Internal helper: ask the user and wait for the response (used by the
    /// agent approval gate). Returns Err on timeout or missing request id.
    pub async fn ask_user_and_wait(
        &self,
        session_id: Option<&str>,
        prompt: impl Into<String>,
        options: Vec<String>,
        allow_custom: bool,
        timeout: std::time::Duration,
    ) -> Result<String, JsonRpcError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel::<String>();
        self.pending_dialogs
            .lock()
            .await
            .insert(request_id.clone(), tx);

        if let Some(sid) = session_id {
            if let Some(session) = self.get_session(sid).await {
                let _ = session.events.send(ServerEvent::AskUserDialog {
                    request_id: request_id.clone(),
                    prompt: prompt.into(),
                    options: options.clone(),
                    allow_custom,
                });
            }
        }

        tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| JsonRpcError::internal_error("Dialog timed out"))?
            .map_err(|_| JsonRpcError::internal_error("Dialog cancelled"))
    }

    async fn handle_session_browser_list(&self) -> Result<Value, JsonRpcError> {
        let mut summaries: Vec<SessionSummary> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // In-memory sessions first.
        {
            let sessions = self.sessions.lock().await;
            for session in sessions.values() {
                let thread = session.thread.lock().await;
                let title = conversation_title(&thread.conversation);
                summaries.push(SessionSummary {
                    id: session.id.clone(),
                    title: title.unwrap_or_else(|| "(empty session)".to_string()),
                    created_at: 0,
                    last_active_at: 0,
                    total_tokens: thread.context.estimated_tokens() as u64,
                    message_count: thread.conversation.total_items(),
                });
                seen.insert(session.id.clone());
            }
        }

        // Persisted threads not currently loaded.
        if let Some(ref store) = self.thread_store {
            if let Ok(ids) = store.list_threads().await {
                for id in ids {
                    if !seen.contains(&id) {
                        if let Ok(thread) = store.load_thread(&id).await {
                            let title = conversation_title(&thread.conversation);
                            summaries.push(SessionSummary {
                                id: id.clone(),
                                title: title.unwrap_or_else(|| "(restored session)".to_string()),
                                created_at: 0,
                                last_active_at: 0,
                                total_tokens: thread.context.estimated_tokens() as u64,
                                message_count: thread.conversation.total_items(),
                            });
                        }
                    }
                }
            }
        }

        summaries.sort_by_key(|b| std::cmp::Reverse(b.last_active_at));
        Ok(serde_json::json!({ "sessions": summaries }))
    }

    async fn handle_ide_context_sync(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::IdeContextParams = parse_params(params)?;
        let key = p
            .active_file
            .clone()
            .unwrap_or_else(|| "<none>".to_string());
        let value = serde_json::to_value(&p).unwrap_or_default();
        self.ide_context.write().await.insert(key.clone(), value);
        Ok(serde_json::json!({
            "synced": true,
            "active_file": p.active_file,
            "open_tabs": p.open_tabs.len(),
        }))
    }

    async fn handle_ide_diff_preview(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::IdeDiffParams = parse_params(params)?;
        let diff = diff_lines(&p.original_content, &p.modified_content);
        Ok(serde_json::json!({
            "file_path": p.file_path,
            "diff": diff,
            "changes": diff.lines().filter(|l| l.starts_with('+') || l.starts_with('-')).count(),
        }))
    }

    async fn handle_auth_login(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let p: api::AuthLoginParams = parse_params(params)?;
        if p.token.trim().is_empty() {
            return Err(JsonRpcError::invalid_params("token is required"));
        }
        let expected = std::env::var("SENTINEL_SERVER_TOKEN").ok();
        if let Some(expected) = expected {
            if !expected.is_empty() && p.token != expected {
                return Err(JsonRpcError::invalid_request("Invalid token"));
            }
        }
        *self.auth_token.write().await = Some(p.token.clone());
        self.analytics
            .emit(AnalyticsEvent::new(EventKind::SessionCreated, None));
        Ok(serde_json::json!({
            "authenticated": true,
            "agent_id": null,
        }))
    }

    async fn handle_auth_logout(&self) -> Result<Value, JsonRpcError> {
        *self.auth_token.write().await = None;
        Ok(serde_json::json!({ "authenticated": false }))
    }

    async fn handle_auth_status(&self) -> Result<Value, JsonRpcError> {
        let token = self.auth_token.read().await;
        Ok(serde_json::json!({
            "authenticated": token.is_some(),
            "agent_id": null,
        }))
    }

    fn handle_config_get(&self) -> Result<Value, JsonRpcError> {
        Ok(serde_json::json!({
            "default_model": self.config.agent.default_model,
            "max_turns": self.config.agent.max_turns,
            "max_iterations": self.config.agent.max_iterations,
            "yolo_mode": self.config.agent.yolo_mode,
            "providers": self.config.providers().iter().map(|p| serde_json::json!({
                "id": p.id,
                "name": p.name,
                "api_key_set": p.resolve_api_key().is_some(),
                "models": p.models.iter().map(|m| serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }))
    }

    async fn handle_diagnostics(&self) -> Result<Value, JsonRpcError> {
        let sessions = self.sessions.lock().await;
        Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_secs": self.started_at.elapsed().as_secs(),
            "active_sessions": sessions.len(),
            "total_tokens_in": self.tokens_in.load(Ordering::Relaxed),
            "total_tokens_out": self.tokens_out.load(Ordering::Relaxed),
            "available_models": self.config.providers().iter()
                .flat_map(|p| p.models.iter().map(|m| m.id.as_str()))
                .collect::<Vec<_>>(),
        }))
    }
}

/// Approximate token estimate (chars / 4) for diagnostics counters.
fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() / 4).max(1) as u64
}

/// First user message of a conversation, truncated for use as a title.
fn conversation_title(conversation: &sentinel_core::conversation::Conversation) -> Option<String> {
    conversation
        .current_turn()
        .and_then(|t| t.user_input())
        .and_then(|item| match item {
            Item::UserMessage { text, .. } => Some(text.clone()),
            _ => None,
        })
        .or_else(|| {
            // Fall back to scanning earlier turns.
            for turn in &conversation.turns {
                if let Some(Item::UserMessage { text, .. }) = turn.user_input() {
                    return Some(text.clone());
                }
            }
            None
        })
        .map(|t| {
            let t = t.replace('\n', " ").trim().to_string();
            if t.chars().count() > 80 {
                t.chars().take(77).collect::<String>() + "..."
            } else {
                t
            }
        })
}

/// Produce a unified-diff-style preview between two strings (line based,
/// longest-common-subsequence alignment).
fn diff_lines(original: &str, modified: &str) -> String {
    if original == modified {
        return String::new();
    }
    let a: Vec<&str> = original.lines().collect();
    let b: Vec<&str> = modified.lines().collect();
    if a.is_empty() && b.is_empty() {
        return String::new();
    }
    if a.is_empty() {
        let mut out = format!("@@ -0,0 +1,{} @@\n", b.len());
        for line in &b {
            out.push('+');
            out.push_str(line);
            out.push('\n');
        }
        return out;
    }
    if b.is_empty() {
        let mut out = format!("@@ -1,{} +0,0 @@\n", a.len());
        for line in &a {
            out.push('-');
            out.push_str(line);
            out.push('\n');
        }
        return out;
    }

    // LCS dynamic programming table.
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut out = String::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut removed: Vec<&str> = Vec::new();
    let mut added: Vec<&str> = Vec::new();

    let flush = |removed: &mut Vec<&str>, added: &mut Vec<&str>, out: &mut String| {
        if removed.is_empty() && added.is_empty() {
            return;
        }
        out.push_str(&format!("@@ -1,{} +1,{} @@\n", removed.len(), added.len()));
        for l in removed.drain(..) {
            out.push('-');
            out.push_str(l);
            out.push('\n');
        }
        for l in added.drain(..) {
            out.push('+');
            out.push_str(l);
            out.push('\n');
        }
    };

    while i < n && j < m {
        if a[i] == b[j] {
            flush(&mut removed, &mut added, &mut out);
            out.push(' ');
            out.push_str(a[i]);
            out.push('\n');
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            removed.push(a[i]);
            i += 1;
        } else {
            added.push(b[j]);
            j += 1;
        }
    }
    while i < n {
        removed.push(a[i]);
        i += 1;
    }
    while j < m {
        added.push(b[j]);
        j += 1;
    }
    flush(&mut removed, &mut added, &mut out);
    out
}

fn parse_params<T: serde::de::DeserializeOwned>(params: Option<Value>) -> Result<T, JsonRpcError> {
    params
        .ok_or_else(|| JsonRpcError::invalid_params("Missing params"))
        .and_then(|v| {
            serde_json::from_value(v).map_err(|e| JsonRpcError::invalid_params(e.to_string()))
        })
}

/// Parse `ncu --csv` output into a flat list of metric name/value/unit triples.
/// The CSV format is:  "ID","Process","Host","Kernel","Invocation","MetricName","MetricUnit","MetricValue"
#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_provider::ProviderError;

    #[test]
    fn diff_preview_insertion() {
        let diff = diff_lines("one\ntwo\nthree\n", "one\ntwo\nTWO\nthree\n");
        assert!(diff.contains("+TWO"), "diff: {}", diff);
        // Pure insertion: the aligned line "two" is kept, not removed.
        assert!(!diff.contains("-two"), "diff: {}", diff);
    }

    #[test]
    fn diff_preview_replacement() {
        let diff = diff_lines("alpha\nbeta\n", "alpha\nBETA\n");
        assert!(diff.contains("-beta"), "diff: {}", diff);
        assert!(diff.contains("+BETA"), "diff: {}", diff);
    }

    #[test]
    fn diff_preview_new_and_empty() {
        assert!(diff_lines("", "hello\nworld\n").starts_with("@@"));
        assert!(diff_lines("bye\n", "").starts_with("@@"));
        assert!(diff_lines("", "").is_empty());
        assert!(diff_lines("same\n", "same\n").is_empty());
    }

    #[test]
    fn title_extraction() {
        let mut c = sentinel_core::conversation::Conversation::new();
        c.add_user_message("hello there");
        let title = conversation_title(&c);
        assert_eq!(title.as_deref(), Some("hello there"));

        let long = "x".repeat(200);
        c.add_user_message(long.clone());
        // First user message stays the title.
        assert_eq!(conversation_title(&c).unwrap().len(), 80);
    }

    #[test]
    fn token_estimate() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert!(estimate_tokens("hello world this is a test") > 1);
    }

    #[tokio::test]
    async fn config_get_reports_api_key_availability() {
        // Ensure the config/get payload flags whether each provider has a key.
        std::env::set_var("OPENAI_API_KEY", "sk-test-abc");
        std::env::remove_var("ANTHROPIC_API_KEY");

        let cfg = Arc::new(SentinelConfig::default());
        let handler = RequestHandler::new(
            cfg.clone(),
            Arc::new(AnalyticsPipeline::default()),
            Arc::new(ToolRegistry::new()),
        );

        let result = handler.handle_config_get().expect("config get");
        let providers = result["providers"].as_array().expect("providers array");

        let openai = providers
            .iter()
            .find(|p| p["id"] == "openai")
            .expect("openai provider");
        assert_eq!(openai["api_key_set"], serde_json::json!(true));

        let anthropic = providers
            .iter()
            .find(|p| p["id"] == "anthropic")
            .expect("anthropic provider");
        assert_eq!(anthropic["api_key_set"], serde_json::json!(false));

        // Every provider entry exposes the flag.
        for p in providers {
            assert!(
                p.get("api_key_set").is_some(),
                "provider {} missing api_key_set",
                p["id"]
            );
        }
    }

    /// Provider stub used only to construct sessions in tests (never called).
    struct NeverProvider {
        info: ProviderInfo,
    }

    #[async_trait::async_trait]
    impl ModelProvider for NeverProvider {
        fn info(&self) -> &sentinel_provider_info::ProviderInfo {
            &self.info
        }

        async fn complete(
            &self,
            _req: &sentinel_protocol::CompletionRequest,
        ) -> Result<sentinel_protocol::CompletionResponse, ProviderError> {
            panic!("NeverProvider must not be called")
        }

        async fn complete_stream(
            &self,
            _req: &sentinel_protocol::CompletionRequest,
        ) -> Result<
            Box<
                dyn tokio_stream::Stream<
                        Item = Result<sentinel_protocol::StreamChunk, ProviderError>,
                    > + Send
                    + Unpin,
            >,
            ProviderError,
        > {
            panic!("NeverProvider must not be called")
        }
    }

    /// Builds a handler with one registered session and a fresh event rx.
    async fn handler_with_session(
        debug_enabled: bool,
    ) -> (
        Arc<RequestHandler>,
        tokio::sync::broadcast::Receiver<ServerEvent>,
    ) {
        let mut cfg = SentinelConfig::default();
        cfg.debug.enabled = debug_enabled;
        let handler = Arc::new(RequestHandler::new(
            Arc::new(cfg),
            Arc::new(AnalyticsPipeline::new()),
            Arc::new(ToolRegistry::new()),
        ));
        let session = Arc::new(crate::session::AppSession::new(
            None,
            Arc::new(NeverProvider {
                info: ProviderInfo::default(),
            }),
            Arc::new(ToolRegistry::new()),
            Arc::new(SentinelConfig::default()),
            Arc::new(AnalyticsPipeline::new()),
        ));
        let rx = session.events.subscribe();
        handler
            .sessions
            .lock()
            .await
            .insert(session.id.clone(), session);
        (handler, rx)
    }

    fn emit_log(level: tracing::Level, message: &str) {
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(crate::logs::LogLayer::new());
        tracing::subscriber::with_default(subscriber, || match level {
            tracing::Level::ERROR => tracing::error!("{}", message),
            tracing::Level::WARN => tracing::warn!("{}", message),
            tracing::Level::INFO => tracing::info!("{}", message),
            tracing::Level::DEBUG => tracing::debug!("{}", message),
            tracing::Level::TRACE => tracing::trace!("{}", message),
        });
    }

    /// Waits for a Log event whose message matches, ignoring events that
    /// belong to other concurrently-running tests sharing the log channel.
    async fn await_log(
        rx: &mut tokio::sync::broadcast::Receiver<ServerEvent>,
        marker: &str,
    ) -> String {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let evt = tokio::time::timeout(remaining, rx.recv())
                .await
                .expect("log event must arrive");
            match evt {
                Ok(ServerEvent::Log { level, message }) if message == marker => return level,
                Ok(ServerEvent::Log { .. }) => continue,
                other => panic!("expected Log event, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn log_bridge_forwards_warn_to_session_events() {
        let (_handler, mut rx) = handler_with_session(true).await;
        emit_log(tracing::Level::WARN, "bridge test warn");
        assert_eq!(
            await_log(&mut rx, "bridge test warn").await,
            "WARN",
            "debug enabled -> warn visible"
        );
    }

    #[tokio::test]
    async fn log_bridge_filters_quiet_levels_without_debug() {
        let (_handler, mut rx) = handler_with_session(false).await;
        emit_log(tracing::Level::INFO, "quiet info line");

        // INFO must never surface (even within a window busy with other
        // tests' log lines, which are skipped).
        let quiet_window = tokio::time::Instant::now() + std::time::Duration::from_millis(400);
        loop {
            let remaining = quiet_window.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(ServerEvent::Log { message, .. })) if message == "quiet info line" => {
                    panic!("INFO must be filtered when debug is disabled")
                }
                Ok(Ok(_)) => continue,
                Err(_) => break,
                other => panic!("unexpected channel state: {:?}", other),
            }
        }

        emit_log(tracing::Level::WARN, "bridge test warn2");
        assert_eq!(
            await_log(&mut rx, "bridge test warn2").await,
            "WARN",
            "warn visible without debug"
        );
    }
}

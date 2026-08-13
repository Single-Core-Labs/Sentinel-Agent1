//! The ACP agent half: implements `acp::Agent` over the streaming host.
//!
//! [`SentinelAgent`] bridges incoming ACP requests (`initialize`,
//! `new_session`, `prompt`, `cancel`) to [`AiHost::stream_prompt`], mapping
//! `HostPromptEvent`s to ACP `SessionNotification`s and funneling tool-call
//! approvals through ACP `RequestPermission` round trips with the client.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::{
    AuthenticateRequest, AuthenticateResponse, CancelNotification, ContentBlock, ContentChunk,
    InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse,
    PermissionOption, PermissionOptionId, PermissionOptionKind, PromptRequest, PromptResponse,
    RequestPermissionRequest, RequestPermissionOutcome,
    SelectedPermissionOutcome, SessionId, SessionNotification, SessionUpdate, StopReason,
    TextContent, ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use sentinel_acp_lib::AcpAgentGatewaySender;
use sentinel_ai_host::{
    HostPromptEvent, PromptEventSink, PromptOutcome, ToolApproval, ToolApprover, ToolCallInfo,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::warn;

const OPT_ALLOW_ONCE: &str = "allow-once";
const OPT_ALLOW_ALWAYS: &str = "allow-always";
const OPT_REJECT: &str = "reject";

/// In-process ACP agent driving the ai host.
pub struct SentinelAgent {
    host: sentinel_ai_host::AiHost,
    /// Outbound channel to the client half (notifications + permission).
    /// This sender implements `acp::Client` (agent→client traffic).
    client: AcpAgentGatewaySender,
    /// Session ids handed out by `new_session` (only the last is active).
    sessions: Mutex<Vec<SessionId>>,
    /// Working directory from the last `NewSessionRequest`.
    cwd: Mutex<PathBuf>,
    /// Tool names the user approved "always" this session.
    allowlist: Arc<Mutex<HashSet<String>>>,
    /// Token for the currently running prompt turn (fresh per prompt).
    current_cancel: Arc<Mutex<Option<CancellationToken>>>,
    /// Skip the permission dialog entirely.
    yolo: bool,
}

impl SentinelAgent {
    pub fn new(host: sentinel_ai_host::AiHost, client: AcpAgentGatewaySender, yolo: bool) -> Self {
        Self {
            host,
            client,
            sessions: Mutex::new(Vec::new()),
            cwd: Mutex::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            allowlist: Arc::new(Mutex::new(HashSet::new())),
            current_cancel: Arc::new(Mutex::new(None)),
            yolo,
        }
    }

    fn session_id() -> SessionId {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        SessionId::new(format!("sentinel-tui-{nanos:x}"))
    }

    /// The active session id (created by `new_session`); notifications and
    /// permissions are attributed to it.
    async fn active_session_id(&self) -> SessionId {
        self.sessions
            .lock()
            .await
            .last()
            .cloned()
            .unwrap_or_else(|| SessionId::new("sentinel-tui"))
    }

    /// Build the emitter that maps host events to ACP session notifications.
    fn emitter(&self, session_id: SessionId) -> PromptEventSink {
        let client = self.client.clone();
        Arc::new(move |event| {
            let update = match event {
                HostPromptEvent::TurnFinished { .. } => return,
                other => map_event(other),
            };
            let _ = client.forward_fire_and_forget(SessionNotification::new(
                session_id.clone(),
                update,
            ));
        })
    }

    /// Run one ACP prompt turn through the streaming host.
    async fn run_prompt(&self, text: &str) -> (PromptOutcome, Result<(), String>) {
        let session_id = self.active_session_id().await;
        let emit = self.emitter(session_id.clone());

        let client = self.client.clone();
        let allowlist = Arc::clone(&self.allowlist);
        let yolo = self.yolo;
        let approve: ToolApprover = Arc::new(move |info: ToolCallInfo| {
            let client = client.clone();
            let allowlist = Arc::clone(&allowlist);
            let session_id = session_id.clone();
            Box::pin(async move {
                if yolo || allowlist.lock().await.contains(&info.name) {
                    return ToolApproval::Allowed;
                }
                let request = permission_request(session_id, &info);
                // `send` is the inherent Send future of the gateway (the
                // acp::Client trait method's async_trait(?Send) future is not).
                let response = client.send(request).await;
                match response {
                    Ok(resp) => match resp.outcome {
                        RequestPermissionOutcome::Cancelled => ToolApproval::CancelTurn,
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome {
                            option_id,
                            ..
                        }) => match option_id.0.as_ref() {
                            OPT_ALLOW_ONCE => ToolApproval::Allowed,
                            OPT_ALLOW_ALWAYS => {
                                allowlist.lock().await.insert(info.name.clone());
                                ToolApproval::Allowed
                            }
                            #[allow(unreachable_patterns)]
                            _ => ToolApproval::Denied,
                        },
                        #[allow(unreachable_patterns)]
                        _ => ToolApproval::Denied,
                    },
                    Err(err) => {
                        warn!(%err, "permission round trip failed; denying tool call");
                        ToolApproval::Denied
                    }
                }
            })
        });

        let token = CancellationToken::new();
        *self.current_cancel.lock().await = Some(token.clone());

        let result = self
            .host
            .stream_prompt(text, emit, approve, Some(token.clone()))
            .await
            .map_err(|e| e.to_string());

        let outcome = match result {
            Ok(outcome) => (outcome, Ok(())),
            Err(err) => (
                PromptOutcome {
                    text: String::new(),
                    tool_results: Vec::new(),
                    cancelled: token.is_cancelled(),
                },
                Err(err),
            ),
        };
        *self.current_cancel.lock().await = None;
        outcome
    }
}

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Agent for SentinelAgent {
    async fn initialize(&self, args: InitializeRequest) -> Result<InitializeResponse, agent_client_protocol::Error> {
        Ok(InitializeResponse::new(args.protocol_version))
    }

    async fn authenticate(&self, _args: AuthenticateRequest) -> Result<AuthenticateResponse, agent_client_protocol::Error> {
        Ok(AuthenticateResponse::new())
    }

    async fn new_session(&self, args: NewSessionRequest) -> Result<NewSessionResponse, agent_client_protocol::Error> {
        let id = Self::session_id();
        self.sessions.lock().await.push(id.clone());
        *self.cwd.lock().await = args.cwd.clone();
        Ok(NewSessionResponse::new(id))
    }

    async fn prompt(&self, args: PromptRequest) -> Result<PromptResponse, agent_client_protocol::Error> {
        let text: Vec<String> = args
            .prompt
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        let text = text.join("\n");

        if text.trim().is_empty() {
            return Ok(PromptResponse::new(StopReason::EndTurn));
        }

        let (outcome, result) = self.run_prompt(&text).await;

        if let Err(err) = result {
            let session_id = self.active_session_id().await;
            let _ = self.client.forward_fire_and_forget(SessionNotification::new(
                session_id,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new(format!("⚠ {err}")),
                ))),
            ));
            return Ok(PromptResponse::new(StopReason::EndTurn));
        }

        let stop_reason = if outcome.cancelled {
            StopReason::Cancelled
        } else {
            StopReason::EndTurn
        };
        Ok(PromptResponse::new(stop_reason))
    }

    async fn cancel(&self, _args: CancelNotification) -> Result<(), agent_client_protocol::Error> {
        if let Some(token) = self.current_cancel.lock().await.take() {
            token.cancel();
        }
        // Any in-flight permission request resolves with the Cancelled
        // outcome on the client side: the modal closes via the cancel key
        // (Esc), and a dropped request future degrades to Denied fail-closed.
        Ok(())
    }
}

/// Map a host prompt event to the ACP session update the client renders.
fn map_event(event: HostPromptEvent) -> SessionUpdate {
    fn chunk(text: impl Into<String>) -> ContentChunk {
        ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
    }
    match event {
        HostPromptEvent::ReasoningDelta(text) => {
            SessionUpdate::AgentThoughtChunk(chunk(text))
        }
        HostPromptEvent::TextDelta(text) => SessionUpdate::AgentMessageChunk(chunk(text)),
        HostPromptEvent::ToolCallStreaming {
            id,
            name,
            args,
            ..
        } => SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            id.unwrap_or_else(|| "streaming".to_string()),
            ToolCallUpdateFields::new()
                .title(name.unwrap_or_else(|| "tool".to_string()))
                .status(ToolCallStatus::Pending)
                .raw_input(serde_json::Value::String(args)),
        )),
        HostPromptEvent::ToolCallStarted { call_id, name } => {
            SessionUpdate::ToolCall(ToolCall::new(call_id, name.clone()).kind(tool_kind(&name)))
        }
        HostPromptEvent::ToolCallFinished {
            call_id,
            name,
            ok,
            output,
        } => SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            call_id,
            ToolCallUpdateFields::new()
                .title(name)
                .status(if ok {
                    ToolCallStatus::Completed
                } else {
                    ToolCallStatus::Failed
                })
                .raw_output(serde_json::Value::String(output)),
        )),
        HostPromptEvent::TurnFinished { .. } => {
            unreachable!("TurnFinished is filtered out by the emitter before mapping")
        }
    }
}

fn build_permission_options() -> Vec<PermissionOption> {
    vec![
        PermissionOption::new(
            PermissionOptionId::new(OPT_ALLOW_ONCE),
            "Allow once",
            PermissionOptionKind::AllowOnce,
        ),
        PermissionOption::new(
            PermissionOptionId::new(OPT_ALLOW_ALWAYS),
            "Always allow",
            PermissionOptionKind::AllowAlways,
        ),
        PermissionOption::new(
            PermissionOptionId::new(OPT_REJECT),
            "Reject",
            PermissionOptionKind::RejectOnce,
        ),
    ]
}

fn permission_request(
    session_id: SessionId,
    info: &ToolCallInfo,
) -> RequestPermissionRequest {
    let tool_call = ToolCallUpdate::new(
        info.call_id.clone(),
        ToolCallUpdateFields::new()
            .title(info.name.clone())
            .status(ToolCallStatus::Pending)
            .raw_input(info.args.clone()),
    );
    RequestPermissionRequest::new(session_id, tool_call, build_permission_options())
}

/// Map a tool name to the ACP tool kind for icon treatment.
fn tool_kind(name: &str) -> ToolKind {
    if name.contains("write") || name.contains("edit") || name.contains("patch") {
        ToolKind::Edit
    } else if name.contains("read") || name.contains("cat") || name.contains("view") {
        ToolKind::Read
    } else if name.contains("search") || name.contains("grep") || name.contains("find") {
        ToolKind::Search
    } else if name.contains("run") || name.contains("shell") || name.contains("exec") {
        ToolKind::Execute
    } else if name.contains("delete") || name.contains("rm") {
        ToolKind::Delete
    } else if name.contains("move") || name.contains("rename") {
        ToolKind::Move
    } else if name.contains("fetch") || name.contains("web") || name.contains("http") {
        ToolKind::Fetch
    } else {
        ToolKind::Other
    }
}
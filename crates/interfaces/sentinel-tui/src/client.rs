//! The ACP client half: implements `acp::Client` inside the TUI process.
//!
//! Notifications arrive here from the agent half via the gateway channel
//! and are forwarded to the UI event loop; permission requests park a
//! oneshot the UI resolves with the user's verdict.

use agent_client_protocol::{
    RequestPermissionRequest, RequestPermissionResponse, RequestPermissionOutcome,
    SessionNotification,
};
use tokio::sync::{mpsc, oneshot};

/// Events the TUI event loop consumes.
#[derive(Debug)]
pub enum UiEvent {
    /// A session notification from the agent (text deltas, tool calls, …).
    Session(SessionNotification),
    /// The agent is waiting on the user's verdict for a tool call.
    PermissionRequested {
        request: RequestPermissionRequest,
        resolve: oneshot::Sender<RequestPermissionResponse>,
    },
    /// The `prompt` call came back (streamed content already delivered).
    PromptCompleted(Result<(), String>),
}

/// The client half: forwards ACP traffic into [`UiEvent`]s.
#[derive(Clone)]
pub struct TuiClient {
    pub(crate) ui_tx: mpsc::UnboundedSender<UiEvent>,
}

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Client for TuiClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> Result<RequestPermissionResponse, agent_client_protocol::Error> {
        let (tx, rx) = oneshot::channel();
        if self
            .ui_tx
            .send(UiEvent::PermissionRequested {
                request: args,
                resolve: tx,
            })
            .is_err()
        {
            // Event loop is gone: fail the request as cancelled rather than
            // hanging the agent.
            return Ok(RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled));
        }
        match rx.await {
            Ok(response) => Ok(response),
            Err(_) => Ok(RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)),
        }
    }

    async fn session_notification(
        &self,
        notification: SessionNotification,
    ) -> Result<(), agent_client_protocol::Error> {
        let _ = self.ui_tx.send(UiEvent::Session(notification));
        Ok(())
    }
}
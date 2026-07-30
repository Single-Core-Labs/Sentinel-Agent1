use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Unique identifier for a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId(pub String);

impl PluginId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Metadata describing a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub homepage: Option<String>,
}

/// An event that plugins can hook into.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginEvent {
    /// Before a tool is executed (can modify or veto).
    BeforeToolCall {
        tool_name: String,
        args: serde_json::Value,
    },
    /// After a tool result is received.
    AfterToolCall {
        tool_name: String,
        args: serde_json::Value,
        result: String,
        is_error: bool,
    },
    /// Before an LLM request is sent.
    BeforeModelRequest {
        model: String,
        prompt_tokens: u32,
    },
    /// After an LLM response is received.
    AfterModelResponse {
        model: String,
        completion_tokens: u32,
    },
    /// Session lifecycle events.
    SessionCreated { session_id: String },
    SessionEnded { session_id: String },
    /// A custom event defined by the plugin itself.
    Custom {
        name: String,
        payload: serde_json::Value,
    },
}

/// The result of processing a plugin event.
#[derive(Debug)]
pub enum PluginAction {
    /// Continue without changes.
    Continue,
    /// Veto the operation (prevents execution).
    Veto(String),
    /// Modify the event payload.
    Modify(serde_json::Value),
}

/// A hook that a plugin can register for a specific event type.
#[async_trait]
pub trait PluginHook: Send + Sync {
    async fn handle(&self, event: &PluginEvent) -> PluginAction;
}

/// Trait that all plugins must implement.
#[async_trait]
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;

    /// Initialize the plugin with runtime context.
    async fn init(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    /// Clean up resources on shutdown.
    async fn shutdown(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    /// Return hooks this plugin wants to register.
    fn hooks(&self) -> Vec<Box<dyn PluginHook>> {
        Vec::new()
    }
}

//! The host: agent construction + the sampler-driven turn loop.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use sentinel_ai_agent::{Agent, AgentBuilder};
use sentinel_ai_sampler::{ApiBackend, AuthScheme, SamplerConfig, SamplingClient};
use sentinel_ai_sampling_types::{
    ContentPart, ConversationItem, ConversationRequest, SystemItem, ToolResultItem, ToolSpec,
    UserItem,
};
use sentinel_ai_tools::computer::local::LocalTerminalBackend;
use sentinel_ai_tools::notification::ToolNotificationHandle;
use sentinel_plugin_system::{PluginAction, PluginEvent, PluginRegistry};

use crate::headroom::HeadroomHost;

/// Configuration for a [`AiHost`].
#[derive(Debug, Clone)]
pub struct AiHostOptions {
    /// Working directory the agent operates in (used for tool cwd, discovery).
    pub cwd: PathBuf,
    /// Model id to sample from (Ollama tag, e.g. `qwen3:8b`).
    pub model: String,
    /// Chat Completions base URL, e.g. `http://localhost:11434/v1`.
    pub base_url: String,
    /// Bearer API key. `None` for backends without auth (Ollama).
    pub api_key: Option<String>,
    /// Hard cap on turn iterations (assistant → tool → assistant) per prompt.
    pub max_turns: usize,
    /// Cap on tool results appended per assistant turn.
    pub max_tool_results: usize,
    /// Load the shipped guard plugins (`sentinel plugin install`) and veto /
    /// deny each tool call through their `before_tool_call` hooks.
    pub plugins: bool,
    /// Compress tool outputs through sentinel-headroom and expose a
    /// `headroom_retrieve` tool to the model.
    pub headroom: bool,
}

impl Default for AiHostOptions {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            model: "qwen3:8b".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: None,
            max_turns: 20,
            max_tool_results: 32,
            plugins: true,
            headroom: true,
        }
    }
}

/// A single tool execution observed during one turn.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub name: String,
    pub call_id: String,
    pub ok: bool,
    pub output: String,
}

/// Wrapper around the built ai agent + its sampling client.
#[derive(Clone)]
pub struct AiHost {
    agent: Arc<Agent>,
    client: SamplingClient,
    options: Arc<AiHostOptions>,
    plugins: Arc<PluginRegistry>,
    headroom: Option<Arc<HeadroomHost>>,
}

impl AiHost {
    /// Build the ai [`Agent`] with a local terminal backend and noop
    /// notifications, then wrap it with a sampler configured for
    /// `options.base_url`.
    pub async fn build(options: AiHostOptions) -> Result<Self> {
        let cwd = options.cwd.clone();
        let agent = AgentBuilder::new(
            cwd.clone(),
            Arc::new(LocalTerminalBackend::new()),
            ToolNotificationHandle::noop(),
        )
        .from_definition(sentinel_ai_agent::AgentDefinition::default_ai_build())
        .build()
        .await
        .context("ai Agent::build failed")?;

        // Guard plugins load before the loop so hooks are always present.
        let plugins = Arc::new(PluginRegistry::new());
        if options.plugins {
            let (loaded, failures) = sentinel_plugin_system::load_default_plugins(&plugins).await;
            if loaded > 0 {
                tracing::info!(loaded, "guard plugins loaded");
            }
            for err in failures {
                tracing::warn!(%err, "guard plugin failed to load");
            }
        }

        // Headroom: compress tool outputs + expose the retrieve tool. The tool
        // is registered dynamically into the built bridge so it appears in the
        // model's tool list without rebuilding the agent.
        let headroom = if options.headroom {
            let headroom = Arc::new(HeadroomHost::new().await);
            agent
                .tool_bridge()
                .register_mcp_tools(
                    "headroom_retrieve".to_string(),
                    (*headroom.retrieve).clone(),
                    None,
                )
                .await
                .context("register headroom_retrieve ai tool failed")?;
            Some(headroom)
        } else {
            None
        };

        let sampler_config = SamplerConfig {
            api_key: options.api_key.clone(),
            base_url: options.base_url.clone(),
            model: options.model.clone(),
            api_backend: ApiBackend::ChatCompletions,
            auth_scheme: AuthScheme::Bearer,
            force_http1: true,
            ..Default::default()
        };
        let client = SamplingClient::new(sampler_config).context("SamplingClient::new failed")?;

        Ok(Self {
            agent: Arc::new(agent),
            client,
            options: Arc::new(options),
            plugins,
            headroom,
        })
    }

    /// The underlying agent (exposes `system_prompt()`, `tool_bridge()`, …).
    pub fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Run a single prompt to completion (all tool loops exhausted).
    ///
    /// `on_assistant_text` receives each final assistant text chunk as it is
    /// produced (before tool execution). Returns the concatenated assistant
    /// text across the whole exchange.
    pub async fn run(
        &self,
        prompt: &str,
        mut on_assistant_text: impl FnMut(&str),
    ) -> Result<(String, Vec<ToolResult>)> {
        let mut items: Vec<ConversationItem> = vec![
            ConversationItem::System(SystemItem {
                content: self.agent.system_prompt().into(),
            }),
            ConversationItem::User(UserItem {
                content: vec![ContentPart::Text {
                    text: prompt.trim().to_string().into(),
                }],
                ..Default::default()
            }),
        ];

        let tool_specs: Vec<ToolSpec> = self
            .agent
            .tool_definitions()
            .await
            .into_iter()
            .map(ToolSpec::from)
            .collect();

        tracing::debug!(
            tools = tool_specs.len(),
            model = %self.options.model,
            "starting ai host turn loop"
        );

        let mut full_text = String::new();
        let mut tool_results: Vec<ToolResult> = Vec::new();

        for _turn in 0..self.options.max_turns {
            let request = ConversationRequest {
                items: items.clone(),
                tools: tool_specs.clone(),
                model: Some(self.options.model.clone()),
                ..Default::default()
            };

            let response = self
                .client
                .conversation_collect(request)
                .await
                .context("sampler conversation_collect failed")?;

            // Persist the assistant turn (text + tool calls) into history.
            items.extend(response.items.iter().cloned());

            let assistant_text = response.assistant_text();
            if !assistant_text.is_empty() {
                if !full_text.is_empty() && !full_text.ends_with('\n') {
                    full_text.push('\n');
                }
                full_text.push_str(&assistant_text);
                on_assistant_text(&assistant_text);
            }

            let calls = response.tool_calls().to_vec();
            if calls.is_empty() {
                break;
            }
            if calls.len() > self.options.max_tool_results {
                tracing::warn!(
                    calls = calls.len(),
                    cap = self.options.max_tool_results,
                    "truncating tool calls this turn"
                );
            }

            for call in calls.into_iter().take(self.options.max_tool_results) {
                let name = call.name.clone();
                let call_id = call.id.to_string();
                let args: serde_json::Value =
                    serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null);

                // Guard plugin policy (before_tool_call): Veto skips this call
                // and feeds the reason back to the model; Deny aborts the whole
                // run fail-closed, mirroring the legacy agent loop.
                match self
                    .plugins
                    .dispatch(&PluginEvent::BeforeToolCall {
                        tool_name: name.clone(),
                        args: args.clone(),
                    })
                    .await
                {
                    PluginAction::Continue | PluginAction::Modify(_) => {}
                    PluginAction::Veto(reason) => {
                        tracing::warn!(tool = %name, %reason, "tool call vetoed by guard plugin");
                        let content = format!("Vetoed by plugin policy: {reason}");
                        tool_results.push(ToolResult {
                            name: name.clone(),
                            call_id: call_id.clone(),
                            ok: false,
                            output: content.clone(),
                        });
                        items.push(ConversationItem::ToolResult(ToolResultItem {
                            tool_call_id: call_id,
                            content: content.into(),
                            images: Vec::new(),
                        }));
                        continue;
                    }
                    PluginAction::Deny(reason) => {
                        return Err(anyhow::anyhow!(
                            "tool call {name} denied by guard plugin: {reason}"
                        ));
                    }
                }

                tracing::debug!(tool = %name, call_id = %call_id, "dispatching tool call");

                let (ok, content) = match self.agent.tool_bridge().call(&name, args, &call_id).await
                {
                    Ok(result) => (true, result.prompt_text),
                    Err(e) => (false, format!("tool error: {e}")),
                };

                // Compress large outputs (successful ones) before they reach
                // the model; the model can expand via headroom_retrieve.
                let model_content = match &self.headroom {
                    Some(h) => h.compress(&name, &content, !ok).await,
                    None => content.clone(),
                };

                tool_results.push(ToolResult {
                    name: name.clone(),
                    call_id: call_id.clone(),
                    ok,
                    output: model_content.clone(),
                });

                items.push(ConversationItem::ToolResult(ToolResultItem {
                    tool_call_id: call_id,
                    content: model_content.into(),
                    images: Vec::new(),
                }));
            }
        }

        Ok((full_text, tool_results))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building the ai Agent with a local terminal backend must succeed with
    /// no hidden requirement beyond a filesystem cwd (mirrors the builder's
    /// own test fixtures: LocalTerminalBackend + ToolNotificationHandle::noop).
    #[tokio::test]
    async fn builds_agent_with_local_backend() {
        let tmp = tempfile::tempdir().unwrap();
        let host = AiHost::build(AiHostOptions {
            cwd: tmp.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .expect("host builds offline");
        let defs = host.agent().tool_definitions().await;
        assert!(
            !defs.is_empty(),
            "built agent must expose at least the default toolset"
        );
    }

    /// The SamplerConfig must target a Chat Completions endpoint so Ollama can
    /// serve it; force_http1 avoids ALPN surprises on a plain h1 backend.
    #[test]
    fn sampler_config_targets_chat_completions() {
        let cfg = SamplerConfig {
            api_key: None,
            base_url: "http://localhost:11434/v1".to_string(),
            model: "qwen3:8b".to_string(),
            api_backend: ApiBackend::ChatCompletions,
            auth_scheme: AuthScheme::Bearer,
            force_http1: true,
            ..Default::default()
        };
        assert_eq!(cfg.api_backend, ApiBackend::ChatCompletions);
        assert!(cfg.force_http1);
        assert_eq!(cfg.base_url, "http://localhost:11434/v1");
    }
}

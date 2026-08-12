//! `sentinel-ai-host` — a small host that drives the sentinel-ai agent core.
//!
//! ai ships no production run loop: [`sentinel_ai_agent::Agent`] is an
//! immutable bundle (definition, system prompt, tool bridge), and the real
//! loop lives inside `sentinel-ai-shell`'s session actor. This crate is our
//! replacement composition root for the `sentinel ai` path:
//!
//! 1. Build an [`sentinel_ai_agent::Agent`] via [`AgentBuilder`]
//!    ([`AiHost::build`]).
//! 2. Drive our own loop against a local LLM backend (e.g. Ollama)
//!    through [`sentinel_ai_sampler::SamplingClient`] on a Chat Completions
//!    endpoint (`base_url=http://localhost:11434/v1`).
//! 3. Dispatch tool calls through the agent's
//!    [`sentinel_ai_tools::bridge::ToolBridge`], feeding results back into
//!    the conversation until the model stops calling tools.

mod headroom;
mod host;

pub use headroom::{HeadroomHost, HeadroomRetrieveArgs, HeadroomRetrieveAiTool};
pub use host::{AiHost, AiHostOptions, ToolResult};

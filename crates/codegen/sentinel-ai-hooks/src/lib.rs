//! # sentinel-ai-hooks
//!
//! Runtime hook system for Ai — file-based discovery, command execution,
//! and policy enforcement.
//!
//! ## Overview
//!
//! This crate provides a minimal hooks system for Ai. Hooks are discovered
//! from dedicated directories (`~/.ai/hooks/` and `<git-worktree-root>/.ai/hooks/`),
//! defined in JSON files (compatible settings format), and executed as child processes.
//!
//! ## v0 scope
//!
//! - Four event types: `session_start`, `pre_tool_use`, `post_tool_use`, `session_end`
//! - Command-backed hooks only
//! - `pre_tool_use` hooks can deny/allow (blocking); all others are non-blocking
//! - Fail-open by default: hook failures do not block normal operation
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use std::path::Path;
//! use sentinel_ai_hooks::discovery::load_hooks;
//! use sentinel_ai_hooks::event::HookEventName;
//!
//! let (registry, errors) = load_hooks(
//!     Some(Path::new("/home/user/.ai/hooks")),
//!     Some(Path::new("/project/.ai/hooks")),
//! );
//!
//! for err in &errors {
//!     eprintln!("hook load warning: {err}");
//! }
//!
//! let pre_hooks = registry.hooks_for(HookEventName::PreToolUse);
//! println!("loaded {} pre_tool_use hooks", pre_hooks.len());
//! ```

pub mod config;
pub mod discovery;
pub mod dispatcher;
mod env_expand;
pub mod error;
pub mod event;
pub mod matcher;
pub mod result;
pub mod runner;
#[cfg(test)]
mod test_support;
pub mod trust;

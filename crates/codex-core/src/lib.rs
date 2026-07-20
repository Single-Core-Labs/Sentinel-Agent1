//! Core logic for a Codex‑style AI agent.
//!
//! This crate provides the foundational pieces needed to run an autonomous
//! agent that can manage conversations, perform compaction of context windows,
//! and expose a JSON‑RPC‑compatible API.  The implementation follows the
//! architectural ideas described in the Codex‑rs documentation (agents, tools,
//! compaction, operational guidelines) while being deliberately lightweight
//! for the purpose of this repository.

pub mod agent;
pub mod compact;
pub mod apply_patch;
pub mod agents_md;

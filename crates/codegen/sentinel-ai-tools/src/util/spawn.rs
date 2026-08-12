//! Cross-platform child-process lifecycle helpers for `tokio::process::Command`.
//!
//! All implementations now live in the lightweight [`sentinel_tty_utils`] crate
//! so that every crate in the workspace can use them without pulling in the
//! heavyweight `sentinel-ai-tools` dependency. This module re-exports the public
//! API for backward compatibility.

pub use sentinel_tty_utils::{
    ProcessGroup, ProcessScope, detach_command, global_process_scope, new_process_group,
};

//! Shared utilities used by both `sentinel-ai-shell` and its downstream clients
//! (e.g. `sentinel-ai-pager-render`). This crate sits upstream of `sentinel-ai-shell`
//! so it must never depend on it.

pub mod clipboard;
pub mod placeholder_images;
pub mod session;
pub mod stderr;
pub mod ui_config;

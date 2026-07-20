//! Utilities for applying a single‑file patch.
//!
//! The real Codex implementation prefers an `apply_patch` helper that builds a
//! minimal diff, respects ASCII‑only files, and avoids destructive Git commands.
//! Here we expose a tiny API that simply overwrites the target file with the
//! supplied content after performing a few safety checks.

use std::{fs, io::Write, path::Path};
use thiserror::Error;

/// Errors that can arise during a patch operation.
#[derive(Debug, Error)]
pub enum PatchError {
    #[error("attempted to modify a file outside the workspace: {0}")]
    PathEscape(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("non‑ASCII content detected")]
    NonAscii,
}

/// Apply `content` to the file at `path`.
///
/// * The file must reside within `workspace_root` – the function ensures the
///   canonicalised path starts with the workspace root to prevent directory‑
///   traversal attacks.
/// * The content must be pure ASCII; any non‑ASCII byte causes `PatchError::NonAscii`.
pub fn apply_patch(workspace_root: &Path, path: &Path, content: &str) -> Result<(), PatchError> {
    let abs_root = workspace_root.canonicalize()?;
    let abs_target = workspace_root.join(path).canonicalize()?;
    if !abs_target.starts_with(&abs_root) {
        return Err(PatchError::PathEscape(abs_target.display().to_string()));
    }
    if !content.is_ascii() {
        return Err(PatchError::NonAscii);
    }
    // Ensure parent directory exists.
    if let Some(parent) = abs_target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&abs_target)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

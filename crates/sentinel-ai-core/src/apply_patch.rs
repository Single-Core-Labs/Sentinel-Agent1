//! Unified-diff patch applier for `sentinel-ai-core`.
//!
//! Parses the standard **unified diff** format (as produced by `git diff`,
//! `diff -u`, or the LLM when asked to emit a patch) and applies each hunk
//! against the real file content.
//!
//! # Format accepted
//! ```text
//! --- a/path/to/file
//! +++ b/path/to/file
//! @@ -L,S +L,S @@  optional heading
//! -removed line
//!  context line
//! +added line
//! ```
//!
//! Multiple hunks per file are supported.  Multi-file diffs are **not**
//! supported by this function; pass each file's diff separately.
//!
//! # Atomicity
//! The patch is written to a sibling temp file first.  Only on complete
//! success is the temp file renamed over the original.  Any failure leaves
//! the original untouched.
//!
//! # Security
//! The workspace-boundary check from the previous implementation is preserved.
//! The ASCII-only restriction has been removed; files are treated as UTF-8.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;

// ─── Public error type ───────────────────────────────────────────────────────

/// All errors that can arise during patch parsing or application.
#[derive(Debug, Error)]
pub enum PatchError {
    /// The target path escapes the workspace root (directory traversal).
    #[error("path '{0}' escapes the workspace root")]
    PathEscape(String),

    /// The diff text could not be parsed as a valid unified diff.
    #[error("malformed diff: {0}")]
    MalformedDiff(String),

    /// A hunk's context lines do not match the actual file content.
    /// This means the patch is stale or was generated against a different
    /// version of the file.
    #[error("stale patch: hunk starting at line {hunk_start} of the diff does not match file content at line {file_line}; expected {expected:?}, found {found:?}")]
    StaleContext {
        hunk_start: usize,
        file_line: usize,
        expected: String,
        found: String,
    },

    /// An I/O error occurred while reading or writing the file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ─── Internal types ──────────────────────────────────────────────────────────

/// A single line in a hunk, tagged with its kind.
#[derive(Debug, Clone, PartialEq)]
enum HunkLine {
    Context(String),
    Added(String),
    Removed(String),
}

/// A parsed hunk from the diff header `@@ -L,S +L,S @@`.
#[derive(Debug)]
struct Hunk {
    /// 1-based line number in the **original** file where this hunk starts.
    orig_start: usize,
    /// Lines (context, added, removed) in order.
    lines: Vec<HunkLine>,
    /// Position in the diff text (line index) for error messages.
    diff_line: usize,
}

// ─── Parser ──────────────────────────────────────────────────────────────────

/// Parse a unified diff string into a list of hunks.
///
/// We skip `---`/`+++` header lines and focus on `@@` headers + body lines.
fn parse_hunks(diff: &str) -> Result<Vec<Hunk>, PatchError> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current: Option<Hunk> = None;

    for (idx, raw_line) in diff.lines().enumerate() {
        if raw_line.starts_with("@@") {
            // Flush previous hunk.
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            // Parse `@@ -ORIG_START[,ORIG_COUNT] +NEW_START[,NEW_COUNT] @@`
            let orig_start = parse_hunk_header(raw_line, idx)?;
            current = Some(Hunk {
                orig_start,
                lines: Vec::new(),
                diff_line: idx + 1,
            });
        } else if raw_line.starts_with("---") || raw_line.starts_with("+++") {
            // File header lines — skip.
            continue;
        } else if let Some(ref mut hunk) = current {
            // Hunk body.
            if let Some(rest) = raw_line.strip_prefix('-') {
                hunk.lines.push(HunkLine::Removed(rest.to_string()));
            } else if let Some(rest) = raw_line.strip_prefix('+') {
                hunk.lines.push(HunkLine::Added(rest.to_string()));
            } else if let Some(rest) = raw_line.strip_prefix(' ') {
                // Context line with a leading space.
                hunk.lines.push(HunkLine::Context(rest.to_string()));
            } else if raw_line.is_empty() {
                // Some tools emit context lines without any prefix when the
                // line itself is blank.
                hunk.lines.push(HunkLine::Context(String::new()));
            }
            // Anything else (e.g. "\ No newline at end of file") is ignored.
        }
    }

    // Flush last hunk.
    if let Some(h) = current {
        hunks.push(h);
    }

    if hunks.is_empty() {
        return Err(PatchError::MalformedDiff(
            "no hunks found in diff".to_string(),
        ));
    }

    Ok(hunks)
}

/// Extract the original-file start line from a `@@ -L[,S] +L[,S] @@` header.
fn parse_hunk_header(header: &str, diff_line_idx: usize) -> Result<usize, PatchError> {
    // The format is: @@ -ORIG[,COUNT] +NEW[,COUNT] @@ [optional text]
    // We need the ORIG number after the '-'.
    let after_at = header
        .strip_prefix("@@")
        .and_then(|s| s.trim_start().strip_prefix('-'))
        .ok_or_else(|| {
            PatchError::MalformedDiff(format!(
                "line {}: cannot parse hunk header: {:?}",
                diff_line_idx + 1,
                header
            ))
        })?;

    // `after_at` is now like "10,5 +12,6 @@ ..."
    let num_str = after_at.split([',', ' ']).next().unwrap_or("");
    let start: usize = num_str.parse().map_err(|_| {
        PatchError::MalformedDiff(format!(
            "line {}: cannot parse orig line number from {:?}",
            diff_line_idx + 1,
            header
        ))
    })?;

    Ok(start)
}

// ─── Applier ─────────────────────────────────────────────────────────────────

/// Apply the hunks to the original file lines, returning the new content.
///
/// `file_lines` is split from the file **without** the final newline stripped,
/// i.e. each entry may or may not contain `\n`.  We work with trimmed content
/// for comparison but preserve original endings in the output where unchanged.
fn apply_hunks(
    file_lines: &[&str],
    hunks: &[Hunk],
) -> Result<String, PatchError> {
    let mut output: Vec<String> = Vec::with_capacity(file_lines.len() + 64);
    // Tracks our position in the original file (0-based index into `file_lines`).
    let mut file_pos: usize = 0;

    for hunk in hunks {
        // `orig_start` is 1-based; convert to 0-based.
        let hunk_orig_start = hunk.orig_start.saturating_sub(1);

        // Emit any unchanged file lines before this hunk.
        while file_pos < hunk_orig_start {
            if file_pos >= file_lines.len() {
                break;
            }
            output.push(file_lines[file_pos].to_string());
            file_pos += 1;
        }

        // Apply hunk lines.
        for hunk_line in &hunk.lines {
            match hunk_line {
                HunkLine::Context(expected) => {
                    // The next original line must match (trimming trailing CR/LF
                    // for comparison so Windows line endings don't cause failures).
                    let actual = file_lines.get(file_pos).copied().unwrap_or("");
                    let actual_trimmed = actual.trim_end_matches(['\r', '\n']);
                    if actual_trimmed != expected.as_str() {
                        return Err(PatchError::StaleContext {
                            hunk_start: hunk.diff_line,
                            file_line: file_pos + 1,
                            expected: expected.clone(),
                            found: actual_trimmed.to_string(),
                        });
                    }
                    output.push(actual.to_string());
                    file_pos += 1;
                }
                HunkLine::Removed(expected) => {
                    // This line should exist in the original and be consumed.
                    let actual = file_lines.get(file_pos).copied().unwrap_or("");
                    let actual_trimmed = actual.trim_end_matches(['\r', '\n']);
                    if actual_trimmed != expected.as_str() {
                        return Err(PatchError::StaleContext {
                            hunk_start: hunk.diff_line,
                            file_line: file_pos + 1,
                            expected: expected.clone(),
                            found: actual_trimmed.to_string(),
                        });
                    }
                    // Do NOT push to output — this line is deleted.
                    file_pos += 1;
                }
                HunkLine::Added(new_content) => {
                    // New line — always push, do NOT advance file_pos.
                    output.push(format!("{}\n", new_content));
                }
            }
        }
    }

    // Emit any remaining file lines after the last hunk.
    while file_pos < file_lines.len() {
        output.push(file_lines[file_pos].to_string());
        file_pos += 1;
    }

    Ok(output.join(""))
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Resolve a path lexically (without touching the filesystem) by processing
/// each component: `..` pops the last component, `.` is skipped, everything
/// else is appended.  This lets us detect traversal attempts before the target
/// file exists.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Apply a unified diff `patch` to the file at `path`.
///
/// # Arguments
/// * `workspace_root` – All target paths must resolve within this directory.
/// * `path`           – Path of the file to patch (relative or absolute).
/// * `patch`          – Unified diff text (UTF-8).
///
/// # Atomicity
/// The function writes output to a temp file adjacent to the target, then
/// renames it over the target.  If anything fails, the original is unchanged.
///
/// # Errors
/// Returns [`PatchError`] for path traversal, malformed diffs, stale context,
/// and I/O errors.
pub fn apply_patch(
    workspace_root: &Path,
    path: &Path,
    patch: &str,
) -> Result<(), PatchError> {
    // ── 1. Workspace boundary check ──────────────────────────────────────────
    // Stage 1: lexical rejection — reject any path that contains `..`
    // components relative to the workspace root.  This catches traversal
    // attempts even when the target file does not exist yet.
    let normalized_root = normalize_path(workspace_root);
    let joined = workspace_root.join(path);
    let normalized = normalize_path(&joined);
    if !normalized.starts_with(&normalized_root) {
        return Err(PatchError::PathEscape(normalized.display().to_string()));
    }

    let target = normalized;
    let abs_root = workspace_root
        .canonicalize()
        .map_err(PatchError::Io)?;

    // Stage 2 (belt-and-suspenders): canonicalize if the file already exists
    // to catch symlink escapes.
    let abs_target: PathBuf = if target.exists() {
        match target.canonicalize() {
            Ok(c) => {
                if !c.starts_with(&abs_root) {
                    return Err(PatchError::PathEscape(c.display().to_string()));
                }
                c
            }
            Err(e) => return Err(PatchError::Io(e)),
        }
    } else {
        target
    };

    // ── 2. Parse the diff ────────────────────────────────────────────────────
    let hunks = parse_hunks(patch)?;

    // ── 3. Read original file (empty string if creating a new file) ──────────
    let original = if abs_target.exists() {
        fs::read_to_string(&abs_target).map_err(PatchError::Io)?
    } else {
        String::new()
    };

    // Split preserving line endings so we can faithfully reconstruct the file.
    // We use a manual split that keeps the `\n` attached to its line.
    let file_lines: Vec<&str> = split_lines_keep_endings(&original);

    // ── 4. Apply hunks ───────────────────────────────────────────────────────
    let new_content = apply_hunks(&file_lines, &hunks)?;

    // ── 5. Atomic write: temp file → rename ─────────────────────────────────
    let parent = abs_target
        .parent()
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(PatchError::Io)?;

    // Use a sibling temp file so the rename is guaranteed to be on the same
    // filesystem (otherwise it would be a copy+delete, not atomic).
    let tmp_path = abs_target.with_extension(format!(
        "patch_{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));

    {
        let mut tmp = fs::File::create(&tmp_path).map_err(PatchError::Io)?;
        tmp.write_all(new_content.as_bytes())
            .map_err(PatchError::Io)?;
        tmp.flush().map_err(PatchError::Io)?;
        // `tmp` is dropped (and thus flushed to OS) here.
    }

    fs::rename(&tmp_path, &abs_target).map_err(|e| {
        // Best-effort cleanup if rename fails.
        let _ = fs::remove_file(&tmp_path);
        PatchError::Io(e)
    })?;

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Split `s` into lines, keeping the `\n` (and `\r\n`) terminator attached
/// to each line, except the last segment which may have no terminator.
fn split_lines_keep_endings(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '\n' {
            result.push(&s[start..=i]);
            start = i + 1;
        }
    }
    if start < s.len() {
        result.push(&s[start..]);
    }
    result
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    // ── Fixture helpers ──────────────────────────────────────────────────────

    /// Create a temporary directory scoped to the test.
    fn tmp_dir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "sentinel_patch_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    // ── Test 1: Clean apply ──────────────────────────────────────────────────

    #[test]
    fn test_clean_apply() {
        let root = tmp_dir();
        write_file(&root, "hello.txt", "line one\nline two\nline three\n");

        let patch = "\
--- a/hello.txt
+++ b/hello.txt
@@ -1,3 +1,3 @@
 line one
-line two
+line TWO
 line three
";
        apply_patch(&root, Path::new("hello.txt"), patch).expect("clean apply should succeed");

        let result = fs::read_to_string(root.join("hello.txt")).unwrap();
        assert_eq!(result, "line one\nline TWO\nline three\n");
    }

    // ── Test 2: Multiple hunks ───────────────────────────────────────────────

    #[test]
    fn test_multiple_hunks() {
        let root = tmp_dir();
        write_file(
            &root,
            "multi.txt",
            "alpha\nbeta\ngamma\ndelta\nepsilon\n",
        );

        let patch = "\
--- a/multi.txt
+++ b/multi.txt
@@ -1,2 +1,2 @@
-alpha
+ALPHA
 beta
@@ -4,2 +4,2 @@
-delta
+DELTA
 epsilon
";
        apply_patch(&root, Path::new("multi.txt"), patch).unwrap();
        let result = fs::read_to_string(root.join("multi.txt")).unwrap();
        assert_eq!(result, "ALPHA\nbeta\ngamma\nDELTA\nepsilon\n");
    }

    // ── Test 3: Stale context ────────────────────────────────────────────────

    #[test]
    fn test_stale_context_rejected() {
        let root = tmp_dir();
        // File has been modified since the patch was generated.
        write_file(&root, "stale.txt", "line one\nline MODIFIED\nline three\n");

        let patch = "\
--- a/stale.txt
+++ b/stale.txt
@@ -1,3 +1,3 @@
 line one
-line two
+line TWO
 line three
";
        let err = apply_patch(&root, Path::new("stale.txt"), patch)
            .expect_err("stale context should be rejected");

        match err {
            PatchError::StaleContext { file_line, .. } => {
                // Line 2 should be where the mismatch is.
                assert_eq!(file_line, 2);
            }
            other => panic!("unexpected error: {:?}", other),
        }

        // Original file must be untouched.
        let result = fs::read_to_string(root.join("stale.txt")).unwrap();
        assert_eq!(result, "line one\nline MODIFIED\nline three\n");
    }

    // ── Test 4: Path traversal attempt ──────────────────────────────────────

    #[test]
    fn test_path_traversal_rejected() {
        let root = tmp_dir();
        // Create target so canonicalize doesn't fail for a different reason.
        let patch = "\
--- a/../../etc/passwd
+++ b/../../etc/passwd
@@ -1,1 +1,1 @@
-root
+pwned
";
        let err = apply_patch(&root, Path::new("../../etc/passwd"), patch)
            .expect_err("path traversal should be rejected");

        assert!(
            matches!(err, PatchError::PathEscape(_)),
            "expected PathEscape, got {:?}",
            err
        );
    }

    // ── Test 5: Non-ASCII / UTF-8 content ───────────────────────────────────

    #[test]
    fn test_non_ascii_utf8() {
        let root = tmp_dir();
        // Japanese, emoji, Arabic — all valid UTF-8.
        write_file(
            &root,
            "unicode.txt",
            "こんにちは\n🎉 party time\nمرحبا\n",
        );

        let patch = "\
--- a/unicode.txt
+++ b/unicode.txt
@@ -1,3 +1,3 @@
 こんにちは
-🎉 party time
+🎊 confetti
 مرحبا
";
        apply_patch(&root, Path::new("unicode.txt"), patch).expect("UTF-8 patch should succeed");

        let result = fs::read_to_string(root.join("unicode.txt")).unwrap();
        assert_eq!(result, "こんにちは\n🎊 confetti\nمرحبا\n");
    }

    // ── Test 6: Malformed diff ───────────────────────────────────────────────

    #[test]
    fn test_malformed_diff_no_hunks() {
        let root = tmp_dir();
        write_file(&root, "file.txt", "hello\n");

        let patch = "this is not a diff at all\njust plain text\n";
        let err = apply_patch(&root, Path::new("file.txt"), patch)
            .expect_err("malformed diff should be rejected");

        assert!(
            matches!(err, PatchError::MalformedDiff(_)),
            "expected MalformedDiff, got {:?}",
            err
        );
        // Original file untouched.
        assert_eq!(fs::read_to_string(root.join("file.txt")).unwrap(), "hello\n");
    }

    // ── Test 7: Malformed hunk header ────────────────────────────────────────

    #[test]
    fn test_malformed_hunk_header() {
        let root = tmp_dir();
        write_file(&root, "bad.txt", "a\nb\n");

        // @@ header with garbage numbers.
        let patch = "\
--- a/bad.txt
+++ b/bad.txt
@@ -notanumber,2 +1,2 @@
-a
+A
 b
";
        let err = apply_patch(&root, Path::new("bad.txt"), patch)
            .expect_err("malformed hunk header should fail");

        assert!(
            matches!(err, PatchError::MalformedDiff(_)),
            "expected MalformedDiff, got {:?}",
            err
        );
    }

    // ── Test 8: New file creation (only '+' lines, empty original) ───────────

    #[test]
    fn test_new_file_creation() {
        let root = tmp_dir();
        // File does NOT exist before applying the patch.
        let patch = "\
--- /dev/null
+++ b/newfile.txt
@@ -0,0 +1,3 @@
+first line
+second line
+third line
";
        apply_patch(&root, Path::new("newfile.txt"), patch)
            .expect("new-file patch should succeed");

        let result = fs::read_to_string(root.join("newfile.txt")).unwrap();
        assert_eq!(result, "first line\nsecond line\nthird line\n");
    }

    // ── Test 9: Atomic — temp file cleaned up on failure ────────────────────

    #[test]
    fn test_no_temp_file_left_on_failure() {
        let root = tmp_dir();
        write_file(&root, "atom.txt", "unchanged\n");

        let patch = "\
--- a/atom.txt
+++ b/atom.txt
@@ -1,1 +1,1 @@
-this does not match
+replacement
";
        let _ = apply_patch(&root, Path::new("atom.txt"), patch);

        // Collect entries in root; there should be no `.tmp` file.
        let tmp_files: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x == "tmp")
                    .unwrap_or(false)
            })
            .collect();

        assert!(
            tmp_files.is_empty(),
            "temp file(s) left behind: {:?}",
            tmp_files
        );
        // Original untouched.
        assert_eq!(
            fs::read_to_string(root.join("atom.txt")).unwrap(),
            "unchanged\n"
        );
    }

    // ── Test 10: Add lines at end of file ────────────────────────────────────

    #[test]
    fn test_append_at_end() {
        let root = tmp_dir();
        write_file(&root, "append.txt", "existing line\n");

        let patch = "\
--- a/append.txt
+++ b/append.txt
@@ -1,1 +1,2 @@
 existing line
+new last line
";
        apply_patch(&root, Path::new("append.txt"), patch).unwrap();
        let result = fs::read_to_string(root.join("append.txt")).unwrap();
        assert_eq!(result, "existing line\nnew last line\n");
    }
}

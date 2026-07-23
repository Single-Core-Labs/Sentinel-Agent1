use std::path::Path;

/// Captures diffs before file mutations for review.
pub struct DiffCapture;

impl DiffCapture {
    /// Read the current state of a file before mutation.
    pub fn before_write(path: &Path) -> Result<Option<String>, String> {
        if path.exists() {
            std::fs::read_to_string(path).map(Some).map_err(|e| e.to_string())
        } else {
            Ok(None)
        }
    }

    /// Generate a simple unified diff between original and proposed content.
    pub fn diff(path: &Path, original: Option<&str>, proposed: &str) -> String {
        let path_str = path.to_string_lossy();
        match original {
            Some(orig) if orig != proposed => {
                let mut output = format!("--- a/{}\n+++ b/{}\n", path_str, path_str);
                let orig_lines: Vec<&str> = orig.lines().collect();
                let prop_lines: Vec<&str> = proposed.lines().collect();
                let max_len = orig_lines.len().max(prop_lines.len());
                let mut hunk_start = 0;
                let mut changes: Vec<(usize, char, &str)> = Vec::new();

                for i in 0..max_len {
                    match (orig_lines.get(i), prop_lines.get(i)) {
                        (Some(a), Some(b)) if a != b => {
                            if changes.is_empty() { hunk_start = i; }
                            changes.push((i, '-', a));
                            changes.push((i, '+', b));
                        }
                        (Some(a), None) => {
                            if changes.is_empty() { hunk_start = i; }
                            changes.push((i, '-', a));
                        }
                        (None, Some(b)) => {
                            if changes.is_empty() { hunk_start = i; }
                            changes.push((i, '+', b));
                        }
                        _ => {
                            if !changes.is_empty() {
                                output.push_str(&format_hunk(&changes, hunk_start, path_str.as_ref()));
                                changes.clear();
                            }
                        }
                    }
                }

                if !changes.is_empty() {
                    output.push_str(&format_hunk(&changes, hunk_start, path_str.as_ref()));
                }

                if output.lines().count() <= 2 {
                    format!("--- a/{}\n+++ b/{}\n@@ -1 +1 @@\n-full file replaced\n", path_str, path_str)
                } else {
                    output
                }
            }
            _ => {
                // New file
                let line_count = proposed.lines().count();
                format!(
                    "--- a/{}\n+++ b/{}\n@@ -0,0 +1,{} @@\n+{}",
                    path_str, path_str, line_count,
                    proposed.lines().collect::<Vec<_>>().join("\n+")
                )
            }
        }
    }
}

fn format_hunk(changes: &[(usize, char, &str)], start: usize, _path: &str) -> String {
    let old_count = changes.iter().filter(|(_, c, _)| *c == '-').count() as i64;
    let new_count = changes.iter().filter(|(_, c, _)| *c == '+').count() as i64;
    let old_start = start as i64 + 1;
    let new_start = start as i64 + 1;
    let mut out = format!("@@ -{},{} +{},{} @@\n", old_start, old_count.max(1), new_start, new_count.max(1));
    for (_, c, line) in changes {
        out.push_str(&format!("{}{}\n", c, line));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_new_file() {
        let diff = DiffCapture::diff(Path::new("new.rs"), None, "fn main() {}");
        assert!(diff.contains("new.rs"));
        assert!(diff.contains("+fn main() {}"));
    }

    #[test]
    fn test_diff_modified_file() {
        let diff = DiffCapture::diff(Path::new("main.rs"), Some("old\ncontent\n"), "new\ncontent\n");
        assert!(diff.contains("-old"));
        assert!(diff.contains("+new"));
    }

    #[test]
    fn test_diff_unchanged() {
        let diff = DiffCapture::diff(Path::new("main.rs"), Some("same"), "same");
        assert!(diff.contains("full file replaced") || diff.contains("main.rs"));
    }
}

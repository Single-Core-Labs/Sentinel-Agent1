use sha1::{Sha1, Digest};

/// A hunk from a unified diff, representing a contiguous set of changes.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub added: Vec<String>,
    pub deleted: Vec<String>,
}

/// Aggregate line statistics from a diff.
#[derive(Debug, Clone, Default)]
pub struct LineStats {
    pub added: u32,
    pub deleted: u32,
}

/// Parse a unified diff string into hunks.
///
/// Supports standard `diff -u` and `git diff` output formats.
/// Extracts lines prefixed with `+` (added) and `-` (deleted),
/// ignoring hunk headers, context lines, and file metadata.
pub fn parse_unified_diff(diff: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;

    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("@@") {
            // Flush previous hunk
            if let Some(hunk) = current_hunk.take() {
                hunks.push(hunk);
            }
            // Parse hunk header: @@ -old,count +new,count @@
            if let Some(rest) = header.split("@@").next() {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let old_part = parts.first().unwrap_or(&"-0,0");
                let new_part = parts.get(1).unwrap_or(&"+0,0");

                let old = parse_hunk_range(old_part);
                let new = parse_hunk_range(new_part);

                current_hunk = Some(DiffHunk {
                    old_start: old.0,
                    old_lines: old.1,
                    new_start: new.0,
                    new_lines: new.1,
                    added: Vec::new(),
                    deleted: Vec::new(),
                });
            }
        } else if let Some(ref mut hunk) = current_hunk {
            if let Some(content) = line.strip_prefix('+') {
                hunk.added.push(content.to_string());
            } else if let Some(content) = line.strip_prefix('-') {
                hunk.deleted.push(content.to_string());
            }
            // Context lines (space prefix) and other lines are ignored
        }
    }

    // Flush last hunk
    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    hunks
}

fn parse_hunk_range(part: &str) -> (u32, u32) {
    let part = part.trim_start_matches('-').trim_start_matches('+');
    let mut parts = part.splitn(2, ',');
    let start = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let count = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    (start, count)
}

/// Generate SHA-1 fingerprints for a set of lines.
///
/// Each line is normalized (trimmed of trailing whitespace) before
/// hashing. Used to fingerprint accepted code changes for deduplication
/// and impact tracking.
pub fn fingerprint_lines(lines: &[String]) -> Vec<String> {
    let mut hasher = Sha1::new();
    lines.iter().map(|line| {
        hasher = Sha1::new();
        hasher.update(line.trim_end().as_bytes());
        hex::encode(hasher.finalize_reset())
    }).collect()
}

/// Compute aggregate line statistics from a unified diff.
///
/// Returns the count of added and deleted lines. The actual
/// fingerprints are intentionally omitted from the analytics payload
/// to reduce data volume; only the aggregate counts are transmitted.
pub fn line_stats(diff: &str) -> LineStats {
    let hunks = parse_unified_diff(diff);
    let mut stats = LineStats::default();
    for hunk in &hunks {
        stats.added += hunk.added.len() as u32;
        stats.deleted += hunk.deleted.len() as u32;
    }
    stats
}

/// Compute line stats and return the SHA-1 fingerprints for later
/// comparison (e.g., to detect duplicate changes across turns).
/// The fingerprints are only used internally; the analytics payload
/// contains only the aggregate counts.
pub fn fingerprint_diff(diff: &str) -> (LineStats, Vec<String>) {
    let stats = line_stats(diff);
    let mut all_lines = Vec::new();
    let hunks = parse_unified_diff(diff);
    for hunk in &hunks {
        all_lines.extend(hunk.added.iter().cloned());
        all_lines.extend(hunk.deleted.iter().cloned());
    }
    let fingerprints = fingerprint_lines(&all_lines);
    (stats, fingerprints)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_diff() {
        let diff = "\
@@ -1,3 +1,4 @@
 foo
-bar
+bar2
+baz
";
        let hunks = parse_unified_diff(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].added, vec!["bar2", "baz"]);
        assert_eq!(hunks[0].deleted, vec!["bar"]);
    }

    #[test]
    fn test_line_stats() {
        let diff = "\
@@ -1,3 +1,4 @@
 foo
-bar
+bar2
+baz
";
        let stats = line_stats(diff);
        assert_eq!(stats.added, 2);
        assert_eq!(stats.deleted, 1);
    }

    #[test]
    fn test_fingerprint_lines() {
        let lines = vec!["hello".to_string(), "world".to_string()];
        let fps = fingerprint_lines(&lines);
        assert_eq!(fps.len(), 2);
        // Known SHA-1 of "hello"
        assert_eq!(fps[0], "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
        // Known SHA-1 of "world"
        assert_eq!(fps[1], "7c211433f02071597741e6ff5a8ea34789abbf43");
    }

    #[test]
    fn test_fingerprint_diff_roundtrip() {
        let diff = "\
@@ -1,1 +1,2 @@
-old
+new1
+new2
";
        let (stats, fps) = fingerprint_diff(diff);
        assert_eq!(stats.added, 2);
        assert_eq!(stats.deleted, 1);
        assert_eq!(fps.len(), 3);
    }
}

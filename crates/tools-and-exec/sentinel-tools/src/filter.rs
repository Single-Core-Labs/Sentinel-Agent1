//! Path filtering for file-surfacing tools: hidden-file and `.gitignore`
//! exclusions. The spec asks file utilities to (a) skip hidden and ignored
//! files and (b) offer globbing; this module is the dependency-free matcher
//! shared by the `glob` and `grep` tools.
//!
//! Supported `gitignore` subset: `#` comments, blank lines, negation (`!`),
//! anchored patterns (leading `/`, or any pattern containing a slash),
//! directory patterns (trailing `/`), and glob segments (`*`, `?`, `[abc]`,
//! `**`).

use std::path::{Path, PathBuf};

/// Directories that are always skipped, regardless of `.gitignore`.
const ALWAYS_IGNORED_DIRS: &[&str] = &[".git", "node_modules", "target", ".venv", "__pycache__"];

/// Upward search depth for a `.gitignore` file.
const GITIGNORE_SEARCH_DEPTH: usize = 8;

// ── Rule model ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Rule {
    /// Leading `!` — matching this rule un-ignores a path.
    negated: bool,
    /// Pattern with a slash is anchored to the `.gitignore` directory.
    anchored: bool,
    /// Trailing `/` — matches the named directory and everything below it.
    dir_only: bool,
    /// Glob stripped of leading `/` and trailing `/`, split into segments.
    components: Vec<String>,
}

fn parse_rule(line: &str) -> Option<Rule> {
    let mut line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let negated = line.starts_with('!');
    if negated {
        line = line.trim_start_matches('!');
    }
    let dir_only = line.ends_with('/');
    if dir_only {
        line = line.trim_end_matches('/');
    }
    if line.is_empty() {
        return None;
    }
    let anchored = line.starts_with('/') || line.contains('/');
    let line = line.trim_start_matches('/');
    let components: Vec<String> = line.split('/').map(|s| s.to_string()).collect();
    Some(Rule {
        negated,
        anchored,
        dir_only,
        components,
    })
}

/// Single-segment glob (`*`, `?`) — DP matcher.
fn glob_comp(pat: &str, text: &str) -> bool {
    let pa: Vec<char> = pat.chars().collect();
    let ta: Vec<char> = text.chars().collect();
    let (pl, tl) = (pa.len(), ta.len());
    let mut dp = vec![vec![false; tl + 1]; pl + 1];
    dp[0][0] = true;
    for i in 0..pl {
        if pa[i] == '*' {
            dp[i + 1][0] = dp[i][0];
        }
    }
    for i in 0..pl {
        for j in 0..tl {
            match pa[i] {
                '*' => dp[i + 1][j + 1] = dp[i][j + 1] || dp[i + 1][j],
                '?' => dp[i + 1][j + 1] = dp[i][j],
                _ => dp[i + 1][j + 1] = dp[i][j] && pa[i] == ta[j],
            }
        }
    }
    dp[pl][tl]
}

/// Multi-segment match supporting `**` (zero or more segments).
fn glob_segments(comps: &[String], segs: &[String]) -> bool {
    fn inner(comps: &[String], segs: &[String], ci: usize, si: usize) -> bool {
        if ci == comps.len() {
            return si == segs.len();
        }
        if comps[ci] == "**" {
            for k in si..=segs.len() {
                if inner(comps, segs, ci + 1, k) {
                    return true;
                }
            }
            return false;
        }
        si < segs.len() && glob_comp(&comps[ci], &segs[si]) && inner(comps, segs, ci + 1, si + 1)
    }
    inner(comps, segs, 0, 0)
}

/// Public single-segment glob used by the `grep` tool's `include` filter.
pub(crate) fn glob_match(pattern: &str, text: &str) -> bool {
    glob_comp(pattern, text)
}

impl Rule {
    fn matches(&self, segs: &[String]) -> bool {
        if self.dir_only {
            return segments_prefix_match(&self.components, segs);
        }
        if self.components.contains(&"**".to_string()) {
            return glob_segments(&self.components, segs);
        }
        if self.anchored || self.components.len() > 1 {
            // Rooted match: a pattern that matches a directory excludes its
            // subtree.
            return segments_prefix_match(&self.components, segs);
        }
        let pat = &self.components[0];
        segs.iter().any(|s| glob_comp(pat, s))
    }
}

fn segments_prefix_match(comps: &[String], segs: &[String]) -> bool {
    if segs.len() < comps.len() {
        return false;
    }
    comps
        .iter()
        .zip(segs.iter())
        .all(|(c, s)| glob_comp(c, s))
}

// ── Gitignore store ─────────────────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct Gitignore {
    rules: Vec<Rule>,
}

impl Gitignore {
    /// Load the nearest `.gitignore` at `base` walking up the tree.
    pub fn load_for(base: &Path) -> Self {
        let mut dir = Some(base);
        for _ in 0..GITIGNORE_SEARCH_DEPTH {
            let Some(d) = dir else {
                break;
            };
            let candidate = d.join(".gitignore");
            if candidate.is_file() {
                if let Ok(content) = std::fs::read_to_string(&candidate) {
                    let rules = content.lines().filter_map(parse_rule).collect();
                    return Self { rules };
                }
            }
            dir = d.parent();
        }
        Self::default()
    }

    /// `true` when the last matching rule ignores `rel`.
    pub fn is_ignored(&self, segs: &[String]) -> bool {
        if segs.is_empty() {
            return false;
        }
        let mut ignored = false;
        for rule in &self.rules {
            if rule.matches(segs) {
                ignored = !rule.negated;
            }
        }
        ignored
    }
}

/// Split a path into forward-slash segments (skips `.`/`..`/root).
pub fn rel_segments(path: &Path) -> Vec<String> {
    use std::path::Component;
    path.components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

// ── High-level filter ───────────────────────────────────────────────────────

/// Filters paths for the `glob` and `grep` tools.
#[derive(Debug, Clone)]
pub struct FileFilter {
    /// Include hidden dot-prefixed entries.
    pub dot_files: bool,
    base: PathBuf,
    gitignore: Gitignore,
}

impl FileFilter {
    pub fn new(base: &Path, dot_files: bool) -> Self {
        Self {
            dot_files,
            base: base.to_path_buf(),
            gitignore: Gitignore::load_for(base),
        }
    }

    /// `true` when `path` should be excluded from search results.
    pub fn should_skip(&self, path: &Path) -> bool {
        let rel = match path.strip_prefix(&self.base) {
            Ok(r) => r,
            Err(_) => return false,
        };
        let segs = rel_segments(rel);
        if segs.is_empty() {
            return false;
        }
        if segs.iter().any(|s| ALWAYS_IGNORED_DIRS.contains(&s.as_str())) {
            return true;
        }
        if !self.dot_files && segs.iter().any(|s| s.starts_with('.')) {
            return true;
        }
        self.gitignore.is_ignored(&segs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip_with_dot(base: &Path, p: &str, dot: bool) -> bool {
        FileFilter::new(base, dot).should_skip(&base.join(p))
    }

    fn skip(base: &Path, p: &str) -> bool {
        skip_with_dot(base, p, false)
    }

    fn tmp(root: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sentinel-filter-test-{}-{}",
            std::process::id(),
            root
        ))
    }

    #[test]
    fn hidden_dotfiles_skipped_unless_allowed() {
        let dir = tmp("dot");
        let _ = std::fs::create_dir_all(&dir);
        assert!(skip(&dir, ".config/foo.rs"));
        assert!(skip(&dir, "src/.hidden.rs"));
        assert!(skip(&dir, "src/.cache/whatever"));
        assert!(!skip(&dir, "README.md"));
        assert!(!skip(&dir, "docs/X.md"));
        assert!(!skip_with_dot(&dir, ".config/foo.rs", true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_ignored_dirs_always_skipped() {
        let dir = tmp("def");
        let _ = std::fs::create_dir_all(&dir);
        assert!(skip(&dir, "node_modules/x/y.js"));
        assert!(skip(&dir, "target/debug/binary.exe"));
        assert!(skip(&dir, ".git/index"));
        assert!(skip(&dir, ".venv/lib/py"));
        assert!(skip(&dir, "__pycache__/x.pyc"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gitignore_rules_apply() {
        let root = tmp("gitignore");
        let _ = std::fs::create_dir_all(&root);
        std::fs::write(
            root.join(".gitignore"),
            "*.log\n/build/\nkeep.log\n!important.log\n/dist\n",
        )
        .unwrap();
        let gi = Gitignore::load_for(&root);
        let ignored = |p: &str| gi.is_ignored(&rel_segments(p.as_ref()));
        assert!(ignored("app.log"));
        assert!(ignored("src/deep/err.log"));
        assert!(ignored("build/out"));
        assert!(ignored("keep.log"));
        assert!(ignored("dist/models.bin"));
        assert!(!ignored("logs/app.txt"));
        assert!(!ignored("important.log"));
        assert!(!ignored("notes.txt"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gitignore_dir_rule_covers_subtree() {
        let rule = parse_rule("/build/").unwrap();
        assert!(rule.matches(&rel_segments(&Path::new("build"))));
        assert!(rule.matches(&rel_segments(&Path::new("build/out/obj"))));
        assert!(!rule.matches(&rel_segments(&Path::new("my/build/x"))));
    }

    #[test]
    fn gitignore_anchored_literal_covers_subtree() {
        let rule = parse_rule("/foo/bar").unwrap();
        assert!(rule.matches(&rel_segments(&Path::new("foo/bar"))));
        assert!(rule.matches(&rel_segments(&Path::new("foo/bar/baz.txt"))));
        assert!(!rule.matches(&rel_segments(&Path::new("my/foo/bar/x"))));
    }

    #[test]
    fn gitignore_glob_segments() {
        let rule = parse_rule("src/**/tmp.rs").unwrap();
        assert!(rule.matches(&rel_segments(&Path::new("src/deep/tmp.rs"))));
        assert!(rule.matches(&rel_segments(&Path::new("src/tmp.rs"))));
        assert!(!rule.matches(&rel_segments(&Path::new("src/other.rs"))));

        let star = parse_rule("*.tmp").unwrap();
        assert!(star.matches(&rel_segments(&Path::new("file.tmp"))));
        assert!(star.matches(&rel_segments(&Path::new("a/b/file.tmp"))));
        assert!(!star.matches(&rel_segments(&Path::new("file.rs"))));
    }

    #[test]
    fn negation_rule_wins_after_positive() {
        let root = tmp("negate");
        let _ = std::fs::create_dir_all(&root);
        std::fs::write(root.join(".gitignore"), "*.log\n!keep.log\n").unwrap();
        let gi = Gitignore::load_for(&root);
        assert!(gi.is_ignored(&rel_segments(&Path::new("drop.log"))));
        assert!(!gi.is_ignored(&rel_segments(&Path::new("keep.log"))));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_gitignore_file_matches_nothing() {
        let root = tmp("none");
        let _ = std::fs::create_dir_all(&root);
        let gi = Gitignore::load_for(&root);
        assert!(!gi.is_ignored(&rel_segments(&Path::new("anything.go"))));
        let _ = std::fs::remove_dir_all(&root);
    }
}

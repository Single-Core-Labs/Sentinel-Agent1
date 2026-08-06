//! File-content context for prompts (`getContextFromPaths` equivalent).
//!
//! The spec aggregates content from configured project paths into the prompt
//! ("getContextFromPaths"), efficiently loading and merging file contents to
//! avoid redundant file-system access. [`FileContext`] reads the
//! `context.paths` entries from `SentinelConfig`, applies `context.exclude`
//! filters, deduplicates by canonical path, and renders the contents as a
//! markdown section that is appended to the system prompt.
//!
//! Only paths that resolve to files are loaded — a directory entry (such as
//! the default `"."`) is skipped, so the whole repository is never dumped
//! into the prompt; the directory *listing* lives in [`ProjectContext`].
//!
//! [`ProjectContext`]: crate::project_context::ProjectContext

use crate::prompt::SystemPromptManager;
use sentinel_config::SentinelConfig;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Per-file content cap (characters) included in the prompt.
const FILE_MAX_CHARS: usize = 16_000;
/// Total content cap (characters) across all aggregated files.
const TOTAL_MAX_CHARS: usize = 48_000;

/// One aggregated file: display path plus (possibly truncated) contents.
#[derive(Debug, Clone)]
pub struct ContextFile {
    pub path: String,
    pub content: String,
}

/// Aggregated file contents for prompt injection.
#[derive(Debug, Clone, Default)]
pub struct FileContext {
    pub files: Vec<ContextFile>,
}

impl FileContext {
    /// Load contents for every `context.paths` entry that is a readable,
    /// non-excluded, non-binary file. Never fails: unreadable entries are
    /// skipped.
    pub fn load(config: &SentinelConfig) -> Self {
        let cwd = std::env::current_dir().ok();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut files: Vec<ContextFile> = Vec::new();
        let mut total_chars: usize = 0;

        for raw in &config.context.paths {
            if total_chars >= TOTAL_MAX_CHARS {
                break;
            }
            let Some(path) = resolve_entry(raw, cwd.as_deref()) else {
                continue;
            };
            if !path.is_file() {
                // Directories (e.g. the default ".") are never dumped.
                continue;
            }
            let Some(canon) = path.canonicalize().ok() else {
                continue;
            };
            if !seen.insert(canon) {
                continue; // dedup: one read per canonical path
            }
            let display = display_path(&path, cwd.as_deref());
            if config.context.exclude.iter().any(|e| display.contains(e)) {
                continue;
            }
            let Some(content) = std::fs::read_to_string(&path).ok() else {
                continue;
            };
            if content.trim().is_empty() || content.contains('\0') {
                continue; // empty or binary
            }
            let truncated: String = content.chars().take(FILE_MAX_CHARS).collect();
            total_chars += truncated.chars().count();
            files.push(ContextFile {
                path: display,
                content: truncated,
            });
        }

        Self { files }
    }

    /// Render as a markdown section; empty when nothing was loaded.
    pub fn render(&self) -> String {
        if self.files.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        out.push_str("## File Context (configured paths)\n");
        for file in &self.files {
            out.push_str(&format!("### {}\n```text\n{}\n```\n", file.path, file.content));
        }
        out
    }

    /// Append the rendered section to a prompt manager's base prompt.
    pub fn apply_to_manager(&self, manager: &mut SystemPromptManager) {
        let rendered = self.render();
        if rendered.is_empty() {
            return;
        }
        let base = manager.base().trim_end().to_string();
        manager.set_base(format!("{}\n\n{}", base, rendered));
    }
}

/// Resolve a `context.paths` entry against the working directory.
fn resolve_entry(raw: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(cwd?.join(path))
    }
}

/// Path relative to cwd when possible, else absolute.
fn display_path(path: &Path, cwd: Option<&Path>) -> String {
    if let Some(cwd) = cwd {
        if let Ok(rel) = path.strip_prefix(cwd) {
            return rel.to_string_lossy().into_owned();
        }
    }
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_config::ContextSettings;

    struct Sandbox {
        dir: PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "sentinel-file-ctx-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn write(&self, rel: &str, content: &str) {
            let path = self.dir.join(rel);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(path, content).unwrap();
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn config_with(paths: Vec<String>, excludes: Vec<String>) -> SentinelConfig {
        SentinelConfig {
            context: ContextSettings {
                paths,
                exclude: excludes,
            },
            ..Default::default()
        }
    }

    #[test]
    fn loads_files_and_skips_directories() {
        let sb = Sandbox::new("files");
        sb.write("a.txt", "hello");
        sb.write("sub/b.rs", "fn b() {}");
        std::fs::create_dir_all(sb.dir.join("dir_only")).unwrap();

        let cfg = config_with(
            vec![
                sb.dir.join("a.txt").to_string_lossy().into_owned(),
                sb.dir.join("sub/b.rs").to_string_lossy().into_owned(),
                sb.dir.join("dir_only").to_string_lossy().into_owned(),
            ],
            vec![],
        );
        let ctx = FileContext::load(&cfg);
        assert_eq!(ctx.files.len(), 2, "directory entries must be skipped");
        assert!(ctx.render().contains("hello"));
        assert!(ctx.render().contains("fn b() {}"));
    }

    #[test]
    fn default_dot_path_does_not_dump_repo() {
        let sb = Sandbox::new("dot");
        sb.write("keep.txt", "x");
        let cfg = config_with(vec![".".into()], vec![]);
        let ctx = FileContext::load(&cfg);
        assert!(
            ctx.files.is_empty(),
            "a bare '.' resolves to a directory and must be skipped"
        );
    }

    #[test]
    fn excludes_are_honored_on_display_path() {
        let sb = Sandbox::new("excl");
        sb.write("src/keep.rs", "pub fn keep() {}");
        sb.write("src/skip_gen.rs", "pub fn skip() {}");
        let cfg = config_with(
            vec![
                sb.dir.join("src/keep.rs").to_string_lossy().into_owned(),
                sb.dir.join("src/skip_gen.rs").to_string_lossy().into_owned(),
            ],
            vec!["skip_gen".into()],
        );
        let ctx = FileContext::load(&cfg);
        assert_eq!(ctx.files.len(), 1);
        assert!(ctx.render().contains("keep"));
        assert!(!ctx.render().contains("skip"));
    }

    #[test]
    fn duplicate_paths_are_read_once() {
        let sb = Sandbox::new("dedup");
        sb.write("a.txt", "once");
        let p = sb.dir.join("a.txt");
        let p2 = sb.dir.join(".").join("a.txt");
        let cfg = config_with(
            vec![
                p.to_string_lossy().into_owned(),
                p2.to_string_lossy().into_owned(),
            ],
            vec![],
        );
        let ctx = FileContext::load(&cfg);
        assert_eq!(ctx.files.len(), 1, "canonical dedup must collapse aliases");
    }

    #[test]
    fn binary_and_empty_files_are_skipped() {
        let sb = Sandbox::new("binary");
        sb.write("data.bin", "abc\x00def");
        sb.write("empty.txt", "   \n");
        sb.write("ok.txt", "fine");
        let cfg = config_with(
            vec![
                sb.dir.join("data.bin").to_string_lossy().into_owned(),
                sb.dir.join("empty.txt").to_string_lossy().into_owned(),
                sb.dir.join("ok.txt").to_string_lossy().into_owned(),
            ],
            vec![],
        );
        let ctx = FileContext::load(&cfg);
        assert_eq!(ctx.files.len(), 1);
        assert!(ctx.render().contains("fine"));
    }

    #[test]
    fn per_file_and_total_caps_apply() {
        let sb = Sandbox::new("caps");
        let big = "x".repeat(20_000);
        sb.write("big.txt", &big);
        sb.write("small.txt", "small content");
        let cfg = config_with(
            vec![
                sb.dir.join("big.txt").to_string_lossy().into_owned(),
                sb.dir.join("small.txt").to_string_lossy().into_owned(),
            ],
            vec![],
        );
        let ctx = FileContext::load(&cfg);
        let big_file = ctx.files.iter().find(|f| f.path.ends_with("big.txt")).unwrap();
        assert!(
            big_file.content.chars().count() <= FILE_MAX_CHARS,
            "per-file cap must hold"
        );
        let total: usize = ctx.files.iter().map(|f| f.content.chars().count()).sum();
        assert!(total <= TOTAL_MAX_CHARS);
    }

    #[test]
    fn empty_context_renders_nothing() {
        let ctx = FileContext::default();
        assert!(ctx.render().is_empty());
        let mut manager = SystemPromptManager::new();
        ctx.apply_to_manager(&mut manager);
        assert!(!manager.render().contains("File Context"));
    }

    #[test]
    fn missing_path_is_skipped() {
        let cfg = config_with(vec!["does/not/exist.md".into()], vec![]);
        let ctx = FileContext::load(&cfg);
        assert!(ctx.files.is_empty());
    }
}

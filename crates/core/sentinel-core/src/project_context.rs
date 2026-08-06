//! Project-aware prompt context.
//!
//! The spec calls for prompts that are tailored per agent with project
//! context, environmental details, and LSP configuration. [`ProjectContext`]
//! discovers those facts (working directory, OS/arch/cores, git root and
//! branch, the project's `AGENTS.md`, configured LSP servers) and renders
//! them as a markdown section appended to the system prompt.

use crate::prompt::SystemPromptManager;
use sentinel_config::SentinelConfig;
use std::path::{Path, PathBuf};

/// Upper bound on the `AGENTS.md` excerpt included in prompts.
const AGENTS_MD_MAX_CHARS: usize = 1200;
/// Lines of `AGENTS.md` excerpt included in prompts.
const AGENTS_MD_MAX_LINES: usize = 40;
/// Maximum top-level directory entries listed in the env info.
const DIR_LISTING_MAX: usize = 24;

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub cwd: String,
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub git_root: Option<String>,
    pub git_branch: Option<String>,
    /// LSP servers as `"id (command)"` (command omitted when empty).
    pub lsp_servers: Vec<String>,
    /// Top-level directory listing of the working directory.
    pub dir_entries: Vec<String>,
    /// Total top-level entries (listing above may be truncated).
    pub dir_total: usize,
    /// Project `AGENTS.md` excerpt, if present in the working directory.
    pub agents_md: Option<String>,
}

impl ProjectContext {
    /// Discover project and environment facts. Never fails: git and file
    /// lookups degrade to `None` when unavailable.
    pub fn discover(config: &SentinelConfig) -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "<unknown>".into());
        let os = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        }
        .into();
        let arch = std::env::consts::ARCH.to_string();
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0);
        let git_root = git_cmd(&["rev-parse", "--show-toplevel"]);
        let git_branch = git_cmd(&["symbolic-ref", "--short", "HEAD"]);
        let lsp_servers: Vec<String> = config
            .lsp_servers
            .iter()
            .map(|l| {
                if l.command.trim().is_empty() {
                    l.id.clone()
                } else {
                    format!("{} ({})", l.id, l.command)
                }
            })
            .collect();
        let (dir_entries, dir_total) = list_dir(Path::new(&cwd));
        let agents_md = read_agents_md(Path::new(&cwd));

        Self {
            cwd,
            os,
            arch,
            cpu_cores,
            git_root,
            git_branch,
            lsp_servers,
            dir_entries,
            dir_total,
            agents_md,
        }
    }

    /// Render the context as a markdown section.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("## Project Context\n");
        out.push_str(&format!("- Working directory: `{}`\n", self.cwd));
        out.push_str(&format!(
            "- Environment: {} {}, {} CPU cores\n",
            self.os, self.arch, self.cpu_cores
        ));
        match (&self.git_root, &self.git_branch) {
            (Some(root), Some(branch)) => {
                out.push_str(&format!(
                    "- Git: branch `{}` at `{}`\n",
                    branch, root
                ));
            }
            (Some(root), None) => {
                out.push_str(&format!("- Git repository root: `{}`\n", root));
            }
            _ => out.push_str("- Git: not a git repository\n"),
        }
        if !self.dir_entries.is_empty() {
            let shown = self.dir_entries.len();
            let more = if shown < self.dir_total {
                format!(" (and {} more)", self.dir_total - shown)
            } else {
                String::new()
            };
            out.push_str(&format!(
                "- Directory: `{}`{}\n",
                self.dir_entries.join(", "),
                more
            ));
        }
        if self.lsp_servers.is_empty() {
            out.push_str("- LSP servers: none\n");
        } else {
            out.push_str(&format!(
                "- LSP servers: {}\n",
                self.lsp_servers.join(", ")
            ));
            out.push_str(
                "- LSP diagnostics: these servers index the project; diagnostics and \
                 definitions are served to the assistant through the workspace integration.\n",
            );
        }
        if let Some(agents_md) = &self.agents_md {
            out.push_str(&format!(
                "- Project rules (AGENTS.md):\n```text\n{}\n```\n",
                agents_md
            ));
        }
        out
    }

    /// Append the rendered context to a prompt manager's base prompt.
    pub fn apply_to_manager(&self, manager: &mut SystemPromptManager) {
        let base = manager.base().trim_end().to_string();
        manager.set_base(format!("{}\n\n{}", base, self.render()));
    }

    /// A ready-to-attach prompt manager with project context injected.
    pub fn inject_into_prompt_manager(config: &SentinelConfig) -> SystemPromptManager {
        let mut manager = SystemPromptManager::new();
        Self::discover(config).apply_to_manager(&mut manager);
        crate::file_context::FileContext::load(config).apply_to_manager(&mut manager);
        manager
    }
}

impl Default for ProjectContext {
    fn default() -> Self {
        Self {
            cwd: "<unknown>".into(),
            os: "Unknown".into(),
            arch: String::new(),
            cpu_cores: 0,
            git_root: None,
            git_branch: None,
            lsp_servers: Vec::new(),
            dir_entries: Vec::new(),
            dir_total: 0,
            agents_md: None,
        }
    }
}

fn git_cmd(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn list_dir(cwd: &Path) -> (Vec<String>, usize) {
    let Ok(read) = std::fs::read_dir(cwd) else {
        return (Vec::new(), 0);
    };
    let mut names = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Skip hidden entries and common build-noise to keep the listing tight.
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        names.push(if is_dir {
            format!("{name}/")
        } else {
            name
        });
    }
    names.sort();
    let total = names.len();
    names.truncate(DIR_LISTING_MAX);
    (names, total)
}

fn read_agents_md(cwd: &Path) -> Option<String> {
    let path = cwd.join("AGENTS.md");
    let content = std::fs::read_to_string(&path).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    let excerpt: Vec<&str> = content.lines().take(AGENTS_MD_MAX_LINES).collect();
    let mut text = excerpt.join("\n");
    if text.chars().count() > AGENTS_MD_MAX_CHARS {
        text = text.chars().take(AGENTS_MD_MAX_CHARS).collect();
        text.push_str("…");
    }
    Some(text)
}

/// Convenience: `$SENTINEL_HOME/events`, else `~/.sentinel/events`.
pub fn default_events_dir() -> PathBuf {
    sentinel_config::default_data_dir().join("events")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ProjectContext {
        ProjectContext {
            cwd: "C:\\repo\\app".into(),
            os: "Windows".into(),
            arch: "x86_64".into(),
            cpu_cores: 8,
            git_root: Some("C:\\repo\\app".into()),
            git_branch: Some("main".into()),
            lsp_servers: vec!["rust-analyzer".into(), "pyright".into()],
            dir_entries: vec!["src/".into(), "Cargo.toml".into(), "README.md".into()],
            dir_total: 25,
            agents_md: Some("## Rules\n- run tests".into()),
        }
    }

    #[test]
    fn render_includes_all_sections() {
        let text = sample().render();
        assert!(text.contains("## Project Context"));
        assert!(text.contains("C:\\repo\\app"));
        assert!(text.contains("Windows"));
        assert!(text.contains("8 CPU cores"));
        assert!(text.contains("branch `main`"));
        assert!(text.contains("rust-analyzer, pyright"));
        assert!(text.contains("AGENTS.md"));
        assert!(text.contains("run tests"));
        assert!(text.contains("Directory"));
        assert!(text.contains("src/"));
    }

    #[test]
    fn directory_listing_reports_truncation() {
        let text = sample().render();
        assert!(
            text.contains("and 22 more"),
            "listing must note remaining entries: {}",
            text
        );
    }

    #[test]
    fn lsp_section_notes_diagnostics_capability() {
        let text = sample().render();
        assert!(text.contains("LSP diagnostics"));
        assert!(text.contains("workspace integration"));
    }

    #[test]
    fn render_without_git_and_lsp() {
        let mut ctx = sample();
        ctx.git_root = None;
        ctx.git_branch = None;
        ctx.lsp_servers = Vec::new();
        ctx.agents_md = None;
        let text = ctx.render();
        assert!(text.contains("not a git repository"));
        assert!(text.contains("LSP servers: none"));
        assert!(!text.contains("AGENTS.md"));
    }

    #[test]
    fn apply_to_manager_appends_section() {
        let mut manager = SystemPromptManager::new();
        sample().apply_to_manager(&mut manager);
        let rendered = manager.render();
        assert!(rendered.starts_with("You are Sentinel"));
        assert!(rendered.contains("## Project Context"));
        assert!(rendered.contains("rust-analyzer"));
    }

    #[test]
    fn inject_into_prompt_manager_uses_default_config() {
        let config = SentinelConfig::default();
        let manager = ProjectContext::inject_into_prompt_manager(&config);
        let rendered = manager.render();
        assert!(rendered.contains("## Project Context"));
        assert!(rendered.contains("Working directory"));
    }

    #[test]
    fn agents_md_excerpt_respects_limits() {
        let dir = std::env::temp_dir().join(format!(
            "sentinel-prompt-ctx-{}-agentsmd",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let mut content = String::new();
        for i in 0..200 {
            content.push_str(&format!("line {}\n", i));
        }
        std::fs::write(dir.join("AGENTS.md"), &content).unwrap();

        let excerpt = read_agents_md(&dir).unwrap();
        assert!(
            excerpt.lines().count() <= AGENTS_MD_MAX_LINES,
            "line cap must hold"
        );
        assert!(excerpt.chars().count() <= AGENTS_MD_MAX_CHARS + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_events_dir_uses_sentinel_home() {
        let _ = std::env::var("SENTINEL_HOME");
        assert!(default_events_dir().ends_with("events"));
    }
}

//! Parser for `AGENTS.md` configuration files.
//!
//! The real Codex system reads hierarchical `AGENTS.md` files: a root file
//! plus optional per-directory files that refine operational guidelines for
//! their subtree (permitted actions, preferred model, test commands, ...).
//!
//! This module parses markdown headings (`#`–`######`) and list items into
//! structured [`AgentsMdSection`]s, and can discover + order the hierarchy
//! under a workspace root so that deeper files take precedence for their
//! scope.

use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentsMdError {
    #[error("failed to read {0}: {1}")]
    Io(String, #[source] std::io::Error),
    #[error("failed to walk directory tree under {0}: {1}")]
    Walk(String, #[source] std::io::Error),
}

/// A heading plus the list items (and nested list lines) that follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentsMdSection {
    /// Heading text without the leading `#` markers.
    pub heading: String,
    /// Heading level (1–6).
    pub level: u8,
    /// Bullet items / numbered items / indented continuation lines.
    pub items: Vec<String>,
}

/// A parsed `AGENTS.md` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentsMd {
    /// Absolute path the file was loaded from (empty for `parse`).
    pub path: PathBuf,
    /// Scope: the directory the file applies to, relative to the workspace
    /// root (`.` for the root file itself).
    pub scope: String,
    /// Raw file content.
    pub raw: String,
    /// Parsed sections in document order.
    pub sections: Vec<AgentsMdSection>,
}

/// Flattened view: every rule item with its scope, so callers can apply
/// nearest‑scope‑wins precedence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentsMdRule {
    /// Directory scope, e.g. `.`, `crates/core`, `packages/desktop-app`.
    pub scope: String,
    /// Section heading this rule belongs to.
    pub heading: String,
    /// The rule text.
    pub text: String,
}

/// Parse raw `AGENTS.md` markdown into sections.
///
/// Only the structural elements are extracted: heading lines (`^#{1,6} `)
/// start a new section; list items (`- `, `* `, `+ `, `N. `) and indented
/// continuation lines are collected as items.  Fenced code blocks are skipped.
pub fn parse_agents_md(content: &str) -> AgentsMd {
    let mut sections: Vec<AgentsMdSection> = Vec::new();
    let mut current: Option<AgentsMdSection> = None;
    let mut in_fence = false;

    for line in content.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.trim().is_empty() {
            continue;
        }

        if let Some(rest) = heading_of(trimmed) {
            let (level, text) = rest;
            if let Some(sec) = current.take() {
                sections.push(sec);
            }
            current = Some(AgentsMdSection {
                heading: text.to_string(),
                level,
                items: Vec::new(),
            });
        } else if let Some(item) = list_item_of(trimmed) {
            match current.as_mut() {
                Some(sec) => sec.items.push(item.to_string()),
                // Items before any heading go under a synthetic preamble.
                None => {
                    current = Some(AgentsMdSection {
                        heading: "Preamble".to_string(),
                        level: 1,
                        items: vec![item.to_string()],
                    });
                }
            }
        }
        // Plain paragraphs are ignored.
    }

    if let Some(sec) = current.take() {
        sections.push(sec);
    }

    AgentsMd {
        path: PathBuf::new(),
        scope: String::new(),
        raw: content.to_string(),
        sections,
    }
}

/// Load and parse the `AGENTS.md` file at `path`.
pub fn load_agents_md(path: &Path) -> Result<AgentsMd, AgentsMdError> {
    let content = fs::read_to_string(path).map_err(|e| AgentsMdError::Io(path.display().to_string(), e))?;
    let mut parsed = parse_agents_md(&content);
    parsed.path = path.to_path_buf();
    parsed.scope = path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    Ok(parsed)
}

/// Discover every `AGENTS.md` under `root` (including the root file itself),
/// returned shallowest‑first so deeper scopes can override.
pub fn discover_agents_md(root: &Path) -> Result<Vec<AgentsMd>, AgentsMdError> {
    let root_abs = root
        .canonicalize()
        .map_err(|e| AgentsMdError::Walk(root.display().to_string(), e))?;
    let mut found = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root_abs.clone()];

    while let Some(dir) = stack.pop() {
        let root_file = dir.join("AGENTS.md");
        if root_file.is_file() {
            found.push(load_agents_md(&root_file)?);
        }
        let entries =
            fs::read_dir(&dir).map_err(|e| AgentsMdError::Walk(dir.display().to_string(), e))?;
        let mut subdirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        // Pop order → we want shallowest first, so push deepest first.
        subdirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
        stack.extend(subdirs);
    }

    // Re-scope relative to the root so ordering and rule scopes are stable.
    for md in found.iter_mut() {
        if let Some(parent) = md.path.parent() {
            md.scope = parent
                .strip_prefix(&root_abs)
                .map(|p| {
                    if p.as_os_str().is_empty() {
                        ".".to_string()
                    } else {
                        p.display().to_string().replace('\\', "/")
                    }
                })
                .unwrap_or_else(|_| md.scope.clone());
        }
    }

    found.sort_by_key(|md| md.scope.matches('/').count());
    Ok(found)
}/// Load the full hierarchy under `root` and flatten it into scoped rules.
///
/// Rules are ordered root‑first; consumers resolving a path should prefer
/// the **last** rule whose scope is a prefix of the target path.
pub fn load_rules(root: &Path) -> Result<Vec<AgentsMdRule>, AgentsMdError> {
    let mut rules = Vec::new();

    for md in discover_agents_md(root)? {
        for section in &md.sections {
            for item in &section.items {
                rules.push(AgentsMdRule {
                    scope: md.scope.clone(),
                    heading: section.heading.clone(),
                    text: item.clone(),
                });
            }
        }
    }
    Ok(rules)
}

// ─── Parsing helpers ─────────────────────────────────────────────────────────

/// If `line` is a heading (`#{1,6} text`), return (level, text).
fn heading_of(line: &str) -> Option<(u8, &str)> {
    let trimmed = line.trim_start();
    let markers = trimmed.chars().take_while(|c| *c == '#').count();
    if markers == 0 || markers > 6 {
        return None;
    }
    let rest = trimmed[markers..].trim();
    if rest.is_empty() {
        return None;
    }
    Some((markers as u8, rest))
}

/// If `line` is a list item (`- `, `* `, `+ `, `N. `) or an indented
/// continuation line, return the item text (indentation preserved for
/// sub-bullets).
fn list_item_of(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            if !rest.trim().is_empty() {
                return Some(rest.trim());
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix('+') {
        if rest.starts_with(' ') {
            let text = rest.trim_start();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    // Numbered items: `1. text`
    if let Some((num, text)) = trimmed.split_once(". ") {
        if !num.is_empty()
            && num.chars().all(|c| c.is_ascii_digit())
            && !text.trim().is_empty()
        {
            return Some(text.trim());
        }
    }
    // Indented continuation lines under a bullet (sub-items).
    if indent > 0 && !trimmed.starts_with('#') {
        return Some(trimmed);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"# Sentinel Agent

## Development Practices

- Run `cargo test -p sentinel-gpu-profiler` after any change
- Run `cargo check --workspace`
1. Keep commits small

## Security

- Never commit secrets
  - Never log API keys
- Prefer least-privilege tokens
"#;

    #[test]
    fn parses_headings_and_items() {
        let md = parse_agents_md(SAMPLE);
        assert_eq!(md.sections.len(), 3);
        assert_eq!(md.sections[0].heading, "Sentinel Agent");
        assert_eq!(md.sections[0].level, 1);
        assert_eq!(md.sections[1].heading, "Development Practices");
        assert_eq!(md.sections[1].level, 2);
        assert_eq!(md.sections[1].items.len(), 3);
        assert!(md.sections[1].items[0].contains("cargo test"));
        assert_eq!(md.sections[2].heading, "Security");
        // Sub-bullet preserved as an item.
        assert!(md.sections[2].items.iter().any(|i| i.contains("Never log API keys")));
    }

    #[test]
    fn ignores_code_fences() {
        let md = parse_agents_md("## Rules\n\n```\n- not an item\n```\n\n- real item\n");
        assert_eq!(md.sections[0].items, vec!["real item".to_string()]);
    }

    #[test]
    fn preamble_items_without_heading() {
        let md = parse_agents_md("- orphan item\n\n## Later\n\n- second\n");
        assert_eq!(md.sections[0].heading, "Preamble");
        assert_eq!(md.sections[0].items, vec!["orphan item".to_string()]);
        assert_eq!(md.sections[1].heading, "Later");
    }

    #[test]
    fn load_and_discover_hierarchy() {
        let root = std::env::temp_dir().join(format!(
            "sentinel_agents_md_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("crates/core")).unwrap();
        fs::create_dir_all(root.join("packages/desktop-app")).unwrap();
        fs::write(root.join("AGENTS.md"), "## Root\n\n- root rule\n").unwrap();
        fs::write(root.join("crates/AGENTS.md"), "## Crates\n\n- crate rule\n").unwrap();
        fs::write(root.join("packages/desktop-app/AGENTS.md"), "## UI\n\n- ui rule\n").unwrap();

        let rules = load_rules(&root).expect("load hierarchy");
        assert_eq!(rules.len(), 3);
        // Shallowest first.
        assert_eq!(rules[0].scope, ".");
        assert_eq!(rules[0].text, "root rule");
        assert_eq!(rules[1].scope, "crates");
        assert_eq!(rules[2].scope, "packages/desktop-app");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_file_errors() {
        let err = load_agents_md(Path::new("definitely/not/here/AGENTS.md")).unwrap_err();
        assert!(err.to_string().contains("failed to read"));
    }
}

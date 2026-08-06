use std::collections::HashMap;

/// Where a prompt section belongs in the assembled prompt.
///
/// Dispatch is centralized here instead of being spread across call sites:
/// builders register a section under an id with a role, and the renderer
/// routes content by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptRole {
    /// Content merged into the system prompt (agent instructions, project
    /// context, diagnostics, memory).
    System,
    /// Content attached to the current user turn.
    User,
    /// Content materialized on demand (tool output, LSP diagnostics for a
    /// specific file).
    ToolContext,
}

/// One registered prompt section: stable id, dispatch role, rendered content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSection {
    /// Stable id such as `project-context`, `agents-md`, `ide-context`.
    pub id: String,
    /// Role the renderer dispatches on.
    pub role: PromptRole,
    /// Rendered markdown content for the section.
    pub content: String,
}

impl PromptSection {
    pub fn new(id: impl Into<String>, role: PromptRole, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            role,
            content: content.into(),
        }
    }
}

/// Registry of prompt sections. Builders register sections with a role; the
/// registry renders each role into its slot. Registration is ordered, and
/// rendering preserves that order within a role.
#[derive(Debug, Clone, Default)]
pub struct PromptRegistry {
    sections: Vec<PromptSection>,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self { sections: Vec::new() }
    }

    /// Register a section for later rendering.
    pub fn register(&mut self, section: PromptSection) {
        self.sections.push(section);
    }

    /// Query the role a registered id dispatches to.
    pub fn role_of(&self, id: &str) -> Option<PromptRole> {
        self.sections
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.role)
    }

    pub fn get(&self, id: &str) -> Option<&PromptSection> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// Whether a section id is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.role_of(id).is_some()
    }

    pub fn sections_by_role<'a>(&'a self, role: PromptRole) -> Vec<&'a PromptSection> {
        self.sections.iter().filter(|s| s.role == role).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sections.len()
    }

    /// Render all [`PromptRole::System`] sections concatenated (in
    /// registration order), joined by blank lines.
    pub fn render_system(&self) -> String {
        self.sections_by_role(PromptRole::System)
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Render all [`PromptRole::User`] sections (injected over the current
    /// user turn).
    pub fn render_user(&self) -> String {
        self.sections_by_role(PromptRole::User)
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Render all [`PromptRole::ToolContext`] sections (materialized on
    /// demand, e.g. when a file with diagnostics is relevant).
    pub fn render_tool_context(&self) -> String {
        self.sections_by_role(PromptRole::ToolContext)
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Render a base system prompt with registered system sections appended.
pub fn render_system_prompt(base_prompt: &str, registry: &PromptRegistry) -> String {
    let system = registry.render_system();
    if system.is_empty() {
        base_prompt.to_string()
    } else {
        format!("{}\n\n{}", base_prompt.trim_end(), system)
    }
}

#[derive(Debug, Clone)]
pub struct SystemPromptManager {
    base_prompt: String,
    variables: HashMap<String, String>,
}

impl SystemPromptManager {
    pub fn new() -> Self {
        Self {
            base_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            variables: HashMap::new(),
        }
    }

    pub fn with_base(mut self, prompt: impl Into<String>) -> Self {
        self.base_prompt = prompt.into();
        self
    }

    pub fn set_variable(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(key.into(), value.into());
    }

    pub fn remove_variable(&mut self, key: &str) {
        self.variables.remove(key);
    }

    pub fn set_base(&mut self, prompt: impl Into<String>) {
        self.base_prompt = prompt.into();
    }

    pub fn render(&self) -> String {
        let mut result = self.base_prompt.clone();
        for (key, value) in &self.variables {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        result
    }

    pub fn base(&self) -> &str {
        &self.base_prompt
    }

    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }
}

impl Default for SystemPromptManager {
    fn default() -> Self {
        Self::new()
    }
}

pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are Sentinel, a coding agent. You help users with software engineering tasks.

You have access to tools that let you read, write, and edit files, execute commands, search code, and search the web.

When you need to use a tool, respond with a tool call. When you have completed the task, provide a summary of what you did.

Guidelines:
- Read files before editing them to understand their content
- Run tests after making changes to verify correctness
- Ask for clarification when instructions are ambiguous
- Use the bash tool for running commands, building, testing
- Use web_search for finding information"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_dispatches_roles_by_id() {
        let mut reg = PromptRegistry::new();
        reg.register(PromptSection::new(
            "project-context",
            PromptRole::System,
            "## Project Context\n- cwd",
        ));
        reg.register(PromptSection::new(
            "active-file",
            PromptRole::ToolContext,
            "## Active File\n- src/main.rs",
        ));
        reg.register(PromptSection::new(
            "turn-hint",
            PromptRole::User,
            "> The user is in the editor.",
        ));

        assert_eq!(reg.role_of("project-context"), Some(PromptRole::System));
        assert_eq!(reg.role_of("active-file"), Some(PromptRole::ToolContext));
        assert_eq!(reg.role_of("turn-hint"), Some(PromptRole::User));
        assert_eq!(reg.role_of("missing"), None);
        assert!(reg.contains("active-file"));
        assert!(!reg.contains("missing"));
        assert_eq!(reg.len(), 3);
    }

    #[test]
    fn render_system_appends_in_registration_order() {
        let mut reg = PromptRegistry::new();
        reg.register(PromptSection::new(
            "a",
            PromptRole::System,
            "## A\n- one",
        ));
        reg.register(PromptSection::new(
            "b",
            PromptRole::System,
            "## B\n- two",
        ));
        reg.register(PromptSection::new("c", PromptRole::User, "user note"));

        let rendered = reg.render_system();
        let a = rendered.find("## A").unwrap();
        let b = rendered.find("## B").unwrap();
        assert!(a < b, "registration order must hold");
        assert!(!rendered.contains("user note"));
    }

    #[test]
    fn render_system_prompt_merges_base_and_system_sections() {
        let mut reg = PromptRegistry::new();
        reg.register(PromptSection::new(
            "ctx",
            PromptRole::System,
            "## Context\n- cwd",
        ));
        let out = render_system_prompt("You are Sentinel.", &reg);
        assert!(out.starts_with("You are Sentinel."));
        assert!(out.contains("## Context"));
        assert!(out.contains("- cwd"));

        let plain = render_system_prompt("You are Sentinel.", &PromptRegistry::new());
        assert_eq!(plain, "You are Sentinel.");
    }

    #[test]
    fn role_renderers_are_isolated() {
        let mut reg = PromptRegistry::new();
        reg.register(PromptSection::new("s", PromptRole::System, "sys"));
        reg.register(PromptSection::new("u", PromptRole::User, "usr"));
        reg.register(PromptSection::new("t", PromptRole::ToolContext, "tool"));
        assert_eq!(reg.render_system(), "sys");
        assert_eq!(reg.render_user(), "usr");
        assert_eq!(reg.render_tool_context(), "tool");
    }
}

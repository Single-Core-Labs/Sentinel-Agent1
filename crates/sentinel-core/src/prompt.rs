use std::collections::HashMap;

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

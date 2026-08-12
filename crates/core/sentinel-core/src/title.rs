//! Title generation prompt (`TitlePrompt` equivalent).
//!
//! The spec dispatches a "title" prompt to a dedicated generator that turns
//! the opening user message into a concise conversation title. [`title_prompt`]
//! builds the user instruction; [`TITLE_SYSTEM_PROMPT`] constrains the output.
//! Generation is best-effort — callers keep their first-message heuristic as a
//! fallback when the LLM call fails.

/// System prompt for the title generator.
pub const TITLE_SYSTEM_PROMPT: &str = "You are a conversation title generator.

Given the first user message of a coding-assistant session, produce a concise title of at most 6 words that captures the task.

Reply with the title only: no quotes, no trailing punctuation, no explanation.";

/// User instruction for the title generator.
pub fn title_prompt(user_input: &str) -> String {
    format!(
        "Create a short title for this session.\n\nFirst user message:\n{}",
        user_input.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_constrains_output() {
        assert!(TITLE_SYSTEM_PROMPT.contains("title"));
        assert!(TITLE_SYSTEM_PROMPT.contains("6 words"));
    }

    #[test]
    fn user_prompt_embeds_the_message() {
        let p = title_prompt("Refactor the auth module to use async/await");
        assert!(p.contains("Refactor the auth module"));
        assert!(p.contains("First user message"));
    }

    #[test]
    fn user_prompt_trims_whitespace() {
        let p = title_prompt("  hello world  \n");
        assert!(!p.contains("\n\nhello"));
        assert!(p.contains("hello world"));
    }
}

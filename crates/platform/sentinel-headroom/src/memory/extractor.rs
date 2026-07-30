use std::sync::Arc;

use super::types::*;
use super::embeddings::text_to_vector;
use super::store::MemoryStore;
use crate::config::Message;

#[derive(Clone)]
pub struct ExtractionConfig {
    pub min_content_length: usize,
    pub max_memories_per_extraction: usize,
    pub min_importance_for_compaction: f64,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            min_content_length: 10,
            max_memories_per_extraction: 10,
            min_importance_for_compaction: 0.2,
        }
    }
}

pub fn parse_inline_memory_blocks(response: &str) -> Vec<ParsedMemory> {
    let mut memories = Vec::new();
    let re = regex::Regex::new(
        r#"(?is)<memory\s*(?:category=["']([^"']+)["']\s*)?>([^<]+)</memory>"#
    ).expect("valid memory regex");

    for cap in re.captures_iter(response) {
        let category_str = cap.get(1).map(|m| m.as_str()).unwrap_or("fact");
        let content = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");

        if content.len() < 5 || content.len() > 2000 {
            continue;
        }

        memories.push(ParsedMemory {
            content: content.to_string(),
            category: MemoryCategory::from_str(category_str),
            importance: infer_importance(content),
        });
    }

    memories
}

pub fn parse_memory_blocks(response: &str) -> Vec<ParsedMemory> {
    parse_inline_memory_blocks(response)
}

pub struct ParsedMemory {
    pub content: String,
    pub category: MemoryCategory,
    pub importance: f64,
}

fn infer_importance(content: &str) -> f64 {
    let decisive_words = ["always", "never", "prefer", "decided", "chose", "selected", "is", "works", "uses", "runs", "critical"];
    let important_words = ["important", "key", "significant", "major", "primary", "main", "core", "essential"];
    let lower = content.to_lowercase();
    let mut score = 0.5f64;
    for w in &decisive_words {
        if lower.contains(w) { score += 0.1; }
    }
    for w in &important_words {
        if lower.contains(w) { score += 0.05; }
    }
    if content.len() > 100 { score += 0.05; }
    score.min(1.0)
}

pub async fn extract_from_dropped_messages(
    dropped_messages: &[Message],
    store: &Arc<dyn MemoryStore>,
    user_id: &str,
    session_id: Option<&str>,
    source_turn: u32,
    config: &ExtractionConfig,
) -> crate::memory::Result<Vec<Memory>> {
    let mut extracted = Vec::new();

    for msg in dropped_messages {
        if msg.content.len() < config.min_content_length {
            continue;
        }

        let candidate_memories = extract_memories_from_text(&msg.content, config);

        for parsed in candidate_memories {
            if parsed.importance < config.min_importance_for_compaction {
                continue;
            }

            if is_duplicate(&extracted, &parsed.content) {
                continue;
            }

            let now = now_seconds();
            let memory = Memory {
                id: generate_memory_id(),
                user_id: user_id.to_string(),
                session_id: session_id.map(|s| s.to_string()),
                agent_id: None,
                content: parsed.content,
                category: parsed.category,
                importance: parsed.importance,
                scope: MemoryScope::Session,
                supersedes: None,
                superseded_by: None,
                supersede_reason: None,
                source: MemorySource::Compaction,
                source_turn,
                created_at: now,
                updated_at: now,
                accessed_at: now,
                access_count: 0,
            };

            store.add(memory.clone()).await?;
            extracted.push(memory);

            if extracted.len() >= config.max_memories_per_extraction {
                break;
            }
        }

        if extracted.len() >= config.max_memories_per_extraction {
            break;
        }
    }

    Ok(extracted)
}

fn extract_memories_from_text(text: &str, config: &ExtractionConfig) -> Vec<ParsedMemory> {
    let mut memories = Vec::new();

    let fact_patterns = [
        (r"(?i)(?:user\s+)?prefers?\s+(\w[\w\s]*(?:over\s+\w[\w\s]*)?)", MemoryCategory::Preference),
        (r"(?i)(?:user\s+)?(?:works?|is\s+(?:a\s+)?(?:senior|lead|principal|staff|junior)?\s*\w+)\s+(?:at|for|as|with)\s+([\w\s]+)", MemoryCategory::Fact),
        (r"(?i)(?:user\s+)?(?:uses?|chose?|selected?|migrated?\s+to)\s+(\w[\w\s]*)", MemoryCategory::Decision),
        (r"(?i)(?:the\s+)?(?:project|app|service|system|repo|codebase)\s+(?:is|uses|runs?|built\s+(?:with|on|in)|written\s+in)\s+([\w\s]+)", MemoryCategory::Entity),
        (r"(?i)(?:current|main|primary|ongoing)\s+(?:goal|task|focus|priority|work|project)\s+(?:is|:)\s*(.+?)(?:\.|$)", MemoryCategory::Context),
        (r"(?i)(?:key|important|notable|crucial|significant)\s+(?:insight|observation|finding|takeaway|lesson):\s*(.+?)(?:\.|$)", MemoryCategory::Insight),
    ];

    for (pattern, category) in &fact_patterns {
        let re = regex::Regex::new(pattern).expect("valid fact pattern");
        for cap in re.captures_iter(text) {
            if let Some(extracted) = cap.get(1) {
                let content = extracted.as_str().trim();
                if content.len() >= config.min_content_length {
                    let full = format!("{}: {}", category.as_str(), content);
                    memories.push(ParsedMemory {
                        content: full,
                        category: category.clone(),
                        importance: infer_importance(content),
                    });
                }
            }
        }
    }

    memories.truncate(config.max_memories_per_extraction);
    memories
}

fn is_duplicate(existing: &[Memory], new_content: &str) -> bool {
    let new_vec = text_to_vector(new_content);
    existing.iter().any(|m| {
        let existing_vec = text_to_vector(&m.content);
        let sim = super::embeddings::cosine_similarity(&new_vec, &existing_vec);
        sim > 0.85
    })
}

pub fn strip_memory_blocks(response: &str) -> String {
    let re = regex::Regex::new(r"(?is)<memory[^>]*>[^<]*</memory>\s*").expect("valid regex");
    let cleaned = re.replace_all(response, "");
    cleaned.trim().to_string()
}

pub fn inject_memory_instruction(system_prompt: &str) -> String {
    let instruction = "\n\nYou have the ability to persist important facts about the user. \
        When you learn something significant about the user's preferences, identity, \
        ongoing work, entities, decisions, or insights, embed a memory block in your response:\n\
        <memory category=\"preference|fact|context|entity|decision|insight\">content</memory>\n\
        Only store concise, factual information. Prefer category=\"preference\" for likes/dislikes, \
        \"fact\" for identity/role, \"context\" for current goals, \"entity\" for project details, \
        \"decision\" for choices made, \"insight\" for derived observations.";

    if system_prompt.contains("<memory") {
        system_prompt.to_string()
    } else {
        format!("{}{}", system_prompt, instruction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_with_category() {
        let text = r#"I'll use Python for this project. <memory category="preference">User prefers Python for backend work</memory>"#;
        let memories = parse_inline_memory_blocks(text);
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].content, "User prefers Python for backend work");
        assert_eq!(memories[0].category, MemoryCategory::Preference);
    }

    #[test]
    fn test_parse_memory_without_category() {
        let text = "I see. <memory>User works at a startup</memory>";
        let memories = parse_inline_memory_blocks(text);
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].category, MemoryCategory::Fact);
    }

    #[test]
    fn test_parse_multiple_memories() {
        let text = r#"<memory category="preference">Likes Go</memory><memory category="fact">Senior engineer</memory>"#;
        let memories = parse_inline_memory_blocks(text);
        assert_eq!(memories.len(), 2);
    }

    #[test]
    fn test_parse_short_content_ignored() {
        let text = "<memory>ab</memory>";
        let memories = parse_inline_memory_blocks(text);
        assert_eq!(memories.len(), 0);
    }

    #[test]
    fn test_strip_memory_blocks() {
        let text = "Hello <memory category=\"fact\">test</memory> World";
        let cleaned = strip_memory_blocks(text);
        assert_eq!(cleaned, "Hello World");
    }

    #[test]
    fn test_importance_inference() {
        let high = infer_importance("User always prefers Rust for performance-critical code");
        let mid = infer_importance("User works at a company");
        assert!(high > mid);
    }

    #[test]
    fn test_inject_instruction_not_duplicated() {
        let already = "You are helpful. <memory category=\"fact\">test</memory>";
        let result = inject_memory_instruction(already);
        assert_eq!(result, already);
    }

    #[test]
    fn test_extract_facts_from_text() {
        let text = "User prefers Python for backend development. User works at fintech startup.";
        let config = ExtractionConfig::default();
        let results = extract_memories_from_text(text, &config);
        assert!(!results.is_empty());
        assert!(results.iter().any(|m| matches!(m.category, MemoryCategory::Preference)));
    }

    #[test]
    fn test_empty_response_yields_no_memories() {
        let memories = parse_inline_memory_blocks("");
        assert!(memories.is_empty());
    }
}

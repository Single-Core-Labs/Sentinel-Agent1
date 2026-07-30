use std::sync::Arc;

use super::types::*;
use super::store::MemoryStore;

#[derive(Clone)]
pub struct InjectionConfig {
    pub max_memories: usize,
    pub min_score: f64,
    pub recency_weight: f64,
    pub importance_weight: f64,
    pub relevance_weight: f64,
    pub format_as_system: bool,
    pub always_include_count: bool,
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            max_memories: 5,
            min_score: 0.15,
            recency_weight: 0.1,
            importance_weight: 0.3,
            relevance_weight: 0.6,
            format_as_system: false,
            always_include_count: true,
        }
    }
}

pub struct MemoryInjector {
    store: Arc<dyn MemoryStore>,
    config: InjectionConfig,
}

impl MemoryInjector {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self {
            store,
            config: InjectionConfig::default(),
        }
    }

    pub fn with_config(store: Arc<dyn MemoryStore>, config: InjectionConfig) -> Self {
        Self {
            store,
            config,
        }
    }

    pub async fn retrieve(&self, query: &str, user_id: &str, session_id: Option<&str>) -> crate::memory::Result<Vec<ScoredMemory>> {
        let mut filter = MemoryFilter::for_user(user_id);
        filter.limit = self.config.max_memories * 3;
        if let Some(sid) = session_id {
            filter.session_id = Some(sid.to_string());
        }
        let mut results = self.store.search(query, &filter).await?;

        let now = now_seconds();
        for sm in &mut results {
            let relevance = sm.relevance;
            let importance = sm.memory.importance;
            let age_days = (now - sm.memory.updated_at) as f64 / 86400.0;
            let recency = (-age_days / 14.0).exp();

            sm.score = relevance * self.config.relevance_weight
                + importance * self.config.importance_weight
                + recency * self.config.recency_weight;
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.retain(|sm| sm.score >= self.config.min_score);
        results.truncate(self.config.max_memories);
        Ok(results)
    }

    pub async fn retrieve_by_category(
        &self, user_id: &str, categories: &[MemoryCategory], limit: usize,
    ) -> crate::memory::Result<Vec<ScoredMemory>> {
        let filter = MemoryFilter {
            user_id: Some(user_id.to_string()),
            categories: Some(categories.to_vec()),
            include_superseded: false,
            limit,
            ..Default::default()
        };
        let results = self.store.search("", &filter).await?;
        Ok(results)
    }

    pub fn format_memories(&self, memories: &[ScoredMemory]) -> Option<String> {
        if memories.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        for sm in memories {
            let cat = sm.memory.category.as_str();
            parts.push(format!("[{}] {}", cat, sm.memory.content));
        }
        let mut result = parts.join("\n");
        if self.config.always_include_count && memories.len() > 1 {
            result = format!("{} ({} remembered facts)", result, memories.len());
        }
        Some(result)
    }

    pub fn format_as_system_block(&self, memories: &[ScoredMemory]) -> Option<String> {
        if memories.is_empty() {
            return None;
        }
        let formatted = self.format_memories(memories)?;
        let memory_count = memories.len();
        Some(format!(
            "\n\n## Known Facts ({})\n{}\n",
            memory_count, formatted,
        ))
    }

    pub async fn inject_into_system_prompt(&self, system_prompt: &str, user_id: &str, session_id: Option<&str>) -> String {
        let memories = self.retrieve(system_prompt, user_id, session_id).await.unwrap_or_default();
        match self.format_as_system_block(&memories) {
            Some(block) => {
                let marker = "<!-- KNOWN_FACTS -->";
                if system_prompt.contains(marker) {
                    let re = regex::Regex::new(r"(?s)<!-- KNOWN_FACTS -->.*?(?:-->|$)").expect("valid regex");
                    re.replace(system_prompt, format!("{}", block)).to_string()
                } else {
                    format!("{}\n{}", system_prompt, block)
                }
            }
            None => system_prompt.to_string(),
        }
    }

    pub fn config(&self) -> &InjectionConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::store::InMemoryStore;

    fn create_test_injector() -> (Arc<dyn MemoryStore>, MemoryInjector) {
        let store = Arc::new(InMemoryStore::new());
        let injector = MemoryInjector::new(store.clone());
        (store, injector)
    }

    fn test_memory(user_id: &str, content: &str, category: MemoryCategory, importance: f64) -> Memory {
        let now = now_seconds();
        Memory {
            id: generate_memory_id(),
            user_id: user_id.to_string(),
            session_id: None,
            agent_id: None,
            content: content.to_string(),
            category,
            importance,
            scope: MemoryScope::User,
            supersedes: None,
            superseded_by: None,
            supersede_reason: None,
            source: MemorySource::Manual,
            source_turn: 0,
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
        }
    }

    #[tokio::test]
    async fn test_retrieve_relevant() {
        let (store, injector) = create_test_injector();
        store.add(test_memory("alice", "Prefers Python for backend work", MemoryCategory::Preference, 0.7)).await.unwrap();
        store.add(test_memory("alice", "Works at a startup", MemoryCategory::Fact, 0.5)).await.unwrap();

        let results = injector.retrieve("python backend", "alice", None).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].memory.content.contains("Python"));
    }

    #[tokio::test]
    async fn test_retrieve_empty_when_no_relevant() {
        let (_, injector) = create_test_injector();
        let results = injector.retrieve("something", "nobody", None).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_format_memories() {
        let injector = MemoryInjector::new(Arc::new(InMemoryStore::new()));
        let memories = vec![
            ScoredMemory {
                memory: Memory {
                    content: "Likes Python".to_string(),
                    category: MemoryCategory::Preference,
                    ..test_memory("u", "Likes Python", MemoryCategory::Preference, 0.5)
                },
                score: 0.8,
                relevance: 0.7,
            }
        ];
        let formatted = injector.format_memories(&memories);
        assert!(formatted.unwrap().contains("[preference]"));
    }

    #[test]
    fn test_format_empty_returns_none() {
        let injector = MemoryInjector::new(Arc::new(InMemoryStore::new()));
        assert!(injector.format_memories(&[]).is_none());
    }

    #[tokio::test]
    async fn test_inject_into_system_prompt() {
        let (store, injector) = create_test_injector();
        store.add(test_memory("alice", "Likes Rust", MemoryCategory::Preference, 0.8)).await.unwrap();

        let prompt = injector.inject_into_system_prompt("You are a helpful assistant.", "alice", None).await;
        assert!(prompt.contains("Likes Rust"));
        assert!(prompt.contains("You are a helpful assistant."));
    }

    #[tokio::test]
    async fn test_inject_into_empty_no_memories() {
        let (_, injector) = create_test_injector();
        let prompt = injector.inject_into_system_prompt("Hello", "nobody", None).await;
        assert_eq!(prompt, "Hello");
    }

    #[tokio::test]
    async fn test_retrieve_by_category() {
        let (store, injector) = create_test_injector();
        store.add(test_memory("alice", "Likes Go", MemoryCategory::Preference, 0.6)).await.unwrap();
        store.add(test_memory("alice", "Works at Co", MemoryCategory::Fact, 0.6)).await.unwrap();

        let prefs = injector.retrieve_by_category("alice", &[MemoryCategory::Preference], 10).await.unwrap();
        assert_eq!(prefs.len(), 1);
        assert!(prefs[0].memory.content.contains("Go"));
    }
}

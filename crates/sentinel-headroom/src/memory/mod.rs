pub mod types;
pub mod embeddings;
pub mod store;
pub mod extractor;
pub mod injector;
pub mod tool;
pub mod config;

use std::sync::Arc;
use thiserror::Error;

pub use types::*;
pub use store::*;
pub use extractor::*;
pub use injector::*;
pub use config::MemoryConfig;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Store error: {0}")]
    Store(String),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Memory not found: {0}")]
    NotFound(String),

    #[error("Lock error: {0}")]
    LockError(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

pub struct PersistentMemory {
    store: Arc<dyn MemoryStore>,
    injector: Arc<MemoryInjector>,
    config: MemoryConfig,
}

impl PersistentMemory {
    pub fn new(store: Arc<dyn MemoryStore>, config: MemoryConfig) -> Self {
        let injector = Arc::new(MemoryInjector::new(store.clone()));
        Self { store, injector, config }
    }

    pub fn with_config(store: Arc<dyn MemoryStore>, injector_config: InjectionConfig, config: MemoryConfig) -> Self {
        let injector = Arc::new(MemoryInjector::with_config(store.clone(), injector_config));
        Self { store, injector, config }
    }

    pub fn store(&self) -> &Arc<dyn MemoryStore> {
        &self.store
    }

    pub fn injector(&self) -> &Arc<MemoryInjector> {
        &self.injector
    }

    pub fn config(&self) -> &MemoryConfig {
        &self.config
    }

    pub async fn add_memory(
        &self, content: &str, category: MemoryCategory, importance: f64,
        user_id: &str, scope: MemoryScope, source: MemorySource, source_turn: u32,
    ) -> Result<Memory> {
        let now = now_seconds();
        let memory = Memory {
            id: generate_memory_id(),
            user_id: user_id.to_string(),
            session_id: None,
            agent_id: None,
            content: content.to_string(),
            category,
            importance: importance.clamp(0.0, 1.0),
            scope,
            supersedes: None,
            superseded_by: None,
            supersede_reason: None,
            source,
            source_turn,
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
        };
        self.store.add(memory).await
    }

    pub async fn search(&self, query: &str, user_id: &str) -> Result<Vec<ScoredMemory>> {
        let filter = MemoryFilter::for_user(user_id);
        self.store.search(query, &filter).await
    }

    pub async fn supersede(&self, old_id: &str, new_content: &str, reason: &str) -> Result<Memory> {
        self.store.supersede(old_id, new_content, reason).await
    }

    pub async fn process_response(&self, response: &str, user_id: &str, session_id: Option<&str>, turn: u32) -> (String, Vec<Memory>) {
        if !self.config.inline_extraction {
            return (response.to_string(), Vec::new());
        }

        let parsed = parse_inline_memory_blocks(response);
        if parsed.is_empty() {
            return (response.to_string(), Vec::new());
        }

        let cleaned = strip_memory_blocks(response);
        let mut stored = Vec::new();

        for pm in parsed {
            let now = now_seconds();
            let memory = Memory {
                id: generate_memory_id(),
                user_id: user_id.to_string(),
                session_id: session_id.map(|s| s.to_string()),
                agent_id: None,
                content: pm.content,
                category: pm.category,
                importance: pm.importance,
                scope: MemoryScope::User,
                supersedes: None,
                superseded_by: None,
                supersede_reason: None,
                source: MemorySource::InlineExtraction,
                source_turn: turn,
                created_at: now,
                updated_at: now,
                accessed_at: now,
                access_count: 0,
            };

            if let Ok(stored_memory) = self.store.add(memory).await {
                stored.push(stored_memory);
            }
        }

        (cleaned, stored)
    }

    pub async fn extract_from_dropped(
        &self, dropped: &[crate::config::Message],
        user_id: &str, session_id: Option<&str>, turn: u32,
    ) -> Result<Vec<Memory>> {
        if !self.config.compaction_extraction {
            return Ok(Vec::new());
        }
        extract_from_dropped_messages(
            dropped, &self.store, user_id, session_id, turn, &self.config.extraction,
        ).await
    }

    pub async fn inject_memories(&self, system_prompt: &str, user_id: &str, session_id: Option<&str>) -> String {
        if !self.config.inject_on_every_turn {
            return system_prompt.to_string();
        }
        self.injector.inject_into_system_prompt(system_prompt, user_id, session_id).await
    }

    pub async fn create_tools(&self) -> Vec<Arc<dyn sentinel_tools::Tool>> {
        vec![
            Arc::new(tool::MemorizeTool::new(self.store.clone())),
            Arc::new(tool::RecallTool::new(self.store.clone())),
            Arc::new(tool::ForgetTool::new(self.store.clone())),
            Arc::new(tool::MemoryStatsTool::new(self.store.clone())),
        ]
    }
}

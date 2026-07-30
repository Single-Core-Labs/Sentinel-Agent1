use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryCategory {
    Preference,
    Fact,
    Context,
    Entity,
    Decision,
    Insight,
}

impl MemoryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryCategory::Preference => "preference",
            MemoryCategory::Fact => "fact",
            MemoryCategory::Context => "context",
            MemoryCategory::Entity => "entity",
            MemoryCategory::Decision => "decision",
            MemoryCategory::Insight => "insight",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "preference" => MemoryCategory::Preference,
            "fact" => MemoryCategory::Fact,
            "context" => MemoryCategory::Context,
            "entity" => MemoryCategory::Entity,
            "decision" => MemoryCategory::Decision,
            "insight" => MemoryCategory::Insight,
            _ => MemoryCategory::Fact,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryScope {
    User,
    Session,
    Agent,
    Turn,
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryScope::User => "user",
            MemoryScope::Session => "session",
            MemoryScope::Agent => "agent",
            MemoryScope::Turn => "turn",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemorySource {
    InlineExtraction,
    ToolCall,
    Compaction,
    Manual,
}

impl MemorySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemorySource::InlineExtraction => "inline",
            MemorySource::ToolCall => "tool",
            MemorySource::Compaction => "compaction",
            MemorySource::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub user_id: String,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub content: String,
    pub category: MemoryCategory,
    pub importance: f64,
    pub scope: MemoryScope,
    pub supersedes: Option<String>,
    pub superseded_by: Option<String>,
    pub supersede_reason: Option<String>,
    pub source: MemorySource,
    pub source_turn: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub accessed_at: i64,
    pub access_count: u64,
}

#[derive(Debug, Clone)]
pub struct ScoredMemory {
    pub memory: Memory,
    pub score: f64,
    pub relevance: f64,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryFilter {
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub categories: Option<Vec<MemoryCategory>>,
    pub scopes: Option<Vec<MemoryScope>>,
    pub min_importance: Option<f64>,
    pub include_superseded: bool,
    pub limit: usize,
    pub offset: usize,
}

impl MemoryFilter {
    pub fn for_user(user_id: &str) -> Self {
        Self {
            user_id: Some(user_id.to_string()),
            include_superseded: false,
            limit: 100,
            ..Default::default()
        }
    }

    pub fn for_session(user_id: &str, session_id: &str) -> Self {
        Self {
            user_id: Some(user_id.to_string()),
            session_id: Some(session_id.to_string()),
            include_superseded: false,
            limit: 100,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total: usize,
    pub active: usize,
    pub superseded: usize,
    pub by_category: Vec<(MemoryCategory, usize)>,
    pub by_scope: Vec<(MemoryScope, usize)>,
    pub by_source: Vec<(MemorySource, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySupersession {
    pub chain: Vec<Memory>,
    pub current: Option<Memory>,
}

pub fn generate_memory_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

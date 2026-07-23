use crate::classifier::ContentType;

#[derive(Clone)]
pub struct HeadroomConfig {
    pub cache_alignment: CacheAlignmentConfig,
    pub content_routing: ContentRoutingConfig,
    pub intelligent_context: IntelligentContextConfig,
    pub ccr: CcrConfig,
}

impl HeadroomConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for HeadroomConfig {
    fn default() -> Self {
        Self {
            cache_alignment: CacheAlignmentConfig::default(),
            content_routing: ContentRoutingConfig::default(),
            intelligent_context: IntelligentContextConfig::default(),
            ccr: CcrConfig::default(),
        }
    }
}

#[derive(Clone)]
pub struct CacheAlignmentConfig {
    pub enabled: bool,
    pub extract_dates: bool,
    pub extract_file_paths: bool,
    pub extract_uuids: bool,
    pub extract_versions: bool,
    pub extract_user_context: bool,
    pub delta_tracking: bool,
}

impl Default for CacheAlignmentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extract_dates: true,
            extract_file_paths: true,
            extract_uuids: true,
            extract_versions: true,
            extract_user_context: true,
            delta_tracking: true,
        }
    }
}

#[derive(Clone)]
pub struct ContentRoutingConfig {
    pub enabled_types: Vec<ContentType>,
    pub min_savings_pct: f64,
    pub max_content_chars: usize,
    pub min_content_chars: usize,
    pub parallel_strategies: bool,
    pub compression_ratio_target: f64,
    pub entropy_preservation: bool,
}

impl Default for ContentRoutingConfig {
    fn default() -> Self {
        Self {
            enabled_types: vec![
                ContentType::Json, ContentType::JsonArray,
                ContentType::SourceCode, ContentType::BuildLog,
                ContentType::SearchResults, ContentType::GitDiff,
                ContentType::PlainText, ContentType::Image,
                ContentType::Html,
            ],
            min_savings_pct: 15.0,
            max_content_chars: 1_000_000,
            min_content_chars: 200,
            parallel_strategies: true,
            compression_ratio_target: 0.5,
            entropy_preservation: true,
        }
    }
}

#[derive(Clone)]
pub struct IntelligentContextConfig {
    pub enabled: bool,
    pub token_budget: usize,
    pub recency_weight: f64,
    pub relevance_weight: f64,
    pub error_weight: f64,
    pub dependency_weight: f64,
    pub preserve_errors: bool,
    pub preserve_tool_dependencies: bool,
    pub recency_decay_half_life: f64,
}

impl Default for IntelligentContextConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            token_budget: 128_000,
            recency_weight: 0.4,
            relevance_weight: 0.3,
            error_weight: 0.2,
            dependency_weight: 0.1,
            preserve_errors: true,
            preserve_tool_dependencies: true,
            recency_decay_half_life: 5.0,
        }
    }
}

#[derive(Clone)]
pub struct CcrConfig {
    pub enabled: bool,
    pub max_entries: usize,
    pub default_ttl_secs: u64,
    pub inject_tool: bool,
    pub inject_retrieval_marker: bool,
    pub feedback_enabled: bool,
    pub store_max_entries: usize,
    pub store_ttl_seconds: u64,
}

impl Default for CcrConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 5000,
            default_ttl_secs: 3600,
            inject_tool: true,
            inject_retrieval_marker: true,
            feedback_enabled: true,
            store_max_entries: 5000,
            store_ttl_seconds: 3600,
        }
    }
}

#[derive(Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

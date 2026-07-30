use crate::classifier::ContentType;

#[derive(Clone)]
pub struct HeadroomConfig {
    pub cache_alignment: CacheAlignmentConfig,
    pub cache_optimizer: CacheOptimizerConfig,
    pub content_routing: ContentRoutingConfig,
    pub intelligent_context: IntelligentContextConfig,
    pub ccr: CcrConfig,
    pub memory: crate::memory::MemoryConfig,
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
            cache_optimizer: CacheOptimizerConfig::default(),
            content_routing: ContentRoutingConfig::default(),
            intelligent_context: IntelligentContextConfig::default(),
            ccr: CcrConfig::default(),
            memory: crate::memory::MemoryConfig::default(),
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
    pub normalize_whitespace: bool,
    pub collapse_blank_lines: bool,
    pub custom_patterns: Vec<String>,
    pub date_patterns: Vec<String>,
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
            normalize_whitespace: true,
            collapse_blank_lines: true,
            custom_patterns: Vec::new(),
            date_patterns: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct CacheOptimizerConfig {
    pub enabled: bool,
    pub auto_detect_provider: bool,
    pub force_provider: crate::cache_optimizer::LlmProvider,
    pub min_cacheable_tokens: usize,
}

impl Default for CacheOptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_detect_provider: true,
            force_provider: crate::cache_optimizer::LlmProvider::Unknown,
            min_cacheable_tokens: 1024,
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
pub struct ScoringWeights {
    pub recency: f64,
    pub semantic_similarity: f64,
    pub toin_importance: f64,
    pub error_indicator: f64,
    pub forward_reference: f64,
    pub token_density: f64,
}

impl ScoringWeights {
    pub fn normalized(&self) -> Vec<f64> {
        let total: f64 = self.recency + self.semantic_similarity + self.toin_importance
            + self.error_indicator + self.forward_reference + self.token_density;
        if total <= 0.0 { return vec![1.0 / 6.0; 6]; }
        vec![
            self.recency / total,
            self.semantic_similarity / total,
            self.toin_importance / total,
            self.error_indicator / total,
            self.forward_reference / total,
            self.token_density / total,
        ]
    }
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            recency: 0.20,
            semantic_similarity: 0.20,
            toin_importance: 0.25,
            error_indicator: 0.15,
            forward_reference: 0.15,
            token_density: 0.05,
        }
    }
}

#[derive(Clone)]
pub struct RollingWindowConfig {
    pub enabled: bool,
    pub keep_system: bool,
    pub keep_last_turns: usize,
    pub output_buffer_tokens: usize,
}

impl Default for RollingWindowConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            keep_system: true,
            keep_last_turns: 3,
            output_buffer_tokens: 4000,
        }
    }
}

#[derive(Clone)]
pub struct IntelligentContextConfig {
    pub enabled: bool,
    pub token_budget: usize,
    pub keep_system: bool,
    pub keep_last_turns: usize,
    pub output_buffer_tokens: usize,
    pub use_importance_scoring: bool,
    pub scoring_weights: ScoringWeights,
    pub toin_integration: bool,
    pub recency_decay_rate: f64,
    pub compress_threshold: f64,
    pub preserve_errors: bool,
    pub preserve_tool_dependencies: bool,
    pub rolling_window: RollingWindowConfig,
}

impl Default for IntelligentContextConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            token_budget: 128_000,
            keep_system: true,
            keep_last_turns: 2,
            output_buffer_tokens: 4000,
            use_importance_scoring: true,
            scoring_weights: ScoringWeights::default(),
            toin_integration: true,
            recency_decay_rate: 0.1,
            compress_threshold: 0.1,
            preserve_errors: true,
            preserve_tool_dependencies: true,
            rolling_window: RollingWindowConfig::default(),
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

#[derive(Debug, Clone)]
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

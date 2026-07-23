use super::extractor::ExtractionConfig;
use super::injector::InjectionConfig;

#[derive(Clone)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub db_path: Option<String>,
    pub user_id: String,
    pub max_memories_per_user: usize,
    pub extraction: ExtractionConfig,
    pub injection: InjectionConfig,
    pub inline_extraction: bool,
    pub compaction_extraction: bool,
    pub tool_extraction: bool,
    pub inject_on_every_turn: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: None,
            user_id: "default".to_string(),
            max_memories_per_user: 500,
            extraction: ExtractionConfig::default(),
            injection: InjectionConfig::default(),
            inline_extraction: true,
            compaction_extraction: true,
            tool_extraction: true,
            inject_on_every_turn: true,
        }
    }
}

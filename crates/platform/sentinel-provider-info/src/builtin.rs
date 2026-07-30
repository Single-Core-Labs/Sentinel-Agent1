use crate::provider::{AuthConfig, ModelEntry, ProviderInfo};
use std::collections::HashMap;

pub fn default_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "openai".into(),
            name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            auth: AuthConfig::EnvKey { var: "OPENAI_API_KEY".into() },
            models: vec![
                ModelEntry { id: "gpt-4o".into(), name: "GPT-4o".into(), context_window: 128000, supports_streaming: true, supports_tools: true },
                ModelEntry { id: "gpt-4o-mini".into(), name: "GPT-4o Mini".into(), context_window: 128000, supports_streaming: true, supports_tools: true },
                ModelEntry { id: "o3-mini".into(), name: "o3 Mini".into(), context_window: 200000, supports_streaming: true, supports_tools: true },
            ],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
        ProviderInfo {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            auth: AuthConfig::EnvKey { var: "ANTHROPIC_API_KEY".into() },
            models: vec![
                ModelEntry { id: "claude-sonnet-4-20250514".into(), name: "Claude Sonnet 4".into(), context_window: 200000, supports_streaming: true, supports_tools: true },
                ModelEntry { id: "claude-haiku-3-5-20241022".into(), name: "Claude Haiku 3.5".into(), context_window: 200000, supports_streaming: true, supports_tools: true },
            ],
            timeout_secs: 180,
            extra_headers: HashMap::new(),
        },
        ProviderInfo {
            id: "google-ai-studio".into(),
            name: "Google AI Studio".into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
            auth: AuthConfig::EnvKey { var: "GOOGLE_API_KEY".into() },
            models: vec![
                ModelEntry { id: "gemini-2.5-flash".into(), name: "Gemini 2.5 Flash".into(), context_window: 1000000, supports_streaming: true, supports_tools: true },
                ModelEntry { id: "gemini-2.5-pro".into(), name: "Gemini 2.5 Pro".into(), context_window: 1000000, supports_streaming: true, supports_tools: true },
            ],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
        ProviderInfo {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com".into(),
            auth: AuthConfig::EnvKey { var: "DEEPSEEK_API_KEY".into() },
            models: vec![
                ModelEntry { id: "deepseek-chat".into(), name: "DeepSeek V3".into(), context_window: 64000, supports_streaming: true, supports_tools: true },
                ModelEntry { id: "deepseek-reasoner".into(), name: "DeepSeek R1".into(), context_window: 64000, supports_streaming: true, supports_tools: false },
            ],
            timeout_secs: 120,
            extra_headers: HashMap::new(),
        },
    ]
}

pub fn local_model_providers() -> Vec<(ProviderInfo, LocalModelConfig)> {
    vec![
        (
            ProviderInfo {
                id: "ollama".into(),
                name: "Ollama".into(),
                base_url: "http://localhost:11434".into(),
                auth: AuthConfig::EnvKey { var: "OLLAMA_API_KEY".into() },
                models: vec![],
                timeout_secs: 300,
                extra_headers: HashMap::new(),
            },
            LocalModelConfig { prefix: "ollama/".into(), base_url_env: "OLLAMA_BASE_URL".into(), api_key_env: "OLLAMA_API_KEY".into(), base_url_default: "http://localhost:11434".into() },
        ),
        (
            ProviderInfo {
                id: "vllm".into(),
                name: "vLLM".into(),
                base_url: "http://localhost:8000".into(),
                auth: AuthConfig::EnvKey { var: "VLLM_API_KEY".into() },
                models: vec![],
                timeout_secs: 300,
                extra_headers: HashMap::new(),
            },
            LocalModelConfig { prefix: "vllm/".into(), base_url_env: "VLLM_BASE_URL".into(), api_key_env: "VLLM_API_KEY".into(), base_url_default: "http://localhost:8000".into() },
        ),
        (
            ProviderInfo {
                id: "lm-studio".into(),
                name: "LM Studio".into(),
                base_url: "http://127.0.0.1:1234".into(),
                auth: AuthConfig::None,
                models: vec![],
                timeout_secs: 300,
                extra_headers: HashMap::new(),
            },
            LocalModelConfig { prefix: "lm-studio/".into(), base_url_env: "LMSTUDIO_BASE_URL".into(), api_key_env: "LMSTUDIO_API_KEY".into(), base_url_default: "http://127.0.0.1:1234".into() },
        ),
        (
            ProviderInfo {
                id: "llamacpp".into(),
                name: "llama.cpp".into(),
                base_url: "http://localhost:8080".into(),
                auth: AuthConfig::None,
                models: vec![],
                timeout_secs: 300,
                extra_headers: HashMap::new(),
            },
            LocalModelConfig { prefix: "llamacpp/".into(), base_url_env: "LLAMACPP_BASE_URL".into(), api_key_env: "LLAMACPP_API_KEY".into(), base_url_default: "http://localhost:8080".into() },
        ),
    ]
}

#[derive(Debug, Clone)]
pub struct LocalModelConfig {
    pub prefix: String,
    pub base_url_env: String,
    pub api_key_env: String,
    pub base_url_default: String,
}

impl LocalModelConfig {
    pub fn resolve_base_url(&self) -> String {
        std::env::var(&self.base_url_env)
            .or_else(|_| std::env::var("LOCAL_LLM_BASE_URL"))
            .unwrap_or_else(|_| self.base_url_default.clone())
    }

    pub fn resolve_api_key(&self) -> String {
        std::env::var(&self.api_key_env)
            .or_else(|_| std::env::var("LOCAL_LLM_API_KEY"))
            .unwrap_or_else(|_| "sk-local-no-key-required".into())
    }

    pub fn strip_prefix<'a>(&self, model_id: &'a str) -> Option<&'a str> {
        model_id.strip_prefix(&self.prefix)
    }
}

pub fn find_local_config(model_id: &str) -> Option<LocalModelConfig> {
    local_model_providers().into_iter().find_map(|(_, config)| {
        if model_id.starts_with(&config.prefix) {
            Some(config)
        } else {
            None
        }
    })
}

pub fn is_local_model_id(model_id: &str) -> bool {
    if model_id.is_empty() || model_id.contains(char::is_whitespace) {
        return false;
    }
    local_model_providers().iter().any(|(_, config)| model_id.starts_with(&config.prefix))
}

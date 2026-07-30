#[derive(Clone)]
pub struct ProxyConfig {
    pub host: String,
    pub port: u16,
    pub openai_api_url: String,
    pub anthropic_api_url: String,
    pub optimize: bool,
    pub cache: bool,
    pub rate_limit: bool,
    pub log_file: Option<String>,
    pub budget: Option<f64>,
    pub llmlingua: bool,
    pub llmlingua_device: String,
    pub llmlingua_rate: f64,
    pub no_telemetry: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8787,
            openai_api_url: "https://api.openai.com".into(),
            anthropic_api_url: "https://api.anthropic.com".into(),
            optimize: true,
            cache: true,
            rate_limit: true,
            log_file: None,
            budget: None,
            llmlingua: false,
            llmlingua_device: "auto".into(),
            llmlingua_rate: 0.3,
            no_telemetry: false,
        }
    }
}

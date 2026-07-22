/// Endpoint: URL construction for provider API requests.
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub base_url: String,
    pub chat_path: String,
}

impl Endpoint {
    pub fn new(base_url: impl Into<String>, chat_path: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            chat_path: chat_path.into(),
        }
    }

    pub fn openai_compatible(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            chat_path: "/v1/chat/completions".into(),
        }
    }

    pub fn anthropic(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            chat_path: "/v1/messages".into(),
        }
    }

    pub fn chat_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = self.chat_path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }
}

impl Default for Endpoint {
    fn default() -> Self {
        Self::openai_compatible("https://api.openai.com")
    }
}

#[async_trait]
pub trait EndpointProvider: Send + Sync {
    fn endpoint(&self) -> &Endpoint;
}

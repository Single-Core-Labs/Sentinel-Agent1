/// Auth: per-request authentication for provider API calls.
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum Auth {
    Bearer { token: String },
    EnvKey { var: String },
    None,
}

impl Auth {
    pub fn from_env(var: &str) -> Option<Self> {
        std::env::var(var).ok().map(|token| Auth::Bearer { token })
    }

    pub fn resolve(&self) -> Option<String> {
        match self {
            Auth::Bearer { token } => Some(token.clone()),
            Auth::EnvKey { var } => std::env::var(var).ok(),
            Auth::None => None,
        }
    }

    pub fn apply(&self, headers: &mut reqwest::header::HeaderMap) {
        if let Some(token) = self.resolve() {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token))
            {
                headers.insert(reqwest::header::AUTHORIZATION, val);
            }
        }
    }
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    fn auth(&self) -> &Auth;
}

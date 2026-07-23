use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("API request failed: {0}")]
    RequestError(String),
    #[error("HTTP client error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("API returned error: {status} - {body}")]
    ApiError { status: u16, body: String },
    #[error("No API key configured for provider {provider}")]
    MissingApiKey { provider: String },
    #[error("Provider not found: {0}")]
    NotFound(String),
    #[error("Stream error: {0}")]
    StreamError(String),
    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("All providers in the router failed")]
    AllProvidersFailed,
    #[error("Rate limited: retry after {retry_after}s")]
    RateLimited { retry_after: u64 },
    #[error("Rate limit exceeded")]
    RateLimitExceeded { retry_after: u64 },
    #[error("Unauthorized: {detail}")]
    Unauthorized { detail: String },
    #[error("Forbidden: {detail}")]
    Forbidden { detail: String },
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Request timed out after {duration}s")]
    Timeout { duration: u64 },
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Server error: {status}")]
    ServerError { status: u16 },
    #[error("Service unavailable")]
    ServiceUnavailable { retry_after: Option<u64> },
}

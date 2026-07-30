use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Error classification for provider failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Permanent failure — model should be marked unavailable.
    Terminal,
    /// Temporary failure — safe to retry with backoff.
    Transient,
    /// Model/endpoint not found.
    NotFound,
    /// Rate limited — retry after a delay.
    RateLimited,
    /// Unknown error kind.
    Unknown,
}

/// Health state for a single model endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelHealth {
    Healthy,
    Degraded,
    Unavailable { reason: String, until: Instant },
}

impl ModelHealth {
    pub fn is_available(&self) -> bool {
        match self {
            Self::Healthy | Self::Degraded => true,
            Self::Unavailable { until, .. } => Instant::now() > *until,
        }
    }
}

/// Tracks health and availability of AI models.
pub struct ModelAvailabilityService {
    models: RwLock<HashMap<String, ModelHealth>>,
    retry_cooldown: Duration,
}

impl ModelAvailabilityService {
    pub fn new(model_names: &[String]) -> Self {
        let mut models = HashMap::new();
        for name in model_names {
            models.insert(name.clone(), ModelHealth::Healthy);
        }
        Self {
            models: RwLock::new(models),
            retry_cooldown: Duration::from_secs(30),
        }
    }

    /// Set cooldown duration for transient unavailable models.
    pub fn with_retry_cooldown(mut self, duration: Duration) -> Self {
        self.retry_cooldown = duration;
        self
    }

    /// Check if a model is currently available.
    pub fn is_available(&self, name: &str) -> bool {
        self.models.read().unwrap_or_else(|e| e.into_inner())
            .get(name)
            .map(|h| h.is_available())
            .unwrap_or(true)
    }

    /// Get health for a model.
    pub fn health(&self, name: &str) -> ModelHealth {
        self.models.read().unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
            .unwrap_or(ModelHealth::Healthy)
    }

    /// Mark a model as healthy.
    pub fn mark_healthy(&self, name: &str) {
        if let Ok(mut w) = self.models.write() {
            w.insert(name.to_string(), ModelHealth::Healthy);
        }
    }

    /// Mark a model as unavailable based on error kind.
    pub fn mark_failure(&self, name: &str, kind: ErrorKind) {
        if let Ok(mut w) = self.models.write() {
            match kind {
                ErrorKind::Terminal | ErrorKind::NotFound => {
                    w.insert(name.to_string(), ModelHealth::Unavailable {
                        reason: format!("{:?}", kind),
                        until: Instant::now() + Duration::from_secs(300),
                    });
                }
                ErrorKind::RateLimited | ErrorKind::Transient => {
                    w.insert(name.to_string(), ModelHealth::Unavailable {
                        reason: format!("{:?}", kind),
                        until: Instant::now() + self.retry_cooldown,
                    });
                }
                ErrorKind::Unknown => {
                    w.insert(name.to_string(), ModelHealth::Degraded);
                }
            }
        }
    }

    /// Return the first available model from a list.
    pub fn first_available<'a>(&self, names: &'a [String]) -> Option<&'a String> {
        names.iter().find(|n| self.is_available(n))
    }

    /// Return all available models from a list, in order.
    pub fn available<'a>(&self, names: &'a [String]) -> Vec<&'a String> {
        names.iter().filter(|n| self.is_available(n)).collect()
    }
}

/// Retry configuration for transient errors.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 10000,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Calculate delay for attempt N (1-indexed).
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let exp = self.base_delay_ms * 2u64.pow(attempt.saturating_sub(1));
        let capped = exp.min(self.max_delay_ms);
        if self.jitter {
            let jitter = rand::random::<u64>() % (capped / 4 + 1);
            Duration::from_millis(capped + jitter)
        } else {
            Duration::from_millis(capped)
        }
    }
}

/// Classify a provider error into an ErrorKind.
pub fn classify_error(err: &crate::ProviderError) -> ErrorKind {
    match err {
        crate::ProviderError::RateLimited { .. }
        | crate::ProviderError::RateLimitExceeded { .. } => ErrorKind::RateLimited,
        crate::ProviderError::NotFound(_) => ErrorKind::NotFound,
        crate::ProviderError::Unauthorized { .. }
        | crate::ProviderError::Forbidden { .. }
        | crate::ProviderError::InvalidRequest(_) => ErrorKind::Terminal,
        crate::ProviderError::Timeout { .. }
        | crate::ProviderError::NetworkError(_)
        | crate::ProviderError::ServerError { .. }
        | crate::ProviderError::ServiceUnavailable { .. } => ErrorKind::Transient,
        _ => ErrorKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_transitions() {
        let svc = ModelAvailabilityService::new(&["gpt-4".into(), "claude-3".into()]);
        assert!(svc.is_available("gpt-4"));
        svc.mark_failure("gpt-4", ErrorKind::Terminal);
        assert!(!svc.is_available("gpt-4"));
        assert!(svc.is_available("claude-3"));
    }

    #[test]
    fn test_first_available() {
        let svc = ModelAvailabilityService::new(&["a".into(), "b".into(), "c".into()]);
        svc.mark_failure("a", ErrorKind::Transient);
        let names = vec!["a".into(), "b".into(), "c".into()];
        assert_eq!(svc.first_available(&names), Some(&"b".to_string()));
    }

    #[test]
    fn test_retry_delay_increases() {
        let cfg = RetryConfig::default();
        let d1 = cfg.delay_for(1);
        let d2 = cfg.delay_for(2);
        let d3 = cfg.delay_for(3);
        assert!(d2 > d1);
        assert!(d3 > d2);
    }

    #[test]
    fn test_classify_rate_limit() {
        let err = crate::ProviderError::RateLimitExceeded { retry_after: 5 };
        assert_eq!(classify_error(&err), ErrorKind::RateLimited);
    }

    #[test]
    fn test_classify_not_found() {
        let err = crate::ProviderError::NotFound("model not found".into());
        assert_eq!(classify_error(&err), ErrorKind::NotFound);
    }
}

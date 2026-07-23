use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
pub struct ProxyStats {
    pub total_requests: u64,
    pub tokens_before: u64,
    pub tokens_after: u64,
    pub tokens_saved: u64,
    pub savings_percent: f64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub errors: u64,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub status: &'static str,
    pub optimize: bool,
    pub uptime_seconds: u64,
}

#[derive(Clone)]
pub struct SharedStats {
    inner: Arc<StatsInner>,
}

struct StatsInner {
    total_requests: AtomicU64,
    tokens_before: AtomicU64,
    tokens_after: AtomicU64,
    tokens_saved: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    errors: AtomicU64,
    started_at: DateTime<Utc>,
    _persistent: RwLock<PersistentSavings>,
}

#[derive(Debug, Clone, Serialize, Default)]
struct PersistentSavings {
    _total_tokens_saved: u64,
    _total_requests: u64,
}

impl SharedStats {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(StatsInner {
                total_requests: AtomicU64::new(0),
                tokens_before: AtomicU64::new(0),
                tokens_after: AtomicU64::new(0),
                tokens_saved: AtomicU64::new(0),
                cache_hits: AtomicU64::new(0),
                cache_misses: AtomicU64::new(0),
                errors: AtomicU64::new(0),
                started_at: Utc::now(),
                _persistent: RwLock::new(PersistentSavings::default()),
            }),
        }
    }

    pub fn record_request(&self, tokens_before: u64, tokens_after: u64) {
        self.inner.total_requests.fetch_add(1, Ordering::Relaxed);
        self.inner.tokens_before.fetch_add(tokens_before, Ordering::Relaxed);
        self.inner.tokens_after.fetch_add(tokens_after, Ordering::Relaxed);
        let saved = tokens_before.saturating_sub(tokens_after);
        self.inner.tokens_saved.fetch_add(saved, Ordering::Relaxed);
    }

    pub fn record_cache_hit(&self) {
        self.inner.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss(&self) {
        self.inner.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.inner.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> ProxyStats {
        let total = self.inner.total_requests.load(Ordering::Relaxed);
        let before = self.inner.tokens_before.load(Ordering::Relaxed);
        let after = self.inner.tokens_after.load(Ordering::Relaxed);
        let saved = self.inner.tokens_saved.load(Ordering::Relaxed);
        ProxyStats {
            total_requests: total,
            tokens_before: before,
            tokens_after: after,
            tokens_saved: saved,
            savings_percent: if before > 0 {
                (saved as f64 / before as f64) * 100.0
            } else {
                0.0
            },
            cache_hits: self.inner.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.inner.cache_misses.load(Ordering::Relaxed),
            errors: self.inner.errors.load(Ordering::Relaxed),
            started_at: self.inner.started_at,
        }
    }

    pub fn health(&self, optimizing: bool) -> HealthStatus {
        let uptime = Utc::now().signed_duration_since(self.inner.started_at);
        HealthStatus {
            status: "healthy",
            optimize: optimizing,
            uptime_seconds: uptime.num_seconds().max(0) as u64,
        }
    }

    pub fn metrics_text(&self) -> String {
        let s = self.snapshot();
        format!(
            "# HELP headroom_requests_total Total requests processed\n\
             # TYPE headroom_requests_total counter\n\
             headroom_requests_total {{mode=\"optimize\"}} {total}\n\
             \n\
             # HELP headroom_tokens_saved_total Total tokens saved\n\
             # TYPE headroom_tokens_saved_total counter\n\
             headroom_tokens_saved_total {saved}\n\
             \n\
             # HELP headroom_compression_ratio Compression ratio\n\
             # TYPE headroom_compression_ratio gauge\n\
             headroom_compression_ratio {ratio}\n\
             \n\
             # HELP headroom_cache_hits_total Total cache hits\n\
             # TYPE headroom_cache_hits_total counter\n\
             headroom_cache_hits_total {hits}\n\
             \n\
             # HELP headroom_cache_misses_total Total cache misses\n\
             # TYPE headroom_cache_misses_total counter\n\
             headroom_cache_misses_total {misses}\n\
             \n\
             # HELP headroom_errors_total Total errors\n\
             # TYPE headroom_errors_total counter\n\
             headroom_errors_total {errors}\n",
            total = s.total_requests,
            saved = s.tokens_saved,
            ratio = s.savings_percent / 100.0,
            hits = s.cache_hits,
            misses = s.cache_misses,
            errors = s.errors,
        )
    }
}

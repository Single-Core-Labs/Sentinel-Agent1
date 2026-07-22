use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use lru::LruCache;
use sha2::{Sha256, Digest};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct CcrEntry {
    pub original: String,
    pub content_type: String,
    pub compressed_preview: String,
    pub stored_at: Instant,
    pub ttl: Duration,
}

impl CcrEntry {
    pub fn is_expired(&self) -> bool {
        self.stored_at.elapsed() > self.ttl
    }
}

pub struct CcrStore {
    cache: Arc<RwLock<LruCache<String, CcrEntry>>>,
    default_ttl: Duration,
}

impl CcrStore {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(max_entries).unwrap_or(NonZeroUsize::new(1000).unwrap())))),
            default_ttl: Duration::from_secs(3600),
        }
    }

    pub fn with_ttl(max_entries: usize, ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(max_entries).unwrap_or(NonZeroUsize::new(1000).unwrap())))),
            default_ttl: ttl,
        }
    }

    pub async fn store(&self, original: String, content_type: &str, preview: String) -> String {
        let mut hash = Sha256::new();
        hash.update(original.as_bytes());
        let key = format!("ccr:{}", hex::encode(hash.finalize()));

        let entry = CcrEntry {
            original,
            content_type: content_type.to_string(),
            compressed_preview: preview,
            stored_at: Instant::now(),
            ttl: self.default_ttl,
        };

        let mut cache = self.cache.write().await;
        cache.put(key.clone(), entry);
        key
    }

    pub async fn store_with_key(&self, key: &str, original: String, content_type: &str, preview: String) {
        let entry = CcrEntry {
            original,
            content_type: content_type.to_string(),
            compressed_preview: preview,
            stored_at: Instant::now(),
            ttl: self.default_ttl,
        };
        let mut cache = self.cache.write().await;
        cache.put(key.to_string(), entry);
    }

    pub async fn retrieve(&self, key: &str) -> Option<String> {
        let mut cache = self.cache.write().await;
        let entry = cache.get(key)?;
        if entry.is_expired() {
            cache.pop(key);
            return None;
        }
        Some(entry.original.clone())
    }

    pub async fn retrieve_entry(&self, key: &str) -> Option<CcrEntry> {
        let mut cache = self.cache.write().await;
        let entry = cache.get(key)?;
        if entry.is_expired() {
            cache.pop(key);
            return None;
        }
        Some(entry.clone())
    }

    pub async fn contains(&self, key: &str) -> bool {
        let mut cache = self.cache.write().await;
        match cache.get(key) {
            Some(entry) if !entry.is_expired() => true,
            _ => false,
        }
    }

    pub async fn remove_expired(&self) -> usize {
        let mut cache = self.cache.write().await;
        let before = cache.len();
        let expired_keys: Vec<String> = cache.iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(k, _)| k.clone())
            .collect();
        for key in &expired_keys {
            cache.pop(key);
        }
        before - cache.len()
    }

    pub async fn len(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

impl Default for CcrStore {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_ccr_store_roundtrip() {
        let store = CcrStore::new(100);
        let key = store.store("hello world".into(), "text", "compressed".into()).await;
        assert!(key.starts_with("ccr:"));
        assert_eq!(store.retrieve(&key).await, Some("hello world".into()));
    }

    #[tokio::test]
    async fn test_ccr_store_missing_key() {
        let store = CcrStore::new(100);
        assert_eq!(store.retrieve("ccr:nonexistent").await, None);
    }

    #[tokio::test]
    async fn test_ccr_store_expiry() {
        let store = CcrStore::with_ttl(100, Duration::from_millis(1));
        let key = store.store("data".into(), "text", "prev".into()).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(store.retrieve(&key).await, None);
    }

    #[tokio::test]
    async fn test_ccr_store_lru_eviction() {
        let store = CcrStore::new(2);
        store.store("a".into(), "text", "p1".into()).await;
        store.store("b".into(), "text", "p2".into()).await;
        store.store("c".into(), "text", "p3".into()).await;
        assert!(!store.contains("ccr:nonexistent").await);
    }

    #[tokio::test]
    async fn test_ccr_store_with_key() {
        let store = CcrStore::new(100);
        store.store_with_key("my_key", "original data".into(), "text", "prev".into()).await;
        assert_eq!(store.retrieve("my_key").await, Some("original data".into()));
    }

    #[tokio::test]
    async fn test_ccr_remove_expired() {
        let store = CcrStore::with_ttl(100, Duration::from_millis(1));
        store.store("a".into(), "text", "p1".into()).await;
        store.store("b".into(), "text", "p2".into()).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        let removed = store.remove_expired().await;
        assert_eq!(removed, 2);
        assert!(store.is_empty().await);
    }
}

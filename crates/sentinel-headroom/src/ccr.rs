use std::collections::{HashMap, HashSet, VecDeque};
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
    pub retrieval_count: u64,
}

impl CcrEntry {
    pub fn is_expired(&self) -> bool {
        self.stored_at.elapsed() > self.ttl
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

fn bm25_score(query_tokens: &[String], doc_tokens: &[String], doc_freq: &HashMap<String, f64>, total_docs: f64, avg_dl: f64) -> f64 {
    if query_tokens.is_empty() || doc_tokens.is_empty() { return 0.0; }
    let k1 = 1.5; let b = 0.75;
    let dl = doc_tokens.len() as f64;
    let mut score = 0.0;
    let mut tf = HashMap::new();
    for t in doc_tokens { *tf.entry(t.clone()).or_insert(0u64) += 1; }
    for qt in query_tokens {
        let df = doc_freq.get(qt).copied().unwrap_or(1.0);
        let idf = ((total_docs - df + 0.5) / (df + 0.5) + 1.0).ln();
        let term_freq = *tf.get(qt).unwrap_or(&0) as f64;
        score += idf * (term_freq * (k1 + 1.0)) / (term_freq + k1 * (1.0 - b + b * dl / avg_dl.max(1.0)));
    }
    score
}

pub fn compute_hash(data: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(data.as_bytes());
    format!("ccr:{}", hex::encode(hash.finalize()))
}

pub fn generate_retrieval_marker(key: &str, content_type: &str, original_len: usize, compressed_len: usize) -> String {
    let saved = if original_len > 0 { (original_len - compressed_len) * 100 / original_len } else { 0 };
    format!(
        "\n[headroom: hash={}; type={}; saved={}%; retrieve via headroom_retrieve(hash=\"{}\")]",
        key, content_type, saved, key
    )
}

pub fn generate_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "headroom_retrieve",
        "description": "Retrieve original uncompressed data from Headroom cache. Use when the compressed preview is insufficient and you need the full content.",
        "parameters": {
            "type": "object",
            "properties": {
                "hash": {
                    "type": "string",
                    "description": "The hash key from the compression marker in the compressed content"
                },
                "query": {
                    "type": "string",
                    "description": "Optional search query to retrieve only relevant portions via BM25 ranking"
                }
            },
            "required": ["hash"]
        }
    })
}

#[derive(Debug, Clone)]
pub struct RetrievalPattern {
    pub hash: String,
    pub query: Option<String>,
    pub retrieved_at: Instant,
    pub matched: bool,
}

pub struct CcrStore {
    cache: Arc<RwLock<LruCache<String, CcrEntry>>>,
    default_ttl: Duration,
    retrieval_log: Arc<RwLock<VecDeque<RetrievalPattern>>>,
    max_retrieval_log: usize,
}

impl CcrStore {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(max_entries.max(1)).unwrap_or(NonZeroUsize::new(1000).unwrap())))),
            default_ttl: Duration::from_secs(3600),
            retrieval_log: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            max_retrieval_log: 1000,
        }
    }

    pub fn with_ttl(max_entries: usize, ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(max_entries.max(1)).unwrap_or(NonZeroUsize::new(1000).unwrap())))),
            default_ttl: ttl,
            retrieval_log: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            max_retrieval_log: 1000,
        }
    }

    pub async fn store(&self, original: String, content_type: &str, preview: String) -> String {
        let key = compute_hash(&original);
        let entry = CcrEntry {
            original,
            content_type: content_type.to_string(),
            compressed_preview: preview,
            stored_at: Instant::now(),
            ttl: self.default_ttl,
            retrieval_count: 0,
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
            retrieval_count: 0,
        };
        let mut cache = self.cache.write().await;
        cache.put(key.to_string(), entry);
    }

    pub async fn store_with_ttl(&self, original: String, content_type: &str, preview: String, ttl: Duration) -> String {
        let key = compute_hash(&original);
        let entry = CcrEntry {
            original,
            content_type: content_type.to_string(),
            compressed_preview: preview,
            stored_at: Instant::now(),
            ttl,
            retrieval_count: 0,
        };
        let mut cache = self.cache.write().await;
        cache.put(key.clone(), entry);
        key
    }

    pub async fn retrieve(&self, key: &str) -> Option<String> {
        let mut cache = self.cache.write().await;
        let entry = cache.get_mut(key)?;
        if entry.is_expired() {
            cache.pop(key);
            return None;
        }
        entry.retrieval_count += 1;
        Some(entry.original.clone())
    }

    pub async fn retrieve_entry(&self, key: &str) -> Option<CcrEntry> {
        let mut cache = self.cache.write().await;
        let entry = cache.get_mut(key)?;
        if entry.is_expired() {
            cache.pop(key);
            return None;
        }
        entry.retrieval_count += 1;
        Some(entry.clone())
    }

    pub async fn search(&self, key: &str, query: &str) -> Option<String> {
        let mut cache = self.cache.write().await;
        let entry = cache.get_mut(key)?;
        if entry.is_expired() {
            cache.pop(key);
            return None;
        }
        entry.retrieval_count += 1;
        let original = &entry.original;
        let lines: Vec<&str> = original.lines().collect();
        if lines.len() < 2 || query.trim().is_empty() {
            return Some(original.clone());
        }
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Some(original.clone());
        }
        let total_docs = lines.len() as f64;
        let tokenized_docs: Vec<Vec<String>> = lines.iter().map(|l| tokenize(l)).collect();
        let avg_dl = tokenized_docs.iter().map(|t| t.len() as f64).sum::<f64>() / total_docs.max(1.0);
        let mut doc_freq: HashMap<String, f64> = HashMap::new();
        for toks in &tokenized_docs {
            let unique: HashSet<&String> = toks.iter().collect();
            for t in unique { *doc_freq.entry(t.clone()).or_insert(0.0) += 1.0; }
        }
        let mut scored: Vec<(f64, usize, &str)> = lines.iter().enumerate()
            .map(|(i, l)| {
                let score = bm25_score(&query_tokens, &tokenized_docs[i], &doc_freq, total_docs, avg_dl);
                (score, i, *l)
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let top_k = scored.len().min(20);
        let result: String = scored[..top_k].iter()
            .map(|(_, _, l)| *l)
            .collect::<Vec<_>>()
            .join("\n");
        Some(result)
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

    pub async fn all_keys(&self) -> Vec<String> {
        let cache = self.cache.read().await;
        cache.iter().map(|(k, _)| k.clone()).collect()
    }

    pub async fn retrieval_stats(&self) -> HashMap<String, (u64, bool)> {
        let cache = self.cache.read().await;
        let mut stats = HashMap::new();
        for (k, entry) in cache.iter() {
            if !entry.is_expired() {
                stats.insert(k.clone(), (entry.retrieval_count, false));
            }
        }
        stats
    }

    pub async fn log_retrieval(&self, hash: String, query: Option<String>, matched: bool) {
        let mut log = self.retrieval_log.write().await;
        if log.len() >= self.max_retrieval_log {
            log.pop_front();
        }
        log.push_back(RetrievalPattern {
            hash,
            query,
            retrieved_at: Instant::now(),
            matched,
        });
    }

    pub async fn recent_retrievals(&self, count: usize) -> Vec<RetrievalPattern> {
        let log = self.retrieval_log.read().await;
        log.iter().rev().take(count).cloned().collect()
    }

    pub async fn most_retrieved_hashes(&self, top_n: usize) -> Vec<(String, u64)> {
        let mut freq: HashMap<String, u64> = HashMap::new();
        let log = self.retrieval_log.read().await;
        for p in log.iter() {
            *freq.entry(p.hash.clone()).or_insert(0) += 1;
        }
        let mut sorted: Vec<_> = freq.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(top_n);
        sorted
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

    #[tokio::test]
    async fn test_compute_hash() {
        let h1 = compute_hash("hello");
        let h2 = compute_hash("hello");
        let h3 = compute_hash("world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert!(h1.starts_with("ccr:"));
    }

    #[tokio::test]
    async fn test_retrieval_marker() {
        let marker = generate_retrieval_marker("ccr:abc", "json", 1000, 200);
        assert!(marker.contains("ccr:abc"));
        assert!(marker.contains("headroom_retrieve"));
        assert!(marker.contains("80%"));
    }

    #[test]
    fn test_tool_schema() {
        let schema = generate_tool_schema();
        assert_eq!(schema["name"], "headroom_retrieve");
        assert!(schema["parameters"]["required"][0] == "hash");
        assert!(schema["parameters"]["properties"]["query"].is_object());
    }

    #[tokio::test]
    async fn test_search_within_cached() {
        let store = CcrStore::new(100);
        let data = "the quick brown fox\njumps over the lazy dog\nthis is something else\nerror: authentication failed\n";
        let key = store.store(data.into(), "text", "compressed".into()).await;
        let result = store.search(&key, "authentication").await.unwrap();
        assert!(result.contains("authentication"));
        assert!(result.len() < data.len());
    }

    #[tokio::test]
    async fn test_search_empty_query_returns_all() {
        let store = CcrStore::new(100);
        let data = "line one\nline two\nline three\n";
        let key = store.store(data.into(), "text", "compressed".into()).await;
        let result = store.search(&key, "").await.unwrap();
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn test_search_missing_key() {
        let store = CcrStore::new(100);
        assert!(store.search("ccr:nonexistent", "query").await.is_none());
    }

    #[tokio::test]
    async fn test_retrieval_count_increments() {
        let store = CcrStore::new(100);
        let key = store.store("data".into(), "text", "preview".into()).await;
        store.retrieve(&key).await;
        store.retrieve(&key).await;
        let stats = store.retrieval_stats().await;
        assert_eq!(stats.get(&key).unwrap().0, 2);
    }

    #[tokio::test]
    async fn test_retrieval_log() {
        let store = CcrStore::new(100);
        store.log_retrieval("hash1".into(), Some("query1".into()), true).await;
        store.log_retrieval("hash2".into(), None, false).await;
        let recent = store.recent_retrievals(10).await;
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].hash, "hash2");
        assert_eq!(recent[1].query.as_deref(), Some("query1"));
    }

    #[tokio::test]
    async fn test_most_retrieved_hashes() {
        let store = CcrStore::new(100);
        store.log_retrieval("a".into(), None, true).await;
        store.log_retrieval("a".into(), None, true).await;
        store.log_retrieval("b".into(), None, true).await;
        let top = store.most_retrieved_hashes(5).await;
        assert_eq!(top[0].0, "a");
        assert_eq!(top[0].1, 2);
    }

    #[tokio::test]
    async fn test_all_keys() {
        let store = CcrStore::new(100);
        store.store_with_key("k1", "d1".into(), "t", "p".into()).await;
        store.store_with_key("k2", "d2".into(), "t", "p".into()).await;
        let keys = store.all_keys().await;
        assert!(keys.contains(&"k1".to_string()));
        assert!(keys.contains(&"k2".to_string()));
    }

    #[tokio::test]
    async fn test_store_with_ttl() {
        let store = CcrStore::new(100);
        let key = store.store_with_ttl("data".into(), "text", "preview".into(), Duration::from_secs(9999)).await;
        assert!(store.contains(&key).await);
    }

    #[tokio::test]
    async fn test_retrieval_count_in_search() {
        let store = CcrStore::new(100);
        let key = store.store("line a\nline b".into(), "text", "preview".into()).await;
        store.search(&key, "a").await;
        let stats = store.retrieval_stats().await;
        assert_eq!(stats.get(&key).unwrap().0, 1);
    }
}

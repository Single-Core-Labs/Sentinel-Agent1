use std::collections::HashMap;
use std::sync::Arc;
use crate::ccr::CcrStore;
use crate::orchestrator::ContentCompressor;

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

fn query_relevance(query_tokens: &[String], target_tokens: &[String]) -> f64 {
    if query_tokens.is_empty() || target_tokens.is_empty() { return 0.0; }
    let q_set: std::collections::HashSet<&str> = query_tokens.iter().map(|s| s.as_str()).collect();
    let t_set: std::collections::HashSet<&str> = target_tokens.iter().map(|s| s.as_str()).collect();
    let intersection: usize = q_set.intersection(&t_set).count();
    intersection as f64 / q_set.len().max(1) as f64
}

pub struct CcrContextTracker {
    ccr: Arc<CcrStore>,
    tracker: Arc<tokio::sync::RwLock<TrackerState>>,
}

struct TrackerState {
    recent_queries: Vec<String>,
    matched_hashes: HashMap<String, f64>,
}

impl CcrContextTracker {
    pub fn new(ccr: Arc<CcrStore>) -> Self {
        Self {
            ccr,
            tracker: Arc::new(tokio::sync::RwLock::new(TrackerState {
                recent_queries: Vec::with_capacity(20),
                matched_hashes: HashMap::new(),
            })),
        }
    }

    pub async fn record_query(&self, query: &str) {
        let mut state = self.tracker.write().await;
        state.recent_queries.push(query.to_string());
        if state.recent_queries.len() > 20 {
            state.recent_queries.remove(0);
        }
    }

    pub async fn find_relevant_cached(&self, query: &str, _compressor: &ContentCompressor) -> Vec<(String, String, f64)> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() { return Vec::new(); }

        let keys = self.ccr.all_keys().await;
        if keys.is_empty() { return Vec::new(); }

        let mut matches: Vec<(String, String, f64)> = Vec::new();
        for key in &keys {
            if let Some(entry) = self.ccr.retrieve_entry(key).await {
                let preview = &entry.compressed_preview;
                let preview_tokens = tokenize(preview);
                let relevance = query_relevance(&query_tokens, &preview_tokens);

                let original_tokens = tokenize(&entry.original);
                let original_relevance = query_relevance(&query_tokens, &original_tokens);
                let combined = relevance.max(original_relevance);

                if combined > 0.15 {
                    matches.push((key.clone(), entry.original.clone(), combined));
                }
            }
        }

        matches.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        let top: Vec<_> = matches.into_iter().take(5).collect();

        let mut state = self.tracker.write().await;
        for (key, _, score) in &top {
            state.matched_hashes.insert(key.clone(), *score);
        }

        top
    }

    pub async fn learn_from_retrieval(&self, hash: &str, retrieved: bool) {
        let mut state = self.tracker.write().await;
        let entry = state.matched_hashes.entry(hash.to_string()).or_insert(0.0);
        if retrieved {
            *entry += 1.0;
        } else {
            *entry = (*entry - 0.5).max(0.0);
        }
    }

    pub async fn build_context_marker(&self) -> Option<String> {
        let state = self.tracker.read().await;
        let mut lines: Vec<String> = Vec::new();

        if !state.matched_hashes.is_empty() {
            lines.push("‖ Headroom CCR: cached data available for retrieval".to_string());
            for (hash, score) in state.matched_hashes.iter().take(5) {
                if *score > 0.3 {
                    lines.push(format!("  [{}] relevance: {:.2}", hash, score));
                }
            }
        }

        if lines.is_empty() { None }
        else { Some(lines.join("\n")) }
    }

    pub async fn detect_query_change(&self, new_query: &str) -> Option<Vec<String>> {
        let state = self.tracker.read().await;
        if state.recent_queries.is_empty() { return None; }

        let new_tokens = tokenize(new_query);
        if new_tokens.is_empty() { return None; }

        let old_query = state.recent_queries.last()?;
        let old_tokens = tokenize(old_query);
        let overlap = query_relevance(&new_tokens, &old_tokens);

        if overlap < 0.3 {
            let different: Vec<String> = new_tokens.iter()
                .filter(|t| !old_tokens.contains(t))
                .cloned()
                .collect();
            if !different.is_empty() { Some(different) } else { None }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ccr::CcrStore;
    use std::sync::Arc;

    fn tracker() -> (CcrContextTracker, Arc<CcrStore>) {
        let ccr = Arc::new(CcrStore::new(100));
        let t = CcrContextTracker::new(ccr.clone());
        (t, ccr)
    }

    #[tokio::test]
    async fn test_record_query() {
        let (t, _) = tracker();
        t.record_query("find authentication errors").await;
        t.record_query("list all files").await;
        let state = t.tracker.read().await;
        assert_eq!(state.recent_queries.len(), 2);
    }

    #[tokio::test]
    async fn test_find_relevant_empty() {
        let (t, _) = tracker();
        let compressor = ContentCompressor::default();
        let results = t.find_relevant_cached("anything", &compressor).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_detect_query_change() {
        let (t, _) = tracker();
        t.record_query("show me the authentication module").await;
        let change = t.detect_query_change("what about error handling").await;
        assert!(change.is_some(), "should detect topic change");
        let change2 = t.detect_query_change("authentication module details").await;
        assert!(change2.is_none(), "similar query should not detect change");
    }

    #[tokio::test]
    async fn test_query_relevance_score() {
        let q = tokenize("authentication error handling");
        let t1 = tokenize("authentication timed out");
        let t2 = tokenize("completely unrelated topic");
        assert!(query_relevance(&q, &t1) > query_relevance(&q, &t2),
            "auth match ({}) should be > unrelated ({})",
            query_relevance(&q, &t1), query_relevance(&q, &t2));
    }

    #[tokio::test]
    async fn test_learn_from_retrieval() {
        let (t, ccr) = tracker();
        let _key = ccr.store("data".into(), "text", "preview".into()).await;
        t.learn_from_retrieval("hash1", true).await;
        let state = t.tracker.read().await;
        assert!(state.matched_hashes.contains_key("hash1"));
    }

    #[tokio::test]
    async fn test_context_marker_with_no_data() {
        let (t, _) = tracker();
        assert!(t.build_context_marker().await.is_none());
    }

    #[tokio::test]
    async fn test_find_relevant_returns_matches() {
        let (t, ccr) = tracker();
        ccr.store_with_key("test_auth", "authentication error: token expired".into(), "text", "auth related compressed".into()).await;
        ccr.store_with_key("test_other", "weather report: sunny".into(), "text", "weather info".into()).await;
        let compressor = ContentCompressor::default();
        let results = t.find_relevant_cached("authentication", &compressor).await;
        assert!(!results.is_empty(), "should find auth matches");
        let top_hash = &results[0].0;
        assert_eq!(top_hash, "test_auth", "should rank auth match first");
    }
}

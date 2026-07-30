use std::collections::HashMap;

const VECTOR_DIM: usize = 256;
const NGRAM_MIN: usize = 2;
const NGRAM_MAX: usize = 3;

pub fn text_to_vector(text: &str) -> Vec<f64> {
    let mut counts = vec![0f64; VECTOR_DIM];
    let ngrams = extract_ngrams(text);
    let total = ngrams.len() as f64;
    if total == 0.0 {
        return counts;
    }
    for ngram in &ngrams {
        let idx = hash_to_index(ngram, VECTOR_DIM);
        counts[idx] += 1.0;
    }
    for c in counts.iter_mut() {
        *c /= total;
    }
    counts
}

fn extract_ngrams(text: &str) -> Vec<String> {
    let cleaned: String = text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let mut ngrams = Vec::new();
    for n in NGRAM_MIN..=NGRAM_MAX {
        for w in &words {
            if w.len() >= n {
                for i in 0..=w.len() - n {
                    ngrams.push(w[i..i + n].to_string());
                }
            }
        }
    }
    for pair in words.windows(2) {
        ngrams.push(format!("{}_{}", pair[0], pair[1]));
    }
    ngrams
}

fn hash_to_index(ngram: &str, dim: usize) -> usize {
    let bytes = ngram.as_bytes();
    let mut h: usize = 2166136261;
    for &b in bytes {
        h ^= b as usize;
        h = h.wrapping_mul(16777619);
    }
    h % dim
}

pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).max(0.0)
}

pub fn keyword_overlap(query: &str, text: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let query_tokens: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|t| t.len() > 2)
        .collect();
    if query_tokens.is_empty() {
        return 0.0;
    }
    let text_lower = text.to_lowercase();
    let matching = query_tokens.iter().filter(|t| text_lower.contains(*t)).count();
    matching as f64 / query_tokens.len() as f64
}

pub fn combined_score(query: &str, text: &str, query_vec: Option<&Vec<f64>>, text_vec: Option<&Vec<f64>>) -> f64 {
    let semantic = match (query_vec, text_vec) {
        (Some(qv), Some(tv)) => cosine_similarity(qv, tv),
        _ => {
            let qv = text_to_vector(query);
            let tv = text_to_vector(text);
            cosine_similarity(&qv, &tv)
        }
    };
    let keyword = keyword_overlap(query, text);
    semantic * 0.6 + keyword * 0.4
}

pub struct EmbeddingCache {
    cache: HashMap<String, Vec<f64>>,
    max_entries: usize,
}

impl EmbeddingCache {
    pub fn new(max_entries: usize) -> Self {
        Self { cache: HashMap::new(), max_entries }
    }

    pub fn get_or_compute(&mut self, text: &str) -> Vec<f64> {
        if let Some(vec) = self.cache.get(text) {
            return vec.clone();
        }
        let vec = text_to_vector(text);
        if self.cache.len() < self.max_entries {
            self.cache.insert(text.to_string(), vec.clone());
        }
        vec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text() {
        let v = text_to_vector("");
        assert_eq!(v.len(), VECTOR_DIM);
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn test_same_text_similarity_is_one() {
        let text = "I prefer Python for backend work";
        let v1 = text_to_vector(text);
        let v2 = text_to_vector(text);
        let sim = cosine_similarity(&v1, &v2);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_similar_texts_have_high_similarity() {
        let v1 = text_to_vector("I like Python programming");
        let v2 = text_to_vector("Python is my favorite language");
        let sim = cosine_similarity(&v1, &v2);
        assert!(sim > 0.3, "similarity should be noticeable: {}", sim);
    }

    #[test]
    fn test_different_texts_have_low_similarity() {
        let v1 = text_to_vector("I like Python programming");
        let v2 = text_to_vector("The weather is nice today");
        let sim = cosine_similarity(&v1, &v2);
        assert!(sim < 0.5, "unrelated texts should have low similarity: {}", sim);
    }

    #[test]
    fn test_keyword_overlap_exact() {
        let score = keyword_overlap("python backend", "I prefer Python for backend work");
        assert!(score > 0.0);
    }

    #[test]
    fn test_keyword_overlap_none() {
        let score = keyword_overlap("rust", "I like Python");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_combined_score_range() {
        let score = combined_score("python", "I use Python daily", None, None);
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn test_embedding_cache_hit() {
        let mut cache = EmbeddingCache::new(100);
        let v1 = cache.get_or_compute("hello world");
        let v2 = cache.get_or_compute("hello world");
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_ngram_different_orders() {
        let v1 = text_to_vector("quick brown fox");
        let v2 = text_to_vector("fox brown quick");
        let sim = cosine_similarity(&v1, &v2);
        assert!(sim > 0.8, "anagrams should be similar: {}", sim);
    }
}

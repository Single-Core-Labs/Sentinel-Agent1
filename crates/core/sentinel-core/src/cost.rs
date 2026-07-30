use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

static MODEL_PRICING: LazyLock<HashMap<&'static str, ModelPrice>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("gpt-4o", ModelPrice { input_per_1k: 0.01, output_per_1k: 0.03 });
    m.insert("gpt-4o-mini", ModelPrice { input_per_1k: 0.0015, output_per_1k: 0.006 });
    m.insert("gpt-5.5", ModelPrice { input_per_1k: 0.01, output_per_1k: 0.03 });
    m.insert("claude-opus-4.8", ModelPrice { input_per_1k: 0.015, output_per_1k: 0.075 });
    m.insert("claude-sonnet-4.6", ModelPrice { input_per_1k: 0.003, output_per_1k: 0.015 });
    m.insert("claude-haiku-3.5", ModelPrice { input_per_1k: 0.0008, output_per_1k: 0.004 });
    m.insert("gemini-2.5-pro", ModelPrice { input_per_1k: 0.00125, output_per_1k: 0.005 });
    m.insert("gemini-2.0-flash", ModelPrice { input_per_1k: 0.0001, output_per_1k: 0.0004 });
    m.insert("deepseek-chat", ModelPrice { input_per_1k: 0.0003, output_per_1k: 0.0015 });
    m.insert("deepseek-v4-pro", ModelPrice { input_per_1k: 0.002, output_per_1k: 0.008 });
    m.insert("openai/gpt-4o", ModelPrice { input_per_1k: 0.01, output_per_1k: 0.03 });
    m.insert("openai/gpt-4o-mini", ModelPrice { input_per_1k: 0.0015, output_per_1k: 0.006 });
    m
});

#[derive(Debug, Clone, Copy)]
pub struct ModelPrice {
    pub input_per_1k: f64,
    pub output_per_1k: f64,
}

#[derive(Debug, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

impl Usage {
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self { prompt_tokens, completion_tokens }
    }

    pub fn total_tokens(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// Estimate the cost of an LLM call based on model and token usage.
pub fn estimate_llm_cost(model: &str, usage: &Usage) -> f64 {
    let key = MODEL_PRICING.keys()
        .find(|k| model.contains(*k))
        .copied()
        .unwrap_or("gpt-4o-mini");
    let price = MODEL_PRICING.get(key).unwrap();
    let input_cost = (usage.prompt_tokens as f64 / 1000.0) * price.input_per_1k;
    let output_cost = (usage.completion_tokens as f64 / 1000.0) * price.output_per_1k;
    input_cost + output_cost
}

/// Estimate the cost of a request before it's made (prompt tokens only).
pub fn estimate_input_cost(model: &str, prompt_tokens: u32) -> f64 {
    let key = MODEL_PRICING.keys()
        .find(|k| model.contains(*k))
        .copied()
        .unwrap_or("gpt-4o-mini");
    let price = MODEL_PRICING.get(key).unwrap();
    (prompt_tokens as f64 / 1000.0) * price.input_per_1k
}

/// Real-time cost tracker across a session.
#[derive(Debug)]
pub struct CostTracker {
    session_spend: AtomicU64,   // stored as microdollars (USD * 1_000_000)
    turn_spend: AtomicU64,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            session_spend: AtomicU64::new(0),
            turn_spend: AtomicU64::new(0),
        }
    }

    pub fn record(&self, model: &str, usage: &Usage) {
        let cost = estimate_llm_cost(model, usage);
        let micros = (cost * 1_000_000.0) as u64;
        self.session_spend.fetch_add(micros, Ordering::Relaxed);
        self.turn_spend.fetch_add(micros, Ordering::Relaxed);
    }

    pub fn record_input(&self, model: &str, prompt_tokens: u32) {
        let cost = estimate_input_cost(model, prompt_tokens);
        let micros = (cost * 1_000_000.0) as u64;
        self.session_spend.fetch_add(micros, Ordering::Relaxed);
        self.turn_spend.fetch_add(micros, Ordering::Relaxed);
    }

    pub fn session_spend(&self) -> f64 {
        self.session_spend.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    pub fn turn_spend(&self) -> f64 {
        self.turn_spend.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    pub fn reset_turn(&self) {
        self.turn_spend.store(0, Ordering::Relaxed);
    }

    pub fn fmt_spend(&self, width: usize) -> String {
        let total = self.session_spend();
        let bar_len = width.max(10);
        if total <= 0.0 {
            return format!("{:width$}", "$0.000", width = bar_len);
        }
        let used_chars = ((total / 10.0_f64.max(total)) * bar_len as f64) as usize;
        let empty_chars = bar_len.saturating_sub(used_chars);
        format!(
            "${:.3} [{}{}]",
            total,
            "█".repeat(used_chars.min(bar_len)),
            "░".repeat(empty_chars),
        )
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_known_model() {
        let usage = Usage::new(1000, 500);
        let cost = estimate_llm_cost("gpt-4o", &usage);
        let expected = (1000.0 / 1000.0 * 0.01) + (500.0 / 1000.0 * 0.03);
        assert!((cost - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn test_estimate_unknown_model_falls_back() {
        let usage = Usage::new(1000, 1000);
        let cost = estimate_llm_cost("custom-model", &usage);
        let expected = (1000.0 / 1000.0 * 0.0015) + (1000.0 / 1000.0 * 0.006);
        assert!((cost - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn test_zero_tokens_zero_cost() {
        let usage = Usage::new(0, 0);
        let cost = estimate_llm_cost("gpt-4o", &usage);
        assert!((cost - 0.0).abs() < f64::EPSILON);
    }
}

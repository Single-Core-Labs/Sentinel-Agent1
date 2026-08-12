use std::collections::HashMap;
use std::sync::LazyLock;

static MODEL_PRICING: LazyLock<HashMap<&'static str, ModelPrice>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "gpt-4o",
        ModelPrice {
            input_per_1k: 0.01,
            output_per_1k: 0.03,
        },
    );
    m.insert(
        "gpt-4o-mini",
        ModelPrice {
            input_per_1k: 0.0015,
            output_per_1k: 0.006,
        },
    );
    m.insert(
        "o3-mini",
        ModelPrice {
            input_per_1k: 0.011,
            output_per_1k: 0.044,
        },
    );
    m.insert(
        "claude-sonnet-4",
        ModelPrice {
            input_per_1k: 0.003,
            output_per_1k: 0.015,
        },
    );
    m.insert(
        "claude-haiku-3-5",
        ModelPrice {
            input_per_1k: 0.0008,
            output_per_1k: 0.004,
        },
    );
    m.insert(
        "gemini-2.5-pro",
        ModelPrice {
            input_per_1k: 0.00125,
            output_per_1k: 0.005,
        },
    );
    m.insert(
        "gemini-2.5-flash",
        ModelPrice {
            input_per_1k: 0.0003,
            output_per_1k: 0.0025,
        },
    );
    m.insert(
        "deepseek-chat",
        ModelPrice {
            input_per_1k: 0.0003,
            output_per_1k: 0.0015,
        },
    );
    m.insert(
        "deepseek-reasoner",
        ModelPrice {
            input_per_1k: 0.00055,
            output_per_1k: 0.00219,
        },
    );
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
        Self {
            prompt_tokens,
            completion_tokens,
        }
    }

    pub fn total_tokens(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// Resolve the price entry for a model id: exact match first, then the
/// longest matching key (so `gpt-4o-mini` never loses to `gpt-4o`).
pub fn price_for(model: &str) -> &'static ModelPrice {
    if let Some(k) = MODEL_PRICING.keys().find(|k| **k == model) {
        return MODEL_PRICING.get(k).unwrap();
    }
    let key = MODEL_PRICING
        .keys()
        .filter(|k| model.contains(*k))
        .max_by_key(|k| k.len())
        .copied()
        .unwrap_or("gpt-4o-mini");
    MODEL_PRICING.get(key).unwrap()
}

/// Estimate the cost of an LLM call based on model and token usage.
pub fn estimate_llm_cost(model: &str, usage: &Usage) -> f64 {
    let price = price_for(model);
    let input_cost = (usage.prompt_tokens as f64 / 1000.0) * price.input_per_1k;
    let output_cost = (usage.completion_tokens as f64 / 1000.0) * price.output_per_1k;
    input_cost + output_cost
}

/// Estimate the cost of a request before it's made (prompt tokens only).
pub fn estimate_input_cost(model: &str, prompt_tokens: u32) -> f64 {
    let price = price_for(model);
    (prompt_tokens as f64 / 1000.0) * price.input_per_1k
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

    #[test]
    fn test_mini_never_resolves_to_gpt4o() {
        // `gpt-4o-mini` contains `gpt-4o`; longest-key match must win.
        let usage = Usage::new(1000, 1000);
        let mini = estimate_llm_cost("gpt-4o-mini", &usage);
        let expected = (1000.0 / 1000.0 * 0.0015) + (1000.0 / 1000.0 * 0.006);
        assert!((mini - expected).abs() < f64::EPSILON, "got {}", mini);
    }

    #[test]
    fn test_dated_claude_id_matches_stable_key() {
        let usage = Usage::new(1000, 1000);
        let cost = estimate_llm_cost("claude-sonnet-4-20250514", &usage);
        let expected = (1000.0 / 1000.0 * 0.003) + (1000.0 / 1000.0 * 0.015);
        assert!((cost - expected).abs() < f64::EPSILON, "got {}", cost);
    }

    #[test]
    fn test_reasoner_priced_over_chat() {
        let usage = Usage::new(1000, 1000);
        let chat = estimate_llm_cost("deepseek-chat", &usage);
        let reasoner = estimate_llm_cost("deepseek-reasoner", &usage);
        assert!(reasoner > chat);
    }

    #[test]
    fn test_exact_match_beats_substring() {
        let usage = Usage::new(1000, 1000);
        let exact = estimate_llm_cost("gemini-2.5-pro", &usage);
        let flash = estimate_llm_cost("gemini-2.5-flash", &usage);
        assert!(exact > flash, "pro {} vs flash {}", exact, flash);
    }
}

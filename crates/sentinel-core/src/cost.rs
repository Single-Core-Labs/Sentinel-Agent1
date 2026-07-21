use std::collections::HashMap;
use std::sync::LazyLock;

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

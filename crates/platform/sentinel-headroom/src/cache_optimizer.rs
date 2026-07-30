use crate::config::CacheOptimizerConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum LlmProvider {
    Anthropic,
    OpenAI,
    Google,
    Unknown,
}

impl LlmProvider {
    pub fn from_model(model: &str) -> Self {
        let m = model.to_lowercase();
        if m.contains("claude") || m.contains("anthropic") { LlmProvider::Anthropic }
        else if m.contains("gpt") || m.contains("o1") || m.contains("o3") || m.contains("text-embedding") { LlmProvider::OpenAI }
        else if m.contains("gemini") || m.contains("palm") || m.contains("google") { LlmProvider::Google }
        else { LlmProvider::Unknown }
    }

    pub fn cache_read_discount(&self) -> f64 {
        match self {
            LlmProvider::Anthropic => 0.90,
            LlmProvider::OpenAI => 0.50,
            LlmProvider::Google => 0.75,
            LlmProvider::Unknown => 0.0,
        }
    }

    pub fn cache_write_premium(&self) -> f64 {
        match self {
            LlmProvider::Anthropic => 0.25,
            _ => 0.0,
        }
    }

    pub fn cache_ttl_seconds(&self) -> u64 {
        match self {
            LlmProvider::Anthropic => 300,
            LlmProvider::OpenAI => 60,
            LlmProvider::Google => 3600,
            LlmProvider::Unknown => 0,
        }
    }

    pub fn min_cacheable_tokens(&self) -> usize {
        match self {
            LlmProvider::Anthropic => 1,
            LlmProvider::OpenAI => 1024,
            LlmProvider::Google => 32768,
            LlmProvider::Unknown => 1024,
        }
    }

    pub fn needs_explicit_markers(&self) -> bool {
        matches!(self, LlmProvider::Anthropic)
    }

    pub fn supports_context_caching(&self) -> bool {
        matches!(self, LlmProvider::Google)
    }
}

#[derive(Debug, Clone)]
pub struct CacheBreakpoint {
    pub message_index: usize,
    pub label: &'static str,
}

#[derive(Debug, Clone)]
pub struct OptimizedMessages {
    pub messages: Vec<crate::config::Message>,
    pub cache_breakpoints: Vec<CacheBreakpoint>,
    pub stable_prefix_tokens: usize,
    pub total_input_tokens: usize,
    pub cacheable_tokens: usize,
    pub estimated_cost_ratio: f64,
    pub summary: String,
}

pub struct CacheOptimizer {
    config: CacheOptimizerConfig,
}

impl CacheOptimizer {
    pub fn new(config: CacheOptimizerConfig) -> Self {
        Self { config }
    }

    pub fn detect_provider(&self, model: &str) -> LlmProvider {
        if self.config.force_provider != LlmProvider::Unknown {
            return self.config.force_provider.clone();
        }
        LlmProvider::from_model(model)
    }

    pub fn optimize(&self, messages: Vec<crate::config::Message>, model: &str) -> OptimizedMessages {
        let provider = self.detect_provider(model);
        let min_cacheable = provider.min_cacheable_tokens();
        let total_tokens = estimate_tokens_all(&messages);

        if total_tokens < min_cacheable {
            return self.passthrough(messages);
        }

        let breakpoints = match provider {
            LlmProvider::Anthropic => self.compute_anthropic_breakpoints(&messages, total_tokens),
            LlmProvider::OpenAI => self.compute_openai_breakpoints(&messages, total_tokens),
            LlmProvider::Google => self.compute_google_breakpoints(&messages, total_tokens),
            LlmProvider::Unknown => Vec::new(),
        };

        let stable_prefix_tokens: usize = messages.iter()
            .take(1)
            .map(|m| estimate_tokens(&m.content))
            .sum();

        let optimized = self.insert_breakpoint_markers(messages, &breakpoints);

        let cacheable_tokens = if provider.needs_explicit_markers() {
            breakpoints.iter()
                .take_while(|bp| bp.label == "system" || bp.label == "conversation_start")
                .map(|bp| {
                    if bp.message_index < optimized.len() {
                        estimate_tokens(&optimized[bp.message_index].content)
                    } else { 0 }
                })
                .sum()
        } else if matches!(provider, LlmProvider::Google) && total_tokens >= min_cacheable {
            total_tokens
        } else {
            stable_prefix_tokens.min(total_tokens)
        };

        let discount = provider.cache_read_discount();
        let non_cacheable = total_tokens.saturating_sub(cacheable_tokens);
        let effective_cost = non_cacheable as f64 + cacheable_tokens as f64 * (1.0 - discount);
        let estimated_cost_ratio = if total_tokens > 0 { effective_cost / total_tokens as f64 } else { 1.0 };

        let result = OptimizedMessages {
            messages: optimized,
            cache_breakpoints: breakpoints,
            stable_prefix_tokens,
            total_input_tokens: total_tokens,
            cacheable_tokens,
            estimated_cost_ratio,
            summary: String::new(),
        };
        let summary = self.format_cache_summary(&result, &provider);
        OptimizedMessages { summary, ..result }
    }

    fn passthrough(&self, messages: Vec<crate::config::Message>) -> OptimizedMessages {
        let total = estimate_tokens_all(&messages);
        let result = OptimizedMessages {
            cache_breakpoints: Vec::new(),
            stable_prefix_tokens: 0,
            total_input_tokens: total,
            cacheable_tokens: 0,
            estimated_cost_ratio: 1.0,
            messages,
            summary: String::new(),
        };
        let provider = LlmProvider::Unknown;
        let summary = self.format_cache_summary(&result, &provider);
        OptimizedMessages { summary, ..result }
    }

    fn compute_anthropic_breakpoints(&self, messages: &[crate::config::Message], total_tokens: usize) -> Vec<CacheBreakpoint> {
        let mut points = Vec::new();
        let mut accumulated = 0usize;
        let min_cacheable = self.config.min_cacheable_tokens;

        for (i, msg) in messages.iter().enumerate() {
            let tokens = estimate_tokens(&msg.content);
            accumulated += tokens;

            if i == 0 && matches!(msg.role, crate::config::MessageRole::System) {
                points.push(CacheBreakpoint { message_index: i, label: "system" });
            } else if accumulated >= min_cacheable && accumulated < total_tokens / 2 {
                if points.is_empty() || points.last().map(|p| p.message_index) != Some(i) {
                    points.push(CacheBreakpoint { message_index: i, label: "conversation_start" });
                }
                break;
            }
        }

        if points.is_empty() && total_tokens >= min_cacheable {
            points.push(CacheBreakpoint { message_index: 0, label: "content" });
        }

        points
    }

    fn compute_openai_breakpoints(&self, messages: &[crate::config::Message], total_tokens: usize) -> Vec<CacheBreakpoint> {
        if total_tokens < 1024 { return Vec::new(); }
        let mut points = Vec::new();
        let mut acc = 0usize;
        for (i, msg) in messages.iter().enumerate() {
            acc += estimate_tokens(&msg.content);
            if acc >= 1024 {
                points.push(CacheBreakpoint { message_index: i, label: "prefix" });
                break;
            }
        }
        points
    }

    fn compute_google_breakpoints(&self, _messages: &[crate::config::Message], total_tokens: usize) -> Vec<CacheBreakpoint> {
        if total_tokens < 32768 { return Vec::new(); }
        vec![
            CacheBreakpoint { message_index: 0, label: "cached_content" }
        ]
    }

    fn insert_breakpoint_markers(&self, mut messages: Vec<crate::config::Message>, breakpoints: &[CacheBreakpoint]) -> Vec<crate::config::Message> {
        for bp in breakpoints.iter().rev() {
            if bp.message_index < messages.len() {
                let marker = match bp.label {
                    "system" => "\n\n[cache_control: breakpoint type=system]",
                    "conversation_start" => "\n\n[cache_control: breakpoint type=conversation]",
                    "content" => "\n\n[cache_control: breakpoint type=content]",
                    "prefix" | "cached_content" => "\n\n[cache_control: breakpoint type=prefix]",
                    _ => "\n\n[cache_control: breakpoint]",
                };
                messages[bp.message_index].content.push_str(marker);
            }
        }
        messages
    }

    pub fn format_cache_summary(&self, result: &OptimizedMessages, provider: &LlmProvider) -> String {
        let discount = provider.cache_read_discount() * 100.0;
        let savings = (1.0 - result.estimated_cost_ratio) * 100.0;
        let write_premium = provider.cache_write_premium() * 100.0;
        let ttl = provider.cache_ttl_seconds();
        let mut out = format!(
            "‖ CacheOptimizer: provider={}, discount={:.0}%, effective_savings={:.1}%\n",
            format!("{:?}", provider).to_lowercase(),
            discount,
            savings,
        );
        out.push_str(&format!("‖   cacheable: {} tokens, total: {} tokens, breakpoints: {}\n",
            result.cacheable_tokens, result.total_input_tokens, result.cache_breakpoints.len()));
        if provider.needs_explicit_markers() && !result.cache_breakpoints.is_empty() {
            out.push_str("‖   cache_control breakpoints inserted for Anthropic\n");
            out.push_str(&format!("‖   cache TTL: {}s, write premium: {:.0}%\n", ttl, write_premium));
        }
        if matches!(provider, LlmProvider::OpenAI) && result.cacheable_tokens >= 1024 {
            out.push_str("‖   prefix caching active (automatic, >= 1024 tokens)\n");
        }
        if matches!(provider, LlmProvider::Google) && result.cacheable_tokens >= 32768 {
            out.push_str("‖   CachedContent API eligible (>= 32768 tokens)\n");
        }
        out
    }
}

fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() { return 0; }
    let chars = text.len();
    let words = text.split_whitespace().count();
    (chars / 4).max(words).min(chars)
}

fn estimate_tokens_all(messages: &[crate::config::Message]) -> usize {
    messages.iter().map(|m| estimate_tokens(&m.content)).sum()
}

pub fn estimate_message_tokens(text: &str) -> usize {
    estimate_tokens(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    fn msg(role: MessageRole, content: &str) -> Message {
        Message { role, content: content.into(), tool_call_id: None, name: None }
    }

    #[test]
    fn test_detect_anthropic() {
        assert_eq!(LlmProvider::from_model("claude-sonnet-4-20250514"), LlmProvider::Anthropic);
        assert_eq!(LlmProvider::from_model("gpt-4o"), LlmProvider::OpenAI);
        assert_eq!(LlmProvider::from_model("gemini-2.0-pro"), LlmProvider::Google);
    }

    #[test]
    fn test_anthropic_breakpoints() {
        let config = CacheOptimizerConfig { min_cacheable_tokens: 1, ..Default::default() };
        let opt = CacheOptimizer::new(config);
        let messages = vec![
            msg(MessageRole::System, "You are a helpful assistant."),
            msg(MessageRole::User, "Hello!"),
        ];
        let result = opt.optimize(messages, "claude-sonnet-4-20250514");
        assert!(!result.cache_breakpoints.is_empty(), "should have anthropic breakpoints");
        assert!(result.messages[0].content.contains("cache_control"),
            "system message should have cache_control marker");
    }

    #[test]
    fn test_openai_prefix() {
        let config = CacheOptimizerConfig { min_cacheable_tokens: 1, ..Default::default() };
        let opt = CacheOptimizer::new(config);
        let messages = vec![
            msg(MessageRole::System, "You are helpful. ".repeat(600).as_str()),
            msg(MessageRole::User, "Hi."),
        ];
        let result = opt.optimize(messages, "gpt-4o");
        assert!(result.cacheable_tokens > 0, "openai should have cacheable tokens");
    }

    #[test]
    fn test_google_cached_content() {
        let config = CacheOptimizerConfig { min_cacheable_tokens: 1, ..Default::default() };
        let opt = CacheOptimizer::new(config);
        let big = "hello world ".repeat(14000);
        let messages = vec![
            msg(MessageRole::System, &big),
            msg(MessageRole::User, "Hi"),
        ];
        let result = opt.optimize(messages, "gemini-2.0-pro");
        assert!(!result.cache_breakpoints.is_empty(), "google should have breakpoints for large content");
    }

    #[test]
    fn test_unknown_provider_no_breakpoints() {
        let opt = CacheOptimizer::new(CacheOptimizerConfig::default());
        let messages = vec![
            msg(MessageRole::System, "You are helpful."),
            msg(MessageRole::User, "Hi."),
        ];
        let result = opt.optimize(messages, "unknown-model");
        assert!(result.cache_breakpoints.is_empty());
    }

    #[test]
    fn test_cache_summary() {
        let opt = CacheOptimizer::new(CacheOptimizerConfig::default());
        let messages = vec![
            msg(MessageRole::System, "You are helpful."),
            msg(MessageRole::User, "Hi."),
        ];
        let result = opt.optimize(messages, "claude-sonnet-4-20250514");
        let summary = opt.format_cache_summary(&result, &LlmProvider::Anthropic);
        assert!(summary.contains("CacheOptimizer"));
        assert!(summary.contains("90%"));
    }

    #[test]
    fn test_force_provider() {
        let config = CacheOptimizerConfig {
            force_provider: LlmProvider::Anthropic,
            ..Default::default()
        };
        let opt = CacheOptimizer::new(config);
        assert_eq!(opt.detect_provider("gpt-4o"), LlmProvider::Anthropic);
    }

    #[test]
    fn test_small_content_passthrough() {
        let config = CacheOptimizerConfig {
            min_cacheable_tokens: 100000,
            ..Default::default()
        };
        let opt = CacheOptimizer::new(config);
        let messages = vec![msg(MessageRole::User, "hi")];
        let result = opt.optimize(messages, "claude-sonnet-4-20250514");
        assert!(result.cache_breakpoints.is_empty());
    }

    #[test]
    fn test_anthropic_discount() {
        assert!((LlmProvider::Anthropic.cache_read_discount() - 0.90).abs() < 0.01);
        assert!((LlmProvider::OpenAI.cache_read_discount() - 0.50).abs() < 0.01);
        assert!((LlmProvider::Google.cache_read_discount() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_google_min_tokens() {
        assert_eq!(LlmProvider::Google.min_cacheable_tokens(), 32768);
        assert_eq!(LlmProvider::OpenAI.min_cacheable_tokens(), 1024);
    }

    #[test]
    fn test_cost_ratio_calculation() {
        let config = CacheOptimizerConfig { min_cacheable_tokens: 1, ..Default::default() };
        let opt = CacheOptimizer::new(config);
        let big = "test data ".repeat(500);
        let messages = vec![msg(MessageRole::System, &big)];
        let result = opt.optimize(messages, "claude-sonnet-4-20250514");
        assert!(result.estimated_cost_ratio < 1.0, "cache should reduce effective cost: {}", result.estimated_cost_ratio);
        assert!(result.estimated_cost_ratio > 0.0);
    }

    #[test]
    fn test_multiple_breakpoints_not_duplicated() {
        let config = CacheOptimizerConfig { min_cacheable_tokens: 1, ..Default::default() };
        let opt = CacheOptimizer::new(config);
        let messages = vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, &"a".repeat(2000)),
            msg(MessageRole::User, &"b".repeat(2000)),
        ];
        let result = opt.optimize(messages, "claude-sonnet-4-20250514");
        let marker_count = result.messages.iter()
            .filter(|m| m.content.contains("cache_control"))
            .count();
        assert!(marker_count <= 2, "should not have excessive breakpoints: {}", marker_count);
    }

    #[test]
    fn test_format_cache_summary_unknown() {
        let opt = CacheOptimizer::new(CacheOptimizerConfig::default());
        let messages = vec![msg(MessageRole::User, "hi")];
        let result = opt.optimize(messages, "unknown");
        let summary = opt.format_cache_summary(&result, &LlmProvider::Unknown);
        assert!(summary.contains("0%"), "unknown provider should have 0% discount");
    }

    #[test]
    fn test_anthropic_cache_control_marker_format() {
        let config = CacheOptimizerConfig { min_cacheable_tokens: 1, ..Default::default() };
        let opt = CacheOptimizer::new(config);
        let messages = vec![msg(MessageRole::System, "system prompt here")];
        let result = opt.optimize(messages, "claude-sonnet-4-20250514");
        assert!(result.messages[0].content.contains("[cache_control: breakpoint type=system]"));
    }
}

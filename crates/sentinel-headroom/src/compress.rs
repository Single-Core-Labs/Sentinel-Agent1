use std::sync::Arc;
use crate::cache_aligner::CacheAligner;
use crate::cache_optimizer::CacheOptimizer;
use crate::config::*;
use crate::intelligent_context::IntelligentContext;
use crate::orchestrator::ContentCompressor;

pub struct Compressor {
    cache_aligner: CacheAligner,
    cache_optimizer: CacheOptimizer,
    content_router: Arc<ContentCompressor>,
    intelligent_context: IntelligentContext,
    config: HeadroomConfig,
}

impl Compressor {
    pub fn new(config: HeadroomConfig) -> Self {
        let content_router = Arc::new(ContentCompressor::from_config(&config));
        Self {
            cache_aligner: CacheAligner::new(config.cache_alignment.clone()),
            cache_optimizer: CacheOptimizer::new(config.cache_optimizer.clone()),
            content_router,
            intelligent_context: IntelligentContext::new(config.intelligent_context.clone()),
            config,
        }
    }

    pub fn with_ccr(ccr: Arc<crate::ccr::CcrStore>, config: HeadroomConfig) -> Self {
        let content_router = Arc::new(ContentCompressor::with_ccr_and_config(ccr, &config));
        Self {
            cache_aligner: CacheAligner::new(config.cache_alignment.clone()),
            cache_optimizer: CacheOptimizer::new(config.cache_optimizer.clone()),
            content_router,
            intelligent_context: IntelligentContext::new(config.intelligent_context.clone()),
            config,
        }
    }

    pub fn content_router(&self) -> &Arc<ContentCompressor> {
        &self.content_router
    }

    pub async fn compress(&mut self, messages: Vec<Message>, model: &str) -> CompressionResult {
        let total_messages = messages.len();
        let total_input_chars: usize = messages.iter().map(|m| m.content.len()).sum();
        let total_input_tokens = messages.iter().map(|m| crate::intelligent_context::estimate_tokens(&m.content)).sum();

        let mut transformed: Vec<Message> = Vec::with_capacity(total_messages);
        let mut total_cacheable_prefixes = 0usize;
        let mut total_dynamic_suffixes = 0usize;
        let mut total_content_compressed = 0usize;
        let mut transformed_roles: Vec<String> = Vec::new();
        let mut ccr_keys: Vec<String> = Vec::new();

        for msg in messages {
            let role_name = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };

            match msg.role {
                MessageRole::System if self.config.cache_alignment.enabled => {
                    let aligned = self.cache_aligner.align(&msg.content);
                    total_cacheable_prefixes += aligned.static_prefix.len();
                    total_dynamic_suffixes += aligned.dynamic_suffix.len();
                    let combined = if aligned.dynamic_suffix.is_empty() {
                        aligned.static_prefix
                    } else {
                        format!("{}\n{}", aligned.static_prefix, aligned.dynamic_suffix)
                    };
                    transformed.push(Message {
                        content: combined,
                        ..msg
                    });
                    transformed_roles.push(format!("{}:aligned", role_name));
                }
                MessageRole::Tool if self.config.content_routing.enabled_types.len() > 0 => {
                    let hint = if let Some(ref tool_name) = msg.name {
                        crate::integration::content_type_for_tool(tool_name)
                    } else {
                        None
                    };
                    let outcome = self.content_router.compress(&msg.content, hint).await;
                    match outcome {
                        crate::orchestrator::CompressOutcome::Compressed { text, retrieval_key, .. } => {
                            total_content_compressed += 1;
                            if let Some(ref key) = retrieval_key {
                                ccr_keys.push(key.clone());
                            }
                            let final_text = if let Some(ref key) = retrieval_key {
                                format!("{} [headroom: hash={}]", text, key)
                            } else {
                                text
                            };
                            transformed.push(Message {
                                content: final_text,
                                ..msg
                            });
                            transformed_roles.push(format!("{}:compressed", role_name));
                        }
                        crate::orchestrator::CompressOutcome::Skipped { .. } => {
                            transformed.push(msg);
                            transformed_roles.push(role_name.to_string());
                        }
                    }
                }
                _ => {
                    transformed.push(msg);
                    transformed_roles.push(role_name.to_string());
                }
            }
        }

        let total_output_chars: usize = transformed.iter().map(|m| m.content.len()).sum();
        let total_output_tokens: usize = transformed.iter().map(|m| crate::intelligent_context::estimate_tokens(&m.content)).sum();

        let cache_summary = if self.config.cache_optimizer.enabled {
            let opt = self.cache_optimizer.optimize(transformed.clone(), model);
            let summary = opt.summary;
            transformed = opt.messages;
            Some(summary)
        } else {
            None
        };

        let intelligence_result = if self.config.intelligent_context.enabled {
            Some(self.intelligent_context.score(transformed.clone()))
        } else {
            None
        };

        let mut ccr_dropped_refs: Vec<(String, usize)> = Vec::new();

        let final_messages: Vec<Message> = match &intelligence_result {
            Some(ref scored) if scored.dropped_count > 0 => {
                let dropped_msgs: Vec<&Message> = {
                    let selected_indices: std::collections::HashSet<usize> =
                        scored.messages.iter().map(|sm| sm.index).collect();
                    transformed.iter().enumerate()
                        .filter(|(i, _)| !selected_indices.contains(i))
                        .map(|(_, m)| m)
                        .collect()
                };

                if !dropped_msgs.is_empty() && self.config.ccr.enabled {
                    for (i, dm) in dropped_msgs.iter().enumerate().take(20) {
                        let dropped_key = format!("ccr:dropped:msg:{}", i);
                        let preview = if dm.content.len() > 200 {
                            format!("{}... [truncated]", &dm.content[..200])
                        } else {
                            dm.content.clone()
                        };
                        self.content_router.ccr().store_with_key(
                            &dropped_key,
                            dm.content.clone(),
                            "dropped_message",
                            preview,
                        ).await;
                        ccr_dropped_refs.push((dropped_key, dm.content.len()));
                    }

                    let marker = format!(
                        "\n\n[headroom: {} message(s) dropped. Cached data available via headroom_retrieve tool]\n",
                        scored.dropped_count
                    );

                    let mut result: Vec<Message> = scored.messages.iter().map(|sm| Message {
                        role: sm.role.clone(),
                        content: sm.content.clone(),
                        tool_call_id: sm.tool_call_id.clone(),
                        name: sm.name.clone(),
                    }).collect();

                    if let Some(last) = result.last_mut() {
                        last.content.push_str(&marker);
                    }
                    result
                } else {
                    scored.messages.iter().map(|sm| Message {
                        role: sm.role.clone(),
                        content: sm.content.clone(),
                        tool_call_id: sm.tool_call_id.clone(),
                        name: sm.name.clone(),
                    }).collect()
                }
            }
            _ => transformed,
        };

        let _final_token_count: usize = final_messages.iter().map(|m| crate::intelligent_context::estimate_tokens(&m.content)).sum();
        let total_output_messages = final_messages.len();

        CompressionResult {
            messages: final_messages,
            metadata: CompressionMetadata {
                total_input_messages: total_messages,
                total_output_messages,
                total_input_chars,
                total_output_chars,
                total_input_tokens,
                total_output_tokens,
                cacheable_prefix_bytes: total_cacheable_prefixes,
                dynamic_suffix_bytes: total_dynamic_suffixes,
                content_compressed_count: total_content_compressed,
                cache_summary,
                ccr_keys,
                roles: transformed_roles,
                intelligent_dropped: intelligence_result.as_ref().map(|r| r.dropped_count).unwrap_or(0),
                intelligent_budget: intelligence_result.as_ref().map(|r| r.budget_tokens).unwrap_or(0),
                ccr_dropped_refs: ccr_dropped_refs.iter().map(|(k, _)| k.clone()).collect(),
            },
        }
    }

    pub fn reset_cache_aligner(&mut self) {
        self.cache_aligner.reset();
    }
}

pub struct CompressionResult {
    pub messages: Vec<Message>,
    pub metadata: CompressionMetadata,
}

pub struct CompressionMetadata {
    pub total_input_messages: usize,
    pub total_output_messages: usize,
    pub total_input_chars: usize,
    pub total_output_chars: usize,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub cacheable_prefix_bytes: usize,
    pub dynamic_suffix_bytes: usize,
    pub content_compressed_count: usize,
    pub cache_summary: Option<String>,
    pub ccr_keys: Vec<String>,
    pub roles: Vec<String>,
    pub intelligent_dropped: usize,
    pub intelligent_budget: usize,
    pub ccr_dropped_refs: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: MessageRole, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            tool_call_id: None,
            name: None,
        }
    }

    fn tool_msg(name: &str, content: &str) -> Message {
        Message {
            role: MessageRole::Tool,
            content: content.to_string(),
            tool_call_id: Some("call_1".into()),
            name: Some(name.to_string()),
        }
    }

    #[tokio::test]
    async fn test_compress_preserves_simple() {
        let config = HeadroomConfig::default();
        let mut compressor = Compressor::new(config);
        let messages = vec![
            msg(MessageRole::System, "You are a helpful assistant."),
            msg(MessageRole::User, "Hello!"),
        ];
        let result = compressor.compress(messages, "test-model").await;
        assert_eq!(result.metadata.total_input_messages, 2);
        assert_eq!(result.metadata.total_output_messages, 2);
    }

    #[tokio::test]
    async fn test_compress_aligns_system_prompt() {
        let config = HeadroomConfig::default();
        let mut compressor = Compressor::new(config);
        let messages = vec![
            msg(MessageRole::System, "Today is 2026-07-22. User: alice."),
            msg(MessageRole::User, "Run tests."),
        ];
        let result = compressor.compress(messages, "test-model").await;
        let sys = &result.messages[0];
        assert!(sys.content.contains("[Context:"), "system prompt should have [Context: ...] suffix: {:?}", sys.content);
    }

    #[tokio::test]
    async fn test_compress_compresses_tool_output() {
        let config = HeadroomConfig::default();
        let mut compressor = Compressor::new(config);
        let mut code = String::new();
        code.push_str("use std::collections::HashMap;\nuse std::fs;\n\n");
        for i in 0..30 {
            code.push_str(&format!(
                "pub fn func_{}() -> i32 {{\n    let x = {};\n    x * 2\n}}\n\n",
                i, i
            ));
        }
        let messages = vec![
            msg(MessageRole::System, "You are a coding assistant."),
            tool_msg("read", &code),
        ];
        let result = compressor.compress(messages, "test-model").await;
        let tool_result = &result.messages[1];
        if tool_result.content.len() < code.len() {
            assert!(result.metadata.content_compressed_count >= 1, "should compress tool output");
        }
    }

    #[tokio::test]
    async fn test_compress_respects_budget() {
        let config = HeadroomConfig {
            intelligent_context: IntelligentContextConfig {
                token_budget: 10,
                ..Default::default()
            },
            cache_alignment: CacheAlignmentConfig {
                enabled: false,
                ..Default::default()
            },
            content_routing: ContentRoutingConfig {
                enabled_types: vec![],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut compressor = Compressor::new(config);
        let messages = vec![
            msg(MessageRole::User, "short"),
            msg(MessageRole::User, "this is a much longer message that should be dropped"),
            msg(MessageRole::User, "tiny"),
        ];
        let result = compressor.compress(messages, "test-model").await;
        assert!(result.metadata.intelligent_dropped > 0, "should drop messages when over budget");
        assert!(result.metadata.total_output_messages < result.metadata.total_input_messages);
    }

    #[tokio::test]
    async fn test_compress_preserves_key_messages() {
        let config = HeadroomConfig {
            intelligent_context: IntelligentContextConfig {
                token_budget: 30,
                error_weight: 10.0,
                ..Default::default()
            },
            cache_alignment: CacheAlignmentConfig {
                enabled: false,
                ..Default::default()
            },
            content_routing: ContentRoutingConfig {
                enabled_types: vec![],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut compressor = Compressor::new(config);
        let messages = vec![
            msg(MessageRole::User, "a"),
            msg(MessageRole::User, "b"),
            msg(MessageRole::User, "ERROR: critical failure"),
            msg(MessageRole::User, "c"),
        ];
        let result = compressor.compress(messages, "test-model").await;
        let has_error = result.messages.iter().any(|m| m.content.contains("ERROR"));
        assert!(has_error, "should preserve error messages");
    }

    #[tokio::test]
    async fn test_compress_empty_input() {
        let config = HeadroomConfig::default();
        let mut compressor = Compressor::new(config);
        let result = compressor.compress(vec![], "test-model").await;
        assert_eq!(result.metadata.total_input_messages, 0);
        assert!(result.messages.is_empty());
    }

    #[tokio::test]
    async fn test_cache_aligner_delta_across_calls() {
        let config = HeadroomConfig::default();
        let mut compressor = Compressor::new(config);
        let msgs1 = vec![msg(MessageRole::System, "Today is 2026-07-22.")];
        let _r1 = compressor.compress(msgs1, "test-model").await;
        let msgs2 = vec![msg(MessageRole::System, "Today is 2026-07-22.")];
        let r2 = compressor.compress(msgs2, "test-model").await;
        assert!(r2.messages[0].content.contains("[Context: no change]"), "second call should detect no change: {:?}", r2.messages[0].content);
    }

    #[tokio::test]
    async fn test_ccr_dropped_messages_stored() {
        let config = HeadroomConfig {
            intelligent_context: IntelligentContextConfig {
                token_budget: 10,
                ..Default::default()
            },
            cache_alignment: CacheAlignmentConfig {
                enabled: false,
                ..Default::default()
            },
            content_routing: ContentRoutingConfig {
                enabled_types: vec![],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut compressor = Compressor::new(config);
        let messages = vec![
            msg(MessageRole::User, "short"),
            msg(MessageRole::User, "this is a much longer message that should definitely be dropped due to budget constraints"),
            msg(MessageRole::User, "tiny"),
        ];
        let result = compressor.compress(messages, "test-model").await;
        assert!(result.metadata.intelligent_dropped > 0, "should drop messages");
        assert_eq!(result.metadata.ccr_dropped_refs.len(), result.metadata.intelligent_dropped.min(20),
            "dropped refs count should match dropped messages up to 20");
    }

    #[tokio::test]
    async fn test_compressed_tool_output_has_marker() {
        let mut code = String::new();
        for i in 0..30 {
            code.push_str(&format!("pub fn func_{}() -> i32 {{ {} }}\n", i, i));
        }
        let config = HeadroomConfig::default();
        let mut compressor = Compressor::new(config);
        let messages = vec![
            msg(MessageRole::System, "help"),
            tool_msg("read", &code),
        ];
        let result = compressor.compress(messages, "test-model").await;
        let tool = &result.messages[1];
        assert!(tool.content.contains("ccr:") || tool.content.contains("headroom"),
            "compressed tool output should contain ccr reference: {}", tool.content);
    }
}

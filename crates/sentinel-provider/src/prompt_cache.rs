use serde_json::Value;

const CACHEABLE_ROLES: &[&str] = &["system", "user"];

pub fn with_prompt_caching(
    messages: &[serde_json::Value],
    tools: Option<&[serde_json::Value]>,
    llm_params: &serde_json::Value,
) -> (Vec<serde_json::Value>, Option<Vec<serde_json::Value>>) {
    let cache_enabled = llm_params
        .get("extra_body")
        .and_then(|eb| eb.get("cache_control"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if !cache_enabled {
        return (messages.to_vec(), tools.map(|t| t.to_vec()));
    }

    let cached_tools = tools.map(tools_with_cache_control);
    let idx = cache_target_index(messages);

    if let Some(idx) = idx {
        let mut cached_messages = messages.to_vec();
        let msg = &cached_messages[idx];
        if let Some(content) = msg.get("content") {
            let cached_content = content_with_cache_control(content);
            if let Some(obj) = cached_messages[idx].as_object_mut() {
                obj.insert("content".into(), cached_content);
            }
        }
        (cached_messages, cached_tools)
    } else {
        (messages.to_vec(), cached_tools)
    }
}

fn cache_target_index(messages: &[serde_json::Value]) -> Option<usize> {
    if messages.len() < 2 {
        return None;
    }

    for idx in (0..messages.len().saturating_sub(1)).rev() {
        let msg = &messages[idx];
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if !CACHEABLE_ROLES.contains(&role) {
            continue;
        }
        if has_cacheable_text(msg.get("content")) {
            return Some(idx);
        }
    }
    None
}

fn has_cacheable_text(content: Option<&Value>) -> bool {
    match content {
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(blocks)) => {
            blocks.iter().any(|block| {
                block.get("type").and_then(|t| t.as_str()) == Some("text")
                    && block.get("text")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| !t.is_empty())
            })
        }
        _ => false,
    }
}

fn content_with_cache_control(content: &Value) -> Value {
    let cache_control = serde_json::json!({"type": "ephemeral"});

    match content {
        Value::String(text) => {
            serde_json::json!([
                {"type": "text", "text": text, "cache_control": cache_control}
            ])
        }
        Value::Array(blocks) => {
            let mut result: Vec<Value> = blocks.to_vec();
            for idx in (0..result.len()).rev() {
                if result[idx].get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(obj) = result[idx].as_object_mut() {
                        obj.insert("cache_control".into(), cache_control);
                    }
                    break;
                }
            }
            Value::Array(result)
        }
        other => other.clone(),
    }
}

fn tools_with_cache_control(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    if tools.is_empty() {
        return tools.to_vec();
    }

    let mut cached = tools.to_vec();
    let cache_control = serde_json::json!({"type": "ephemeral"});
    if let Some(last) = cached.last_mut() {
        if let Some(obj) = last.as_object_mut() {
            obj.insert("cache_control".into(), cache_control);
        }
    }
    cached
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cache_disabled_when_param_false() {
        let msgs = vec![
            json!({"role": "system", "content": "You are a helpful assistant."}),
            json!({"role": "user", "content": "Hello"}),
        ];
        let params = json!({"extra_body": {"cache_control": false}});
        let (cached, _) = with_prompt_caching(&msgs, None, &params);
        assert_eq!(cached, msgs);
    }

    #[test]
    fn test_cache_added_to_system_message() {
        let msgs = vec![
            json!({"role": "system", "content": "You are a helpful assistant."}),
            json!({"role": "user", "content": "Hello"}),
        ];
        let params = json!({"extra_body": {"cache_control": true}});
        let (cached, _) = with_prompt_caching(&msgs, None, &params);
        let system = &cached[0];
        let content = system.get("content").unwrap().as_array().unwrap();
        assert_eq!(content[0].get("type").unwrap(), "text");
        assert!(content[0].get("cache_control").is_some());
    }

    #[test]
    fn test_cache_added_to_last_cacheable_user() {
        let msgs = vec![
            json!({"role": "user", "content": "First message"}),
            json!({"role": "assistant", "content": "Response"}),
            json!({"role": "user", "content": "Second message"}),
        ];
        let params = json!({"extra_body": {"cache_control": true}});
        let (cached, _) = with_prompt_caching(&msgs, None, &params);
        let user = &cached[0];
        let content = user.get("content").unwrap().as_array().unwrap();
        assert!(content[0].get("cache_control").is_some());
    }

    #[test]
    fn test_cache_added_to_last_tool() {
        let msgs = vec![
            json!({"role": "system", "content": "System prompt"}),
        ];
        let tools = vec![
            json!({"name": "read", "description": "Read a file", "input_schema": {"type": "object", "properties": {}}}),
            json!({"name": "write", "description": "Write a file", "input_schema": {"type": "object", "properties": {}}}),
        ];
        let params = json!({"extra_body": {"cache_control": true}});
        let (_, cached_tools) = with_prompt_caching(&msgs, Some(&tools), &params);
        let cached_tools = cached_tools.unwrap();
        assert!(cached_tools.last().unwrap().get("cache_control").is_some());
    }

    #[test]
    fn test_no_cache_for_single_message() {
        let msgs = vec![
            json!({"role": "user", "content": "Hello"}),
        ];
        let params = json!({"extra_body": {"cache_control": true}});
        let (cached, _) = with_prompt_caching(&msgs, None, &params);
        assert_eq!(cached, msgs);
    }
}
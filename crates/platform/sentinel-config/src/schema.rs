use serde_json::{json, Value};

/// Build the JSON Schema describing `SentinelConfig` (sentinel.toml).
///
/// Hand-rolled (no `schemars` dependency) so the shape stays explicit and
/// versioned. Consumers: IDE validation + autocompletion for sentinel.toml,
/// exposed via the `sentinel schema` CLI subcommand.
pub fn config_json_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://sentinel.dev/schemas/sentinel-config.schema.json",
        "title": "SentinelConfig",
        "description": "Configuration for the sentinel AI coding agent.",
        "type": "object",
        "properties": {
            "agent": {
                "type": "object",
                "description": "Agent behaviour settings.",
                "properties": {
                    "default_model": {
                        "type": "string",
                        "description": "Model used when none is passed explicitly. Must be provided by a configured provider.",
                        "default": "gpt-4o"
                    },
                    "max_turns": { "type": "integer", "minimum": 1, "default": 50 },
                    "max_iterations": { "type": "integer", "minimum": 1, "default": 100 },
                    "max_tokens": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Per-turn completion token budget. Unset = provider default."
                    },
                    "reasoning_effort": {
                        "type": "string",
                        "enum": ["low", "medium", "high"],
                        "description": "Reasoning effort for reasoning models."
                    },
                    "yolo_mode": { "type": "boolean", "description": "Auto-approve tool actions (dangerous).", "default": false },
                    "verbose": { "type": "boolean", "default": false }
                }
            },
            "providers": {
                "type": "array",
                "description": "LLM providers. Built-in providers (openai, anthropic, google-ai-studio, deepseek, ollama, vllm, lm-studio, llamacpp) are merged with these.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Unique provider id used to reference the provider." },
                        "name": { "type": "string" },
                        "base_url": { "type": "string", "format": "uri" },
                        "provider": {
                            "type": "string",
                            "description": "Provider type",
                            "enum": [
                                "openai",
                                "anthropic",
                                "google-ai-studio",
                                "deepseek",
                                "ollama",
                                "vllm",
                                "lm-studio",
                                "llamacpp",
                                "openrouter"
                            ]
                        },
                        "disabled": {
                            "type": "boolean",
                            "description": "Whether the provider is disabled",
                            "default": false
                        },
                        "auth": {
                            "oneOf": [
                                { "type": "object", "properties": { "var": { "type": "string" } }, "required": ["var"], "additionalProperties": false, "description": "API key from an environment variable." },
                                { "type": "object", "properties": { "token": { "type": "string" } }, "required": ["token"], "additionalProperties": false, "description": "Static bearer token." },
                                { "type": "object", "properties": { "api_key": { "type": "string" } }, "required": ["api_key"], "additionalProperties": false, "description": "API key for the provider, inline." },
                                { "type": "null" }
                            ],
                            "description": "How to resolve the API key: env var name, inline api_key, inline token, or none."
                        },
                        "models": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "name": { "type": "string" },
                                    "context_window": { "type": "integer", "minimum": 0, "default": 0 },
                                    "supports_streaming": { "type": "boolean", "default": false },
                                    "supports_tools": { "type": "boolean", "default": false }
                                },
                                "required": ["id", "name"]
                            }
                        },
                        "timeout_secs": { "type": "integer", "minimum": 1, "default": 120 },
                        "extra_headers": {
                            "type": "object",
                            "description": "Extra HTTP headers sent with every request.",
                            "additionalProperties": { "type": "string" }
                        }
                    },
                    "required": ["id", "name", "base_url", "models"]
                }
            },
            "mcp_servers": {
                "type": "array",
                "description": "Model Context Protocol servers exposed as tools.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "name": { "type": "string" },
                        "transport": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "description": "Spawn the MCP server as a subprocess.",
                                    "properties": {
                                        "type": { "enum": ["stdio"] },
                                        "command": { "type": "string", "description": "Command to execute for the MCP server" },
                                        "args": { "type": "array", "items": { "type": "string" } },
                                        "env": { "type": "object", "additionalProperties": { "type": "string" } }
                                    },
                                    "required": ["type", "command"]
                                },
                                {
                                    "type": "object",
                                    "description": "Connect to a remote MCP server over http or websocket.",
                                    "properties": {
                                        "type": { "enum": ["http", "websocket"] },
                                        "url": { "type": "string", "format": "uri" },
                                        "headers": { "type": "object", "additionalProperties": { "type": "string" } }
                                    },
                                    "required": ["type", "url"]
                                }
                            ],
                            "description": "Transport tagged by type: stdio, http or websocket."
                        }
                    },
                    "required": ["id", "name", "transport"]
                }
            },
            "thread_store": {
                "type": "string",
                "enum": ["memory", "json", "sqlite"],
                "description": "Persistence backend for conversation threads.",
                "default": "memory"
            },
            "debug": {
                "type": "object",
                "description": "Debug flags.",
                "properties": {
                    "enabled": { "type": "boolean", "default": false },
                    "verbose": { "type": "boolean", "default": false }
                }
            },
            "context": {
                "type": "object",
                "description": "Working context paths scanned by the agent.",
                "properties": {
                    "paths": { "type": "array", "items": { "type": "string" }, "default": ["."] },
                    "exclude": { "type": "array", "items": { "type": "string" }, "default": [] }
                }
            },
            "theme": {
                "type": "object",
                "description": "TUI theme selection.",
                "properties": {
                    "name": { "type": "string", "default": "opencode-dark" }
                }
            },
            "lsp_servers": {
                "type": "array",
                "description": "Language Server Protocol servers for IDE integration.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "command": { "type": "string", "default": "" },
                        "args": { "type": "array", "items": { "type": "string" }, "default": [] },
                        "languages": { "type": "array", "items": { "type": "string" }, "default": [] }
                    },
                    "required": ["id"]
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_valid_object_with_draft_meta() {
        let s = config_json_schema();
        assert_eq!(s["type"], "object");
        assert_eq!(
            s["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert!(s["properties"]["agent"].is_object());
        assert_eq!(s["properties"]["providers"]["type"], "array");
    }

    #[test]
    fn schema_covers_all_spec_sections() {
        let s = config_json_schema();
        let props = s["properties"].as_object().unwrap();
        for section in [
            "agent",
            "providers",
            "mcp_servers",
            "thread_store",
            "debug",
            "context",
            "theme",
            "lsp_servers",
        ] {
            assert!(props.contains_key(section), "missing section: {section}");
        }
    }

    #[test]
    fn providers_have_disabled_kind_and_api_key() {
        let s = config_json_schema();
        let p = &s["properties"]["providers"]["items"]["properties"];
        assert_eq!(p["disabled"]["default"], false);
        assert_eq!(p["disabled"]["type"], "boolean");
        assert_eq!(p["provider"]["type"], "string");
        let kinds: Vec<&str> = p["provider"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for known in ["openai", "anthropic", "ollama", "vllm"] {
            assert!(kinds.contains(&known), "missing provider kind: {known}");
        }
        let auth_forms: Vec<&str> = p["auth"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.get("properties"))
            .filter_map(|props| props.get("api_key"))
            .map(|_| "api_key")
            .collect();
        assert_eq!(auth_forms, vec!["api_key"], "auth must accept inline api_key");
    }

    #[test]
    fn mcp_transport_is_conditional_on_type() {
        let s = config_json_schema();
        let forms = s["properties"]["mcp_servers"]["items"]["properties"]["transport"]["oneOf"]
            .as_array()
            .unwrap();
        assert_eq!(forms.len(), 2, "stdio and remote transports");

        let stdio = &forms[0];
        let stdio_required: Vec<&str> = stdio["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(stdio_required, vec!["type", "command"]);
        assert_eq!(stdio["properties"]["command"]["description"], "Command to execute for the MCP server");

        let remote = &forms[1];
        let remote_required: Vec<&str> = remote["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(remote_required, vec!["type", "url"]);
    }

    #[test]
    fn agent_has_token_limit_and_reasoning_effort() {
        let s = config_json_schema();
        let a = &s["properties"]["agent"]["properties"];
        assert_eq!(a["max_tokens"]["type"], "integer");
        assert_eq!(a["max_tokens"]["minimum"], 1);
        assert_eq!(a["reasoning_effort"]["type"], "string");
        assert_eq!(
            a["reasoning_effort"]["enum"],
            serde_json::json!(["low", "medium", "high"])
        );
    }
}

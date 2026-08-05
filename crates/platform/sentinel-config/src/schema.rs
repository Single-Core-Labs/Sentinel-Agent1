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
                        "id": { "type": "string" },
                        "name": { "type": "string" },
                        "base_url": { "type": "string", "format": "uri" },
                        "auth": {
                            "oneOf": [
                                { "type": "object", "properties": { "var": { "type": "string" } }, "required": ["var"], "additionalProperties": false },
                                { "type": "object", "properties": { "token": { "type": "string" } }, "required": ["token"], "additionalProperties": false },
                                { "type": "null" }
                            ],
                            "description": "How to resolve the API key: env var name, inline token, or none."
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
                            "type": "object",
                            "description": "Transport tagged by type: stdio, http or websocket.",
                            "properties": {
                                "type": { "enum": ["stdio", "http", "websocket"] },
                                "command": { "type": "string" },
                                "args": { "type": "array", "items": { "type": "string" } },
                                "env": { "type": "object", "additionalProperties": { "type": "string" } },
                                "url": { "type": "string", "format": "uri" },
                                "headers": { "type": "object", "additionalProperties": { "type": "string" } }
                            },
                            "required": ["type"]
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
}

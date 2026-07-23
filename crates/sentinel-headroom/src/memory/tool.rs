use std::sync::Arc;
use async_trait::async_trait;
use sentinel_tools::{Tool, ToolContext, ToolOutput};

use super::types::*;
use super::store::MemoryStore;

pub struct MemorizeTool {
    store: Arc<dyn MemoryStore>,
}

impl MemorizeTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemorizeTool {
    fn name(&self) -> &str { "headroom_memorize" }

    fn description(&self) -> &str {
        "Store a fact, preference, decision, or other information in persistent memory. \
         This information will be recalled in future conversations. \
         Use this when the user explicitly tells you something they want remembered, \
         or when you discover an important fact about the user, their project, or their preferences."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The fact to remember. Be concise and specific. Example: 'User prefers Python for backend work'"
                },
                "category": {
                    "type": "string",
                    "enum": ["preference", "fact", "context", "entity", "decision", "insight"],
                    "description": "Category of the memory",
                    "default": "fact"
                },
                "importance": {
                    "type": "number",
                    "description": "How important this memory is (0.0 to 1.0)",
                    "default": 0.5
                },
                "user_id": {
                    "type": "string",
                    "description": "The user this memory belongs to (defaults to current user)"
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let content = args["content"].as_str().unwrap_or("");
        if content.is_empty() {
            return ToolOutput::err("content is required");
        }
        if content.len() > 2000 {
            return ToolOutput::err("content must be 2000 characters or fewer");
        }

        let user_id = args.get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let category_str = args.get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("fact");
        let category = MemoryCategory::from_str(category_str);

        let importance = args.get("importance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        let now = now_seconds();
        let memory = Memory {
            id: generate_memory_id(),
            user_id: user_id.to_string(),
            session_id: None,
            agent_id: None,
            content: content.to_string(),
            category,
            importance,
            scope: MemoryScope::User,
            supersedes: None,
            superseded_by: None,
            supersede_reason: None,
            source: MemorySource::ToolCall,
            source_turn: 0,
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
        };

        match self.store.add(memory.clone()).await {
            Ok(m) => ToolOutput::ok(serde_json::json!({
                "status": "stored",
                "memory_id": m.id,
                "content": m.content,
                "category": m.category.as_str(),
                "importance": m.importance,
            }).to_string()),
            Err(e) => ToolOutput::err(format!("Failed to store memory: {}", e)),
        }
    }
}

pub struct RecallTool {
    store: Arc<dyn MemoryStore>,
}

impl RecallTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for RecallTool {
    fn name(&self) -> &str { "headroom_recall" }

    fn description(&self) -> &str {
        "Search stored memories for information about the user, their preferences, \
         project context, decisions, or any other previously stored facts. \
         Use this when you need to remember something the user told you in the past."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to search for (semantic search over all memories)"
                },
                "category": {
                    "type": "string",
                    "enum": ["preference", "fact", "context", "entity", "decision", "insight", ""],
                    "description": "Filter by category (optional)",
                    "default": ""
                },
                "top_k": {
                    "type": "integer",
                    "description": "How many results to return (1-20)",
                    "default": 5
                },
                "user_id": {
                    "type": "string",
                    "description": "The user to recall memories for (defaults to current user)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let query = args["query"].as_str().unwrap_or("");
        if query.is_empty() {
            return ToolOutput::err("query is required");
        }

        let user_id = args.get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let top_k = args.get("top_k")
            .and_then(|v| v.as_i64())
            .unwrap_or(5)
            .clamp(1, 20) as usize;

        let category_filter = args.get("category")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| vec![MemoryCategory::from_str(s)]);

        let mut filter = MemoryFilter::for_user(user_id);
        filter.categories = category_filter;
        filter.limit = top_k;

        match self.store.search(query, &filter).await {
            Ok(results) => {
                if results.is_empty() {
                    return ToolOutput::ok(serde_json::json!({
                        "status": "no_results",
                        "message": "No matching memories found"
                    }).to_string());
                }

                let memories: Vec<serde_json::Value> = results.iter().map(|sm| {
                    serde_json::json!({
                        "content": sm.memory.content,
                        "category": sm.memory.category.as_str(),
                        "importance": sm.memory.importance,
                        "score": format!("{:.2}", sm.score),
                        "stored": sm.memory.created_at,
                    })
                }).collect();

                ToolOutput::ok(serde_json::json!({
                    "status": "found",
                    "count": memories.len(),
                    "memories": memories,
                }).to_string())
            }
            Err(e) => ToolOutput::err(format!("Failed to search memories: {}", e)),
        }
    }
}

pub struct ForgetTool {
    store: Arc<dyn MemoryStore>,
}

impl ForgetTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for ForgetTool {
    fn name(&self) -> &str { "headroom_forget" }

    fn description(&self) -> &str {
        "Delete a specific memory by its ID. Use after headroom_recall to find the ID."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "memory_id": {
                    "type": "string",
                    "description": "The ID of the memory to delete"
                }
            },
            "required": ["memory_id"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let memory_id = args["memory_id"].as_str().unwrap_or("");
        if memory_id.is_empty() {
            return ToolOutput::err("memory_id is required");
        }

        match self.store.delete(memory_id).await {
            Ok(true) => ToolOutput::ok(serde_json::json!({
                "status": "deleted",
                "memory_id": memory_id,
            }).to_string()),
            Ok(false) => ToolOutput::err(format!("Memory not found: {}", memory_id)),
            Err(e) => ToolOutput::err(format!("Failed to delete memory: {}", e)),
        }
    }
}

pub struct MemoryStatsTool {
    store: Arc<dyn MemoryStore>,
}

impl MemoryStatsTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemoryStatsTool {
    fn name(&self) -> &str { "headroom_memory_stats" }

    fn description(&self) -> &str {
        "Get statistics about stored memories for the current user."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": {
                    "type": "string",
                    "description": "The user to get stats for (defaults to current user)"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
        let user_id = args.get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        match self.store.stats(user_id).await {
            Ok(stats) => ToolOutput::ok(serde_json::json!({
                "total": stats.total,
                "active": stats.active,
                "superseded": stats.superseded,
                "by_category": stats.by_category.iter().map(|(c, n)| serde_json::json!({
                    "category": c.as_str(),
                    "count": n,
                })).collect::<Vec<_>>(),
            }).to_string()),
            Err(e) => ToolOutput::err(format!("Failed to get stats: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::store::InMemoryStore;

    fn create_store() -> Arc<InMemoryStore> {
        Arc::new(InMemoryStore::new())
    }

    #[tokio::test]
    async fn test_memorize_requires_content() {
        let store = create_store();
        let tool = MemorizeTool::new(store);
        let result = tool.execute(serde_json::json!({}), &ToolContext::new()).await;
        assert!(result.is_error);
        assert!(result.text.contains("required"));
    }

    #[tokio::test]
    async fn test_memorize_and_recall() {
        let store = create_store();
        let memorize = MemorizeTool::new(store.clone());
        let recall = RecallTool::new(store.clone());

        let mem_result = memorize.execute(serde_json::json!({
            "content": "User prefers Python for backend work",
            "category": "preference",
            "importance": 0.8,
        }), &ToolContext::new()).await;
        assert!(!mem_result.is_error, "memorize failed: {}", mem_result.text);

        let rec_result = recall.execute(serde_json::json!({
            "query": "python preference",
            "top_k": 5,
        }), &ToolContext::new()).await;
        assert!(!rec_result.is_error, "recall failed: {}", rec_result.text);
        assert!(rec_result.text.contains("found"), "should find results: {}", rec_result.text);
    }

    #[tokio::test]
    async fn test_recall_no_results() {
        let store = create_store();
        let recall = RecallTool::new(store);
        let result = recall.execute(serde_json::json!({
            "query": "nonexistent",
        }), &ToolContext::new()).await;
        assert!(!result.is_error);
        assert!(result.text.contains("no_results"));
    }

    #[tokio::test]
    async fn test_forget() {
        let store = create_store();
        let memorize = MemorizeTool::new(store.clone());
        let forget = ForgetTool::new(store.clone());
        let recall = RecallTool::new(store.clone());

        let mem_result = memorize.execute(serde_json::json!({
            "content": "Test memory",
            "category": "fact",
        }), &ToolContext::new()).await;

        let parsed: serde_json::Value = serde_json::from_str(&mem_result.text).unwrap();
        let memory_id = parsed["memory_id"].as_str().unwrap().to_string();

        let forget_result = forget.execute(serde_json::json!({
            "memory_id": memory_id,
        }), &ToolContext::new()).await;
        assert!(!forget_result.is_error);

        let rec_result = recall.execute(serde_json::json!({
            "query": "Test memory",
        }), &ToolContext::new()).await;
        assert!(rec_result.text.contains("no_results"));
    }

    #[tokio::test]
    async fn test_memory_stats() {
        let store = create_store();
        let memorize = MemorizeTool::new(store.clone());
        let stats_tool = MemoryStatsTool::new(store.clone());

        memorize.execute(serde_json::json!({
            "content": "Fact one",
            "category": "fact",
        }), &ToolContext::new()).await;

        memorize.execute(serde_json::json!({
            "content": "Prefers dark mode",
            "category": "preference",
        }), &ToolContext::new()).await;

        let result = stats_tool.execute(serde_json::json!({}), &ToolContext::new()).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.text).unwrap();
        assert_eq!(parsed["total"].as_i64(), Some(2));
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type NodeId = String;
pub type EdgeId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    pub id: NodeId,
    pub label: String,
    pub parent_id: Option<NodeId>,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEdge {
    pub id: EdgeId,
    pub source_id: NodeId,
    pub target_id: NodeId,
    pub relation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGraph {
    pub root_id: Option<NodeId>,
    pub nodes: Vec<AgentNode>,
    pub edges: Vec<AgentEdge>,
}

impl AgentGraph {
    pub fn new() -> Self {
        Self { root_id: None, nodes: Vec::new(), edges: Vec::new() }
    }

    pub fn add_node(&mut self, label: &str, parent_id: Option<&str>) -> NodeId {
        let id = Uuid::new_v4().to_string();
        let node = AgentNode {
            id: id.clone(),
            label: label.to_string(),
            parent_id: parent_id.map(String::from),
            metadata: serde_json::Value::Null,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        if self.root_id.is_none() {
            self.root_id = Some(id.clone());
        }
        self.nodes.push(node);
        id
    }

    pub fn add_edge(&mut self, source_id: &str, target_id: &str, relation: &str) {
        let id = Uuid::new_v4().to_string();
        self.edges.push(AgentEdge {
            id,
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            relation: relation.to_string(),
        });
    }

    pub fn children_of(&self, node_id: &str) -> Vec<&AgentNode> {
        self.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(node_id)).collect()
    }

    pub fn path_to_root(&self, node_id: &str) -> Vec<&AgentNode> {
        let mut path = Vec::new();
        let mut current = self.nodes.iter().find(|n| n.id == node_id);
        while let Some(node) = current {
            path.push(node);
            current = node.parent_id.as_ref()
                .and_then(|pid| self.nodes.iter().find(|n| n.id == *pid));
        }
        path
    }
}

impl Default for AgentGraph {
    fn default() -> Self {
        Self::new()
    }
}

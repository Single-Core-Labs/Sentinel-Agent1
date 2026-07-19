use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Status of a thread spawn edge between parent and child agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpawnEdgeStatus {
    Open,
    Closed,
}

/// A directed edge representing a parent-child spawn relationship.
///
/// When an agent spawns a sub-agent, an edge is created from the
/// parent thread ID to the child thread ID. The edge tracks the
/// lifecycle state (Open while the child is active, Closed when done).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSpawnEdge {
    pub id: String,
    pub parent_thread_id: String,
    pub child_thread_id: String,
    pub status: SpawnEdgeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ThreadSpawnEdge {
    pub fn new(parent_thread_id: &str, child_thread_id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            parent_thread_id: parent_thread_id.to_string(),
            child_thread_id: child_thread_id.to_string(),
            status: SpawnEdgeStatus::Open,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_open(&self) -> bool {
        self.status == SpawnEdgeStatus::Open
    }

    pub fn is_closed(&self) -> bool {
        self.status == SpawnEdgeStatus::Closed
    }
}

/// A node in the agent graph — one thread of execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    pub thread_id: String,
    pub parent_id: Option<String>,
}

/// In-memory agent graph (tree topology).
///
/// Maintains parent-child relationships between agent threads.
/// The root thread has no parent. All children are ordered
/// deterministically by creation time.
#[derive(Debug, Clone)]
pub struct AgentGraph {
    edges: Vec<ThreadSpawnEdge>,
}

impl AgentGraph {
    pub fn new() -> Self {
        Self { edges: Vec::new() }
    }

    pub fn add_edge(&mut self, edge: ThreadSpawnEdge) {
        self.edges.push(edge);
    }

    pub fn upsert_edge(&mut self, parent: &str, child: &str, status: SpawnEdgeStatus) {
        if let Some(edge) = self.edges.iter_mut()
            .find(|e| e.parent_thread_id == parent && e.child_thread_id == child)
        {
            edge.status = status;
            edge.updated_at = Utc::now();
        } else {
            self.edges.push(ThreadSpawnEdge {
                id: Uuid::new_v4().to_string(),
                parent_thread_id: parent.to_string(),
                child_thread_id: child.to_string(),
                status,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }
    }

    pub fn set_edge_status(&mut self, parent: &str, child: &str, status: SpawnEdgeStatus) {
        if let Some(edge) = self.edges.iter_mut()
            .find(|e| e.parent_thread_id == parent && e.child_thread_id == child)
        {
            edge.status = status;
            edge.updated_at = Utc::now();
        }
    }

    /// Get immediate children of a thread, ordered by creation time.
    pub fn children(&self, thread_id: &str) -> Vec<&ThreadSpawnEdge> {
        let mut result: Vec<_> = self.edges.iter()
            .filter(|e| e.parent_thread_id == thread_id)
            .collect();
        result.sort_by_key(|e| e.created_at);
        result
    }

    /// Get all descendants (recursive), ordered by creation time.
    pub fn descendants(&self, thread_id: &str) -> Vec<&ThreadSpawnEdge> {
        let mut result = Vec::new();
        let mut stack = vec![thread_id.to_string()];
        while let Some(current) = stack.pop() {
            let mut children: Vec<_> = self.edges.iter()
                .filter(|e| e.parent_thread_id == current)
                .collect();
            children.sort_by_key(|e| e.created_at);
            for child in &children {
                result.push(*child);
                stack.push(child.child_thread_id.clone());
            }
        }
        result
    }

    pub fn root(&self) -> Vec<&ThreadSpawnEdge> {
        let mut roots: Vec<_> = self.edges.iter()
            .filter(|e| {
                !self.edges.iter().any(|other| other.child_thread_id == e.parent_thread_id)
            })
            .collect();
        roots.sort_by_key(|e| e.created_at);
        roots
    }

    pub fn nodes(&self) -> Vec<AgentNode> {
        let mut map: std::collections::BTreeMap<String, Option<String>> = std::collections::BTreeMap::new();
        for edge in &self.edges {
            map.entry(edge.parent_thread_id.clone()).or_insert(None);
            map.entry(edge.child_thread_id.clone()).or_insert(Some(edge.parent_thread_id.clone()));
        }
        map.into_iter().map(|(thread_id, parent_id)| AgentNode { thread_id, parent_id }).collect()
    }

    pub fn clear(&mut self) {
        self.edges.clear();
    }
}

impl Default for AgentGraph {
    fn default() -> Self {
        Self::new()
    }
}

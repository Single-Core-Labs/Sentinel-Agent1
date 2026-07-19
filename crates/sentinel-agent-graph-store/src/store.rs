use rusqlite::{Connection, params};
use std::sync::Mutex;
use crate::graph::{AgentNode, AgentEdge};

pub struct GraphStore {
    conn: Mutex<Connection>,
}

impl GraphStore {
    pub fn new(path: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path)
            .map_err(|e| StoreError::OpenError(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_nodes (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                parent_id TEXT,
                metadata TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_edges (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation TEXT NOT NULL
            );"
        ).map_err(|e| StoreError::QueryError(e.to_string()))?;

        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn insert_node(&self, node: &AgentNode) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO agent_nodes (id, label, parent_id, metadata, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                node.id,
                node.label,
                node.parent_id,
                serde_json::to_string(&node.metadata).unwrap_or_default(),
                node.created_at,
            ],
        ).map_err(|e| StoreError::QueryError(e.to_string()))?;
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> Result<Option<AgentNode>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, label, parent_id, metadata, created_at FROM agent_nodes WHERE id = ?1"
        ).map_err(|e| StoreError::QueryError(e.to_string()))?;

        let mut rows = stmt.query_map(params![id], |row| {
            let metadata_str: String = row.get(3)?;
            Ok(AgentNode {
                id: row.get(0)?,
                label: row.get(1)?,
                parent_id: row.get(2)?,
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                created_at: row.get(4)?,
            })
        }).map_err(|e| StoreError::QueryError(e.to_string()))?;

        match rows.next() {
            Some(Ok(node)) => Ok(Some(node)),
            Some(Err(e)) => Err(StoreError::QueryError(e.to_string())),
            None => Ok(None),
        }
    }

    pub fn children_of(&self, parent_id: &str) -> Result<Vec<AgentNode>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, label, parent_id, metadata, created_at FROM agent_nodes WHERE parent_id = ?1"
        ).map_err(|e| StoreError::QueryError(e.to_string()))?;

        let nodes = stmt.query_map(params![parent_id], |row| {
            let metadata_str: String = row.get(3)?;
            Ok(AgentNode {
                id: row.get(0)?,
                label: row.get(1)?,
                parent_id: row.get(2)?,
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                created_at: row.get(4)?,
            })
        }).map_err(|e| StoreError::QueryError(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(nodes)
    }

    pub fn insert_edge(&self, edge: &AgentEdge) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO agent_edges (id, source_id, target_id, relation) VALUES (?1, ?2, ?3, ?4)",
            params![edge.id, edge.source_id, edge.target_id, edge.relation],
        ).map_err(|e| StoreError::QueryError(e.to_string()))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "DELETE FROM agent_nodes; DELETE FROM agent_edges;"
        ).map_err(|e| StoreError::QueryError(e.to_string()))
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Failed to open store: {0}")]
    OpenError(String),
    #[error("Query error: {0}")]
    QueryError(String),
}

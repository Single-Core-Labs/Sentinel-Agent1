use async_trait::async_trait;
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};
use chrono::{DateTime, Utc};

use crate::graph::{ThreadSpawnEdge, SpawnEdgeStatus, AgentNode};
use crate::store::{AgentGraphStore, GraphStoreError};

/// SQLite-backed implementation of `AgentGraphStore`.
///
/// Uses a single table `thread_spawn_edges` to store parent-child
/// relationships. All list operations return results in deterministic
/// order (by `created_at`, then `id`).
#[derive(Debug, Clone)]
pub struct LocalAgentGraphStore {
    conn: Arc<Mutex<Connection>>,
}

impl LocalAgentGraphStore {
    /// Open (or create) the SQLite database at the given path.
    /// Uses in-memory DB if path is `:memory:`.
    pub fn open(path: &str) -> Result<Self, GraphStoreError> {
        let conn = Connection::open(path)
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        let store = Self { conn: Arc::new(Mutex::new(conn)) };
        store.init_tables()?;
        Ok(store)
    }

    /// Open an in-memory SQLite database (for testing).
    pub fn in_memory() -> Result<Self, GraphStoreError> {
        Self::open(":memory:")
    }

    fn init_tables(&self) -> Result<(), GraphStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS thread_spawn_edges (
                id TEXT PRIMARY KEY,
                parent_thread_id TEXT NOT NULL,
                child_thread_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'Open',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_parent ON thread_spawn_edges(parent_thread_id);
            CREATE INDEX IF NOT EXISTS idx_child ON thread_spawn_edges(child_thread_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_parent_child ON thread_spawn_edges(parent_thread_id, child_thread_id);"
        ).map_err(|e| GraphStoreError::DatabaseError(e.to_string()))
    }

    fn parse_edge(row: &rusqlite::Row) -> rusqlite::Result<ThreadSpawnEdge> {
        let status_str: String = row.get(3)?;
        let status = match status_str.as_str() {
            "Open" => SpawnEdgeStatus::Open,
            "Closed" => SpawnEdgeStatus::Closed,
            _ => SpawnEdgeStatus::Open,
        };
        Ok(ThreadSpawnEdge {
            id: row.get(0)?,
            parent_thread_id: row.get(1)?,
            child_thread_id: row.get(2)?,
            status,
            created_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
            updated_at: row.get::<_, String>(5)?.parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

#[async_trait]
impl AgentGraphStore for LocalAgentGraphStore {
    async fn upsert_thread_spawn_edge(
        &self,
        parent_thread_id: &str,
        child_thread_id: &str,
        status: SpawnEdgeStatus,
    ) -> Result<(), GraphStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        let status_str = match status {
            SpawnEdgeStatus::Open => "Open",
            SpawnEdgeStatus::Closed => "Closed",
        };

        conn.execute(
            "INSERT INTO thread_spawn_edges (id, parent_thread_id, child_thread_id, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(parent_thread_id, child_thread_id)
             DO UPDATE SET status = ?4, updated_at = ?6",
            params![
                uuid::Uuid::new_v4().to_string(),
                parent_thread_id,
                child_thread_id,
                status_str,
                now,
                now,
            ],
        ).map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn set_thread_spawn_edge_status(
        &self,
        parent_thread_id: &str,
        child_thread_id: &str,
        status: SpawnEdgeStatus,
    ) -> Result<(), GraphStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        let status_str = match status {
            SpawnEdgeStatus::Open => "Open",
            SpawnEdgeStatus::Closed => "Closed",
        };

        let rows = conn.execute(
            "UPDATE thread_spawn_edges SET status = ?1, updated_at = ?2
             WHERE parent_thread_id = ?3 AND child_thread_id = ?4",
            params![status_str, now, parent_thread_id, child_thread_id],
        ).map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        if rows == 0 {
            return Err(GraphStoreError::EdgeNotFound {
                parent: parent_thread_id.to_string(),
                child: child_thread_id.to_string(),
            });
        }
        Ok(())
    }

    async fn list_thread_spawn_children(
        &self,
        thread_id: &str,
    ) -> Result<Vec<ThreadSpawnEdge>, GraphStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, parent_thread_id, child_thread_id, status, created_at, updated_at
             FROM thread_spawn_edges
             WHERE parent_thread_id = ?1
             ORDER BY created_at ASC, id ASC"
        ).map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        let edges = stmt.query_map(params![thread_id], Self::parse_edge)
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        Ok(edges)
    }

    async fn list_thread_spawn_descendants(
        &self,
        thread_id: &str,
    ) -> Result<Vec<ThreadSpawnEdge>, GraphStoreError> {
        let mut result = Vec::new();
        let mut stack = vec![thread_id.to_string()];

        while let Some(current) = stack.pop() {
            let children = self.list_thread_spawn_children(&current).await?;
            for child in &children {
                result.push(child.clone());
                stack.push(child.child_thread_id.clone());
            }
        }

        Ok(result)
    }

    async fn get_thread_parent(
        &self,
        thread_id: &str,
    ) -> Result<Option<ThreadSpawnEdge>, GraphStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, parent_thread_id, child_thread_id, status, created_at, updated_at
             FROM thread_spawn_edges
             WHERE child_thread_id = ?1
             LIMIT 1"
        ).map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        let mut rows = stmt.query_map(params![thread_id], Self::parse_edge)
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        match rows.next() {
            Some(Ok(edge)) => Ok(Some(edge)),
            Some(Err(e)) => Err(GraphStoreError::DatabaseError(e.to_string())),
            None => Ok(None),
        }
    }

    async fn list_root_threads(&self) -> Result<Vec<ThreadSpawnEdge>, GraphStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT e.id, e.parent_thread_id, e.child_thread_id, e.status, e.created_at, e.updated_at
             FROM thread_spawn_edges e
             LEFT JOIN thread_spawn_edges p ON e.parent_thread_id = p.child_thread_id
             WHERE p.id IS NULL
             ORDER BY e.created_at ASC, e.id ASC"
        ).map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        let edges = stmt.query_map([], Self::parse_edge)
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        Ok(edges)
    }

    async fn list_all_nodes(&self) -> Result<Vec<AgentNode>, GraphStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT parent_thread_id FROM thread_spawn_edges
             UNION
             SELECT DISTINCT child_thread_id FROM thread_spawn_edges
             ORDER BY 1"
        ).map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        let thread_ids: Vec<String> = stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        let mut nodes = Vec::new();
        for tid in thread_ids {
            let parent = Self::get_parent_for_node(&conn, &tid)?;
            nodes.push(AgentNode { thread_id: tid, parent_id: parent });
        }

        Ok(nodes)
    }

    async fn clear(&self) -> Result<(), GraphStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        conn.execute("DELETE FROM thread_spawn_edges", [])
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;
        Ok(())
    }
}

impl LocalAgentGraphStore {
    fn get_parent_for_node(conn: &Connection, thread_id: &str) -> Result<Option<String>, GraphStoreError> {
        let mut stmt = conn.prepare(
            "SELECT parent_thread_id FROM thread_spawn_edges WHERE child_thread_id = ?1 LIMIT 1"
        ).map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        let mut rows = stmt.query_map(params![thread_id], |row| row.get::<_, String>(0))
            .map_err(|e| GraphStoreError::DatabaseError(e.to_string()))?;

        match rows.next() {
            Some(Ok(pid)) => Ok(Some(pid)),
            Some(Err(e)) => Err(GraphStoreError::DatabaseError(e.to_string())),
            None => Ok(None),
        }
    }
}

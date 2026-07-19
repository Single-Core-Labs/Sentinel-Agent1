use async_trait::async_trait;
use crate::graph::{ThreadSpawnEdge, SpawnEdgeStatus, AgentNode};

/// Storage-neutral interface for managing agent thread topology.
///
/// Defines the contract for persisting and querying parent-child
/// relationships among spawned agent threads. Implementations may
/// use SQLite, PostgreSQL, in-memory stores, or other backends.
#[async_trait]
pub trait AgentGraphStore: Send + Sync {
    /// Create or update a spawn edge between a parent thread and a child thread.
    async fn upsert_thread_spawn_edge(
        &self,
        parent_thread_id: &str,
        child_thread_id: &str,
        status: SpawnEdgeStatus,
    ) -> Result<(), GraphStoreError>;

    /// Set the status of an existing spawn edge.
    async fn set_thread_spawn_edge_status(
        &self,
        parent_thread_id: &str,
        child_thread_id: &str,
        status: SpawnEdgeStatus,
    ) -> Result<(), GraphStoreError>;

    /// Get immediate children of a thread, deterministically ordered.
    async fn list_thread_spawn_children(
        &self,
        thread_id: &str,
    ) -> Result<Vec<ThreadSpawnEdge>, GraphStoreError>;

    /// Get all descendants (recursive), deterministically ordered.
    async fn list_thread_spawn_descendants(
        &self,
        thread_id: &str,
    ) -> Result<Vec<ThreadSpawnEdge>, GraphStoreError>;

    /// Get the parent of a thread, if any.
    async fn get_thread_parent(
        &self,
        thread_id: &str,
    ) -> Result<Option<ThreadSpawnEdge>, GraphStoreError>;

    /// List all root threads (no parent), deterministically ordered.
    async fn list_root_threads(&self) -> Result<Vec<ThreadSpawnEdge>, GraphStoreError>;

    /// Get all nodes in the graph.
    async fn list_all_nodes(&self) -> Result<Vec<AgentNode>, GraphStoreError>;

    /// Remove all data from the store.
    async fn clear(&self) -> Result<(), GraphStoreError>;
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphStoreError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Edge not found: parent={parent}, child={child}")]
    EdgeNotFound { parent: String, child: String },
}

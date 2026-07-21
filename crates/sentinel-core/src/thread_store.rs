use async_trait::async_trait;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::budget::BudgetGuard;
use crate::thread::AgentThread;
use crate::conversation::Conversation;

/// Thread persisted representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedThread {
    pub id: String,
    pub conversation: Conversation,
    pub turn: u32,
    pub iterations: u32,
    pub max_turns: u32,
    pub max_iterations: u32,
    pub yolo_mode: bool,
    pub parent_thread_id: Option<String>,
    pub budget_cost_cap_usd: Option<f64>,
    pub budget_total_spend_usd: f64,
}

impl From<&AgentThread> for SavedThread {
    fn from(t: &AgentThread) -> Self {
        Self {
            id: t.id.to_string(),
            conversation: t.conversation.clone(),
            turn: t.turn,
            iterations: t.iterations,
            max_turns: t.max_turns,
            max_iterations: t.max_iterations,
            yolo_mode: t.yolo_mode,
            parent_thread_id: t.parent_thread_id.clone(),
            budget_cost_cap_usd: t.budget.cost_cap_usd,
            budget_total_spend_usd: t.budget.total_spend_usd,
        }
    }
}

impl SavedThread {
    pub fn into_thread(self) -> AgentThread {
        let mut budget = BudgetGuard::new(self.budget_cost_cap_usd, self.yolo_mode);
        budget.total_spend_usd = self.budget_total_spend_usd;
        if let Some(cap) = self.budget_cost_cap_usd {
            if self.budget_total_spend_usd >= cap {
                budget.exhausted = true;
            }
        }
        AgentThread {
            id: Uuid::parse_str(&self.id).unwrap_or_else(|_| Uuid::new_v4()),
            status: crate::thread::ThreadStatus::Idle,
            conversation: self.conversation,
            context: crate::context::ContextManager::new(128000),
            turn: self.turn,
            iterations: self.iterations,
            max_turns: self.max_turns,
            max_iterations: self.max_iterations,
            yolo_mode: self.yolo_mode,
            parent_thread_id: self.parent_thread_id,
            budget,
        }
    }
}

#[async_trait]
pub trait ThreadStore: Send + Sync {
    async fn save_thread(&self, thread: &AgentThread) -> Result<(), ThreadStoreError>;
    async fn load_thread(&self, thread_id: &str) -> Result<AgentThread, ThreadStoreError>;
    async fn list_threads(&self) -> Result<Vec<String>, ThreadStoreError>;
    async fn delete_thread(&self, thread_id: &str) -> Result<(), ThreadStoreError>;
    async fn fork_thread(&self, thread_id: &str) -> Result<AgentThread, ThreadStoreError>;
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThreadStoreError {
    #[error("Thread not found: {0}")]
    NotFound(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Store error: {0}")]
    Store(String),
}

// JSON file based implementation (existing)
pub struct JsonFileThreadStore {
    dir: std::path::PathBuf,
}

impl JsonFileThreadStore {
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn thread_path(&self, thread_id: &str) -> std::path::PathBuf {
        self.dir.join(format!("{}.json", thread_id))
    }
}

#[async_trait]
impl ThreadStore for JsonFileThreadStore {
    async fn save_thread(&self, thread: &AgentThread) -> Result<(), ThreadStoreError> {
        let saved: SavedThread = thread.into();
        let json = serde_json::to_string_pretty(&saved)
            .map_err(|e| ThreadStoreError::Serialization(e.to_string()))?;
        tokio::fs::create_dir_all(&self.dir).await
            .map_err(|e| ThreadStoreError::Io(e.to_string()))?;
        tokio::fs::write(self.thread_path(&saved.id), json).await
            .map_err(|e| ThreadStoreError::Io(e.to_string()))
    }

    async fn load_thread(&self, thread_id: &str) -> Result<AgentThread, ThreadStoreError> {
        let path = self.thread_path(thread_id);
        let json = tokio::fs::read_to_string(&path).await
            .map_err(|_| ThreadStoreError::NotFound(thread_id.to_string()))?;
        let saved: SavedThread = serde_json::from_str(&json)
            .map_err(|e| ThreadStoreError::Serialization(e.to_string()))?;
        Ok(saved.into_thread())
    }

    async fn list_threads(&self) -> Result<Vec<String>, ThreadStoreError> {
        let mut ids = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&self.dir).await
            .map_err(|e| ThreadStoreError::Io(e.to_string()))?;
        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| ThreadStoreError::Io(e.to_string()))? {
            if entry.path().extension().map_or(false, |e| e == "json") {
                if let Some(stem) = entry.path().file_stem() {
                    ids.push(stem.to_string_lossy().to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    async fn delete_thread(&self, thread_id: &str) -> Result<(), ThreadStoreError> {
        let path = self.thread_path(thread_id);
        tokio::fs::remove_file(&path).await
            .map_err(|_| ThreadStoreError::NotFound(thread_id.to_string()))
    }

    async fn fork_thread(&self, thread_id: &str) -> Result<AgentThread, ThreadStoreError> {
        let thread = self.load_thread(thread_id).await?;
        let forked_conversation = thread.conversation.clone();
        let mut forked = AgentThread::new(thread.max_turns, thread.max_iterations, thread.yolo_mode);
        forked.conversation = forked_conversation;
        forked.parent_thread_id = Some(thread.id.to_string());
        self.save_thread(&forked).await?;
        Ok(forked)
    }
}

// SQLite-backed implementation
#[cfg(feature = "sqlite")]
#[derive(Debug, Clone)]
pub struct SqliteThreadStore {
    conn: Arc<Mutex<Connection>>,
}

#[cfg(feature = "sqlite")]
impl SqliteThreadStore {
    /// Open or create the SQLite database at the given path.
    pub fn new(path: impl Into<std::path::PathBuf>) -> Result<Self, ThreadStoreError> {
        let path_buf = path.into();
        let conn = Connection::open(&path_buf)
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        let store = Self { conn: Arc::new(Mutex::new(conn)) };
        store.init_tables()?;
        Ok(store)
    }

    fn init_tables(&self) -> Result<(), ThreadStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS threads (
                thread_id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                data TEXT NOT NULL,
                schema_version INTEGER NOT NULL
            );"
        ).map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
#[cfg(feature = "sqlite")]
impl ThreadStore for SqliteThreadStore {
    async fn save_thread(&self, thread: &AgentThread) -> Result<(), ThreadStoreError> {
        let saved: SavedThread = thread.into();
        let json = serde_json::to_string_pretty(&saved)
            .map_err(|e| ThreadStoreError::Serialization(e.to_string()))?;
        let thread_id = saved.id;
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock()
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO threads (thread_id, created_at, updated_at, data, schema_version) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![thread_id, now.clone(), now, json, 1usize],
        ).map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        Ok(())
    }

    async fn load_thread(&self, thread_id: &str) -> Result<AgentThread, ThreadStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT data FROM threads WHERE thread_id = ?1")
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        let mut rows = stmt.query(params![thread_id])
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        if let Some(row) = rows.next().map_err(|e| ThreadStoreError::Store(e.to_string()))? {
            let data: String = row.get(0)
                .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
            let saved: SavedThread = serde_json::from_str(&data)
                .map_err(|e| ThreadStoreError::Serialization(e.to_string()))?;
            Ok(saved.into_thread())
        } else {
            Err(ThreadStoreError::NotFound(thread_id.to_string()))
        }
    }

    async fn list_threads(&self) -> Result<Vec<String>, ThreadStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT thread_id FROM threads ORDER BY thread_id ASC")
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        let rows = stmt.query_map([], |row| row.get(0))
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        let mut ids: Vec<String> = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        ids.sort();
        Ok(ids)
    }

    async fn delete_thread(&self, thread_id: &str) -> Result<(), ThreadStoreError> {
        let conn = self.conn.lock()
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        let rows = conn.execute("DELETE FROM threads WHERE thread_id = ?1", params![thread_id])
            .map_err(|e| ThreadStoreError::Store(e.to_string()))?;
        if rows == 0 {
            Err(ThreadStoreError::NotFound(thread_id.to_string()))
        } else {
            Ok(())
        }
    }

    async fn fork_thread(&self, thread_id: &str) -> Result<AgentThread, ThreadStoreError> {
        let thread = self.load_thread(thread_id).await?;
        let forked_conversation = thread.conversation.clone();
        let mut forked = AgentThread::new(thread.max_turns, thread.max_iterations, thread.yolo_mode);
        forked.conversation = forked_conversation;
        forked.parent_thread_id = Some(thread.id.to_string());
        self.save_thread(&forked).await?;
        Ok(forked)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_sqlite_thread_store_persistence() {
        let dir = std::env::temp_dir().join(format!("sqlite_thread_store_test_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        let db_path = dir.join("threads.db");

        let store1 = SqliteThreadStore::new(&db_path).expect("failed to init store");
        let thread = AgentThread::new(10, 20, false);
        store1.save_thread(&thread).await.expect("save failed");
        drop(store1);

        let store2 = SqliteThreadStore::new(&db_path).expect("failed to re-open store");
        let loaded = store2.load_thread(&thread.id.to_string()).await.expect("load failed");

        assert_eq!(thread.id, loaded.id);
        assert_eq!(thread.max_turns, loaded.max_turns);
        assert_eq!(thread.max_iterations, loaded.max_iterations);
        assert_eq!(thread.yolo_mode, loaded.yolo_mode);
        assert_eq!(thread.parent_thread_id, loaded.parent_thread_id);
        assert_eq!(thread.turn, loaded.turn);
        assert_eq!(thread.iterations, loaded.iterations);
        assert_eq!(thread.conversation, loaded.conversation);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

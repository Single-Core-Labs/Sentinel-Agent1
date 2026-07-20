use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::thread::AgentThread;
use crate::conversation::Conversation;

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
        }
    }
}

impl SavedThread {
    pub fn into_thread(self) -> AgentThread {
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
        loop {
            let entry = read_dir.next_entry().await
                .map_err(|e| ThreadStoreError::Io(e.to_string()))?;
            match entry {
                Some(entry) => {
                    if entry.path().extension().map_or(false, |e| e == "json") {
                        if let Some(stem) = entry.path().file_stem() {
                            ids.push(stem.to_string_lossy().to_string());
                        }
                    }
                }
                None => break,
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

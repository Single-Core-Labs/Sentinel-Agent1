use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    SessionCreated {
        session_id: String,
        timestamp: DateTime<Utc>,
        model: String,
    },
    UserMessage {
        session_id: String,
        timestamp: DateTime<Utc>,
        content: String,
    },
    AssistantText {
        session_id: String,
        timestamp: DateTime<Utc>,
        text: String,
    },
    ToolCall {
        session_id: String,
        timestamp: DateTime<Utc>,
        tool_call_id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        session_id: String,
        timestamp: DateTime<Utc>,
        tool_call_id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    TurnEnd {
        session_id: String,
        timestamp: DateTime<Utc>,
        turn: u32,
        iteration: u32,
    },
    SessionEnded {
        session_id: String,
        timestamp: DateTime<Utc>,
        reason: String,
    },
    Error {
        session_id: String,
        timestamp: DateTime<Utc>,
        message: String,
    },
}

impl SessionEvent {
    pub fn session_id(&self) -> &str {
        match self {
            SessionEvent::SessionCreated { session_id, .. } => session_id,
            SessionEvent::UserMessage { session_id, .. } => session_id,
            SessionEvent::AssistantText { session_id, .. } => session_id,
            SessionEvent::ToolCall { session_id, .. } => session_id,
            SessionEvent::ToolResult { session_id, .. } => session_id,
            SessionEvent::TurnEnd { session_id, .. } => session_id,
            SessionEvent::SessionEnded { session_id, .. } => session_id,
            SessionEvent::Error { session_id, .. } => session_id,
        }
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            SessionEvent::SessionCreated { timestamp, .. } => *timestamp,
            SessionEvent::UserMessage { timestamp, .. } => *timestamp,
            SessionEvent::AssistantText { timestamp, .. } => *timestamp,
            SessionEvent::ToolCall { timestamp, .. } => *timestamp,
            SessionEvent::ToolResult { timestamp, .. } => *timestamp,
            SessionEvent::TurnEnd { timestamp, .. } => *timestamp,
            SessionEvent::SessionEnded { timestamp, .. } => *timestamp,
            SessionEvent::Error { timestamp, .. } => *timestamp,
        }
    }

    pub fn variant_name(&self) -> &'static str {
        match self {
            SessionEvent::SessionCreated { .. } => "session_created",
            SessionEvent::UserMessage { .. } => "user_message",
            SessionEvent::AssistantText { .. } => "assistant_text",
            SessionEvent::ToolCall { .. } => "tool_call",
            SessionEvent::ToolResult { .. } => "tool_result",
            SessionEvent::TurnEnd { .. } => "turn_end",
            SessionEvent::SessionEnded { .. } => "session_ended",
            SessionEvent::Error { .. } => "error",
        }
    }
}

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, event: SessionEvent);
    async fn read(&self, session_id: &str) -> Vec<SessionEvent>;
    async fn stream(
        &self,
        session_id: &str,
    ) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin>;
}

#[derive(Debug)]
pub struct NullEventStore;

#[async_trait]
impl EventStore for NullEventStore {
    async fn append(&self, _event: SessionEvent) {}
    async fn read(&self, _session_id: &str) -> Vec<SessionEvent> {
        Vec::new()
    }
    async fn stream(
        &self,
        _session_id: &str,
    ) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin> {
        Box::new(tokio_stream::empty())
    }
}

#[derive(Debug)]
pub struct VecEventStore {
    events: std::sync::Mutex<Vec<SessionEvent>>,
}

impl VecEventStore {
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl Default for VecEventStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventStore for VecEventStore {
    async fn append(&self, event: SessionEvent) {
        let mut guard = self.events.lock().unwrap();
        guard.push(event);
    }

    async fn read(&self, _session_id: &str) -> Vec<SessionEvent> {
        let guard = self.events.lock().unwrap();
        guard.clone()
    }

    async fn stream(
        &self,
        _session_id: &str,
    ) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin> {
        let events = {
            let guard = self.events.lock().unwrap();
            guard.clone()
        };
        Box::new(tokio_stream::iter(events))
    }
}

pub type SharedEventStore = Arc<dyn EventStore>;

/// Dependency-free file-backed event store: one JSON Lines file per session
/// under `dir/{session_id}.jsonl`. Append-only and safe to reopen across
/// processes; used when the `sqlite` feature is off.
#[derive(Debug)]
pub struct JsonFileEventStore {
    dir: std::path::PathBuf,
    lock: std::sync::Mutex<()>,
}

impl JsonFileEventStore {
    pub fn new(dir: impl AsRef<std::path::Path>) -> Self {
        let dir = dir.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&dir);
        Self {
            dir,
            lock: std::sync::Mutex::new(()),
        }
    }

    fn session_file(&self, session_id: &str) -> std::path::PathBuf {
        self.dir.join(format!("{}.jsonl", session_id))
    }
}

#[async_trait]
impl EventStore for JsonFileEventStore {
    async fn append(&self, event: SessionEvent) {
        use std::io::Write;
        let line = serde_json::to_string(&event).unwrap_or_default();
        let path = self.session_file(event.session_id());
        let _guard = self.lock.lock().unwrap();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "{}", line);
        }
    }

    async fn read(&self, session_id: &str) -> Vec<SessionEvent> {
        let path = self.session_file(session_id);
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        content
            .lines()
            .filter_map(|line| serde_json::from_str::<SessionEvent>(line).ok())
            .collect()
    }

    async fn stream(
        &self,
        session_id: &str,
    ) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin> {
        Box::new(tokio_stream::iter(self.read(session_id).await))
    }
}

/// A durable event store rooted at `dir`: SQLite (`dir/session_events.db`)
/// when the `sqlite` feature is enabled, else JSON Lines files.
pub fn create_event_store_in(dir: &std::path::Path) -> SharedEventStore {
    #[cfg(feature = "sqlite")]
    {
        if let Ok(store) = SqliteEventStore::new(
            &dir.join("session_events.db").to_string_lossy(),
        ) {
            return store;
        }
    }
    Arc::new(JsonFileEventStore::new(dir))
}

pub fn create_event_store() -> SharedEventStore {
    if cfg!(feature = "sqlite") {
        #[cfg(feature = "sqlite")]
        {
            let store: SharedEventStore = match SqliteEventStore::new(":memory:") {
                Ok(s) => s,
                Err(_) => Arc::new(NullEventStore),
            };
            store
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = ();
            Arc::new(NullEventStore)
        }
    } else {
        Arc::new(NullEventStore)
    }
}

#[cfg(feature = "sqlite")]
pub struct SqliteEventStore {
    conn: std::sync::Mutex<rusqlite::Connection>,
}

#[cfg(feature = "sqlite")]
impl SqliteEventStore {
    pub fn new(path: &str) -> Result<Arc<Self>, rusqlite::Error> {
        let mut conn = rusqlite::Connection::open(path)?;
        crate::sqlite_migrations::run_migrations(&mut conn)?;
        Ok(Arc::new(Self {
            conn: std::sync::Mutex::new(conn),
        }))
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl EventStore for SqliteEventStore {
    async fn append(&self, event: SessionEvent) {
        let payload = serde_json::to_string(&event).unwrap_or_default();
        let event_type = event.variant_name();
        let session_id = event.session_id().to_string();
        let timestamp = event.timestamp().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO session_events (session_id, timestamp, event_type, payload) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, timestamp, event_type, payload],
        );
    }

    async fn read(&self, session_id: &str) -> Vec<SessionEvent> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT payload FROM session_events WHERE session_id = ?1 ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map(rusqlite::params![session_id], |row| {
                let payload: String = row.get(0)?;
                Ok(payload)
            })
            .unwrap();
        rows.filter_map(|r| r.ok())
            .filter_map(|p| serde_json::from_str(&p).ok())
            .collect()
    }

    async fn stream(
        &self,
        session_id: &str,
    ) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin> {
        let events = self.read(session_id).await;
        Box::new(tokio_stream::iter(events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_null_store_noop() {
        let store = NullEventStore;
        store
            .append(SessionEvent::SessionCreated {
                session_id: "s1".into(),
                timestamp: Utc::now(),
                model: "gpt-4".into(),
            })
            .await;
        let events = store.read("s1").await;
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_vec_store_append_read() {
        let store = VecEventStore::new();
        store
            .append(SessionEvent::UserMessage {
                session_id: "s1".into(),
                timestamp: Utc::now(),
                content: "hello".into(),
            })
            .await;
        let events = store.read("s1").await;
        assert_eq!(events.len(), 1);
        match &events[0] {
            SessionEvent::UserMessage { content, .. } => assert_eq!(content, "hello"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn test_vec_store_stream() {
        let store = VecEventStore::new();
        store
            .append(SessionEvent::SessionCreated {
                session_id: "s1".into(),
                timestamp: Utc::now(),
                model: "gpt-4".into(),
            })
            .await;
        store
            .append(SessionEvent::TurnEnd {
                session_id: "s1".into(),
                timestamp: Utc::now(),
                turn: 1,
                iteration: 1,
            })
            .await;
        use tokio_stream::StreamExt;
        let mut stream = store.stream("s1").await;
        let first = stream.next().await;
        assert!(first.is_some());
        let second = stream.next().await;
        assert!(second.is_some());
        let third = stream.next().await;
        assert!(third.is_none());
    }

    #[tokio::test]
    async fn test_event_session_id_accessor() {
        let event = SessionEvent::Error {
            session_id: "test_sid".into(),
            timestamp: Utc::now(),
            message: "something broke".into(),
        };
        assert_eq!(event.session_id(), "test_sid");
    }

    #[tokio::test]
    async fn test_create_event_store_default() {
        let store = create_event_store();
        store
            .append(SessionEvent::SessionCreated {
                session_id: "s1".into(),
                timestamp: Utc::now(),
                model: "gpt-4".into(),
            })
            .await;
        let events = store.read("s1").await;
        // NullEventStore returns empty; the sqlite store persists the event.
        #[cfg(feature = "sqlite")]
        assert_eq!(events.len(), 1);
        #[cfg(not(feature = "sqlite"))]
        assert!(events.is_empty());
    }

    fn temp_event_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sentinel-events-test-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn test_jsonl_store_append_read() {
        let dir = temp_event_dir("jsonl");
        let store = JsonFileEventStore::new(&dir);
        store
            .append(SessionEvent::UserMessage {
                session_id: "s9".into(),
                timestamp: Utc::now(),
                content: "hello".into(),
            })
            .await;
        store
            .append(SessionEvent::AssistantText {
                session_id: "s9".into(),
                timestamp: Utc::now(),
                text: "hi".into(),
            })
            .await;
        let events = store.read("s9").await;
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], SessionEvent::UserMessage { .. }));
        assert!(matches!(&events[1], SessionEvent::AssistantText { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_jsonl_store_persists_across_reopen() {
        let dir = temp_event_dir("jsonl-reopen");
        {
            let store = JsonFileEventStore::new(&dir);
            store
                .append(SessionEvent::TurnEnd {
                    session_id: "s10".into(),
                    timestamp: Utc::now(),
                    turn: 2,
                    iteration: 3,
                })
                .await;
        }
        let reopened = JsonFileEventStore::new(&dir);
        let events = reopened.read("s10").await;
        assert_eq!(events.len(), 1);
        match &events[0] {
            SessionEvent::TurnEnd { turn, iteration, .. } => {
                assert_eq!(*turn, 2);
                assert_eq!(*iteration, 3);
            }
            _ => panic!("wrong variant"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_create_event_store_in_writes_durable_log() {
        let dir = temp_event_dir("create-in");
        let store = create_event_store_in(&dir);
        store
            .append(SessionEvent::UserMessage {
                session_id: "s11".into(),
                timestamp: Utc::now(),
                content: "persist me".into(),
            })
            .await;
        // JSONL fallback must leave a file on disk; sqlite feature writes a db.
        #[cfg(not(feature = "sqlite"))]
        assert!(dir.join("s11.jsonl").exists());
        #[cfg(feature = "sqlite")]
        assert!(dir.join("session_events.db").exists());

        let reopened = create_event_store_in(&dir);
        let events = reopened.read("s11").await;
        assert_eq!(events.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

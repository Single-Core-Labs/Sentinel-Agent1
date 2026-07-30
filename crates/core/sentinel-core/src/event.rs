use std::sync::Arc;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
}

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, event: SessionEvent);
    async fn read(&self, session_id: &str) -> Vec<SessionEvent>;
    async fn stream(&self, session_id: &str) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin>;
}

#[derive(Debug)]
pub struct NullEventStore;

#[async_trait]
impl EventStore for NullEventStore {
    async fn append(&self, _event: SessionEvent) {}
    async fn read(&self, _session_id: &str) -> Vec<SessionEvent> {
        Vec::new()
    }
    async fn stream(&self, _session_id: &str) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin> {
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

    async fn stream(&self, _session_id: &str) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin> {
        let events = {
            let guard = self.events.lock().unwrap();
            guard.clone()
        };
        Box::new(tokio_stream::iter(events))
    }
}

pub type SharedEventStore = Arc<dyn EventStore>;

pub fn create_event_store() -> SharedEventStore {
    if cfg!(feature = "sqlite") {
        #[cfg(feature = "sqlite")]
        {
            Arc::new(SqliteEventStore::new(":memory:").unwrap_or_else(|_| Arc::new(NullEventStore)))
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
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS session_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_session_events_session_id
                ON session_events(session_id);"
        )?;
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
        let event_type = std::mem::discriminant(&event).variant_name()
            .unwrap_or("unknown");
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
        let mut stmt = conn.prepare(
            "SELECT payload FROM session_events WHERE session_id = ?1 ORDER BY id"
        ).unwrap();
        let rows = stmt.query_map(rusqlite::params![session_id], |row| {
            let payload: String = row.get(0)?;
            Ok(payload)
        }).unwrap();
        rows.filter_map(|r| r.ok())
            .filter_map(|p| serde_json::from_str(&p).ok())
            .collect()
    }

    async fn stream(&self, session_id: &str) -> Box<dyn tokio_stream::Stream<Item = SessionEvent> + Send + Unpin> {
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
        store.append(SessionEvent::SessionCreated {
            session_id: "s1".into(),
            timestamp: Utc::now(),
            model: "gpt-4".into(),
        }).await;
        let events = store.read("s1").await;
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_vec_store_append_read() {
        let store = VecEventStore::new();
        store.append(SessionEvent::UserMessage {
            session_id: "s1".into(),
            timestamp: Utc::now(),
            content: "hello".into(),
        }).await;
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
        store.append(SessionEvent::SessionCreated {
            session_id: "s1".into(),
            timestamp: Utc::now(),
            model: "gpt-4".into(),
        }).await;
        store.append(SessionEvent::TurnEnd {
            session_id: "s1".into(),
            timestamp: Utc::now(),
            turn: 1,
            iteration: 1,
        }).await;
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
        store.append(SessionEvent::SessionCreated {
            session_id: "s1".into(),
            timestamp: Utc::now(),
            model: "gpt-4".into(),
        }).await;
        let events = store.read("s1").await;
        // NullEventStore returns empty
        assert!(events.is_empty());
    }
}

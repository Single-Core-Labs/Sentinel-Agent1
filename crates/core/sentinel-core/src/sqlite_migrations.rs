//! Versioned schema migrations for the SQLite stores.
//!
//! Every store that opens a SQLite database calls [`run_migrations`], which
//! applies each pending entry of [`MIGRATIONS`] exactly once inside a
//! transaction, recording applied versions in a `schema_migrations` table.
//!
//! Rules:
//! - Never edit or reorder an existing entry — the version is its index + 1.
//! - Append a new entry for every schema change.

#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection};

/// Ordered list of migrations. Version of entry `i` is `i + 1`.
pub const MIGRATIONS: &[&str] = &[
    // v1: conversation threads
    "CREATE TABLE IF NOT EXISTS threads (
        thread_id TEXT PRIMARY KEY,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        data TEXT NOT NULL,
        schema_version INTEGER NOT NULL
    );",
    // v2: session event log
    "CREATE TABLE IF NOT EXISTS session_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        event_type TEXT NOT NULL,
        payload TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_session_events_session_id
        ON session_events(session_id);",
];

/// Apply connection-level PRAGMAs for performance and data integrity.
///
/// Mirrors the "Connect" step of a typical persistence layer:
/// - `journal_mode=WAL` — concurrent readers/writers without blocking;
/// - `synchronous=NORMAL` — safe durability with WAL, without full fsync cost;
/// - `foreign_keys=ON` — enforce referential integrity declared in migrations;
/// - `busy_timeout=5000` — wait instead of failing with `SQLITE_BUSY` when
///   another process (e.g. a background worker) holds the write lock.
#[cfg(feature = "sqlite")]
pub fn configure_connection(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
}

/// Apply all pending migrations to `conn` inside a transaction.
#[cfg(feature = "sqlite")]
pub fn run_migrations(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;

    let applied: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT version FROM schema_migrations")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };

    let tx = conn.transaction()?;
    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = (index + 1) as i64;
        if applied.contains(&version) {
            continue;
        }
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, chrono::Utc::now().to_rfc3339()],
        )?;
    }
    tx.commit()
}

#[cfg(all(feature = "sqlite", test))]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_once_and_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);

        let threads: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='threads'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(threads, 1);

        let events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='session_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(events, 1);
    }

    #[test]
    fn fresh_db_reports_full_version_set() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let rows: Vec<i64> = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, vec![1, 2]);
    }

    #[test]
    fn configure_connection_applies_integrity_pragmas() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&mut conn).unwrap();

        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);

        let busy_timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5000);
    }
}

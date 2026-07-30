use async_trait::async_trait;
use lru::LruCache;
use rusqlite::{params, Connection};
use std::num::NonZeroUsize;
use tokio::sync::Mutex;

use super::embeddings::{combined_score, EmbeddingCache};
use super::types::*;

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn add(&self, memory: Memory) -> crate::memory::Result<Memory>;
    async fn get(&self, id: &str) -> crate::memory::Result<Option<Memory>>;
    async fn search(&self, query: &str, filter: &MemoryFilter) -> crate::memory::Result<Vec<ScoredMemory>>;
    async fn supersede(&self, old_id: &str, new_content: &str, reason: &str) -> crate::memory::Result<Memory>;
    async fn get_history(&self, id: &str) -> crate::memory::Result<Vec<Memory>>;
    async fn delete(&self, id: &str) -> crate::memory::Result<bool>;
    async fn clear(&self, user_id: &str) -> crate::memory::Result<usize>;
    async fn stats(&self, user_id: &str) -> crate::memory::Result<MemoryStats>;
    async fn count(&self, user_id: &str) -> crate::memory::Result<usize>;
}

pub struct SqliteMemoryStore {
    conn: Mutex<Connection>,
    cache: Mutex<LruCache<String, Memory>>,
    embed_cache: Mutex<EmbeddingCache>,
}

impl SqliteMemoryStore {
    pub fn open(path: &str) -> crate::memory::Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(500).unwrap())),
            embed_cache: Mutex::new(EmbeddingCache::new(1000)),
        };
        store.init_tables()?;
        Ok(store)
    }

    pub fn in_memory() -> crate::memory::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(500).unwrap())),
            embed_cache: Mutex::new(EmbeddingCache::new(1000)),
        };
        store.init_tables()?;
        Ok(store)
    }

    fn init_tables(&self) -> crate::memory::Result<()> {
        let conn = self.conn.try_lock().map_err(|e| crate::memory::MemoryError::LockError(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                session_id TEXT,
                agent_id TEXT,
                content TEXT NOT NULL,
                category TEXT NOT NULL,
                importance REAL NOT NULL DEFAULT 0.5,
                scope TEXT NOT NULL DEFAULT 'user',
                supersedes TEXT,
                superseded_by TEXT,
                supersede_reason TEXT,
                source TEXT NOT NULL DEFAULT 'manual',
                source_turn INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                accessed_at INTEGER NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_memories_user ON memories(user_id);
            CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);
            CREATE INDEX IF NOT EXISTS idx_memories_supersedes ON memories(supersedes);
            CREATE INDEX IF NOT EXISTS idx_memories_superseded_by ON memories(superseded_by);
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                content, category,
                content='memories',
                content_rowid='rowid',
                tokenize='porter unicode61'
            );
            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, content, category) VALUES (new.rowid, new.content, new.category);
            END;
            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content, category) VALUES('delete', old.rowid, old.content, old.category);
            END;
            CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content, category) VALUES('delete', old.rowid, old.content, old.category);
                INSERT INTO memories_fts(rowid, content, category) VALUES (new.rowid, new.content, new.category);
            END;"
        )?;
        Ok(())
    }
}

// ── Row mapping (sync) ──────────────────────────────────────────

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        session_id: row.get("session_id")?,
        agent_id: row.get("agent_id")?,
        content: row.get("content")?,
        category: MemoryCategory::from_str(&row.get::<_, String>("category")?),
        importance: row.get("importance")?,
        scope: match row.get::<_, String>("scope")?.as_str() {
            "session" => MemoryScope::Session,
            "agent" => MemoryScope::Agent,
            "turn" => MemoryScope::Turn,
            _ => MemoryScope::User,
        },
        supersedes: row.get("supersedes")?,
        superseded_by: row.get("superseded_by")?,
        supersede_reason: row.get("supersede_reason")?,
        source: match row.get::<_, String>("source")?.as_str() {
            "inline" => MemorySource::InlineExtraction,
            "tool" => MemorySource::ToolCall,
            "compaction" => MemorySource::Compaction,
            _ => MemorySource::Manual,
        },
        source_turn: row.get("source_turn")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        accessed_at: row.get("accessed_at")?,
        access_count: row.get("access_count")?,
    })
}

fn build_where_clause(filter: &MemoryFilter) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut param_values = Vec::new();

    if let Some(ref uid) = filter.user_id {
        conditions.push(format!("m.user_id = ?{}", param_values.len() + 1));
        param_values.push(uid.clone());
    }
    if let Some(ref sid) = filter.session_id {
        conditions.push(format!("m.session_id = ?{}", param_values.len() + 1));
        param_values.push(sid.clone());
    }
    if let Some(ref aid) = filter.agent_id {
        conditions.push(format!("m.agent_id = ?{}", param_values.len() + 1));
        param_values.push(aid.clone());
    }
    if !filter.include_superseded {
        conditions.push("m.superseded_by IS NULL".to_string());
    }
    if let Some(ref cats) = filter.categories {
        let cat_strs: Vec<String> = cats.iter().map(|c| c.as_str().to_string()).collect();
        let placeholders: Vec<String> = cat_strs.iter().enumerate()
            .map(|(i, _)| format!("?{}", param_values.len() + i + 1)).collect();
        conditions.push(format!("m.category IN ({})", placeholders.join(",")));
        param_values.extend(cat_strs);
    }
    if let Some(ref scopes) = filter.scopes {
        let scope_strs: Vec<String> = scopes.iter().map(|s| s.as_str().to_string()).collect();
        let placeholders: Vec<String> = scope_strs.iter().enumerate()
            .map(|(i, _)| format!("?{}", param_values.len() + i + 1)).collect();
        conditions.push(format!("m.scope IN ({})", placeholders.join(",")));
        param_values.extend(scope_strs);
    }
    if let Some(min_imp) = filter.min_importance {
        conditions.push(format!("m.importance >= ?{}", param_values.len() + 1));
        param_values.push(min_imp.to_string());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    (where_clause, param_values)
}

fn recency_factor(updated_at: i64, half_life_secs: f64) -> f64 {
    let age = (now_seconds() - updated_at).max(0) as f64;
    (-age / half_life_secs).exp()
}

// ── SqliteMemoryStore trait impl ──────────────────────────────

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn add(&self, memory: Memory) -> crate::memory::Result<Memory> {
        {
            let conn = self.conn.lock().await;
            conn.execute(
                "INSERT INTO memories (id, user_id, session_id, agent_id, content, category, importance, scope, supersedes, superseded_by, supersede_reason, source, source_turn, created_at, updated_at, accessed_at, access_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    memory.id, memory.user_id, memory.session_id, memory.agent_id,
                    memory.content, memory.category.as_str(), memory.importance, memory.scope.as_str(),
                    memory.supersedes, memory.superseded_by, memory.supersede_reason,
                    memory.source.as_str(), memory.source_turn,
                    memory.created_at, memory.updated_at, memory.accessed_at, memory.access_count,
                ],
            )?;
        }
        let mut cache = self.cache.lock().await;
        cache.put(memory.id.clone(), memory.clone());
        Ok(memory)
    }

    async fn get(&self, id: &str) -> crate::memory::Result<Option<Memory>> {
        {
            let mut cache = self.cache.lock().await;
            if let Some(m) = cache.get(id) {
                return Ok(Some(m.clone()));
            }
        }
        let result = {
            let conn = self.conn.lock().await;
            let mut stmt = conn.prepare("SELECT * FROM memories WHERE id = ?1")?;
            stmt.query_row(params![id], |row| row_to_memory(row)).ok()
        };
        if let Some(ref m) = result {
            let mut cache = self.cache.lock().await;
            cache.put(id.to_string(), m.clone());
        }
        Ok(result)
    }

    async fn search(&self, query: &str, filter: &MemoryFilter) -> crate::memory::Result<Vec<ScoredMemory>> {
        if query.trim().is_empty() {
            return self.search_by_filter(filter).await;
        }

        let limit = filter.limit.max(1).min(200);

        // Build WHERE conditions inline with ?2+ numbering (?1 is FTS MATCH)
        let mut conditions: Vec<String> = Vec::new();
        let mut where_params: Vec<String> = Vec::new();

        if let Some(ref uid) = filter.user_id {
            conditions.push(format!("m.user_id = ?{}", where_params.len() + 2));
            where_params.push(uid.clone());
        }
        if let Some(ref sid) = filter.session_id {
            conditions.push(format!("m.session_id = ?{}", where_params.len() + 2));
            where_params.push(sid.clone());
        }
        if let Some(ref aid) = filter.agent_id {
            conditions.push(format!("m.agent_id = ?{}", where_params.len() + 2));
            where_params.push(aid.clone());
        }
        if !filter.include_superseded {
            conditions.push("m.superseded_by IS NULL".to_string());
        }
        if let Some(ref cats) = filter.categories {
            let cat_strs: Vec<String> = cats.iter().map(|c| c.as_str().to_string()).collect();
            let placeholders: Vec<String> = cat_strs.iter().enumerate()
                .map(|(i, _)| format!("?{}", where_params.len() + i + 2)).collect();
            conditions.push(format!("m.category IN ({})", placeholders.join(",")));
            where_params.extend(cat_strs);
        }
        if let Some(ref scopes) = filter.scopes {
            let scope_strs: Vec<String> = scopes.iter().map(|s| s.as_str().to_string()).collect();
            let placeholders: Vec<String> = scope_strs.iter().enumerate()
                .map(|(i, _)| format!("?{}", where_params.len() + i + 2)).collect();
            conditions.push(format!("m.scope IN ({})", placeholders.join(",")));
            where_params.extend(scope_strs);
        }
        if let Some(min_imp) = filter.min_importance {
            conditions.push(format!("m.importance >= ?{}", where_params.len() + 2));
            where_params.push(min_imp.to_string());
        }

        let and_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("AND {}", conditions.join(" AND "))
        };

        let limit_param_idx = where_params.len() + 2;

        // SQL query — fully synchronous, no awaits
        let memories: Vec<Memory> = {
            let conn = self.conn.lock().await;
            let sql = format!(
                "SELECT m.* FROM memories_fts f JOIN memories m ON m.rowid = f.rowid
                 WHERE memories_fts MATCH ?1 {} ORDER BY rank, m.importance DESC LIMIT ?{}",
                and_clause, limit_param_idx
            );

            let fts_query = query.split_whitespace()
                .map(|w| format!("\"{}\"", w.replace('"', "")))
                .collect::<Vec<_>>()
                .join(" OR ");

            let mut stmt = conn.prepare(&sql)?;

            let mut all_params: Vec<String> = Vec::new();
            all_params.push(fts_query);
            all_params.extend(where_params);
            all_params.push(limit.to_string());

            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                all_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

            let rows = stmt.query_map(param_refs.as_slice(), |row| row_to_memory(row))?;
            let mut results = Vec::new();
            for row in rows {
                if let Ok(m) = row {
                    results.push(m);
                }
            }
            results
        };

        // Async scoring (embedding cache)
        let mut embed_cache = self.embed_cache.lock().await;
        let query_vec = embed_cache.get_or_compute(query);

        let mut scored: Vec<ScoredMemory> = memories.into_iter().map(|m| {
            let mem_vec = embed_cache.get_or_compute(&m.content);
            let relevance = combined_score(query, &m.content, Some(&query_vec), Some(&mem_vec));
            let recency = recency_factor(m.updated_at, 86400.0 * 7.0);
            let score = relevance * 0.6 + m.importance * 0.3 + recency * 0.1;
            ScoredMemory { memory: m, score, relevance }
        }).collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }

    async fn supersede(&self, old_id: &str, new_content: &str, reason: &str) -> crate::memory::Result<Memory> {
        let old = self.get(old_id).await?
            .ok_or_else(|| crate::memory::MemoryError::NotFound(old_id.to_string()))?;

        let now = now_seconds();
        let new_memory = Memory {
            id: generate_memory_id(),
            content: new_content.to_string(),
            supersedes: Some(old_id.to_string()),
            supersede_reason: Some(reason.to_string()),
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
            ..old
        };

        {
            let conn = self.conn.lock().await;
            conn.execute(
                "UPDATE memories SET superseded_by = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_memory.id, now, old_id],
            )?;
        }

        {
            let mut cache = self.cache.lock().await;
            cache.pop(old_id);
        }

        self.add(new_memory.clone()).await
    }

    async fn get_history(&self, id: &str) -> crate::memory::Result<Vec<Memory>> {
        let mut chain = Vec::new();
        let mut current_id = Some(id.to_string());

        while let Some(cid) = current_id {
            if let Some(m) = self.get(&cid).await? {
                chain.push(m.clone());
                current_id = m.supersedes.clone();
            } else {
                break;
            }
        }

        chain.reverse();
        Ok(chain)
    }

    async fn delete(&self, id: &str) -> crate::memory::Result<bool> {
        let affected = {
            let conn = self.conn.lock().await;
            conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?
        };
        if affected > 0 {
            let mut cache = self.cache.lock().await;
            cache.pop(id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn clear(&self, user_id: &str) -> crate::memory::Result<usize> {
        let affected = {
            let conn = self.conn.lock().await;
            conn.execute("DELETE FROM memories WHERE user_id = ?1", params![user_id])?
        };
        if affected > 0 {
            let mut cache = self.cache.lock().await;
            cache.clear();
        }
        Ok(affected)
    }

    async fn stats(&self, user_id: &str) -> crate::memory::Result<MemoryStats> {
        let (total, active, superseded, by_category, by_scope, by_source) = {
            let conn = self.conn.lock().await;

            let total: usize = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE user_id = ?1", params![user_id],
                |r| r.get(0),
            )?;

            let active: usize = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE user_id = ?1 AND superseded_by IS NULL", params![user_id],
                |r| r.get(0),
            )?;

            let superseded: usize = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE user_id = ?1 AND superseded_by IS NOT NULL", params![user_id],
                |r| r.get(0),
            )?;

            let mut by_category = Vec::new();
            let mut cat_stmt = conn.prepare(
                "SELECT category, COUNT(*) as cnt FROM memories WHERE user_id = ?1 AND superseded_by IS NULL GROUP BY category ORDER BY cnt DESC"
            )?;
            let cat_rows = cat_stmt.query_map(params![user_id], |r| {
                let cat: String = r.get(0)?;
                let cnt: usize = r.get(1)?;
                Ok((cat, cnt))
            })?;
            for row in cat_rows {
                if let Ok((cat_str, cnt)) = row {
                    by_category.push((MemoryCategory::from_str(&cat_str), cnt));
                }
            }

            let mut by_scope = Vec::new();
            let mut scope_stmt = conn.prepare(
                "SELECT scope, COUNT(*) as cnt FROM memories WHERE user_id = ?1 AND superseded_by IS NULL GROUP BY scope ORDER BY cnt DESC"
            )?;
            let scope_rows = scope_stmt.query_map(params![user_id], |r| {
                let scope: String = r.get(0)?;
                let cnt: usize = r.get(1)?;
                Ok((scope, cnt))
            })?;
            for row in scope_rows {
                if let Ok((scope_str, cnt)) = row {
                    let scope = match scope_str.as_str() {
                        "session" => MemoryScope::Session,
                        "agent" => MemoryScope::Agent,
                        "turn" => MemoryScope::Turn,
                        _ => MemoryScope::User,
                    };
                    by_scope.push((scope, cnt));
                }
            }

            let mut by_source = Vec::new();
            let mut src_stmt = conn.prepare(
                "SELECT source, COUNT(*) as cnt FROM memories WHERE user_id = ?1 AND superseded_by IS NULL GROUP BY source ORDER BY cnt DESC"
            )?;
            let src_rows = src_stmt.query_map(params![user_id], |r| {
                let src: String = r.get(0)?;
                let cnt: usize = r.get(1)?;
                Ok((src, cnt))
            })?;
            for row in src_rows {
                if let Ok((src_str, cnt)) = row {
                    let src = match src_str.as_str() {
                        "inline" => MemorySource::InlineExtraction,
                        "tool" => MemorySource::ToolCall,
                        "compaction" => MemorySource::Compaction,
                        _ => MemorySource::Manual,
                    };
                    by_source.push((src, cnt));
                }
            }

            (total, active, superseded, by_category, by_scope, by_source)
        };

        Ok(MemoryStats { total, active, superseded, by_category, by_scope, by_source })
    }

    async fn count(&self, user_id: &str) -> crate::memory::Result<usize> {
        let count = {
            let conn = self.conn.lock().await;
            conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE user_id = ?1 AND superseded_by IS NULL",
                params![user_id],
                |r| r.get(0),
            )?
        };
        Ok(count)
    }
}

impl SqliteMemoryStore {
    async fn search_by_filter(&self, filter: &MemoryFilter) -> crate::memory::Result<Vec<ScoredMemory>> {
        let (where_clause, _) = build_where_clause(filter);
        let limit = filter.limit.max(1).min(200);
        let offset = filter.offset;

        let memories: Vec<Memory> = {
            let conn = self.conn.lock().await;
            let sql = if where_clause.is_empty() {
                format!("SELECT * FROM memories m ORDER BY m.importance DESC, m.created_at DESC LIMIT ?1 OFFSET ?2")
            } else {
                format!(
                    "SELECT * FROM memories m {} ORDER BY m.importance DESC, m.created_at DESC LIMIT ?1 OFFSET ?2",
                    where_clause
                )
            };

            let mut stmt = conn.prepare(&sql)?;
            let all_params: Vec<String> = std::iter::once(limit.to_string())
                .chain(std::iter::once(offset.to_string()))
                .collect();
            let param_refs: Vec<&dyn rusqlite::types::ToSql> = all_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

            let rows = stmt.query_map(param_refs.as_slice(), |row| row_to_memory(row))?;
            let mut results = Vec::new();
            for row in rows {
                if let Ok(m) = row {
                    results.push(m);
                }
            }
            results
        };

        let mut scored: Vec<ScoredMemory> = memories.into_iter().map(|m| {
            let relevance = 0.5;
            let score = m.importance * 0.7 + recency_factor(m.updated_at, 86400.0 * 7.0) * 0.3;
            ScoredMemory { memory: m, score, relevance }
        }).collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored)
    }
}

// ── InMemoryStore ─────────────────────────────────────────────

pub struct InMemoryStore {
    memories: Mutex<Vec<Memory>>,
    embed_cache: Mutex<EmbeddingCache>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            memories: Mutex::new(Vec::new()),
            embed_cache: Mutex::new(EmbeddingCache::new(1000)),
        }
    }
}

#[async_trait]
impl MemoryStore for InMemoryStore {
    async fn add(&self, memory: Memory) -> crate::memory::Result<Memory> {
        let mut mems = self.memories.lock().await;
        mems.push(memory.clone());
        Ok(memory)
    }

    async fn get(&self, id: &str) -> crate::memory::Result<Option<Memory>> {
        let mems = self.memories.lock().await;
        Ok(mems.iter().find(|m| m.id == id).cloned())
    }

    async fn search(&self, query: &str, filter: &MemoryFilter) -> crate::memory::Result<Vec<ScoredMemory>> {
        let filtered = {
            let mems = self.memories.lock().await;
            mems.iter()
                .filter(|m| {
                    if let Some(ref uid) = filter.user_id { if m.user_id != *uid { return false; } }
                    if let Some(ref sid) = filter.session_id { if m.session_id.as_deref() != Some(sid) { return false; } }
                    if !filter.include_superseded && m.superseded_by.is_some() { return false; }
                    if let Some(ref cats) = filter.categories { if !cats.contains(&m.category) { return false; } }
                    if let Some(ref scopes) = filter.scopes { if !scopes.contains(&m.scope) { return false; } }
                    if let Some(min_imp) = filter.min_importance { if m.importance < min_imp { return false; } }
                    true
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        let mut embed_cache = self.embed_cache.lock().await;
        let query_vec = embed_cache.get_or_compute(query);

        let mut scored: Vec<ScoredMemory> = filtered.into_iter().map(|m| {
            let mem_vec = embed_cache.get_or_compute(&m.content);
            let relevance = combined_score(query, &m.content, Some(&query_vec), Some(&mem_vec));
            let recency = recency_factor(m.updated_at, 86400.0 * 7.0);
            let score = relevance * 0.6 + m.importance * 0.3 + recency * 0.1;
            ScoredMemory { memory: m, score, relevance }
        }).collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(filter.limit.max(1).min(200));
        Ok(scored)
    }

    async fn supersede(&self, old_id: &str, new_content: &str, reason: &str) -> crate::memory::Result<Memory> {
        let new = {
            let mut mems = self.memories.lock().await;
            let old = mems.iter_mut().find(|m| m.id == old_id)
                .ok_or_else(|| crate::memory::MemoryError::NotFound(old_id.to_string()))?;
            old.superseded_by = Some(generate_memory_id());
            old.updated_at = now_seconds();

            let new = Memory {
                id: generate_memory_id(),
                content: new_content.to_string(),
                supersedes: Some(old_id.to_string()),
                supersede_reason: Some(reason.to_string()),
                created_at: now_seconds(),
                updated_at: now_seconds(),
                accessed_at: now_seconds(),
                access_count: 0,
                ..old.clone()
            };
            mems.push(new.clone());
            new
        };
        Ok(new)
    }

    async fn get_history(&self, id: &str) -> crate::memory::Result<Vec<Memory>> {
        let mems = self.memories.lock().await;
        let mut chain = Vec::new();
        let mut current_id = Some(id.to_string());
        while let Some(cid) = current_id {
            if let Some(m) = mems.iter().find(|m| m.id == cid) {
                chain.push(m.clone());
                current_id = m.supersedes.clone();
            } else {
                break;
            }
        }
        chain.reverse();
        Ok(chain)
    }

    async fn delete(&self, id: &str) -> crate::memory::Result<bool> {
        let mut mems = self.memories.lock().await;
        let len_before = mems.len();
        mems.retain(|m| m.id != id);
        Ok(mems.len() != len_before)
    }

    async fn clear(&self, user_id: &str) -> crate::memory::Result<usize> {
        let mut mems = self.memories.lock().await;
        let len_before = mems.len();
        mems.retain(|m| m.user_id != user_id);
        Ok(len_before - mems.len())
    }

    async fn stats(&self, user_id: &str) -> crate::memory::Result<MemoryStats> {
        let (total, active, _superseded, by_category, by_scope, by_source) = {
            let mems = self.memories.lock().await;
            let user_mems: Vec<&Memory> = mems.iter().filter(|m| m.user_id == user_id).collect();
            let total = user_mems.len();
            let active = user_mems.iter().filter(|m| m.superseded_by.is_none()).count();

            let mut cat_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for m in &user_mems {
                if m.superseded_by.is_none() {
                    *cat_counts.entry(m.category.as_str().to_string()).or_default() += 1;
                }
            }
            let by_category: Vec<(MemoryCategory, usize)> = cat_counts.into_iter()
                .map(|(k, v)| (MemoryCategory::from_str(&k), v)).collect();

            let mut scope_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for m in &user_mems {
                if m.superseded_by.is_none() {
                    *scope_counts.entry(m.scope.as_str().to_string()).or_default() += 1;
                }
            }
            let by_scope: Vec<(MemoryScope, usize)> = scope_counts.into_iter()
                .map(|(k, v)| {
                    let scope = match k.as_str() {
                        "session" => MemoryScope::Session,
                        "agent" => MemoryScope::Agent,
                        "turn" => MemoryScope::Turn,
                        _ => MemoryScope::User,
                    };
                    (scope, v)
                }).collect();

            let mut src_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for m in &user_mems {
                if m.superseded_by.is_none() {
                    *src_counts.entry(m.source.as_str().to_string()).or_default() += 1;
                }
            }
            let by_source: Vec<(MemorySource, usize)> = src_counts.into_iter()
                .map(|(k, v)| {
                    let src = match k.as_str() {
                        "inline" => MemorySource::InlineExtraction,
                        "tool" => MemorySource::ToolCall,
                        "compaction" => MemorySource::Compaction,
                        _ => MemorySource::Manual,
                    };
                    (src, v)
                }).collect();

            (total, active, total - active, by_category, by_scope, by_source)
        };

        Ok(MemoryStats { total, active, superseded: total - active, by_category, by_scope, by_source })
    }

    async fn count(&self, user_id: &str) -> crate::memory::Result<usize> {
        let mems = self.memories.lock().await;
        Ok(mems.iter().filter(|m| m.user_id == user_id && m.superseded_by.is_none()).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_memory(user_id: &str, content: &str, category: MemoryCategory) -> Memory {
        let now = now_seconds();
        Memory {
            id: generate_memory_id(),
            user_id: user_id.to_string(),
            session_id: None,
            agent_id: None,
            content: content.to_string(),
            category,
            importance: 0.5,
            scope: MemoryScope::User,
            supersedes: None,
            superseded_by: None,
            supersede_reason: None,
            source: MemorySource::Manual,
            source_turn: 0,
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
        }
    }

    #[tokio::test]
    async fn test_add_and_get() {
        let store = InMemoryStore::new();
        let m = test_memory("alice", "likes Python", MemoryCategory::Preference);
        store.add(m.clone()).await.unwrap();
        let retrieved = store.get(&m.id).await.unwrap().unwrap();
        assert_eq!(retrieved.content, "likes Python");
    }

    #[tokio::test]
    async fn test_search_finds_relevant() {
        let store = InMemoryStore::new();
        store.add(test_memory("alice", "Prefers Python for backend", MemoryCategory::Preference)).await.unwrap();
        store.add(test_memory("alice", "Works at fintech startup", MemoryCategory::Fact)).await.unwrap();
        let results = store.search("python", &MemoryFilter::for_user("alice")).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].memory.content.contains("Python"));
    }

    #[tokio::test]
    async fn test_supersede_chain() {
        let store = InMemoryStore::new();
        let m1 = store.add(test_memory("bob", "Works at Google", MemoryCategory::Fact)).await.unwrap();
        let m2 = store.supersede(&m1.id, "Works at Anthropic", "job change").await.unwrap();
        let history = store.get_history(&m2.id).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "Works at Google");
        assert_eq!(history[1].content, "Works at Anthropic");
    }

    #[tokio::test]
    async fn test_superseded_excluded_by_default() {
        let store = InMemoryStore::new();
        let m1 = store.add(test_memory("bob", "Old fact", MemoryCategory::Fact)).await.unwrap();
        store.supersede(&m1.id, "New fact", "updated").await.unwrap();
        let results = store.search("fact", &MemoryFilter::for_user("bob")).await.unwrap();
        assert!(results.iter().all(|r| r.memory.superseded_by.is_none()));
    }

    #[tokio::test]
    async fn test_clear_user() {
        let store = InMemoryStore::new();
        store.add(test_memory("alice", "A", MemoryCategory::Fact)).await.unwrap();
        store.add(test_memory("bob", "B", MemoryCategory::Fact)).await.unwrap();
        let cleared = store.clear("alice").await.unwrap();
        assert_eq!(cleared, 1);
        assert_eq!(store.count("alice").await.unwrap(), 0);
        assert_eq!(store.count("bob").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_stats() {
        let store = InMemoryStore::new();
        store.add(test_memory("alice", "Likes Go", MemoryCategory::Preference)).await.unwrap();
        store.add(test_memory("alice", "Works at Co", MemoryCategory::Fact)).await.unwrap();
        let stats = store.stats("alice").await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 2);
    }

    #[tokio::test]
    async fn test_sqlite_add_and_get() {
        let store = SqliteMemoryStore::in_memory().unwrap();
        let m = test_memory("alice", "likes Rust", MemoryCategory::Preference);
        store.add(m.clone()).await.unwrap();
        let retrieved = store.get(&m.id).await.unwrap().unwrap();
        assert_eq!(retrieved.content, "likes Rust");
    }

    #[tokio::test]
    async fn test_sqlite_search_fts() {
        let store = SqliteMemoryStore::in_memory().unwrap();
        store.add(test_memory("alice", "Prefers Python for data science", MemoryCategory::Preference)).await.unwrap();
        store.add(test_memory("alice", "Works at a startup", MemoryCategory::Fact)).await.unwrap();
        let results = store.search("python data", &MemoryFilter::for_user("alice")).await.unwrap();
        assert!(!results.is_empty(), "should find python preference");
    }

    #[tokio::test]
    async fn test_sqlite_supersede() {
        let store = SqliteMemoryStore::in_memory().unwrap();
        let m1 = store.add(test_memory("bob", "Works at Google", MemoryCategory::Fact)).await.unwrap();
        let m2 = store.supersede(&m1.id, "Works at Anthropic", "changed jobs").await.unwrap();
        let history = store.get_history(&m2.id).await.unwrap();
        assert_eq!(history.len(), 2);
    }

    #[tokio::test]
    async fn test_sqlite_stats() {
        let store = SqliteMemoryStore::in_memory().unwrap();
        store.add(test_memory("alice", "Likes Go", MemoryCategory::Preference)).await.unwrap();
        store.add(test_memory("alice", "Works at Co", MemoryCategory::Fact)).await.unwrap();
        let stats = store.stats("alice").await.unwrap();
        assert_eq!(stats.total, 2);
    }

    #[test]
    fn test_recency_factor() {
        let f = recency_factor(now_seconds(), 86400.0);
        assert!((f - 1.0).abs() < 0.01);
        let old_f = recency_factor(now_seconds() - 86400 * 30, 86400.0 * 7.0);
        assert!(old_f < 0.5);
    }
}

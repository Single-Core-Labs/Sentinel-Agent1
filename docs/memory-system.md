# Persistent Memory System

`sentinel-headroom` includes a multi-strategy persistent memory module that
extracts, stores, searches, and injects structured memories across sessions.

## Architecture

```
User Message / LLM Response
         │
         ▼
  [Extractor] ──── inline <memory> blocks + compaction fact extraction
         │
         ▼
  [MemoryStore] ── SQLite (FTS5) or InMemory store
         │
         ▼
  [Injector] ──── weighted scoring + system prompt injection
         │
         ▼
  [Compressor] ── memory extraction on IntelligentContext drop,
                  memory injection during system prompt build
```

## Modules

| Module | File | Responsibility |
|--------|------|----------------|
| `types` | `src/memory/types.rs` | `Memory`, `MemoryCategory`, `MemoryScope`, `MemorySource`, `MemoryFilter`, `ScoredMemory`, `MemoryStats`, `MemorySupersession` |
| `store` | `src/memory/store.rs` | `MemoryStore` trait, `SqliteMemoryStore` (SQLite + FTS5), `InMemoryStore` (testing) |
| `embeddings` | `src/memory/embeddings.rs` | Character n-gram TF-IDF vectors, cosine similarity, keyword overlap, `EmbeddingCache` |
| `extractor` | `src/memory/extractor.rs` | Inline `<memory>` block parsing, compaction fact extraction, `strip_memory_blocks()` |
| `injector` | `src/memory/injector.rs` | `MemoryInjector` — scored retrieval, multi-format injection |
| `tool` | `src/memory/tool.rs` | Agent tools: `headroom_memorize`, `headroom_recall`, `headroom_forget`, `headroom_memory_stats` |
| `config` | `src/memory/config.rs` | `MemoryConfig`, `ExtractionConfig`, `InjectionConfig` |

## Memory Types

```rust
pub enum MemoryCategory {
    Fact,          // "Works at Acme Corp"
    Preference,    // "Prefers Python over Go"
    Observation,   // "Seems to prefer async patterns"
    Task,          // "Working on feature X"
    Relationship,  // "Reports to Alice"
    Custom(String),
}

pub enum MemoryScope {
    Session,     // only this session
    User,        // all sessions for this user
    Agent,       // all sessions for this agent
    Global,      // all users/agents
}

pub enum MemorySource {
    Explicit,    // direct <memory> tool call
    Extracted,   // automatic compaction extraction
    Inferred,    // deduced from conversation patterns
    Imported,    // imported from external source
}
```

## Stores

### SqliteMemoryStore (production)

- File-based or `:memory:` SQLite database.
- Full-text search via FTS5 virtual table with triggers for sync.
- LRU cache (512 entries) for hot memory access.
- Supersession chain: `supersede()` creates new version, links old via `superseded_by`.
- Send-safe via `Arc<Mutex<Connection>>`.

### InMemoryStore (testing)

- `Vec<Memory>` with in-memory filtering and search.
- Uses the same embeddings for scoring.
- No persistence across restarts.

## Embeddings

Character n-gram (trigram) TF-IDF vectors (256 dimensions):

```rust
pub fn char_ngram_tfidf(text: &str, n: usize, dim: usize) -> Vec<f64>
```

Combined relevance score:
```
score = cosine_similarity(tfidf(a), tfidf(b)) * 0.7
       + keyword_overlap(a, b) * 0.3
```

`EmbeddingCache` stores up to 500 computed vectors with FNV-1a hash keys.

## Extraction

### Inline Memory Blocks

The LLM can emit structured memories directly in responses:

```xml
<memory category="preference">User prefers dark mode</memory>
<memory category="fact" importance="0.9">User is a senior engineer</memory>
```

Parsed by `parse_memory_blocks()` — category, content, optional importance.
Stripped from visible output by `strip_memory_blocks()`.

### Compaction Extraction

When messages are dropped by `IntelligentContext`, `extract_facts_from_text()`
scans for:
- `I am / I'm ...` → Fact
- `I work at / for ...` → Fact
- `I prefer / like / love / enjoy / hate / dislike ...` → Preference
- `I'm working on ...` → Task
- `I'm learning ...` → Observation
- `My name is / I'm called ...` → Fact

## Injection

`MemoryInjector` retrieves relevant memories and injects them into the system
prompt. Scoring:

```
final_score = relevance * 0.6 + importance * 0.3 + recency * 0.1
```

Recency uses exponential decay with a 7-day half-life. Memories below
`min_score` (default 0.15) are filtered out.

### Injection Formats

Inline block:
```
<memory category="preference">User prefers dark mode</memory>
```

System block (with `<!-- KNOWN_FACTS -->` marker):
```
<!-- KNOWN_FACTS -->
- [preference] User prefers dark mode
- [fact] User works at Acme Corp
```

## Agent Tools

| Tool | Function | Schema |
|------|----------|--------|
| `headroom_memorize` | Store a fact/preference/observation | `{ content, category?, importance?, scope? }` |
| `headroom_recall` | Search stored memories | `{ query, category?, limit? }` |
| `headroom_forget` | Delete a specific memory | `{ id }` |
| `headroom_memory_stats` | View memory statistics | `{ user_id? }` |

## Wiring

### In Compressor (`src/compress.rs`)

```rust
pub struct Compressor {
    pub memory: Option<PersistentMemory>,
    // ... other fields
}
```

- **On drop** (IntelligentContext): `memory.extract_from_dropped(&dropped_msgs)`
- **On system prompt**: `memory.inject_memories(&system_prompt, user_id)`

### In Config (`src/config.rs`)

```rust
pub struct HeadroomConfig {
    pub memory: MemoryConfig,
    // ...
}
```

Default: `MemoryConfig { enabled: false }` — must be explicitly enabled with
a store path.

## Test Coverage

| Module | Tests |
|--------|-------|
| embeddings | 8 — empty, same/different texts, cache, keyword overlap, combined score |
| extractor | 9 — inline parse, multiple blocks, category, short content, strip, inject instruction, fact extraction |
| injector | 7 — format, inject, retrieve, category filter, empty |
| store | 11 — add/get, search, supersede, clear, stats, SQLite roundtrip, FTS search, recency factor |
| tool | 5 — memorize, recall, forget, stats, validation |

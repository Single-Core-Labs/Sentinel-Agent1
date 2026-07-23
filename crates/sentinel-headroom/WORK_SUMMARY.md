# sentinel-headroom — Compression Pipeline (Work Summary)

## Overview

`sentinel-headroom` is a multi-stage message compression pipeline that reduces LLM context window usage via alignment, content-type routing, intelligent dropping, cache optimization, and a Compress-Cache-Retrieve (CCR) system.

## Architecture

```
Messages
  │
  ▼
CacheAligner     — extract dates, paths, UUIDs, versions; replace with placeholders;
│                   delta tracking across calls, whitespace normalization
▼
ContentRouter    — classify by content type (code/json/log/html/image/diff/search/text)
│                   → route to type-specific CompressionStrategy
▼
CacheOptimizer   — insert provider-specific cache markers:
│                   Anthropic: cache_control breakpoint markers
│                   OpenAI:    prefix token detection
│                   Google:    CachedContent eligibility
▼
IntelligentContext — score messages by recency, error weight, tool deps
│                     → drop lowest-scored when over token budget
▼
CCR              — store dropped messages in CcrStore (BM25 searchable)
│                   → retrieval markers in compressed output
▼
Orchestrator      — headroom_retrieve tool schema, proactive context expansion
```

## Components

### CacheAligner (`cache_aligner.rs`)
- Extracts dynamic context (dates, times, file paths, UUIDs, versions, user info, temp dirs)
- Replaces with placeholders (`<DATE_1>`, `<UUID_1>`, etc.)
- Delta tracking: detects changes across calls, emits `<CONTEXT_CHANGED:>` or `<CONTEXT: no change>`
- Whitespace normalization and blank line collapsing
- Custom regex patterns via config
- 9 tests

### ContentRouter / Orchestrator (`orchestrator.rs`)
- Routes messages to type-specific compressors (code-aware, JSON, logs, HTML, diff, search, text, image-aware)
- CCR integration: stores compressed tool output, generates retrieval keys with hash
- `headroom_retrieve` tool schema with optional BM25 query
- `handle_retrieve()` and `proactive_expand()` methods
- 12 tests

### CacheOptimizer (`cache_optimizer.rs`)
- Provider detection: `claude`/`anthropic` → Anthropic, `gpt`/`o1`/`o3` → OpenAI, `gemini`/`palm` → Google
- Provider-specific breakpoint strategies:
  - **Anthropic**: inserts `[cache_control: breakpoint type=system|conversation|content]` markers, 90% read discount
  - **OpenAI**: prefix token detection at 1024-token threshold, 50% discount
  - **Google**: CachedContent eligibility for ≥32768 tokens, 75% discount
- Cost ratio estimation
- `format_cache_summary()` for reporting
- `force_provider` override in config
- 14 tests

### IntelligentContext (`intelligent_context.rs`)
- Token budget enforcement: scores messages by recency, error weight, tool dependency
- Drops lowest-scored messages when over budget
- 6 tests

### CCR (`ccr.rs`)
- LRU-evicting store with TTL expiry
- BM25 search within cached entries
- Retrieval markers in compressed tool output
- TOIN (tool-call ID) tracking
- `most_retrieved_hashes()` for tracker
- 18 tests

### CCR Tracker (`ccr_tracker.rs`)
- Proactive context expansion: `find_relevant_cached()` with query relevance scoring
- Query change detection
- Retrieval pattern learning
- 7 tests

### Compress (`compress.rs`)
- Pipeline orchestrator: calls CacheAligner → ContentRouter → CacheOptimizer → IntelligentContext → CCR
- `model` parameter passed through to CacheOptimizer
- `CompressionMetadata` with cache_summary (provider, discount, token savings)
- 12 tests

## Integration Points

- `sentinel-cli` (`exec.rs`): creates compressor + registers `headroom_retrieve` tool
- `sentinel-app-server` (`handler.rs`, `session.rs`, `server.rs`): `new_with_headroom()`, `new_with_compressor()`
- `sentinel-ai-exec` (`lib.rs`): registers `HeadroomRetrieveTool` in ToolRegistry
- `sentinel-ai-tui` (`app_server_session.rs`): registers `HeadroomRetrieveTool` in ToolRegistry

## Configuration

```rust
HeadroomConfig {
    cache_alignment: CacheAlignmentConfig { enabled, extract_dates, extract_file_paths,
        extract_uuids, extract_versions, extract_user_context, delta_tracking,
        normalize_whitespace, collapse_blank_lines, custom_patterns },
    cache_optimizer: CacheOptimizerConfig { enabled, auto_detect_provider,
        force_provider: LlmProvider, min_cacheable_tokens },
    content_routing: ContentRoutingConfig { enabled_types, min_content_chars, ... },
    intelligent_context: IntelligentContextConfig { enabled, token_budget, error_weight, ... },
    ccr: CcrConfig { enabled, max_entries, default_ttl_secs, ... },
}
```

## Test Stats

- **185 tests total**, all passing
- `cargo check --workspace` clean (0 warnings from headroom)

## Key Files

| File | Lines | Purpose |
|------|-------|---------|
| `config.rs` | 181 | All config structs |
| `cache_aligner.rs` | 220 | Dynamic context extraction & replacement |
| `cache_optimizer.rs` | 408 | Provider-specific cache markers |
| `compress.rs` | 440 | Pipeline orchestrator |
| `orchestrator.rs` | ~400 | Content routing, CCR, retrieval tool |
| `ccr.rs` | ~300 | BM25-keyed cache store |
| `ccr_tracker.rs` | ~200 | Proactive expansion |
| `intelligent_context.rs` | ~200 | Token budget enforcement |
| `integration.rs` | 288 | HeadroomRetrieveTool, pipeline glue |
| `lib.rs` | 31 | Module declarations & re-exports |

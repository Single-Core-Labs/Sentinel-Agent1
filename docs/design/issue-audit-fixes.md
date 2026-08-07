# Issue Audit & Fixes — #95–#117

Status: **all 14 issues verified; 4 fixed this round, 10 confirmed already resolved by later refactors.** `cargo test --workspace` green (837 passed, 0 failed), `cargo check --workspace` clean.

Every issue was re-verified against the current tree before touching code. Issues that the codebase had already outgrown (crate removed, handler rewritten, tests added) are marked **already resolved** with the evidence; issues still present were fixed and are marked **fixed**.

## Fixed this round

| # | Issue | Fix | Evidence |
|---|---|---|---|
| 95 | analytics dedup fingerprint set was `clear()`-ed past 10k entries, so dedup silently stopped working for everything within the flush cycle | Extracted a bounded `FingerprintWindow` (`check_and_insert` + `clear`) that prunes the **oldest** fingerprints past the cap while keeping the recent window deduplicated. 4 unit tests cover dedup, prune-oldest-keeps-recent, clear, and fingerprint discrimination | `crates/platform/sentinel-analytics/src/queue.rs` |
| 96 | `local.rs` REPL had no `/ai` command to hand off to full agent mode | Added `/ai [model]` slash command: spawns `sentinel ai [model]` as a child process with inherited stdio; the TUI/agent takes over the terminal and control returns to the REPL when it exits. Listed in `/help` | `crates/interfaces/sentinel-cli/src/local.rs` (`cmd_ai`, dispatch arm, help) |
| 97 | `local.rs` `/ai` handoff skipped; `run_shell` was the only shell access | Same `/ai` command as #96; the existing `run_shell` (PowerShell/sh wrapper) is unchanged and remains the single command path for `/ssh` and friends | `crates/interfaces/sentinel-cli/src/local.rs:640` (pre-existing) |
| 98 | analytics used unbounded mpsc channels → memory could grow without bound under load | Both `queue.rs` and `pipeline.rs` now use bounded channels (cap 8192) with `try_send`; on overflow the event is dropped with a warning (telemetry is loss-tolerant, the hot path never blocks) | `crates/platform/sentinel-analytics/src/queue.rs`, `crates/platform/sentinel-analytics/src/pipeline.rs` |
| 111 | `handle_stream` returned `Err` when the sink send failed, so a normal client disconnect surfaced as "Server error" (and in stdio mode bubbled up through `run_stdio`) | Added `send_ok` helper: a failed send is treated as a client disconnect → log at debug and break the loop returning `Ok(())`. Reply-forwarder send failures are dropped silently. Errors are still printed by `cmd_start` when they are real | `crates/server/sentinel-app-server/src/server.rs` |

## Already resolved (verified in current tree)

| # | Issue | Resolution / current state |
|---|---|---|
| 100 | `sentinel-auth` crate | **Removed.** Auth now lives in the CLI (`sentinel auth`, `crates/interfaces/sentinel-cli/src/auth.rs`), the transport `Authenticator` (`sentinel-app-server-transport`), and a per-handler `auth_token` (`handler.rs`) |
| 102 | app server had no graceful shutdown | **Fixed.** `shutdown::install_signal_handler()` wired in `sentinel-cli/src/server.rs:55` and `web.rs:129`; `run_tcp_with_shutdown` / `run_http_with_dir_with_shutdown` select on the watch channel and stop cleanly; LSP clients shut down after |
| 103 | handler session methods not async-safe | **Resolved by rewrite.** `handler.rs` is fully async (`handle` + every `handle_*` are `async fn`); sessions live behind `tokio::sync::Mutex<HashMap<…, Arc<AppSession>>>` |
| 104 | server run methods didn't return errors to the caller | **Fixed.** `run_stdio`/`run_http`/`run_tcp` return `Result`; `cmd_start` prints errors instead of exiting silently |
| 107 | provider API key / creation path | **Resolved.** `ProviderInfo::from_info` + `resolve_api_key` (env-var based, `ollama` default) in `crates/platform/sentinel-provider/src/provider.rs`; multi-backend auto-detection in `backend.rs` |
| 112 | handler gap (file write / legacy behavior) | **Resolved by rewrite.** `handler.rs` (1,627 lines) now covers sessions, dialogs, IDE context, LSP diagnostics, headroom memory injection/extraction, config overrides, and event subscriptions — all with tests (53 in `sentinel-app-server`) |
| 113 | ccr BM25 scored with empty `doc_freq` | **Fixed.** `ccr.rs` computes real document frequencies per search (`doc_freq` map, `bm25_score`), bounded by the LRU (500 entries) |
| 114 | headroom memory store cloned whole `Memory` objects during search scoring | **Resolved by refactor.** Search moves memories and scores via the embedding cache (`store.rs` search); the only remaining clones are the LRU cache boundary (one on `get` hit, one on `add`), inherent to the owned-return `MemoryStore` trait and bounded by the LRU |
| 116 | ccr broken `use c` import + missing stats | **Fixed.** Import gone; `retrieval_stats`, `log_retrieval`, `recent_retrievals`, `most_retrieved_hashes` exist with tests (ccr.rs:303–332) |
| 117 | sentinel-auth leftovers | **Same as #100.** Crate and references fully removed from the workspace |

## Verification

```
cargo check --workspace          # clean (only pre-existing unused-import warning in sentinel-config)
cargo test --workspace           # 837 passed, 0 failed, 32 test binaries
cargo test -p sentinel-analytics # 19 lib + 6 integration (new FingerprintWindow tests)
```

## Notes

- Windows gotcha encountered: `LNK1104` linking the app-server test binary — a stale test process held the exe. No sentinel process was running; the lock was transient and the retry succeeded.
- The `FingerprintWindow` refactor was tested at the unit level because the reducer's once-only semantics mask dedup behavior at the queue level (a duplicate `TurnEnded` is only emitted once regardless of dedup).
- `dedup_window.clear()` at flush boundaries is intentional: dedup is scoped per flush cycle; only the >cap overflow behavior changed.

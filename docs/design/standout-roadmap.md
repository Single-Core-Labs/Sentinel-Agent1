# Sentinel Standout Roadmap — System Design

**Date:** 2026-07-31
**Status:** Design approved for Phase 1 (Auto-Optimize Loop); Phases 2–4 scoped.
**Goal:** Make Sentinel the agent that no generic coding agent (Cline, OpenCode, Codex, Claude Code) can clone in a quarter — by being **hardware-grounded** (GPU-native optimization), **zero-token for measurable work** (cost story), **safe by architecture** (policy-as-code), and **complete as a platform** (IDE, persistence, autonomy, distribution).

---

## 0. Positioning statement

> Generic agents turn tokens into text. Sentinel turns tokens into **measured GPU performance**.
> The measurable part of a task never touches the LLM; the LLM only exercises judgment on data the local stack already gathered, measured, and ranked.

Four pillars, in leverage order:

| # | Pillar | Thesis | Status today |
|---|--------|--------|--------------|
| 1 | Auto-Optimize Loop | Write → Profile → Optimize → Prove, as agent-loop behavior | Pieces exist as CLI commands only (`/emulate --sweep`, `/bench kernel`, `/optimize`); not wired into the agent |
| 2 | Cost Story | Measurable work is deterministic and free; benchmark vs. LLM-only agents | No harness, no published numbers |
| 3 | Platform Story | IDE extension, persistent memory, autonomous mode, one-command install | App-server + TS TUI + graph store exist but unassembled |
| 4 | Safety Moat | Policy-as-code + packaged guard plugins for enterprise | Policy engine + plugin system exist; no docs, no examples |

---

## 1. Pillar 1 — Auto-Optimize Loop (Phase 1: implement now)

### 1.1 Vision

When the agent writes or edits a GPU kernel file (CUDA, Triton, Mojo, Numba, PyTorch, CUTE, CUDA Tile, TileLang), it must **never** ship raw code to the user without measured evidence. Sentinel's loop:

```
LLM writes kernel ──► auto profile (emulate sweep ~100 configs, deterministic, ~0 tokens)
        │                        │
        │                        ▼
        │             bottleneck analysis + best config + recs
        │                        │
        │                        ▼
        ├────────────────► LLM sees report in tool result, rewrites kernel
        │                        │
        │                        ▼
        └────────────────► optional "prove": real nvcc compile + timed run,
                           estimate_speedup(before, after) reported to user
```

### 1.2 Components

| Component | Where | Responsibility |
|-----------|-------|----------------|
| `GpuOptimizeKernelTool` (standalone tool `gpu_optimize_kernel`) | `sentinel-cli/src/gpu_optimize.rs` (NEW) | Model-callable: analyze any kernel file → sweep report + bottleneck + suggestions; optional `run_real_bench` → nvcc compile+timed run ("prove") |
| `AutoOptimizeWrapper` | same file | Wraps the builtin `write`/`edit` tools in the agent registry: after a successful write/edit of a recognized kernel file, appends a compact `[auto kernel optimization]` report to the tool result. Non-kernels pass through untouched (cheap: language detection only) |
| Registry wiring | `sentinel-cli/src/ai.rs` | After `ToolRegistry::new()`: replace `write`/`edit` entries with wrappers (same names → approval/diff-capture logic in `agent.rs` keyed on `name == "write"`/`"edit"` still works), register `gpu_optimize_kernel` |
| Report builder | `gpu_optimize.rs` | Compact, LLM-friendly text: kernel name, language, detected GPU/arch, best config (grid/block/smem/regs) + score, top-3 alternatives, bottleneck (primary+secondary), roofline intensity, 3–5 concrete suggestions, real-bench timing when available. Sweep table NOT dumped wholesale (context budget) |

### 1.3 Data flow (auto path)

1. `agent.rs` `execute_tools_concurrent` runs `write`/`edit` via the wrapped tool.
2. Wrapper calls inner tool; on success reads `args["file_path"]`.
3. Path resolution order: absolute → `ctx.sandbox_dir` → `ctx.workspace_dir` → cwd.
4. `langs::detect_language(filename, source)`; `GpuLanguage::Unknown` → return inner output unchanged.
5. `emulate::run_config_sweep(source, generate_sweep_configs(source), arch)` where arch = GPU detected by `vram::architecture_from_name` (fallback Ampere86/RTX 30-series default).
6. `optimizer::analyze_bottlenecks(emulate(source, best_config))`.
7. Append report to tool output. Model sees it on the next turn and can rewrite with `edit` → wrapper fires again → agent converges.

### 1.4 "Prove" path (model-requested, via `gpu_optimize_kernel`)

- `run_real_bench: true` → `bench::benchmark_kernel_real` on best config (spawned via `tokio::task::spawn_blocking`, nvcc is blocking) + `estimate_config` baseline.
- `verify: true` (future) → after the model's rewritten kernel lands, run the same bench again and emit `estimate_speedup(before, after)` — the "prove" closing the loop. (Phase 1.5: implement as a second call with `baseline_path` arg.)

### 1.5 Non-goals (Phase 1)

- No auto-rewrite by the tool itself (the LLM owns the code; Sentinel owns the evidence).
- No apply_patch interception (multi-file diffs; covered by standalone tool + Phase 1.5 `verify`).
- No cross-file/whole-project optimization passes.

### 1.6 Acceptance criteria (Phase 1)

- [ ] `gpu_optimize_kernel` tool registered and visible to the model in `sentinel ai` (tool_defs list).
- [ ] Writing a `.cu`/`.py` (Triton) kernel through `sentinel ai` auto-appends an optimization report; writing `README.md` does not.
- [ ] Report contains: best config, bottleneck, ≥3 concrete suggestions; ≤ ~2.5k chars.
- [ ] Unit tests in `gpu_optimize.rs`: non-kernel passthrough, kernel detection, report content, missing file error, arch resolution fallback, real-bench opt-in.
- [ ] `cargo test --workspace` + `cargo check --workspace` green.

---

## 2. Pillar 2 — Cost Story (Phase 2)

### 2.1 Methodology

Same task, two execution paths:

| Path | Runs | LLM tokens |
|------|------|-----------|
| Sentinel (local) | `sentinel ai --local --prompt "/emulate test-kernels/x.cu --sweep"` | 0 |
| LLM-only agent | prompt an agent (e.g., `sentinel ai --prompt "analyze x.cu..."`) | tokens for every tool call |

Tasks: (a) kernel sweep/recommendation, (b) GPU stats snapshot, (c) dmon anomaly detection, (d) config sweep + best-config selection, (e) SSH remote profile.

### 2.2 Artifacts

- `scripts/cost-benchmark.ps1` (NEW): runs both paths per task, parses token counts from the session summary line (`total_prompt_tokens`/`total_completion_tokens` are tracked by `Agent`), emits a Markdown table `docs/design/cost-results.md`.
- `docs/design/cost-story.md` (NEW): methodology, the table, and the README headline template ("Measurable work is free: 0 tokens for profiling/bench/emulate").

### 2.3 Acceptance criteria

- [ ] Script runs both paths headless (`SENTINEL_NON_INTERACTIVE=1`), no TTY needed.
- [ ] Output table with per-task token deltas and $ estimate at $/Mtok.
- [ ] Documented rerun instructions.

---

## 3. Pillar 3 — Platform Story (Phases 3–5, staged)

### 3.1 IDE extension (Phase 3)
- Reuse `sentinel-app-server` (RPC over TCP/WebSocket, already multi-transport) + `sentinel-app-server-client` + TS TUI components.
- Package a VS Code extension (WebView chat + GPU bar) talking to the daemon. Single extension entry: "Sentinel: attach to workspace".

### 3.2 Persistence / memory (Phase 4)
- Wire `sentinel-agent-graph-store` (thread graph: nodes, edges, status, children, persistence) into agent context: `--resume <id>` already loads threads; add auto-suggestion "continue thread X" at session start, and store GPU artifacts (best configs per kernel path) as graph nodes keyed by file hash → reuse without re-sweeping (memoized optimization).

### 3.3 Autonomous mode (Phase 4/5)
- `sentinel ai --watch <gpu_job>`: background task samples dmon; anomaly detectors (already exist) fire `notify` tool / plugin hook on compute/thermal/PCIe anomalies. Daemonize via `sentinel-app-server-daemon`.

### 3.4 Distribution (Phase 5)
- `sentinel install` (PowerShell/bash script): pulls release binary, writes `sentinel.toml`, sets PATH, optionally registers the VS Code extension. Cargo-based builds become dev-only.

### 3.5 Acceptance criteria (staged, per phase)
- Phase 3: extension sends prompt → daemon runs agent → response + GPU bar render in WebView.
- Phase 4: `--resume` lists prior threads with one-line summaries; repeated optimization of unchanged kernel file is instant (cache hit).
- Phase 4/5: `--watch` flags anomaly within 2 poll intervals and notifies.
- Phase 5: `curl | sh`-style install on clean Windows/macOS/Linux.

---

## 4. Pillar 4 — Safety Moat (Phase 2: implement now)

### 4.1 Artifacts
- `examples/plugins/` (NEW), three ready-to-install guard plugins, each with `sentinel-plugin.toml` + Windows `.cmd` + Unix `.sh` hooks (no BOM — session gotcha):
  1. `workspace-guard` — veto `write`/`edit`/`apply_patch` when `file_path` escapes the workspace.
  2. `web-guard` — allowlist domains for `web_search`/`web_fetch` (default: deny non-allowlisted).
  3. `command-guard` — veto `run_shell_command` matching destructive patterns (`rm -rf /`, `format`, `del /s`, `> /dev/sda`…).
- `docs/design/policy-moat.md` (NEW): threat model, install steps (`sentinel plugin install examples/plugins/workspace-guard`), hook contract recap, and the enterprise pitch (all-or-nothing approvals vs. Sentinel's fail-closed scriptable gates).

### 4.2 Acceptance criteria
- [ ] Each plugin installs via `sentinel plugin install <dir>`; `sentinel plugin list` shows it.
- [ ] Live: with `command-guard` installed, agent's `rm -rf` attempt returns `Vetoed by plugin policy: ...` (fail-closed).
- [ ] Docs cover authoring a new plugin from the template.

---

## 5. Roadmap summary

| Phase | Scope | Effort | Dependencies |
|-------|-------|--------|--------------|
| **1** | Auto-Optimize Loop (wrapper + standalone tool + tests) | ~0.5–1 day | None (this doc) |
| **2** | Cost harness + guard plugins + policy docs | ~0.5 day | Phase 1 registry wiring |
| 3 | VS Code extension on app-server | 2–4 days | app-server maturity |
| 4 | Graph-store memory + memoized optimization | 1–2 days | graph-store fields |
| 5 | Autonomous watch + installer | 2 days | daemon, analytics |

Phases 1–2 are fully specified above and are the current build target. Phases 3–5 are scoped, not started.

---

## 6. Design decisions & risks

| Decision | Rationale | Risk / Mitigation |
|----------|-----------|-------------------|
| Wrapper tool (name `write`) instead of a core hook in `sentinel-core` | Zero changes to core; approval/diff logic keyed on tool name still works; wrapper lives where the gpu-profiler dep already exists | Only `ai.rs` registry gets wrappers; nested agents use plain registry → acceptable, noted in docs |
| Auto mode never runs real nvcc bench | Feedback must be fast (~100ms), not 10–30s; "prove" is opt-in via `gpu_optimize_kernel` | Verified by test that auto report excludes bench section |
| Report ≤ ~2.5k chars | Tool output competes for context window | Sweep table formatted to top-3; full data available on demand |
| Sweep arch from detected GPU, fallback Ampere86 | Deterministic without nvcc/GPU query at sweep time | `vram::architecture_from_name` may return None → default documented |
| Fail-closed policy is the default posture | Session precedent (PolicyEngine fail-closed) | Examples emphasize explicit `allow` lines |

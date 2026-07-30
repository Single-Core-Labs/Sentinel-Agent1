# RightNow AI-Inspired Features: System Design

## 1. Vision

Four features inspired by RightNow AI, spanning the full stack:

| # | Feature | RightNow Equivalent | Priority |
|---|---------|-------------------|----------|
| 1 | **Instant GPU Kernel Analysis** | GPU kernel metrics while typing | P0 |
| 2 | **GPU-Aware Model Management** | SOTA LLMs that know your GPU | P0 |
| 3 | **Multi-Backend Local LLM** | Local LLM support (Ollama/vLLM/LM Studio) | P0 |
| 4 | **Smart Profiling Terminal** | Auto-profile GPU code & diagnose over SSH | P1 |

**Cardinal rule**: All deterministic operations must be zero-cost (no LLM token spend). LLM-backed analysis is additive, not primary.

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│  packages/cli-agent (Solid.js + OpenTUI)                        │
│  ┌─────────────┐ ┌──────────────┐ ┌─────────────────────────┐   │
│  │ ChatPanel   │ │ GPUMonitor  │ │ ModelSelector           │   │
│  │ (messages)  │ │ (live GPU%) │ │ (VRAM-compatible list)  │   │
│  └─────────────┘ └──────────────┘ └─────────────────────────┘   │
└──────────────────────┬──────────────────────────────────────────┘
                       │ WebSocket JSON-RPC
┌──────────────────────▼──────────────────────────────────────────┐
│  crates/server/sentinel-app-server                              │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  ws_handler -> Router -> Agent::run_with_approval()     │    │
│  │  New methods: gpu/query, gpu/profile, model/backends    │    │
│  └─────────────────────────────────────────────────────────┘    │
└──────────────────────┬──────────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────────┐
│  crates/interfaces/sentinel-cli/src/local.rs                    │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │
│  │ cmd_gpu  │ │cmd_profile│ │cmd_backend│ │ cmd_ssh  │           │
│  │ +branch  │ │ +parser  │ │ +switch  │ │ +profile │           │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘           │
│                                                                  │
│  crates/platform/sentinel-provider/src/local.rs                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ LocalProvider (OpenAI-compatible: Ollama/vLLM/LM Studio) │   │
│  │ + backend auto-detection + model listing per backend     │   │
│  └──────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

---

## 3. Feature Details

### Feature 1: Instant GPU Kernel Analysis

**Goal**: Analyze CUDA/Numba/Mojo GPU kernels for performance bottlenecks without sending code to an LLM.

**Zero-cost analysis commands**:

```
/profile                               # Profile current GPU with nvidia-smi + ncu
/profile parse <file.cu>               # Static analysis of CUDA source
/profile ssh user@host <file.cu>       # Remote analysis
```

**Implementation**:

```
crates/tools-and-exec/
  sentinel-gpu-profiler/               # NEW CRATE
    src/
      lib.rs                           # Public API: profile(), analyze_source()
      ncu.rs                           # Parse nvidia-ncu (Nsight Compute) CLI output
      sm_usage.rs                      # Parse nvidia-smi query for SM throughput
      cuda_kernel.rs                   # Lightweight CUDA source parser (no full AST)
                                        # Detect: block size, shared mem, divergent branches,
                                        # uncoalesced access patterns, bank conflicts
```

**Analysis rules** (deterministic, no LLM):

| Pattern | Rule | Severity |
|---------|------|----------|
| `__syncthreads()` inside `if/else` | Divergent barrier → warp stall | error |
| `shared` array > 48KB | Shared memory oversubscription | warn |
| `blockDim.x < 32` | Underutilized warp | warn |
| `gridDim.x < 80` | Too few blocks to hide latency | info |
| `atomicAdd` inside loop | Serialized contention | warn |
| `memcpy` in inner loop | Host-device sync in hot path | error |

**LLM extension** (opt-in, costs tokens):
```
/profile analyze <file.cu> --llm       # Deep analysis via agent LLM
```
Sends parsed profile output + source to the active chat model for recommendations.

---

### Feature 2: GPU-Aware Model Management

**Goal**: Only show models that fit in available VRAM. Cloud/local split with clear labels.

**CLI commands**:
```
/models                               # Current: list all pulled models
/models vram                          # NEW: filter by VRAM compatibility
/models cloud                         # NEW: show cloud model mappings
```

**Backend logic** (extend `recommend` feature in `local.rs`):

```
detect_vram() -> Option<f64>          # Parse nvidia-smi total memory in GB
filter_by_vram(models, vram)          # Cross-ref model parameter size vs VRAM
                                       # Rule: param_count_gb * 2.5 <= vram_gb
                                       # (2.5x factor: weights + kv cache + activations)

model_db = HashMap {                   # Embedded known model sizes
  "llama3.2:1b"   => 0.6,             // GB (FP16)
  "llama3.2:3b"   => 1.8,
  "qwen2.5:1.5b"  => 0.9,
  "deepseek-r1:8b" => 5.2,
  "llama3.1:8b"   => 4.9,
  ...
}
```

**Cloud model indicator**: `/recommend cloud` shows which models have cloud equivalents:
```
Cloud replacements available:
  llama3.1:8b     → claude-3-haiku     (4.9GB, $0.25/M tokens)
  llama3.3:70b    → claude-3-sonnet    (42GB, $3.00/M tokens)
  deepseek-r1:67b → claude-3-opus      (40GB, $15.00/M tokens)
```

---

### Feature 3: Multi-Backend Local LLM Support

**Goal**: Auto-detect and seamlessly switch between Ollama, vLLM, and LM Studio.

**Current state**: `LocalProvider` in `sentinel-provider/src/local.rs` uses OpenAI-compatible API at configurable `base_url`. Works with Ollama at `http://localhost:11434/v1`.

**Enhancements**:

```
// Extend LocalProvider with backend auto-detection

enum LocalBackend {
    Ollama { base_url: String, version: String },
    Vllm { base_url: String, version: String },
    LmStudio { base_url: String, version: String },
}

impl LocalProvider {
    pub async fn auto_detect() -> Vec<LocalBackend> {
        // Check well-known endpoints:
        //   http://localhost:11434/v1/models   -> Ollama
        //   http://localhost:8000/v1/models    -> vLLM
        //   http://localhost:1234/v1/models    -> LM Studio
    }

    pub async fn list_backend_models(backend: &LocalBackend) -> Vec<ModelEntry> {
        // GET /v1/models -> parse model IDs
    }

    pub fn switch_backend(&mut self, backend: LocalBackend, model: String) {
        // Update base_url and model name in LocalProvider
    }
}
```

**CLI commands**:
```
/backends              # List detected local LLM backends
/backends switch <n>   # Switch active backend
```

**Output example**:
```
Detected backends:
  1. Ollama     http://localhost:11434/v1   (3 models)
  2. LM Studio  http://localhost:1234/v1    (2 models)
  3. vLLM       http://localhost:8000/v1    (0 models - not running)
```

---

### Feature 4: Smart Profiling Terminal

**Goal**: Profile GPU code and diagnose bottlenecks locally or over SSH, with structured output.

**CLI commands**:
```
/ssh profile <user@host> <duration_s>   # Profile remote GPU over SSH
/profile ncu <duration_s>               # Local Nsight Compute profile
/profile log <file>                      # Parse existing ncu/nvidia-smi log
```

**SSH profile flow**:
```
User: /ssh profile ubuntu@10.0.0.1 5

System:
  1. ssh ubuntu@10.0.0.1 "nvidia-smi dmon -s pucvmet -d 1 -c 5"
  2. Parse CSV output into structured table
  3. Detect anomalies:
     - GPU idle < 30% → CPU-bound
     - Mem bandwidth < 50% → compute-bound
     - PCIe tx/rx > 5% → data transfer bottleneck
  4. Suggest mitigations (deterministic rules)
```

**Profile output format**:
```
Profiling ubuntu@10.0.0.1 for 5s...
  Time  GPU%  Mem%  Enc%  Dec%  PCIeTx  PCIeRx
  0:01   92%   78%    0%    0%   1.2GB   0.8GB
  0:02   95%   81%    0%    0%   1.1GB   0.7GB
  ...

Analysis: GPU-intensive workload (avg 93% util)
  → Memory bandwidth headroom: 19%   (compute-bound)
  → PCIe transfer: 1.1 GB/s avg     (minor)
  → Recommendation: Increase batch size to improve throughput
```

---

## 4. Crate Dependency Map

```
sentinel-cli
  ├── sentinel-provider         (extended: backend detection)
  ├── sentinel-provider-info    (extended: model size DB + cloud map)
  ├── sentinel-tools            (unchanged)
  ├── sentinel-core             (unchanged)
  ├── sentinel-config           (unchanged)
  └── sentinel-gpu-profiler     NEW: deterministic GPU analysis

sentinel-ai-tui
  └── sentinel-gpu-profiler     (optional, for /profile display)
```

No new crate dependencies beyond `serde_json`, `regex` (CUDA parser), and `chrono` (profile timestamps) — all already in workspace.

---

## 5. Frontend (OpenTUI + React) Changes

### cli-agent (Solid.js + OpenTUI) — `packages/cli-agent/`

| Component | Change |
|-----------|--------|
| `App.tsx` | Add GPU utilization strip in status bar (poll `/gpu --json` every 2s) |
| `App.tsx` | `/models` command now shows VRAM compatibility badge |
| `App.tsx` | `/backends` handler for multi-backend support |
| `types.ts` | Add `GpuStats`, `BackendInfo`, `ProfileResult` types |
| `backend.ts` | Add `callWithStream` for real-time GPU metrics |

### desktop-app (React + Tauri) — `packages/desktop-app/`

| Component | Change |
|-----------|--------|
| `App.tsx` | GPU dashboard panel (live utilization graph) |
| New: `components/GPUPanel.tsx` | Live nvidia-smi polling + color-coded bars |
| New: `components/ProfileView.tsx` | Structured profile output with charts |

---

## 6. Implementation Order

### Phase 1 (Current — P0)
1. **Multi-backend detection** — Extend `LocalProvider` with `auto_detect()` and `/backends` command
2. **GPU-aware model filtering** — Add VRAM detection to `detect()`, filter `recommend()` output, add embedded model size database
3. **Frontend status bar** — GPU utilization in OpenTUI status bar

### Phase 2 (Next — P0)
4. **CUDA kernel analysis** — `sentinel-gpu-profiler` crate with deterministic rules
5. **`/profile` command** — Local `ncu` integration + structured output

### Phase 3 (P1)
6. **Smart SSH profiling** — `/ssh profile` with remote nvidia-smi dmon + anomaly detection
7. **Profile log parser** — Parse existing ncu/nvidia-smi logs from file
8. **LLM-enhanced analysis** — `--llm` flag to send profile to agent for deep analysis

---

## 7. Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Zero-cost by default | User explicitly rejected LLM-based tool calls for GPU/SSH ops |
| Deterministic rules over ML | CUDA kernel analysis uses pattern matching, not learned models |
| Embedded model size DB | Avoids external API calls; update with each release |
| Backend detection via `/v1/models` | OpenAI-compatible endpoint common to Ollama, vLLM, LM Studio |
| Async SSH via `tokio::process::Command` | Already works in `cmd_ssh`; no new dependency needed |
| TUI polling every 2s | Balances freshness vs overhead for GPU metrics display |

---

## 8. Testing Strategy

| Feature | Test Approach |
|---------|---------------|
| GPU analyzer rules | Unit tests for each CUDA pattern rule |
| Model VRAM filtering | Parametrized tests with model DB × VRAM values |
| Backend detection | Mock HTTP server for each backend type |
| Profile parser | Parse known nvidia-smi dmon CSV outputs |
| SSH profiling | Integration tests via `sentinel-exec` local executor |

Tests go in: `crates/tools-and-exec/sentinel-gpu-profiler/tests/`, `crates/interfaces/sentinel-cli/tests/`.

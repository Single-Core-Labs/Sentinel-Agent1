# Agent Notes

## Workspace Structure

```
ml-intern-main/
├── crates/
│   ├── tools-and-exec/sentinel-gpu-profiler/   # GPU kernel analysis, profiling, bench
│   │   ├── src/langs.rs       — 8-language detection (CUDA, Triton, Mojo, Numba, PyTorch, CUTE, CUDA Tile, TileLang)
│   │   ├── src/cuda.rs        — 7 CUDA pattern rules, static analysis
│   │   ├── src/profile.rs     — dmon CSV parser, 5 anomaly detectors, profile summary
│   │   ├── src/bench.rs       — kernel config sweeper, heuristic scoring, real nvcc benchmark
│   │   ├── src/vram.rs        — GPU stats, VRAM detection, SM count, compute capability, NVCC flags
│   │   └── src/model_db.rs    — 22-model DB with VRAM/cloud alternatives
│   ├── interfaces/sentinel-cli/src/local.rs   — All slash commands (CLI REPL)
│   ├── platform/sentinel-provider/src/backend.rs — Multi-backend auto-detection
│   └── server/sentinel-app-server/src/handler.rs — RPC handlers
├── packages/cli-agent/src/App.tsx              — OpenTUI frontend with GPU bar
├── docs/design/rightnow-features.md            — Architecture doc
└── test-kernels/                               — Test CUDA/Triton kernel files
```

## Running

- **AI agent (CLI):** `cargo run --bin sentinel -- ai` — full interactive agent with LLM provider
- **Local REPL (no LLM):** `cargo run --bin sentinel -- ai --local` — GPU/SSH zero-cost slash commands
- **Test all:** `cargo test -p sentinel-gpu-profiler` (25 tests)
- **Compile check:** `cargo check --workspace`

## Local REPL Slash Commands (zero-cost, no LLM spend)

All deterministic GPU/SSH operations. The agent system prompt includes rich GPU context (SM count, compute capability, driver version, NVCC flags).

| Command | Description |
|---|---|
| `/gpu` | GPU stats summary (name, VRAM, util, temp) |
| `/gpu ps` | Running GPU processes via nvidia-smi pmon |
| `/gpu detailed` | Full nvidia-smi -q output |
| `/profile` | GPU profile summary |
| `/profile <file>` | Analyze kernel source (CUDA/Triton/Mojo/Numba/PyTorch/CUTE) with block size recs |
| `/profile dmon <sec>` | Real-time nvidia-smi dmon with 5 anomaly detectors |
| `/profile log <file>` | Parse existing dmon log file |
| `/profile benchmark <file>` | Compile + run kernel with nvcc (requires VS build tools on Windows) |
| `/bench` | Token throughput benchmark of current LLM model |
| `/bench kernel <file>` | Auto-sweep block sizes with heuristic scoring + top 3 recommendations |
| `/backends` | Discover local LLM backends (Ollama, vLLM, LM Studio) |
| `/ssh <host> <cmd>` | Run command remotely (zero-cost) |
| `/ssh profile <host> <sec>` | Remote GPU profiling with anomaly detection |
| `/ssh info <host>` | Remote nvidia-smi -q summary |
| `/recommend` | Hardware-aware model recommendations + per-language block size recs |
| `/info` | System, model, and token info |
| `/models` | List pulled Ollama models |
| `/show` | Current model metadata |
| `/pull <name>` | Pull a model from Ollama |
| `/stats` | Conversation statistics |
| `/clear` | Clear screen |
| `/help` or `/h` | Show all commands |

## GPU Profiler Design

- **All deterministic, zero-cost** — no LLM token spend for GPU/SSH operations
- `sentinel-gpu-profiler` is a workspace crate at `crates/tools-and-exec/`
- 8-language detection via `langs::detect_language(filename, source)` — checks extension then content patterns
- Each language has specific analyzer functions in `langs.rs` (Triton, Mojo, Numba, PyTorch, CUTE, CUDA Tile, TileLang)
- `langs::recommended_block_sizes(lang, compute_capability)` returns optimal block configs per language/SM architecture
- `profile::analyze_profile()` runs 5 anomaly detectors: compute-bound, memory-bound, CPU-bound, PCIe-bound, thermal
- `bench::benchmark_kernel_real()` tries nvcc compile+run, falls back to `bench::estimate_config()` heuristic
- `vram::query_extended_gpu_info()` returns full GPU state including driver version, clocks, power, PCIe

## Development Practices

- Run `cargo test -p sentinel-gpu-profiler` and `cargo check --workspace` after any change
- All GPU operations are checked for conditional compilation (`cfg!(target_os = ...)`)
- Use `run_shell()` for external command execution (wraps PowerShell on Windows, sh on Linux)
- New languages for kernel analysis: add enum variant in `GpuLanguage`, detection in `detect_language()`, analyzer in `analyze_*()`, then wire in `cmd_profile`
- Use `langs::recommended_block_sizes()` for language-specific config hints

## System Info

- **GPU:** NVIDIA GeForce RTX 4050 Laptop GPU (6 GB VRAM, SM86, CUDA 13.3)
- **OS:** Windows (PowerShell 5.1 for commands)
- **nvcc:** Available (CUDA 13.3, needs VS Build Tools for compilation)
- **Ollama:** Running locally with qwen3:8b and mistral models

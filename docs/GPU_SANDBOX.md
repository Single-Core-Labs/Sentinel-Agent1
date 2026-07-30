# GPU Sandbox & Accelerated Research

## Architecture

The GPU sandbox provides process isolation for GPU-accelerated workloads (local inference, ML training, fine-tuning) across Windows, Linux, and macOS.

```
User Intent → Agent → Resource Detector → GPU Scheduler
                                            │
                          ┌──────────────────┼──────────────────┐
                          ▼                  ▼                  ▼
                     Local GPU          Cloud GPU          CPU Fallback
                  (CUDA/Metal/ROCm)   (Modal/RunPod)      (llama.cpp)
```

## Sandbox Trait

The `OSJailSandbox` (`crates/tools-and-exec/sentinel-exec/src/jail.rs`) supports three isolation modes:

| Platform | Mode | Mechanism |
|---|---|---|
| Windows | `JobObject` | Windows Job Objects + security token limits |
| Linux | `Bubblewrap` | `bwrap` with namespace unsharing, read-only root |
| macOS | `Seatbelt` | `sandbox-exec` profile isolation |

## Hardware Detection

| Resource | Detection Method |
|---|---|
| OS | `cfg!(target_os)` + `std::env::consts::ARCH` |
| CPU cores | `std::thread::available_parallelism()` |
| RAM | `wmic` (Win), `sysctl hw.memsize` (mac), `/proc/meminfo` (Linux) |
| GPU | `nvidia-smi` / `rocminfo` / `system_profiler SPDisplaysDataType` |

### Low-End Hardware Fallback

| Condition | Action |
|---|---|
| No GPU | CPU-only: `llama.cpp`, no GPU offload |
| ≤8 GB RAM | Q2_K quantized models ≤1B params |
| i3 CPU | Disable speculative decoding, Flash Attention |
| Too slow | Prompt: "Provision cloud GPU? [y/N]" |

**Minimum agent RAM:** 128 MB. **Minimum local inference:** 4 GB RAM, x86-64 with AVX2.

## GPU Access by Platform

### Local GPU (Ollama / vLLM / llama.cpp)

The `/local` TUI command handles the full pipeline:

1. Detect OS, CPU, RAM, GPU
2. Download + install Ollama if missing
3. Start `ollama serve`, wait for ready
4. Pull the model, report completion

### Cloud GPU Provisioning

| Provider | Integration |
|---|---|
| Modal | Container-based serverless GPU |
| RunPod | Spot/on-demand instance provisioning |

Each experiment is containerized with a pinned environment for reproducibility.

## Cost Tracking & Budgeting

- Per-experiment cost tracking across spot and on-demand instances
- Auto-provisioning when local resources insufficient
- Cost-aware scheduling: spot instances preferred, fallback to on-demand
- Budget enforcement via `usage_thresholds` module

## CLI Commands

```bash
sentinel exec --sandbox <mode> <model> <prompt>   # Sandboxed execution
/local <model>                                     # Auto-detect & pull local model
/local llama3.2:3b                                 # Specific model pull
```

## Research Workflow

The agent automates the full ML research pipeline:

1. Detect GPU + RAM → select appropriate model/quantization
2. Install dependencies (CUDA, PyTorch, etc.)
3. Provision cloud GPU if local resources insufficient
4. Containerize experiment with pinned env
5. Run hyperparameter sweeps with cost-aware scheduling
6. Persist experiment: env, hyperparams, metrics, artifacts

## Known Gaps

- DockerSandbox and CloudSandbox resolvers are stubbed but not fully wired into the agent loop
- GPU scheduling across multiple concurrent experiments is not yet implemented
- Cloud provider integration (Modal, RunPod) requires API key configuration beyond basic env vars

## References

- `crates/tools-and-exec/sentinel-exec/src/jail.rs` — OS isolation implementation
- `crates/interfaces/sentinel-ai-tui/src/local_model.rs` — Local model auto-setup
- `docs/GPU_SANDBOX_ARCHITECTURE.md` — Full design draft (legacy)

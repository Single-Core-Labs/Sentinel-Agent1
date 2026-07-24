# GPU-Accelerated ML/DL/NLP Research in the CLI

## Why

Every ML experiment today requires the same tedious setup:

| Problem | Time wasted |
|---------|-------------|
| Install CUDA + cuDNN + matching PyTorch | 30-60 min |
| Resolve driver/package conflicts | 15-30 min |
| Manually provision cloud GPU instances | 20-40 min |
| SSH keys, port forwarding, `rsync` artifacts | 10-20 min |
| Track which run used which env + config | perpetual |

A CLI agent that automates the full pipeline lets researchers go from idea → result without leaving the terminal.

## Architecture

```
                         Agent CLI
                              │
              ┌───────────────┼───────────────┐
              │               │               │
         ┌────┴────┐    ┌────┴────┐    ┌─────┴─────┐
         │detect   │    │scheduling│    │experiment │
         │gpu/ram  │    │(local vs│    │runner     │
         │vram     │    │ cloud)  │    │(container)│
         └────┬────┘    └────┬────┘    └─────┬─────┘
              │              │               │
              ▼              ▼               ▼
    ┌───────────────────────────────────────────────┐
    │           Execution Layer                      │
    │  ┌────────────┐  ┌────────┐  ┌──────────────┐ │
    │  │  Local GPU  │  │  CPU   │  │  Cloud GPU   │ │
    │  │(CUDA/Metal)│  │fallback│  │(Modal/RunPod)│ │
    │  └────────────┘  └────────┘  └──────┬───────┘ │
    └──────────────────────────────────────┼─────────┘
                                          │
                               ┌──────────┴──────────┐
                               │  Auto-provision      │
                               │  + container per job │
                               │  + cost-aware (spot) │
                               └─────────────────────┘
```

## Layers

### Layer 1 — Local detection
- Detect GPU (NVIDIA via `nvidia-smi`, AMD via `rocminfo`, Apple via `system_profiler`)
- Read VRAM, total RAM, CPU cores
- If sufficient → run directly. If insufficient → offer cloud.

### Layer 2 — Auto cloud provisioning
- Integrated with Modal / RunPod / Vast.ai API
- Spin up the exact GPU needed (A10G, A100, H100)
- Pre-built container with CUDA + PyTorch + common libs
- SSH-less: agent streams logs + results via API

### Layer 3 — Experiment runner
- Containerized per job (reproducible environments)
- Hyperparameter sweeps defined inline
- Artifacts (checkpoints, logs, metrics) auto-synced
- Every run snapshots: env, commit hash, hyperparams, metrics

### Layer 4 — Cost-aware scheduling
- Spot instances (cheap, may preempt) vs on-demand
- Budget caps per experiment / per session
- Agent auto-selects cheapest GPU that meets VRAM requirements

## CLI workflow

```
> /run-finetune --model llama --dataset my-corpus --lora-r 8,16 --lr 1e-4,5e-5
  🔍 Detecting local GPU... RTX 4090 (24 GB) ✓
  ⚠  VRAM enough for batch-size 4, but sweep needs 3 runs
  ⏳ Provisioning 3x A10G on Modal (est. $0.18)
  ✓ Run 1/3: lr=1e-4, r=8  → loss 0.23 (12 min)
  ✓ Run 2/3: lr=5e-5, r=8  → loss 0.19 (11 min)
  ✓ Run 3/3: lr=1e-4, r=16 → loss 0.17 (14 min)
  📊 Best: lr=1e-4, r=16 → report.md + checkpoint.pt saved
  🧹 Cloud instances terminated. Total cost: $0.72
```

## Is this a great idea?

Yes — it collapses the research iteration loop from **hours of infrastructure management** to **seconds of intent**. The agent becomes an ML infrastructure operator: the researcher stays in the problem space instead of the environment space. The difference between *managing infrastructure* and *doing research*.

## Implementation plan

| Phase | What | Priority |
|-------|------|----------|
| 1 | `/local` — detect system, install Ollama, run local models | ✅ Done |
| 2 | GPU detection + VRAM query → `/run` command with local execution | Next |
| 3 | Cloud provider integration (Modal API) | Next+ |
| 4 | Experiment tracker + artifact store | Later |
| 5 | Hyperparameter sweeps + cost scheduler | Later |

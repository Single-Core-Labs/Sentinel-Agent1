# GPU Emulator & Auto Optimizer — Testing Guide

## Build & Run Tests

```bash
cargo build --workspace
cargo test -p sentinel-gpu-profiler   # 58 tests, 0 failures expected
cargo run --bin sentinel -- ai --local   # Launch local REPL with slash commands
```

---

## 1. GPU Emulator — Cycle-Approximate Kernel Simulation

Emulates a kernel across any GPU architecture (Pascal → Blackwell) without physical hardware.

```bash
# Single architecture (default: RTX 3090, RTX 4090, H100)
/emulate test-kernels/vec_add.cu

# Multi-arch comparison across all 10 architectures
/emulate test-kernels/matmul.cu --all

# Custom architecture by compute capability or GPU name
/emulate test-kernels/vec_add.cu --arch=sm_86
/emulate test-kernels/vec_add.cu --arch=h100
/emulate test-kernels/vec_add.cu --arch=a100,4090,h100
```

**Output includes:** total cycles, IPC, execution time, occupancy %, SM utilization %, bottleneck type, memory coalescing %, bank conflicts, register spills, roofline analysis.

**Tests:** `test_emulate_full_pipeline`, `test_multi_arch_comparison`, `test_tensor_core_detection`, `test_zero_thread_handling`

---

## 2. SM Utilization — Stall-Aware Warp Scheduler Model

SM utilization accounts for warp scheduler throughput and memory stalls. Higher is better.

```bash
# Check SM utilization for different block sizes
/emulate test-kernels/vec_add.cu --arch=sm_86

# Compare 32-thread vs 256-thread blocks (SM util should differ)
# The execution report shows "SM Util: XX%" in the Occupancy section
```

**Tests:** `test_sm_util_in_range`, `test_sm_util_zero_for_no_instructions`, `test_sm_util_differs_by_occupancy`

---

## 3. Config Sweep — Find the Best Launch Config

Auto-sweeps ~100 block × shared-memory combinations, scores each with a 6-factor formula, and recommends the best.

```bash
# Full sweep on default architecture
/emulate test-kernels/vec_add.cu --sweep

# Sweep for a specific architecture
/emulate test-kernels/matmul.cu --arch=sm_90 --sweep
```

**Output:** Ranked top-12 table with Cycles, IPC, SM Util, Occupancy, Coalescing, Time, and Score. Below the table: `★ Best Config` with detailed metrics and 7 pattern-based recommendations (low occupancy, poor coalescing, bank conflicts, register spills, etc.).

**Scoring formula:** Cycles (30%) + Occupancy (20%) + SM Util (15%) + Coalescing (10%) + Sector Util (5%) + IPC (10%), minus bank conflict penalty (−5% each, cap −30%) and register spill penalty (−0.2% each, cap −10%).

**Tests:** `test_generate_sweep_configs_produces_configs`, `test_run_config_sweep_returns_sorted`, `test_sweep_result_in_emulate_output`, `test_detect_best_config_returns_top`, `test_score_entry_reflects_performance`

---

## 4. Auto Optimizer — Bottleneck Analysis + Speedup Estimation

Analyzes a kernel, identifies bottlenecks, builds an LLM optimization prompt, and estimates speedup.

```bash
# Analyze and build optimization prompt
/optimize test-kernels/matmul.cu

# Target specific architecture
/optimize test-kernels/vec_add.cu --arch=sm_90

# Show what the AI knows about your hardware
/gpu-context
```

**Output:** Bottleneck report (primary + details), speedup table with before/after cycles, time, and speedup×, compilation status, and optimization notes. To run the full LLM rewrite pipeline, set `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` and run `sentinel ai` instead of `sentinel ai --local`.

**Tests:** `test_analyze_bottlenecks_from_emulate`, `test_build_prompt_contains_gpu_context`, `test_speedup_positive`, `test_optimize_output_formatting`, `test_extract_kernel_from_code_block`, `test_compute_diff_shows_changes`, `test_format_provider_table_output`

---

## 5. Full Workflow Integration Test

```bash
cargo run --bin sentinel -- ai --local

# Step 1 — See what the AI knows about your GPU
/gpu-context

# Step 2 — Emulate a kernel on your GPU
/emulate test-kernels/vec_add.cu --arch=sm_86

# Step 3 — Find the best launch config
/emulate test-kernels/matmul.cu --sweep

# Step 4 — Compare across architectures
/emulate test-kernels/matmul.cu --arch=sm_86,sm_89,sm_90

# Step 5 — Get optimization suggestions
/optimize test-kernels/matmul.cu
```

---

## Quick Troubleshooting

| Problem | Fix |
|---------|-----|
| `/emulate` shows no output | Use absolute path or run from repo root |
| All configs score 0 | Kernel source is empty or comments only |
| Occupancy 0% | Block size exceeds 1024 threads |
| SM util 0% | Kernel has zero arithmetic/memory ops |
| `/optimize` no LLM output | Set API key or use `sentinel ai` (not `--local`) |
| Sweep slow (~100ms) | Expected — 80 configs × 5-stage pipeline |

---

## Architecture Support

| GPU | Compute Capability | Alias |
|-----|-------------------|-------|
| Tesla P100 / GTX 1080 | 6.1 | `sm_61`, `pascal` |
| Tesla V100 | 7.0 | `sm_70`, `volta` |
| RTX 2080 / T4 | 7.5 | `sm_75`, `turing` |
| A100 | 8.0 | `sm_80` |
| RTX 3090 | 8.6 | `sm_86`, `ampere` |
| RTX 4090 | 8.9 | `sm_89`, `ada` |
| H100 | 9.0 | `sm_90`, `hopper` |
| B200 / RTX 5090 | 10.0 | `sm_100`, `blackwell` |

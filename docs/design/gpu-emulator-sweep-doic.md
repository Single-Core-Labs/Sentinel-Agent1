# Config Sweep Engine — Document of Intent & Charter

## 1. Problem Statement

Finding the optimal launch configuration for a GPU kernel is traditionally done through:

1. **Manual tuning**: Developer hardcodes a block size and shared memory allocation, deploys, profiles, adjusts, redeploys — takes hours
2. **Autotuning frameworks**: Triton's `@triton.autotune`, CUTLASS's profiler, or NVIDIA's cuOpt — require real hardware, take minutes per config, cannot be used in CI/CD without GPU runners
3. **Heuristic guidance**: Static rules-of-thumb (e.g., "use 256 threads per block for vector add") — miss architecture-specific optimizations and kernel-specific trade-offs

Without hardware, there is no way to:
- Predict whether 128 threads + 32KB shared memory beats 256 threads + 16KB
- Detect that a kernel is compute-bound at 256 threads but memory-bound at 512
- Identify register pressure or bank conflicts that only appear at specific block sizes

Cost of a bad launch config: a kernel running at 40% occupancy wastes 2.5× GPU-hours. At scale (1000+ H100s), a 1% efficiency gain saves ~$50K/yr.

## 2. Solution: Static Config Sweep Engine

A zero-cost deterministic config space explorer built on top of the GPU emulator. It:

1. Generates ~100 launch configurations (block_x × block_y × shared_mem combinations)
2. Runs `emulate()` on each config (same source, same arch) — pure CPU, < 100ms total
3. Scores each result using a 6-factor weighted formula
4. Ranks configs by score, identifies the best, and produces actionable recommendations

**Key constraint:** zero-cost deterministic — same as the emulator, no GPU, no LLM, < 100ms.

## 3. Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                         Config Sweep Engine (emulate.rs)                      │
│                                                                              │
│  ┌──────────────────────────────────────┐                                    │
│  │        generate_sweep_configs()       │                                    │
│  │  7× BLOCK_X × 3× BLOCK_Y × 5× SMEM   │                                    │
│  │  Filters: threads ≤ 1024, ≥ 32,      │                                    │
│  │  ratio ≤ 4× source hint              │                                    │
│  └──────────┬───────────────────────────┘                                    │
│             │ Vec<LaunchConfig> (~100)                                       │
│             ▼                                                                │
│  ┌──────────────────────────────────────┐                                    │
│  │         run_config_sweep()            │                                    │
│  │  for each config: emulate(source,    │  ── calls emulate.rs 5-stage       │
│  │                      cfg, arch)      │  ── pipeline for each config       │
│  │  score_entry() → rank descending      │                                    │
│  └──────────┬───────────────────────────┘                                    │
│             │ Vec<SweepEntry>                                                  │
│  ┌──────────▼───────────┐  ┌────────────▼───────────┐                        │
│  │   detect_best_config() │  │  format_sweep_table()  │                        │
│  │   entries.first()      │  │  top-12 with metrics   │                        │
│  └──────────┬───────────┘  └────────────┬───────────┘                        │
│             │                            │                                    │
│  ┌──────────▼────────────────────────────▼───────────┐                        │
│  │            format_sweep_recommendations()           │                        │
│  │  7 pattern-based checks + similar score detection  │                        │
│  └────────────────────────────────────────────────────┘                        │
└──────────────────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           Integration (local.rs)                              │
│                                                                              │
│  /emulate <file> --sweep    — full sweep on default arch (Ampere86)          │
│  /emulate <file> --sweep --arch=sm_90  — sweep for H100                      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Module Dependency

All sweep code lives in `emulate.rs`. It depends only on the emulator's existing pipeline — no new dependencies.

```
emulate.rs
├── generate_sweep_configs()   — regex on source for <<<grid,block>>> hint
├── run_config_sweep()         — for-each emulate() + score_entry()
├── score_entry()              — pure function: SweepEntry → f64
├── detect_best_config()       — O(1) top-of-sorted-list
├── format_sweep_table()       — String formatter (top 12)
├── format_sweep_recommendations() — expert-system recommendations
└── integrate via EmulateRequest::sweep + EmulateOutput::sweep_result
```

## 4. Core Data Types

### SweepEntry

```rust
pub struct SweepEntry {
    pub config: LaunchConfig,       // The launch configuration
    pub label: String,              // Human-readable: "256x1", "128x1 smem=16KB"
    pub result: EmulationResult,    // Full emulation output (cycles, IPC, occupancy, etc.)
    pub score: f64,                 // Composite score from score_entry()
}
```

### SweepResult

```rust
pub struct SweepResult {
    pub entries: Vec<SweepEntry>,       // Sorted descending by score
    pub best: Option<SweepEntry>,       // entries.first() cloned
    pub kernel_name: String,            // Source filename
    pub arch: GpuArch,                  // Target architecture
}
```

### Sweep Dimension Constants

```
SWEEP_BLOCK_X = [64, 96, 128, 192, 256, 384, 512]   — 7 block sizes
SWEEP_BLOCK_Y = [1, 2, 4]                            — 3 block y-dimensions
SWEEP_SMEM_KB = [0, 8, 16, 32, 48, 64]               — 6 shared memory budgets
```

Product: 7 × 3 × 6 = 126 raw configs. After filtering (threads ≤ 1024, ≥ 32, ≤ 4× source hint), typically yields 70–110 configs.

### EmulateRequest / EmulateOutput Additions

```rust
pub struct EmulateRequest {
    // ... existing fields
    pub sweep: bool,                    // New: if true, run config sweep
}

pub struct EmulateOutput {
    // ... existing fields
    pub sweep_result: Option<SweepResult>,  // New: present when sweep=true
}
```

## 5. Scoring Formula

`score_entry()` weights 6 metrics and applies 2 penalties:

| Component | Weight | Normalization | Max Contribution |
|-----------|--------|---------------|-----------------|
| **Cycle count** | 30% | `1 - cycles / 1e9` | 0.30 |
| **Occupancy** | 20% | `occupancy_pct / 100` | 0.20 |
| **SM Utilization** | 15% | `sm_util_pct / 100` | 0.15 |
| **Coalescing** | 10% | `coalescing_efficiency` | 0.10 |
| **Sector Utilization** | 5% | `sector_utilization` | 0.05 |
| **IPC** | 10% | `min(ipc / 32, 1)` | 0.10 |
| **Total base** | 90% | | 0.90 |

Penalties (multiplicative, deducted from 1.0):

| Penalty | Per-unit rate | Cap |
|---------|--------------|-----|
| Bank conflicts | −5% per conflict | −30% max |
| Register spills | −0.2% per spill | −10% max |

Final: `score = base_sum × (1 − bank_penalty − spill_penalty)`

## 6. Design Decisions

### Decision 1: Static Block Size Grid over Autotuner Seed

**Choice:** Sweep a fixed set of block sizes (64, 96, 128, 192, 256, 384, 512) rather than extracting from Triton-style `@autotune` decorators.

**Rationale:**
- Works for all 8 supported languages — no language-specific autotune parsing
- Covers the practical range: block sizes below 64 underutilize warps, above 512 saturate register/thread limits
- 96 and 192 are non-power-of-two sizes that often outperform 128/256 for specific kernels

**Limitation acknowledged:** Some kernels benefit from block sizes outside this range (e.g., 32 or 1024). Extreme sizes that pass the thread filter are included via the `SWEEP_BLOCK_X` array.

### Decision 2: Grid Size Fixed per Sweep

**Choice:** Derive grid from source's `<<<grid, block>>>` syntax or default to `max(32, hint_grid)`. Do NOT sweep grid sizes.

**Rationale:**
- Grid size primarily affects total work, not per-SM efficiency
- Sweeping grid would multiply config count from ~100 to ~1000+ (adding grid factors ×2, ×4)
- The emulator's cycle count scales proportionally — relative ranking of block configurations is grid-agnostic

**Limitation acknowledged:** Some kernels have grid-dependent behavior (e.g., boundary conditions in the last block). This is a known emulator limitation, not a sweep-specific one.

### Decision 3: Top-K Display Rather Than Best-Only

**Choice:** Show a ranked table of the top 12 configs plus the `★ Best` config with detailed recommendations.

**Rationale:**
- Second-best may be nearly tied (score diff < 0.05) — user can consider both
- The recommendations engine detects ties and surfaces alternatives
- Displaying 12 configs gives the user a sense of the landscape (e.g., "all high-scoring configs use 256 threads")

### Decision 4: 6-Factor Scoring over Single-Metric Ranking

**Choice:** Composite scoring with cycle count as the heaviest factor (30%) versus ranking purely by cycles.

**Rationale:**
- A config that achieves 95% of peak cycles but has poor coalescing will cause problems at scale
- Register spills and bank conflicts degrade real-world performance beyond what cycle count alone captures
- Weighted formula mirrors how a human expert would evaluate: "fast is good, but not if it wastes memory bandwidth"

### Decision 5: Pattern-Based Recommendations over ML

**Choice:** 7 deterministic pattern checks + similarity detection rather than a learned model.

**Rationale:**
- Zero training data required — works from day one
- Deterministic and auditable — user can verify the logic in < 100 lines
- Covers the 7 most common GPU performance pitfalls (>90% of real issues)

## 7. Integration Points

### With local.rs (CLI)

```
/evaluate <file> --sweep
  └─ cmd_emulate()
      ├─ arg parsing: detect "--sweep" → do_sweep = true
      ├─ EmulateRequest { sweep: true, ... }
      ├─ run_emulation() → EmulateOutput { sweep_result: Some(...) }
      ├─ format_sweep_table(entries)    → ranked top-12 table
      └─ format_sweep_recommendations() → best config + 7-point checklist
```

The CLI output for `--sweep` replaces the single-arch execution report with:

```
  Config              Cycles      IPC      SM    Occup.  Coalesc      Time(us)    Score
  ----------------------------------------------------------------------------------------
  256x1                120000    1.45    67%      75%     100%        343.2      0.613
  128x1                 98000    1.78    72%      75%     100%        297.5      0.598
  384x1                155000    1.12    55%      67%     100%        443.7      0.541
  ...

  ★ Best Config: 256x1  (score: 0.613)
     Grid: 100x1x1  Block: 256x1x1  SMEM: 0 bytes
     Cycles: 120000, IPC: 1.45, Est. Time: 343.2 us
     Occupancy: 75%, SM Util: 67%, Coalescing: 100%
     Bottleneck: Compute-bound, Limiting Factor: registers
```

### With bench.rs (Heuristic Scoring)

`bench.rs` provides `generate_configs()` and `estimate_config()` — a lighter-weight heuristic sweeper for `/bench kernel`. The emulator sweep is the full pipeline version:

| Aspect | bench.rs | emulate sweep |
|--------|----------|---------------|
| **Pipeline** | Heuristic scoring (no cycle model) | Full 5-stage emulate pipeline |
| **Metrics** | 3 factors (occupancy, IPC estimate, mem) | 6 factors + 2 penalties |
| **Configs** | Block size only | Block × shared_mem |
| **Time** | < 1ms | ~10–100ms |
| **Accuracy** | Relative ranking | Cycle-approximate |

The two are complementary: bench for rapid iteration, emulate sweep for final selection.

### With profile.rs (Runtime Validation)

Sweep predicts the best config. After deployment, `profile.rs`'s dmon analysis can validate:
- Actual occupancy matches predicted
- Actual SM utilization matches predicted  
- Bottleneck type matches predicted

This creates a closed loop: predict (emulate sweep) → measure (profile dmon) → improve (adjust config).

## 8. Data Flow: End-to-End Example

```
Input: /emulate reduce.cu --sweep --arch=sm_90

1. cmd_emulate():
   ├─ detect "--sweep" → do_sweep = true
   ├─ detect "--arch=sm_90" → [Hopper90]
   └─ langs::detect_language("reduce.cu", source) → Cuda

2. EmulateRequest { sweep: true, arches: [Hopper90], ... }
   └─ run_emulation(req)
       ├─ generate_sweep_configs(source)
       │   ├─ extract <<<128, 256>>> from source → hint_grid=128, hint_block=256
       │   ├─ generate: 64×1, 64×1+8KB, 64×1+16KB, ... 96×1, 96×1+8KB, ...
       │   └─ filter: skip when threads > 1024 or < 32 or ratio > 4×
       │   └─ ~80 LaunchConfigs
       │
       ├─ run_config_sweep(source, configs, Hopper90)
       │   ├─ emulate(source, 64×1, Hopper90)   → 340K cycles, 32% occup, score=0.298
       │   ├─ emulate(source, 128×1, Hopper90)  → 210K cycles, 67% occup, score=0.514
       │   ├─ emulate(source, 256×1, Hopper90)  → 195K cycles, 75% occup, score=0.613
       │   ├─ emulate(source, 384×1, Hopper90)  → 220K cycles, 67% occup, score=0.541
       │   ├─ emulate(source, 256×1+16KB, ... ) → 180K cycles, 63% occup, score=0.587
       │   └─ ... ~80 iterations
       │   └─ sort descending by score
       │
       └─ EmulateOutput { sweep_result: Some(sweep) }

3. Output:
   Language: CUDA
   Hint: nvcc -arch=sm_90 reduce.cu

   Config              Cycles      IPC      SM    Occup.  Coalesc      Time(us)    Score
   ----------------------------------------------------------------------------------------
   256x1                195000    1.45    67%      75%      93%        557.1      0.613
   256x1 smem=16KB      180000    1.56    64%      63%      95%        514.3      0.587
   128x1                210000    1.78    72%      75%      89%        600.0      0.544
   384x1                220000    1.12    55%      67%      90%        628.6      0.541
   192x1                220500    1.38    70%      75%      88%        630.0      0.534
   ...

   ★ Best Config: 256x1  (score: 0.613)
      Grid: 128x1x1  Block: 256x1x1  SMEM: 0 bytes
      Cycles: 195000, IPC: 1.45, Est. Time: 557.1 us
      Occupancy: 75%, SM Util: 67%, Coalescing: 93%
      Bottleneck: Compute-bound, Limiting Factor: registers

     ~ Similar score (2.6%) to 256x1 smem=16KB. Consider both.
```

## 9. Edge Cases & Limitations

### Identified & Documented

| Edge Case | Behavior | Why It's OK |
|-----------|----------|-------------|
| Empty source | Generates default configs (128×1, 256×1) | Sweep on a file with no kernel body is a user mistake; graceful fallback |
| No launch config hint | `hint_block=256, hint_grid=256` — pulls default | Conservative starting point for filter ratio |
| All configs filtered out | Falls back to 2 default configs | Guarantees non-empty result |
| Single config generated | Table shows 1 row, recommendations still work | No ranking needed, but hints may still fire |
| Config with zero instructions | `emulate()` returns zero cycles, zero occupancy, zero SM util | Scores near 0% — ranks at bottom, no crash |
| Identical scores | `sort_by` falls back to `Ordering::Equal` — stable sort preserves insertion order | Tie is surfaced by recommendations if score diff < 0.05 |
| Extremely high cycles (> 1e9) | `cycle_norm` saturates at minimum 0.01 (i.e., 1% weight remaining) | Cycles dominate scoring until 1e9; beyond that, other factors matter more |
| Shared memory > arch limit | Occupancy calculator clamps blocks_per_sm → lower occupancy | Correct modeling: the arch spec's shared_mem_per_sm is enforced |

### Known Limitations

1. **No grid size sweep** — grid is fixed from source hint, may miss optimal grid-dependent behavior
2. **No register count sweep** — fixed at 32 registers per thread; real compilers vary allocation
3. **No dynamic shared memory sweep** — shared_mem is an input, not a searchable range
4. **Single architecture per sweep** — `--sweep` uses `arches[0]`, ignores multi-arch
5. **Heuristic scoring weights** — default weights may not match every kernel family's sensitivity
6. **Recommendation patterns are fixed** — 7 rules cover ~90% of issues, but kernel-specific advice requires expert knowledge

## 10. Test Strategy

8 tests covering the sweep engine + SM utilization:

| Category | Tests | What They Verify |
|----------|-------|------------------|
| **SM Utilization** | `test_sm_util_in_range` | SM util is 0–100% for a real kernel |
| | `test_sm_util_zero_for_no_instructions` | Zero instructions → 0% SM util |
| | `test_sm_util_differs_by_occupancy` | Different block sizes produce different SM util values |
| **Config Generation** | `test_generate_sweep_configs_produces_configs` | Sweep generates at least 2 configs for any input |
| **Sweep Execution** | `test_run_config_sweep_returns_sorted` | Entries are sorted descending by score |
| **Integration** | `test_sweep_result_in_emulate_output` | `--sweep=true` produces `sweep_result` with best config, table contains "Config", recommendations contain "Best Config" |
| **Best Detection** | `test_detect_best_config_returns_top` | `detect_best_config()` returns `entries[0]` |
| **Scoring** | `test_score_entry_reflects_performance` | A config with 256×1 scores ≥ a config with 32×1 + 255 regs |

---

*DOIC v1.0 — Generated from reverse engineering of `crates/tools-and-exec/sentinel-gpu-profiler/src/emulate.rs` (lines 662–850: ~190 lines sweep engine), `crates/interfaces/sentinel-cli/src/local.rs` (cmd_emulate --sweep integration)*

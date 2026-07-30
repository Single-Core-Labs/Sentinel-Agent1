# AI Features — Document of Intent & Charter

## 1. Problem Statement

GPU programming is hard. Developers face three pain points:

1. **Writing correct CUDA kernels** requires deep knowledge of memory hierarchy, warp scheduling, occupancy, tensor cores, and architecture-specific intrinsics. Even experienced engineers waste hours on race conditions and bank conflicts.
2. **Optimizing existing kernels** is manual and iterative — profile, guess, tweak, re-profile, repeat. A single iteration takes 5–15 minutes with Nsight Compute.
3. **Generic AI suggestions** waste time. A chat model that doesn't know your GPU (RTX 4050 vs H100 vs RTX 4090), your compute capability (sm_86 vs sm_90), or your VRAM budget (6 GB vs 80 GB) produces irrelevant or harmful advice.

Existing approaches fail because:
- **Copilot/GPT**: Generic CUDA knowledge, no awareness of the user's specific GPU, SM count, VRAM, or tensor core availability
- **Nsight Compute**: Powerful but steep learning curve, no code generation
- **Autotuners (Triton, CUTLASS)**: Require GPU hardware to run, cannot suggest architectural changes
- **Manual optimization**: Hours of profiling → guess → retry

Cost of inefficiency: A kernel at 40% occupancy wastes 2.5× GPU-hours. At 1000 H100s at $40/hr, that's $100K/hr in wasted compute.

## 2. Solution: Two AI Modes

### Chat Assistant (Interactive GPU-Aware Coding)

A chat interface where:
- The AI knows the user's exact GPU (name, SM count, compute capability, VRAM, driver version, CUDA version, tensor core support)
- Every suggestion is tailored to the detected hardware — no "if you have an H100" qualifiers
- Users write, debug, and optimize kernels through natural conversation
- Custom agents can be created with skills (reusable toolkits for specific tasks)
- MCP (Model Context Protocol) integrations extend tooling

### Auto Optimizer (One-Click Kernel Optimization)

A pipeline that:
1. Accepts a CUDA kernel source file and optional NCU (Nsight Compute) profiling data or fallback heuristic data from the emulator
2. Reads the profiling data to identify exact bottlenecks (memory-bound, compute-bound, occupancy-limited, register spills, bank conflicts)
3. Sends kernel + profile to an AI model with targeted optimization instructions
4. The AI rewrites the kernel (applies tiling, coalescing, tensor core mapping, occupancy tuning, shared memory padding)
5. Verifies correctness (compilation check + output comparison on a small test input)
6. Reports speedup estimate using the emulator's before/after comparison
7. Instant rollback to original if results are worse

**Key constraint:** All GPU detection, profiling analysis, and speedup estimation happen through the existing zero-cost deterministic pipeline (vram, emulate, profile). AI is used only for the creative task — writing and optimizing code.

## 3. Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                         AI Features Layer (local.rs + new modules)             │
│                                                                              │
│  ┌─────────────────────────────────┐  ┌─────────────────────────────────┐    │
│  │       Chat Assistant            │  │        Auto Optimizer           │    │
│  │  ┌───────────────────────────┐  │  │  ┌───────────────────────────┐  │    │
│  │  │ GPU-Context Injection     │  │  │  │ read_ncu_profile()        │  │    │
│  │  │ (combines hardware spec   │  │  │  │ or read_emulator_suggest()│  │    │
│  │  │ into system prompt)       │  │  │  └──────────┬───────────────┘  │    │
│  │  └───────────────────────────┘  │  │             ▼                   │    │
│  │  ┌───────────────────────────┐  │  │  ┌───────────────────────────┐  │    │
│  │  │ Agent + Skills Engine     │  │  │  │ Bottleneck Classifier     │  │    │
│  │  │ (loads .agents/skills/    │  │  │  │ (profile + emulator data) │  │    │
│  │  │ as reusable context)      │  │  │  └──────────┬───────────────┘  │    │
│  │  └───────────────────────────┘  │  │             ▼                   │    │
│  │  ┌───────────────────────────┐  │  │  ┌───────────────────────────┐  │    │
│  │  │ MCP Client/Server        │  │  │  │ LLM Optimization Prompt   │  │    │
│  │  │ (Model Context Protocol) │  │  │  │ → rewrite kernel           │  │    │
│  │  └───────────────────────────┘  │  │  └──────────┬───────────────┘  │    │
│  └─────────────────────────────────┘  │             ▼                   │    │
│                                        │  ┌───────────────────────────┐  │    │
│                                        │  │ Correctness Verifier      │  │    │
│                                        │  │ (nvcc compile + test run) │  │    │
│                                        │  └──────────┬───────────────┘  │    │
│                                        │             ▼                   │    │
│                                        │  ┌───────────────────────────┐  │    │
│                                        │  │ Speedup Reporter          │  │    │
│                                        │  │ (emulate both → compare)  │  │    │
│                                        │  └───────────────────────────┘  │    │
│                                        └─────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────────────┘
                           │                    │
              ┌────────────▼─────┐   ┌──────────▼──────────┐
              │  LLM Provider    │   │  GPU Profiler Stack  │
              │  (OpenAI, Claude,│   │  vram, emulate,      │
              │   Ollama, etc.)  │   │  profile, bench      │
              └──────────────────┘   └─────────────────────┘
```

### Module Dependency Graph

```
local.rs                       # CLI entry: cmd_ai(), cmd_optimize() slash commands
├── sentinel-core::Agent      # Agent loop (handles inference + tool execution)
├── sentinel-provider         # Provider dispatch (cloud + local)
├── sentinel-gpu-profiler
│   ├── vram                  # Hardware detection → GPU context string
│   ├── emulate               # Before/after speedup estimation
│   ├── profile               # NCU/dmon profile parsing
│   └── bench                 # Heuristic scorer
├── optimizer.rs (NEW)         # Auto Optimizer pipeline
│   ├── ProfileReader         # NCU data → BottleneckReport
│   ├── KernelOptimizer       # LLM-driven kernel rewrite
│   ├── CorrectnessVerifier   # Compile + test
│   └── SpeedupEstimator      # emulate() before vs after
├── skills/                   # Custom agent skills (loaded from .agents/skills/)
└── mcp/                      # MCP client integration
```

## 4. Core Data Types

### GPU Context (existing, enhanced)

```rust
pub struct GpuContext {
    pub gpu_name: String,
    pub compute_capability: String,   // "8.6"
    pub sm_count: u32,                // 20 (RTX 4050 laptop)
    pub vram_total_gb: f64,
    pub vram_used_gb: f64,
    pub driver_version: String,
    pub cuda_version: String,         // "13.3"
    pub tensor_cores: bool,
    pub clocks: GpuClocks,
    pub pcie: PcieInfo,
    pub recommended_block_sizes: Vec<BlockSizeRec>,
}
```

Serialized as a structured context block injected into every system prompt.

### Auto Optimizer Types

```rust
pub struct OptimizeRequest {
    pub source: String,               // Original kernel source
    pub filename: String,
    pub ncu_profile: Option<String>,  // Raw NCU output (optional)
    pub language: GpuLanguage,
    pub target_arch: GpuArch,
    pub launch_config: LaunchConfig,
    pub provider: ProviderKind,       // Which AI model to use
}

pub struct OptimizeOutput {
    pub original_source: String,
    pub optimized_source: String,
    pub diff: String,                 // Unified diff
    pub bottleneck_report: BottleneckReport,
    pub speedup_estimate: SpeedupResult,
    pub compiled_ok: bool,
    pub correctness_passed: Option<bool>,
    pub llm_response: String,         // Raw LLM optimization notes
}

pub struct BottleneckReport {
    pub primary_bottleneck: &'static str,  // "Memory-bound", "Compute-bound", "Occupancy-limited"
    pub occupancy_pct: f64,
    pub sm_util_pct: f64,
    pub coalescing_efficiency: f64,
    pub bank_conflicts: u64,
    pub register_spills: u64,
    pub details: Vec<String>,
}

pub struct SpeedupResult {
    pub before_cycles: u64,
    pub after_cycles: u64,
    pub before_time_us: f64,
    pub after_time_us: f64,
    pub speedup_x: f64,
    pub improvement_pct: f64,
}
```

### Provider UI Types

```rust
pub struct ProviderChoice {
    pub category: ProviderCategory,     // Cloud, Local, Private
    pub name: &'static str,
    pub models: Vec<ModelEntry>,
    pub configured: bool,               // Has API key / is running
}

pub enum ProviderCategory {
    Cloud,
    Local,
    Private,
}
```

## 5. Design Decisions

### Decision 1: GPU Context as System Prompt Injection

**Choice:** Serialize hardware detection into a structured text block injected into every LLM system prompt, rather than requiring the model to call a `get_gpu_info` tool.

**Rationale:**
- Zero latency — no tool call round-trip
- The model always has context, even for the first response
- Works with any provider (OpenAI, Anthropic, Ollama) — no tool-calling requirement
- ~200 tokens, negligible cost

### Decision 2: Auto Optimizer Runs on the Emulator, Not Real Hardware

**Choice:** Use `emulate::emulate()` for before/after speedup estimation rather than nvcc compile+run.

**Rationale:**
- Deterministic, zero-cost, no GPU required
- Works in CI/CD without GPU runners
- Sub-100ms vs minutes for compile+profile cycle
- The emulator is already validated against real hardware

**Fallback:** If nvcc is available, `--compile` flag runs actual compilation to verify correctness.

### Decision 3: NCU Profile as Optional Enhancement

**Choice:** The optimizer works with the emulator alone (zero-cost). NCU data provides more precise bottleneck identification but is optional.

**Rationale:**
- Demos work on any machine, even without NVIDIA tools
- The emulator already detects memory-bound vs compute-bound, coalescing, bank conflicts, register spills
- NCU input parser can be added incrementally
- Users with NCU get more targeted optimizations

### Decision 4: Correctness Verification via Compilation + Reference Output

**Choice:** After optimization, compile the new kernel and run it against a reference input. Compare output bitwise.

**Rationale:**
- Prevents the AI from generating code that looks right but computes wrong results
- Catches subtle errors (signed/unsigned mismatch, index off-by-one, race conditions)
- The compilation check alone catches syntax errors and type mismatches

**Fallback:** If nvcc is not available, compilation check is skipped but correctness is marked as "unverified."

### Decision 5: Custom Agents via Skills (File-Based)

**Choice:** Skills are Markdown files in `.agents/skills/` that describe reusable capabilities (CUDA warp-level primitives, TensorRT deployment, cuBLAS wrapper patterns). The agent loads them as additional system prompt context.

**Rationale:**
- Zero-code skill creation — any user can write a skill file
- Git-trackable, shareable across team
- MCP integration uses a similar file-based protocol

## 6. Integration Points

### With local.rs (CLI Slash Commands)

The Chat Assistant is the default mode when running `sentinel ai --local` (already exists). The Auto Optimizer adds:

```
/optimize <file>                          # Auto-optimize a kernel
/optimize <file> --ncu=profile.ncu-rep    # Optimize with NCU data
/optimize <file> --provider=claude        # Use specific AI provider
/optimize <file> --no-verify              # Skip correctness check
/optimize <file> --rollback               # Restore original

/gpu-context                              # Show the GPU context being injected
/skills                                   # List loaded skills
/skill <name> <task>                      # Run a skill task
```

### With Provider System

The provider picker already exists in `local.rs` startup. Enhancements:
- Categorize as Cloud / Local / Private in display
- Show configured status (API key present vs missing)
- Allow model override per session with `/model`

### With GPU Profiler Stack

```
OptimizeRequest
├── vram::query_extended_gpu_info()     → GPU context string
├── emulate::emulate(source, config, arch)  → before result
├── emulate::run_config_sweep(source, configs, arch) → best config (optional)
├── profile::analyze_profile()          → if NCU data provided
└── optimizer::run()
    ├── (LLM call to rewrite)
    ├── emulate::emulate(new_source, config, arch) → after result
    └── correctness::verify()           → nvcc compile + test
```

## 7. Data Flow: End-to-End Auto Optimizer

```
Input: /optimize matmul.cu
       GPU: RTX 4050 Laptop (sm_86, 20 SMs, 6 GB)
       Language: CUDA

──────────────────────────────────────────────────────────────────

Step 1 — Read + Profile
──────────────────────────────────────────────────────────────────

  vram::query_extended_gpu_info()
    → GPU context: RTX 4050 (sm_86, 20 SMs, 6 GB, driver 572.83)

  emulate::emulate(source, config, arch)
    → Before result:
        Cycles: 4,200,000
        IPC: 0.85
        Occupancy: 67%
        SM Util: 52%
        Bottleneck: Memory-bound
        Coalescing: 45%
        Bank conflicts: 12
        Register spills: 8

  BottleneckReport:
    Primary: Memory-bound (poor coalescing 45%, 12 bank conflicts)
    Secondary: Register pressure (8 spills)
    Details: ["Strided global memory access pattern detected",
              "ThreadIdx.x-based shared memory bank conflicts at stride 32"]

──────────────────────────────────────────────────────────────────

Step 2 — LLM Optimizes
──────────────────────────────────────────────────────────────────

  Prompt to LLM (Claude / GPT-4o / local):

    "Optimize this CUDA matmul kernel for RTX 4050 (sm_86, 20 SMs, 6GB).
    Current bottlenecks:
    - Memory-bound with 45% coalescing efficiency
    - 12 shared memory bank conflicts at stride 32
    - 8 register spills (~2% performance penalty)
    - Occupancy: 67% (limited by registers)

    Apply: tiling for coalescing, padding shared arrays to avoid bank
    conflicts, reduce register pressure by moving loop-invariant code.

    Return ONLY the complete rewritten kernel."

  LLM response:
    → Optimized kernel with:
      - Tiled access pattern (16×16 tiles)
      - Shared memory padding (+1 column) for bank conflict avoidance
      - Hoisted loop invariants outside inner loop
      - __launch_bounds__ to help register allocation

──────────────────────────────────────────────────────────────────

Step 3 — Verify Correctness
──────────────────────────────────────────────────────────────────

  Write optimized kernel to temp file
  Run: nvcc -o test_verify.exe matmul_opt.cu
    → Compilation: OK

  Run test with known input:
    ./test_verify.exe
    → Output matches reference (16×16 matrix multiply)

──────────────────────────────────────────────────────────────────

Step 4 — Estimate Speedup
──────────────────────────────────────────────────────────────────

  emulate::emulate(optimized_source, config, arch)
    → After result:
        Cycles: 1,890,000
        IPC: 1.89
        Occupancy: 83%
        SM Util: 78%
        Coalescing: 94%
        Bank conflicts: 0
        Register spills: 0

  SpeedupResult:
    Before: 4,200,000 cycles (2,478 μs)
    After:  1,890,000 cycles (1,115 μs)
    Speedup: 2.22×
    Improvement: 55%

──────────────────────────────────────────────────────────────────

Step 5 — Output
──────────────────────────────────────────────────────────────────

  ╔══════════════════════════════════════════════════════════════╗
  ║                    Optimization Results                      ║
  ╠══════════════════════════════════════════════════════════════╣
  ║  File: matmul.cu                                            ║
  ║  GPU:  RTX 4050 (sm_86)                                     ║
  ║                                                              ║
  ║  Before            After          Speedup                    ║
  ║  ────────────────  ────────────   ────────                   ║
  ║  4,200,000 cycles  1,890,000 cyc  2.22×                      ║
  ║  67% occupancy     83% occupancy   +24%                      ║
  ║  52% SM util       78% SM util     +50%                      ║
  ║  45% coalescing    94% coalescing  +109%                     ║
  ║  12 bank conflicts 0 bank confl.   -100%                     ║
  ║  8 register spills 0 reg spills   -100%                     ║
  ║                                                              ║
  ║  Correctness: ✓ PASSED                                       ║
  ║                                                              ║
  ║  Optimizations applied:                                      ║
  ║  • 16×16 tiling for coalesced global access                  ║
  ║  • Shared memory padding (stride 33 instead of 32)           ║
  ║  • Loop-invariant code motion                                ║
  ║  • __launch_bounds__(256, 4) for register optimization       ║
  ╚══════════════════════════════════════════════════════════════╝
```

## 8. Edge Cases & Limitations

### Identified & Documented

| Edge Case | Behavior | Why It's OK |
|-----------|----------|-------------|
| No GPU detected | Uses generic fallback context ("No GPU detected — suggestions target sm_86") | The emulator works without GPU; user is warned |
| Optimizer produces worse code | Speedup shows < 1.0×; user is prompted to rollback | The diff is saved; rollback restores original |
| LLM output has syntax errors | Compilation verification catches them; user sees error | When nvcc unavailable, marks as "compilation unchecked" |
| NCU profile not available | Falls back to emulator-only bottleneck detection | Emulator covers coalescing, occupancy, bank conflicts, register spills |
| Multiple kernels in one file | Optimizer asks which kernel to target | User selects via index or name |
| Optimizer times out (LLM slow) | Cancelable via Ctrl+C; shows partial results | Non-destructive; original file unchanged |
| Provider rate-limited | Falls back to local model if available | Auto-retry with backoff; user can switch providers |
| No test input for correctness | Does pattern-match: same function signature, same types | Conservative — will fail only if types mismatch |

### Known Limitations

1. **Emulator-based speedup is an estimate** — real hardware may differ by ±20%. The emulator is validated against known kernels but is not a replacement for hardware profiling.
2. **NCU parser is best-effort** — Nsight Compute output format varies by version. Unknown fields are skipped, not errored.
3. **Correctness verification covers only the test case** — a kernel that passes one test may fail on edge inputs. The optimizer notes this limitation.
4. **Single-kernel focus** — multi-kernel pipelines (e.g., flash attention with separate Q/K/V/V accum kernels) are not optimized end-to-end.
5. **No persistent optimization state** — closing and reopening loses optimization history. (`/optimize journal` is a future feature.)

## 9. Future Improvements

### Short Term (this sprint)
- `/optimize <file>` with emulator-based bottleneck detection and LLM rewrite
- Diff display with before/after metric table
- Compile verification (nvcc where available)
- Rollback command

### Medium Term
- NCU `.ncu-rep` file parser for precise bottleneck data
- Multi-kernel optimization within a file
- `/optimize batch <dir>` — batch optimize all kernels in a directory
- Optimization journal (persistent history across sessions)

### Long Term
- CI integration: `rightnow optimize --ci` in GitHub Actions
- Performance regression detection: "This kernel is 15% slower than last week's version"
- Learning from past optimizations: build a dataset of which optimizations worked for which kernel patterns
- Custom optimization strategies as skills — users write their own optimization recipes

## 10. Test Strategy

10+ tests covering the optimizer and GPU context:

| Category | Tests | What They Verify |
|----------|-------|------------------|
| **GPU Context** | `test_gpu_context_injection` | Context string contains SM count, compute cap, VRAM |
| | `test_gpu_context_serialization` | Context renders to valid structured text |
| **Bottleneck Report** | `test_bottleneck_from_emulate` | Emulator output → correct bottleneck classification |
| | `test_bottleneck_with_ncu` | NCU-parsed data → correct bottleneck (when NCU provided) |
| **Optimizer Pipeline** | `test_optimize_noop_kernel` | Optimizer handles a kernel with no improvement possible |
| | `test_optimize_add` | Vector add optimization produces valid CUDA |
| | `test_optimize_matmul` | Matmul with tiling suggestion compiles |
| **Correctness** | `test_compile_verification_valid` | Valid kernel passes nvcc check (when nvcc available) |
| | `test_compile_verification_syntax_error` | Invalid kernel fails gracefully |
| **Speedup** | `test_speedup_positive` | Optimized kernel shows ≥ 1.0× speedup |
| | `test_speedup_worse_rollback` | < 1.0× speedup triggers rollback suggestion |
| **Provider Display** | `test_provider_categorization` | Cloud/Local/Private categories display correctly |

---

*DOIC v1.0 — Generated from reverse engineering of existing modules (`crates/tools-and-exec/sentinel-gpu-profiler/src/`, `crates/interfaces/sentinel-cli/src/local.rs`, `crates/platform/sentinel-provider/src/`) plus new optimizer pipeline design.*

# Real GPU Access — System Design (DOIC)

## 1. Problem Statement

Today, architecture selection exists but is **emulation-only**:

- `/emulate <file> --arch=sm_90` picks a spec from the in-memory `ARCH_SPECS` DB
  (`sentinel-gpu-profiler/src/emulate.rs:41-52`) — pure simulation, no GPU touched.
- Real execution is limited to the host GPU (RTX 4050, sm_86) via `nvcc` in
  `bench::benchmark_kernel_real()`.
- `docs/GPU_SANDBOX.md:94-96` lists the gap explicitly: **Modal/RunPod provisioning
  is stubbed, not wired into the agent loop, and needs API keys beyond basic env vars.**

Developers cannot point at "H100 (sm_90)" or "Blackwell B200 (sm_102)" and get
**measured evidence** — only estimates. This document designs the missing layer:
a **cloud GPU orchestrator** that turns the same `GpuArch` selection into a real,
bounded, billable run on real hardware, backed by the existing safety/cost gates.

The emulator stays the zero-cost default. Real runs are opt-in and always metered.

## 2. Current State (What We Reuse)

| Asset | Location | Reuse |
|---|---|---|
| `GpuArch` enum + `ARCH_SPECS` (10 GPUs) | `sentinel-gpu-profiler/src/emulate.rs` | Single source of truth for arch identity + specs |
| `parse_emulate_arches()` / `parse_arch_arg()` | `local.rs:774`, `gpu_optimize.rs:87` | Reuse for `--arch=` CLI parsing |
| `bench::benchmark_kernel_real()` | `sentinel-gpu-profiler/src/bench.rs` | Host-side compile+time harness |
| `UsageThreshold`, `YoloBudget` | `sentinel-core/src/approval.rs:50-201` | Cost gates (soft/hard limits) |
| `PermissionRule` / permission engine | `sentinel-core/src/approval.rs:10-35` | Ask-before-provision approval |
| `OSJailSandbox` | `sentinel-exec/src/jail.rs` | Reused local-side for wrapping io |
| `ssh` transport | `sentinel-cli/src/local.rs:903 cmd_ssh` | v1 transport for BYO hardware |
| `BackendInfo`/provider auto-detect | `sentinel-provider/src/backend.rs` | Pattern for provider registry |
| `SecretSanitizer`, `BudgetGuard` | `sentinel-core` | Keys never logged, spend reserved/reconciled |

**Key insight:** the emulator already *taxes* an architecture identity. The
orchestrator only adds an **endpoint + a meter**. All identity is reused; nothing in
`emulate.rs` changes.

## 3. Goals & Non-Goals

### Goals

1. **Same selector, two backends.** `--arch=h100` means "emulate" via a default
   (zero cost) and "run real" via `/gpu run ... --arch=h100` (metered).
2. **Provider-agnostic.** At least RunPod (Pods + Serverless) and Modal (GPU App),
   plus **BYO-SSH**. New provider = one struct.
3. **Always metered.** Every real run estimates cost first, gates through
   `UsageThreshold`/`YoloBudget`, and reports actual cost after.
4. **Teardown-guaranteed.** A provisioned GPU is *never* left running (RAII guard +
   watchdog + `--timeout`).
5. **Deterministic fallback.** Any error → a flag maps to the emulator path, so the
   feature never blocks development.

### Non-Goals

- Native driver/toolchain management on remote (cloud images are pre-baked).
- Multi-node / MPC queueing in v1 (listed as roadmap).
- Replacing Nsight Compute parity; we collect `nvidia-smi`, `nvcc` compile flags, and
  wall-time of a host compile+run.
- Autoscaling beyond "one requested instance".

## 4. Architecture

```
            ┌───────────────────────────────────────────────────────────────┐
            │           sentinel-gpu-orchestrator (NEW crate)               │
            │                                                               │
 /gpu run   │   cmd_gpu_run → Orchestrator::run(Desired)                    │
 ──────────►│        │                                                      │
            │        ▼                                                      │
 emulator   │  ┌─────────────────────┐   ┌─────────────────────────────┐    │
 (zero-cost)│  │ Gate                │   │ Provider registry           │    │
 fallback:  │  │  estimate cost      │   │  runpod.rs | modal.rs |      │    │
 /emulate + │  │  UsageThreshold     │   │  ssh.rs (BYO)                │    │
 --estimate-│  │  YoloBudget          │   │  print→select provider+arch │    │
 only       │  └─────────┬───────────┘   └──────────────┬──────────────┘    │
            │            │                              │                    │
            │            ▼                              ▼                    │
            │  ┌───────────────────────────────────────────────────────┐    │
            │  │                 Provider trait (Send+Sync)             │    │
            │  │  provision → exec → fetch_artifact → teardown (RAII)  │    │
            │  └───────────────────────────────────────────┬───────────┘    │
            │                                              │                │
            │                                              ▼                │
            │  ┌───────────────────────────────────────────────────────┐    │
            │  │  Job lifecycle                                      │    │
            │  │  Queued → Provisioning → Running → Collecting →      │    │
            │  │  Succeeded | Failed | Cancelled                      │    │
            │  └──────────────────────────────────────┬───────────────┘    │
            │                                         │                     │
            │                                         ▼                     │
            │  ┌───────────────────────────────────────────────────────┐    │
            │  │  Evidence collector                                    │    │
            │  │  compile+run harness → wall-time, nvidia-smi dmon,     │    │
            │  │  exit code, stdout tail                                │    │
            │  └──────────────────────────────────────┬───────────────┘    │
            │                                         │                     │
            │  Actual cost → post-hoc reconcile        ▼                    │
            └──────────────────────────────────────┤ ledger (spend)       ┘
                                                  │
        ┌─────────────────────────────────────────┴──────────────────────────┐
        │  Execution backends                                                 │
        │  RunPod Pod / Serverless      Modal GPU app access                  │
        │  BYO SSH box (opt-in)          Local host (RTX 4050, sm_86)        │
        └─────────────────────────────────────────────────────────────────────┘
```

### Module Dependency Graph

```
sentinel-cli (local.rs)
  → sentinel-gpu-orchestrator            (NEW: crates/tools-and-exec/sentinel-gpu-orchestrator)
      ├── registry.rs                     # provider selection + Instance catalog
      ├── catalog.rs                     # arch → instance-type + $-per-hour table
      ├── orchestrator.rs                # run() state machine, RAII teardown
      ├── provider/mod.rs                # Provider trait (provision, exec, teardown, fetch)
      ├── provider/modal.rs              # Modal serverless endpoints
      ├── provider/runpod.rs             # RunPod Pods + Serverless REST
      ├── provider/ssh.rs                # BYO — reuses cmd_ssh transport
      ├── estimate.rs                    # cost estimator (price·hours + probe overhead)
      ├── evidence.rs                    # remote run harness + result struct
      └── lib.rs
  → sentinel-gpu-profiler                 (UNCHANGED — arch/enum/specs, bench harness)
  → sentinel-core (approval.rs)          (already-existed gates)
  → sentinel-exec (jail)                 (already-existed; wrapping local IO)
```

**No cycles:** orchestrator depends on `sentinel-gpu-profiler` (metadata + harness),
`sentinel-core` (gates), `sentinel-exec`. It never touches `langs.rs` internals.

## 5. Core Data Types

### Target (identical identity to GpuArch)

```rust
pub struct GpuTarget {
    pub arch: GpuArch,            // GpuArch::Hopper90, e.g.
    pub provider: ProviderKind,   // RunPod | Modal | Ssh | emulator
    pub instance: Option<String>, // default from catalog if None
}

pub enum ProviderKind { Emulator, Ssh, RunPod, Modal }
```

### Instance Catalog (pure data, mirrors ARCH_SPECS)

```rust
pub struct Instance {
    pub provider: ProviderKind,
    pub arch: GpuArch,
    pub sku: &'static str,             // "SECURE-H100-SXM-80GB-1", "POD-4090"
    pub vram_gb: u32,
    pub price_hr: f64,                 // USD / instance-hour
    pub spot_eligible: bool,
}
```

Tier-1 catalog (prices are representative, to be validated during PI):

| Target | GpuArch | RunPod | Modal | Notes |
|---|---|---|---|---|
| H100 SXM/PCIe | Hopper90 | 80GB H100 $2.5–4.0/hr | H100 x100 | Primary datacenter SKU |
| H200 SXM | Hopper92 | 141GB H200 | — | Large-memory reasoning |
| B200 / 5090 | Blackwell100 | B200 $6–7/hr | — | Flagship next-gen |
| RTX 4090 | Ada89 | 24GB $0.3–0.5/hr | 4090x1 | Cheap launch config runs |
| A100 SXM | Ampere80 | 80GB $1.1–3.5/hr | A100-x1 | Ampere legacy compat |

Instance→arch mapping is separated from `ARCH_SPECS`: emulator specs never round-trip
through cloud prices.

### Desired / Job

```rust
pub struct Desired {
    pub target: GpuTarget,
    pub source_path: String,
    pub launch: Option<LaunchConfig>,
    pub max_hrs: f64,          // hard run cap → teardown
    pub budget_usd: f64,       // per-run cap
    pub benchmark: bool,       // auto-compile+time (host harness)
    pub mode: SandboxMode,     // Pod vs Serverless
}

pub struct Job {
    pub id: String,            // uuid
    pub provider: ProviderKind,
    pub instance: String,
    pub status: JobStatus,     // Queued→Provisioning→Running→Collecting→Succeeded/Failed
    pub provisioned_hrs: f64,
    pub start_at, end_at: Option<DateTime<Utc>>,
    pub estimated_usd: Option<f64>,
    pub actual_usd: f64,
    pub evidence: Option<Evidence>,
}
```

### Evidence

```rust
pub struct Evidence {
    pub host_gpu: String,
    pub driver: String,
    pub nvcc: Option<String>,
    pub wall_time_secs: f64,
    pub gpu_util_avg: f64,          // nvidia-smi dmon
    pub mem_util_avg: f64,
    pub exit_code: i32,
    pub stdout_tail: Vec<String>,
}
```

## 6. Cost Control (the meter)

Reuses `sentinel-core` gates unchanged — no new accounting model.

```
1. estimate = price_hr × (probe_hrs + max(run_hrs, 0.25h))   // probe floor
2. usage_threshold.check(session_spend, estimate) → UsageCheckResult
      Allowed            → continue
      RequiresApproval   → surface $ + user "y/N"  (or yolo budget)
      Blocked            → stop, tell user, suggest emulator fallback
3. budget reserve  (BudgetGate::reserve, from existing sentinel-core)
4. on job end: actual = price_hr × wall_hrs; reconcile
5. adversarial: never print keys; SecretSanitizer on any output
```

Budget sources:
- `SENTINEL_GPU_BUDGET_USD` (session budget, default 0)
- `SENTINEL_GPU_MAX_HRS` (6 default)
- Per-provider keys in env only: `RUNPOD_API_KEY`,
  `MODAL_TOKEN_ID` + `MODAL_TOKEN_SECRET`, ssh via `SENTINEL_SSH_*`.

## 7. Provider Trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn price_hr(&self, inst: &Instance) -> f64;
    fn name(&self) -> &'static str;

    async fn provision(&mut self, target: &GpuTarget, desired: &Desired) -> Result<Provisioned>;
    async fn exec(&self, prov: &Provisioned, remote_cmd: &str) -> Result<ExecOutput>;
    async fn fetch_artifact(&self, prov: &Provisioned, remote_path: &str) -> Result<Vec<u8>>;
    async fn teardown(&self, prov: &Provisioned) -> Result<()>;   // must be idempotent
}
```

- **Ssh**: wraps the existing `cmd_ssh` shell (`local.rs:903 cmd_ssh`). Single
  machine, no provision call; teardown is a no-op. Costs nothing beyond what the
  user's own box charges, and proves the trait before any cloud spend happens.
- **RunPod**: REST `/pods`, GraphQL for serverless; SSH key upload; pod auto-create
  from a pre-baked CUDA image (e.g. `runpod/pytorch:2.6.0-cuda12.4.1-cudnn9.2-devel`);
  teardown deletes the pod.
- **Modal**: `modal run` (GPU App) or HTTP backend; source files are mounted into
  the container via a generated backend; teardown stops the function and drops the
  cold container. Providers hold credentials, never echo them.

## 8. CLI & Tool Surface

`/gpu` gains real slots (aligned with AGENTS.md command style):

```
/gpu cloud                        # list providers configured + tier-1 price table
/gpu run <file> --arch h100       # dispatch to cloud: RunPod default if key present
                                  #   --provider runpod|modal|ssh
                                  #   --budget 3.0 --max-hrs 1 --bench
/gpu run <file> --arch b200 --estimate-only   # print price + emulator hint, no spend
/gpu status <job-id>              # job, streamed tail, live cost
/gpu stop <job-id>                # teardown now (safe, idempotent)
/gpu cost [--session]             # actual spend ledger
/gpu backends                     # existing /backends style: shows cloud readiness
```

Tool layer (agent-callable, mirrors `GpuOptimizeKernelTool` registration
`gpu_optimize.rs:40`):
```
gpu_real_bench { file_path, arch, provider?, budget? } → Evidence + cost
```

`--estimate-only` is the safe gate, and one rule is absolute: **never auto-provision**.
It prints price + emulator hint before any spend, so a user can batch-emulate, then
spend only on the top candidates.

## 8.1 End-to-End Flow

```
Input: /gpu run matmul.cu --arch h100 --provider runpod --bench

1. parse_arch_arg("h100") → GpuArch::Hopper90        (reuses gpu_optimize.rs:87)
2. registry: RunPod + Hopper90 → SKU "SECURE H100", $3.20/hr
3. emulate::emulate(source, cfg, Hopper90) → est 2.1M cycles   [free preflight]
4. estimate = 3.20 × (0.07 probe + 0.5 run) ≈ $1.82
5. gate: usage_threshold.check(session_spend=0, estimate=1.82)
     → within soft limit → continue. If RequiresApproval, request "y/N".
     If Blocked → abort, suggest /emulate.
6. provision pod (15–60 s) → ssh key
7. upload matmul.cu → run (fixed recipe):
     nvcc -arch=sm_90 matmul.cu -o matmul
     nvidia-smi dmon -s u -d 1 -c 30 & ./matmul; wait; kill %1
8. teardown (RAII guard; always runs even if step 7 fails)
9. evidence → wall_time, avg util, exit code, stdout tail → report
10. reconcile ledger += actual (3.20 × 0.41 ≈ $1.31)
    Display: emulator-predicted vs measured side by side.
```

### Failure matrix (builtin)

| Failure | Behavior |
|---|---|
| Budget blocked | Never provision; suggest `/emulate` instead |
| Provision timeout (> 6 min) | Cancel job, refund (nothing provisioned) |
| Remote compile error | Collect stderr, teardown, mark Failed (never partial) |
| User "no" at approval | Abort before any spend — zero cost leak |
| Provider 5xx | Retry ×2 backoff, then fallback → `ssh` if possible, else emulator |
| Ctrl+C mid-run | Teardown held in the same guard |

## 9. Security

- Keys only from env, never persisted to thread/files; `SecretSanitizer` runs on ALL
  captured output before it touches the store or terminal.
- Teardown is the guard after the job; no job outlives `SENTINEL_GPU_MAX_HRS`.
- Permissions: a new `gpu_provision` permission pattern (Ask by default,
  Allow only if user sets config — reuse PermissionRule engine).
- The orchestrator itself runs under the existing `OSJailSandbox` for the *local*
  temp dir, mirroring the host harness policy.
- Every remote command is a fixed, allowlisted recipe — free text never passes
  through (unlike `cmd_ssh` raw).

## 10. Testing Strategy

Offline, no keys required:

| Test | What it verifies |
|---|---|
| `test_catalog_coverage` | Tier-1 arch ↔ instance mapping present; prices > 0 |
| `test_estimate_blocks_over_budget` | `UsageCheckResult::Blocked` path; no provision call (`MockProvider`) |
| `test_teardown_on_compile_fail` | Mock provider fails → teardown guard still runs; job ends in `Failed` |
| `test_teardown_idempotent` | Double teardown is a no-op |
| `test_parsing_reuses_gpu_arch` | `--arch h100`/`b200`/`4090` → correct GpuArch |
| `test_secret_never_leaked` | Provider error containing key → output is redacted |
| `test_ssh_provider_integration` | If a byo host is configured, run remote bench end-to-end |
| `test_modal_client_offline` | HTTP layer mocked, Tensor → provisioning request shape |

Result proof points run `cargo test -p sentinel-gpu-profiler` (47) +
`cargo test -p sentinel-gpu-orchestrator` (new) + `cargo check --workspace`.

## 11. Decisions

| # | Decision | Alternatives | Rationale |
|---|---|---|---|
| D1 | Same `GpuArch` for emulation+cloud | New cloud-only enum | One identity; `--arch` flags already exist |
| D2 | RunPod as first cloud, Modal second | Reverse | Pods SSH-first matches our `cmd_ssh`; fast v1 |
| D3 | BYO SSH provider in v1 | Only-run Pods | Zero added infra proves the trait before we pay for a real provider |
| D4 | Cost gating via existing `UsageThreshold`/`YoloBudget` | New accounting crate | Reuse, one truth; tests already exist |
| D5 | `--estimate-only` is the only way to preview; never auto-provision | Auto-provision on first ask | Keeps dollar security in the human |
| D6 | Fixed allowlist remote recipe | Free-form remote shell | We already have that in `cmd_ssh`; this layer must constrain |

## 12. Scope of Change

- **New crate** `crates/tools-and-exec/sentinel-gpu-orchestrator` (~1.2k lines).
- **CLI additions** in `sentinel-cli/src/local.rs` (`cmd_gpu` user commands) +
  `gpu_optimize.rs` (adds `gpu_real_bench` tool).
- **Config additions** in `sentinel-config` (`[gpu_budget]`, provider keys via `.env`).
- **Zero edits** to `emulate.rs`, `langs.rs`, `bench.rs`, `approval.rs`/accounting.
- Docs updates: `AGENTS.md`, `GPU_SANDBOX.md` (remove the "stubbed" line once the
  first real run lands).
- Optional `src/provider/modal.rs` behind a feature flag `modal` (adds `modal-cli`
  dependency), keeping the core build lean.

---
*Design v1 — companion to `gpu-cloud-access-plan.md`.*
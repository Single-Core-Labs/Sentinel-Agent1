# Real GPU Access — System Plan

*Companion to `gpu-cloud-access-design.md`.* Turns the emulator's `GpuArch`
selection into real, metered, teardown-guaranteed runs on cloud/remote GPU
hardware, gated by the existing approval + budget system.

## 1. Targets (Tier-1)

| Priority | GpuArch | Real-world GPU | Why |
|---|---|---|---|
| P1 | `Hopper90` | H100 SXM/PCIe (80 GB) | Datacenter convergence SKU; emulator's reference arch |
| P1 | `Ada89` | RTX 4090 (24 GB) | Cheap launch-pad: validates a run for < $1 |
| P2 | `Blackwell100` | B200 / RTX 5090 | Flagship; validates sm_100 targets |
| P2 | `Ampere80` | A100 (80 GB) | Legacy ISV requirement |

Every plan gate fails softly: an error or unpaid budget falls back to `/emulate`.

## 2. Phase Plan (Plan-Implementation, gate-per-phase)

### Phase 0 — Catalog & Local Translation (no spend, ~2-3 PL)

Build facts only in Phase 0: `catalog.rs` + provider data + a passive arch→SKU map.
- [ ] `InstanceCatalog` constant: tier-1 arch → provider instances for RunPod/Modal with
      current prices (`price_hr`, `vram_gb`, `spot_eligible`).
- [ ] `GpuTarget`, `ProviderKind`, `Desired`, `Job` data types.
- [ ] `/gpu cloud` lists providers, keys found, price table; `--estimate-only`
      prints cost for an arch + returns the emulator hint (no spend).
- [ ] Unit tests: `test_catalog_coverage`, `test_parsing_reuses_gpu_arch`.

**Exit gate:** `cargo test -p sentinel-gpu-profiler` + `cargo check --workspace`
green; `/gpu price --arch h100` prints `≈$3.20/hr` without an env key.

---

### Phase 1 — BYO SSH provider (opt-in transport, spend = user's existing box)

Reuses `cmd_ssh` (`local.rs:903`); one real transport validates the trait:
- [ ] `Provider` trait + `ssh.rs` provider (no-op teardown).
- [ ] Orchestrator lifecycle state machine (Queued→…→Failed) + RAII teardown guard.
- [ ] `Evidence` collection harness (wall-time, dmon sample, exit code, stdout tail).
- [ ] `cmd_gpu run <file> --arch <x> --provider ssh` end-to-end on a test host.
- [ ] Cost meter wired to `usage_threshold.check()` — even for SSH (probe floor).
- [ ] Budget errors never leak; `SecretRedactor` applied to all captured output.

**Exit gate:** one real BYO host completes a `/gpu run ... --bench` with evidence
table and ledger entry; Ctrl+C mid-run tears down cleanly; `test_teardown_idempotent`
passes.

---

### Phase 2 — RunPod (first paid cloud, pods + serverless)

- `Provider::runpod` REST client (`/pods`, GraphQL); key upload; pod auto-create on
  a pre-baked CUDA image; deterministic teardown (delete pod) with retry.
- `/gpu run ... --provider runpod --arch h100` and `--estimate-only` both live.
- Env keys: `RUNPOD_API_KEY`; `.env` sample in docs; `SecretSanitizer` on all captures.
- Add a `gpu_provision` permission (Ask default) and enforce via `PermissionRule`.
- Cost dashboard: `/gpu cost [--session]` ledger, reconcile at job end.
**Exit gate:** one real run on RTX 4090 (≤ $0.5) completes with evidence + `$0.42`-ish
ledger line; teardown verified by absent pod after run.

---

### Phase 3 — Modal (GPU app/serverless second cloud)

- `Provider::modal` (behind the optional `modal` feature flag): generate an
  entrypoint, mount source, request a GPU class; teardown = stop function/cold container.
- Reuse every orchestration + metering component from Phases 0-2.
- Warm-pool reuse (same SKU within X seconds) is a roadmap option, not a requirement.
**Exit gate:** modal run on `H100-x1` completes, teardown no-residual, budget report to the cent.

---

### Phase 4 — Agent & tool integration

- `gpu_real_bench` tool (mirrors `register_gpu_tools` in `gpu_optimize.rs:417`),
  agent-callable: `{ file_path, arch, provider?, budget? } → Evidence`.
- A model-driven fallback: an agent asked "is this faster than RTX 4090?" can run
  `/gpu run --arch ada89 --estimate-only` then optionally the full run — bounded
  by the budget field.
- `AGENTS.md` + `GPU_SANDBOX.md` updates (replace the "stubbed" Known-Gap line).
**Exit gate:** E2E demo: `/emulate` → pick candidate → `/gpu run ... --arch b200
--budget 3` → evidence table vs emulator prediction.

---

### Phase 5 — Hardening / scale (backlog)

- Multi-arch matrix runs (`--arches=h100,b200,4090`) as a batch queue.
- Spot vs on-demand; spot bidding auto-fallback; auto-suggest-cheapest arch that
  satisfies the job.
- CI integration (`rightnow gpu --ci`) with a per-PR GPU budget; memoized evidence
  per (kernel-hash, arch).

## 3. Milestones & Schedule (points)

| Milestone | Phase | Deliverable |
|---|---|---|
| M1 | P0 | `/gpu cloud`, `--estimate-only`, catalog + budget gate green |
| M2 | P1 | BYO SSH runs with evidence + teardown (spend on existing box) |
| M3 | P2 | RunPod run on RTX 4090 end-to-end ≤ $0.5 |
| M4 | P3 | Modal run end-to-end; ledger to the cent |
| M5 | P4 | Agent tool + fallback wiring |

## 4. Dependencies & Risks

| Risk | Mitigation |
|---|---|
| RunPod image not matching local `nvcc` | Pin CUDA version in the pod image (NVCC 13.3→ 12.8 pod) with a documented env |
| Cost runaway | Hard `SENTINEL_GPU_MAX_HRS` + `--budget` cap + human approval via the existing gate |
| Teardown failure | Idempotent teardown + watchdog + notify user with a "stop" one-liner |
| Provider 5xx / rate-limit | Retry ×2 + fallback to SSH host if available, else emulator |
| Key leakage | env-only + `SecretSanitizer` + no persistence beyond the process |

## 5. Definition of Done (per phase)

- All new tests pass (sentinel-gpu-profiler 47 + orchestrator suite) and
  `cargo check --workspace` clean.
- `cargo test -p sentinel-gpu-profiler` stays green (no regressions in emulator).
- No key ever printed; `SENTINEL_GPU_BUDGET_USD` respected.
- `--estimate-only` never spends; the emulator remains the zero-cost default.

---
*Plan v1 — phases imperative, stay in phase-gates.*
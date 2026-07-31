# Policy Moat — scriptable, fail-closed gates between the model and the machine

Most coding agents offer a single binary choice: approve everything (yolo) or
review every tool call (all-or-nothing). Both scale badly — yolo is unsafe,
full-review is a click-tax that users bypass. Sentinel's answer is a
**policy hook plane**: an external script that receives every tool call
(and model request / session lifecycle event) and returns a verdict:
`allow`, `veto <reason>`, or (via the CLI `--hook-command` contract) `ask`.

This document is the threat model, the hook contract, the shipped guards, and
the enterprise pitch.

## Threat model

| Threat | Without gates | With guards |
|--------|---------------|-------------|
| Agent writes outside the workspace (`../../etc/hosts`) | File lands anywhere the OS allows | `workspace-guard` vetoes |
| Agent exfiltrates/learns from arbitrary web domains | Any URL fetchable | `web-guard` deny-by-default allowlist |
| Destructive command (`rm -rf /`, `dd if=… of=/dev/sda`, `format c:`) | Sandbox may or may not catch it | `command-guard` vetoes before execution |
| Prompt-injection via fetched content | Injected instructions execute | `web-guard` limits reach; sandbox limits blast radius |
| Compromised/token-hungry model | unbounded spend | budget via `BeforeModelRequest` hook + provider budgets |

Layered defense: **policy hooks (fast, auditable) → approval gate (human) →
OS sandbox (last line)**. The hook plane is not a replacement for the sandbox;
it is the gate in front of it.

## Hook contract (recap)

- Plugin dir with `sentinel-plugin.toml` → hooks point at scripts
  (`before_tool_call`, `after_tool_call`, `before_model_request`,
  `after_model_response`, `session_created`, `session_ended`).
- Invocation: `<script> <event_type> <tool_name>`, full event JSON on stdin.
- `before_tool_call` verdict is the **first stdout line**: `veto <reason>`
  (or `deny <reason>`) blocks the call; anything else continues.
  The model sees `Vetoed by plugin policy: <reason>` as the tool result.
- Fail-closed guidance: veto verdicts are honored verbatim; scripts that
  cannot produce a verdict (crash/timeout) currently **fail open** (Continue)
  — so shipped guard scripts are written to always print a verdict.
- `--hook-command <cmd>` on `sentinel ai` is the same contract without a
  plugin directory (stdout: `allow` | `deny <reason>` | `ask`).
- Windows: hooks run under `cmd /C`; Unix under `sh -c`. Scripts must be
  BOM-free and CRLF-safe (see session log gotchas).

## Shipped guards (`examples/plugins/`)

| Guard | Vetoes | Default posture |
|-------|--------|-----------------|
| `workspace-guard` | `write`/`edit`/`apply_patch` where `file_path` escapes cwd (`..`, absolute-outside) | allow inside workspace |
| `web-guard` | `web_fetch`/`web_search` against non-allowlisted domains | deny everything |
| `command-guard` | `run_shell_command` matching destructive patterns (`rm -rf /`, `format`, `del /s`, `dd if=`, `mkfs`, `diskpart`, fork-bomb) | deny matches |

Install: `sentinel plugin install examples/plugins/workspace-guard` (one
directory each). `sentinel plugin list` shows them; loaded on the next
`sentinel ai` run. On Unix, one-time `ln -s guard.sh guard` per plugin
(git cannot store a symlink that also checks out correctly on Windows).

### Live verification (Windows, this machine)

```
sentinel plugin list
#   command-guard v0.1.0 — Vetoes run_shell_command matching destructive patterns

sentinel ai --prompt "delete everything with rm -rf /" --yolo
# → tool result: Vetoed by plugin policy: veto destructive command: rm -rf /
```

## The enterprise pitch

- **Policy as code, not as dialog.** Approvals as a stream of scriptable
  verdicts means policy can live in the repo, be reviewed in PRs, be tested,
  and be shared across a fleet. All-or-nothing UIs can't.
- **Fail-closed by design.** The default shipped postures are deny-heavy
  (web, destructive commands). Vendors integrate Sentinel where the blast
  radius of a wrong model call is unacceptable.
- **Auditable.** Every veto is a first-class event; the same event bus that
  vetoes can log, alert, and feed an enterprise SIEM.
- **No vendor lock.** Hooks are plain executables — PowerShell, sh, Python,
  or a Rust binary — so policy can reuse existing security tooling.

## Future (roadmap §4)

- Budget enforcement in `BeforeModelRequest` (spend ceiling per session).
- `ask` verdict plumbing for the TS TUI.
- Sandbox-escape policy (verdicts conditioned on sandbox availability).

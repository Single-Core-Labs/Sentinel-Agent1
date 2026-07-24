#!/usr/bin/env python
"""
Side-by-side e2e comparison harness.

Runs the same task prompts against both the Python agent (agent.main) and the
Rust CLI (sentinel exec), then compares their outputs structurally.

Usage:
  uv run python tests/e2e/compare_harness.py [--model MODEL] [--skip TASKS]
  uv run python tests/e2e/compare_harness.py --model gpt-4o-mini --skip code_gen

Environment:
  SENTINEL_E2E_SKIP  Comma-separated task names to skip
  SENTINEL_E2E_MODEL Model name (or use --model)
  SENTINEL_E2E_JSON  Path to write JSON report (default: e2e_results.json)

Exit code: 0 if all tasks succeeded in both agents, 1 otherwise.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Any

# ── Repository layout ─────────────────────────────────────────────────────
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SENTINEL_BIN = REPO_ROOT / "target" / "debug" / "sentinel.exe"
AGENT_DIR = REPO_ROOT / "agent"
PYTHON_AGENT_CMD = ["uv", "run", "python", "-m", "agent.main"]
RUST_AGENT_CMD = [str(SENTINEL_BIN), "exec"]

# ANSI escape removal
_ANSI_RE = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\].*?\x07")


# ── Task definitions ──────────────────────────────────────────────────────
@dataclass
class TaskSpec:
    name: str
    prompt: str
    description: str
    expected_keywords: list[str] = field(default_factory=list)


TASKS: list[TaskSpec] = [
    TaskSpec(
        name="simple_greeting",
        prompt="Say hello and introduce yourself briefly in one sentence.",
        description="Basic greeting - tests that both agents produce output.",
        expected_keywords=["hello", "sentinel", "agent"],
    ),
    TaskSpec(
        name="read_cargo_toml",
        prompt="Read the contents of Cargo.toml in the current directory and list the workspace members.",
        description="File I/O and structured reading.",
        expected_keywords=["workspace", "members", "sentinel"],
    ),
    TaskSpec(
        name="code_generation",
        prompt="Write a Python function that computes fibonacci numbers using recursion.",
        description="Code generation capability.",
        expected_keywords=["def", "fibonacci", "return"],
    ),
]


# ── Result types ──────────────────────────────────────────────────────────
@dataclass
class AgentRunResult:
    success: bool
    stdout: str
    stderr: str
    duration_s: float
    exit_code: int


@dataclass
class TaskComparison:
    task_name: str
    prompt: str
    description: str
    rust: AgentRunResult
    python: AgentRunResult
    structural_match: bool
    error: str | None = None


# ── Agent runners ─────────────────────────────────────────────────────────
def _strip_ansi(text: str) -> str:
    return _ANSI_RE.sub("", text)


def _find_sentinel_binary() -> Path | None:
    """Locate the sentinel binary. CARGO_BIN_EXE_sentinel wins, then debug build."""
    env_bin = os.environ.get("CARGO_BIN_EXE_sentinel")
    if env_bin:
        p = Path(env_bin)
        if p.is_file():
            return p
    if SENTINEL_BIN.is_file():
        return SENTINEL_BIN
    return None


def run_python_agent(prompt: str, model: str | None, timeout_s: int = 300) -> AgentRunResult:
    cmd = PYTHON_AGENT_CMD + ["--no-stream"]
    if model:
        cmd += ["--model", model]
    cmd.append(prompt)
    start = time.monotonic()
    try:
        r = subprocess.run(
            cmd,
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True,
            timeout=timeout_s,
        )
        duration = time.monotonic() - start
        return AgentRunResult(
            success=r.returncode == 0,
            stdout=_strip_ansi(r.stdout),
            stderr=_strip_ansi(r.stderr),
            duration_s=duration,
            exit_code=r.returncode,
        )
    except subprocess.TimeoutExpired:
        duration = time.monotonic() - start
        return AgentRunResult(
            success=False,
            stdout="",
            stderr=f"TIMEOUT after {timeout_s}s",
            duration_s=duration,
            exit_code=-1,
        )
    except FileNotFoundError as e:
        return AgentRunResult(
            success=False,
            stdout="",
            stderr=f"Launch error: {e}",
            duration_s=0.0,
            exit_code=-1,
        )


def run_rust_agent(prompt: str, model: str | None = None, timeout_s: int = 300) -> AgentRunResult:
    bin_path = _find_sentinel_binary()
    if bin_path is None:
        return AgentRunResult(
            success=False,
            stdout="",
            stderr="sentinel binary not found. Run `cargo build --bin sentinel` first.",
            duration_s=0.0,
            exit_code=-1,
        )
    model_id = model or os.environ.get("SENTINEL_E2E_MODEL", "openrouter/auto")
    cmd = [str(bin_path), "exec", model_id, prompt]
    start = time.monotonic()
    try:
        r = subprocess.run(
            cmd,
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True,
            timeout=timeout_s,
        )
        duration = time.monotonic() - start
        return AgentRunResult(
            success=r.returncode == 0,
            stdout=_strip_ansi(r.stdout),
            stderr=_strip_ansi(r.stderr),
            duration_s=duration,
            exit_code=r.returncode,
        )
    except subprocess.TimeoutExpired:
        duration = time.monotonic() - start
        return AgentRunResult(
            success=False,
            stdout="",
            stderr=f"TIMEOUT after {timeout_s}s",
            duration_s=duration,
            exit_code=-1,
        )
    except FileNotFoundError as e:
        return AgentRunResult(
            success=False,
            stdout="",
            stderr=f"Launch error: {e}",
            duration_s=duration if 'duration' in dir() else 0.0,
            exit_code=-1,
        )


# ── Comparison logic ──────────────────────────────────────────────────────
def _extract_keywords(text: str, min_len: int = 4) -> set[str]:
    words = set()
    for token in text.split():
        token = token.strip(".,!?;:()[]{}'\"")
        if len(token) >= min_len and not token.startswith(("http", "https", "//")):
            words.add(token.lower())
    return words


def _normalize_for_comparison(text: str) -> str:
    lines = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        if line.startswith(("#", "//", "[stdout]", "[stderr]", "---")):
            continue
        if "history_size=" in line:
            continue
        if "Model:" in line and "Tool runtime:" in line:
            continue
        lines.append(line)
    return "\n".join(lines)


def structural_match(rust_text: str, python_text: str) -> bool:
    rust_keys = _extract_keywords(rust_text)
    python_keys = _extract_keywords(python_text)
    if not rust_keys or not python_keys:
        return False
    overlap = rust_keys & python_keys
    min_len = min(len(rust_keys), len(python_keys))
    return len(overlap) / min_len >= 0.25


# ── Reporting ─────────────────────────────────────────────────────────────
def print_separator(char: str = "=", width: int = 66):
    print(char * width)


def format_duration(s: float) -> str:
    if s < 60:
        return f"{s:.1f}s"
    return f"{s / 60:.1f}m {s % 60:.0f}s"


def report_comparison(tc: TaskComparison):
    status = (
        "OK"
        if tc.rust.success and tc.python.success and tc.structural_match
        else "FAIL"
    )
    print(f"\n  {status}  {tc.task_name}")
    print(f"      Prompt: {tc.prompt[:80]}")
    print(
        f"      Rust:   {'OK' if tc.rust.success else 'FAIL'} "
        f"({format_duration(tc.rust.duration_s)}, exit={tc.rust.exit_code})"
    )
    print(
        f"      Python: {'OK' if tc.python.success else 'FAIL'} "
        f"({format_duration(tc.python.duration_s)}, exit={tc.python.exit_code})"
    )
    print(f"      Structural match: {tc.structural_match}")
    if tc.error:
        print(f"      Error: {tc.error}")
    if not tc.rust.success and tc.rust.stderr:
        preview = tc.rust.stderr[:200]
        print(f"      Rust stderr: {preview}")
    if not tc.python.success and tc.python.stderr:
        preview = tc.python.stderr[:200]
        print(f"      Python stderr: {preview}")


def generate_json_report(results: list[TaskComparison]) -> dict[str, Any]:
    return {
        "summary": {
            "total": len(results),
            "rust_ok": sum(1 for r in results if r.rust.success),
            "python_ok": sum(1 for r in results if r.python.success),
            "structural_matches": sum(1 for r in results if r.structural_match),
            "all_ok": all(r.rust.success and r.python.success for r in results),
        },
        "tasks": [
            {
                "name": tc.task_name,
                "prompt": tc.prompt,
                "rust": asdict(tc.rust),
                "python": asdict(tc.python),
                "structural_match": tc.structural_match,
            }
            for tc in results
        ],
    }


# ── Main ──────────────────────────────────────────────────────────────────
def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Side-by-side e2e comparison harness")
    p.add_argument("--model", default=None, help="Model override for Python agent")
    p.add_argument(
        "--skip",
        default=os.environ.get("SENTINEL_E2E_SKIP", ""),
        help="Comma-separated task names to skip",
    )
    p.add_argument(
        "--json",
        default=os.environ.get("SENTINEL_E2E_JSON", "e2e_results.json"),
        help="Path to write JSON report",
    )
    p.add_argument(
        "--timeout", type=int, default=300, help="Per-task timeout in seconds"
    )
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    skip_set = {s.strip() for s in args.skip.split(",") if s.strip()}
    model = args.model or os.environ.get("SENTINEL_E2E_MODEL")

    print()
    print_separator("=")
    print("  Sentinel E2E Comparison Harness")
    print(f"  Python agent:  {' '.join(PYTHON_AGENT_CMD)}")
    bin_path = _find_sentinel_binary()
    if bin_path:
        print(f"  Rust agent:    {bin_path}")
    else:
        print("  Rust agent:    NOT BUILT - run `cargo build --bin sentinel`")
    print(f"  Model:         {model or '(default from config)'}")
    print(f"  Tasks:         {len(TASKS)} defined, skipping {skip_set}")
    print_separator("=")

    results: list[TaskComparison] = []
    for task in TASKS:
        if task.name in skip_set:
            print(f"\n  -  {task.name}  (skipped)")
            continue

        print(f"\n  >> {task.name}")
        print(f"     {task.description}")
        print_separator()

        rust_result = run_rust_agent(task.prompt, model=model, timeout_s=args.timeout)
        python_result = run_python_agent(task.prompt, model, timeout_s=args.timeout)

        match = structural_match(rust_result.stdout, python_result.stdout)
        tc = TaskComparison(
            task_name=task.name,
            prompt=task.prompt,
            description=task.description,
            rust=rust_result,
            python=python_result,
            structural_match=match,
        )
        results.append(tc)
        report_comparison(tc)
        print_separator()

    # Summary
    print()
    print_separator("=")
    print("  SUMMARY")
    print_separator("=")
    rust_ok = sum(1 for r in results if r.rust.success)
    python_ok = sum(1 for r in results if r.python.success)
    matches = sum(1 for r in results if r.structural_match)
    total = len(results)
    print(f"  Rust pass:    {rust_ok}/{total}")
    print(f"  Python pass:  {python_ok}/{total}")
    print(f"  Struct match: {matches}/{total}")
    print_separator("=")

    # Speed comparison (only if both succeeded)
    paired = [(r.rust, r.python) for r in results if r.rust.success and r.python.success]
    if paired:
        rust_total = sum(r.duration_s for r, _ in paired)
        python_total = sum(p.duration_s for _, p in paired)
        if rust_total > 0:
            ratio = python_total / rust_total
            print(f"  Python/Rust time ratio: {ratio:.2f}x")
            if ratio > 1.0:
                print(f"  Rust was {(ratio - 1) * 100:.0f}% faster overall")
            else:
                print(f"  Python was {(1 / ratio - 1) * 100:.0f}% faster overall")
        print_separator("=")

    # Write JSON report
    report_path = Path(args.json)
    report_data = generate_json_report(results)
    report_path.write_text(json.dumps(report_data, indent=2), encoding="utf-8")
    print(f"\n  JSON report written to {report_path}")

    all_ok = all(r.rust.success and r.python.success for r in results)
    print(f"\n  {'ALL PASSED' if all_ok else 'SOME FAILED'}")
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())

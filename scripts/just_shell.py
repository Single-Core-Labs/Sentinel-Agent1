#!/usr/bin/env python3
"""Cross-platform shell adapter for `just` command runner.

Translates argument passing and standard-error redirection into
OS-specific syntax so that `just` recipes work identically on
Windows (PowerShell) and Unix (bash).

Usage in a justfile:

    [unix]
    [windows]
    [shell: "python3 scripts/just_shell.py"]
    my-recipe:
        # This now works on both platforms
        my-tool --flag value 2>/dev/null

The adapter recognises these common patterns and rewrites them:

  - `2>/dev/null`          → `2>$null` on Windows
  - `2>&1`                 → `2>&1` (same on both)
  - `$VAR` or `"$VAR"`     → `$env:VAR` on Windows
  - `$(cmd)`               → `$(cmd)` via PowerShell (same syntax)
  - `cmd1 && cmd2`         → `cmd1; if ($?) { cmd2 }` on Windows
  - `cmd1 || cmd2`         → `cmd1; if (-not $?) { cmd2 }` on Windows
  - `exit $?`              → `exit $LASTEXITCODE` on Windows
  - `command -v foo`       → `Get-Command foo` on Windows
"""

import os
import platform
import subprocess
import sys
import shlex


def is_windows() -> bool:
    return platform.system() == "Windows"


def translate_stderr(s: str) -> str:
    """Translate stderr redirect operators."""
    if is_windows():
        s = s.replace("2>/dev/null", "2>$null")
        s = s.replace("2>&-", "2>$null")
    return s


def translate_env_var(s: str) -> str:
    """Translate $VAR references to PowerShell syntax on Windows."""
    if not is_windows():
        return s

    import re

    def _replace_var(m: re.Match) -> str:
        name = m.group(1) or m.group(2)
        return f"$env:{name}"

    s = re.sub(r'\$(\{)?(\w+)(?(1)\})', _replace_var, s)
    return s


def translate_and(s: str) -> str:
    """Translate && to PowerShell-safe conditional."""
    if not is_windows():
        return s

    parts = s.split("&&")
    if len(parts) <= 1:
        return s

    translated = []
    for part in parts:
        stripped = part.strip()
        if stripped:
            translated.append(stripped)

    if len(translated) >= 2:
        chain = translated[0]
        for cmd in translated[1:]:
            chain = f"{chain}; if ($?) {{ {cmd} }}"
        return chain
    return s


def translate_or(s: str) -> str:
    """Translate || to PowerShell-safe conditional."""
    if not is_windows():
        return s

    parts = s.split("||")
    if len(parts) <= 1:
        return s

    translated = []
    for part in parts:
        stripped = part.strip()
        if stripped:
            translated.append(stripped)

    if len(translated) >= 2:
        chain = translated[0]
        for cmd in translated[1:]:
            chain = f"{chain}; if (-not $?) {{ {cmd} }}"
        return chain
    return s


def translate_command(cmd: str) -> str:
    """Apply all platform-specific translations to a command string."""
    cmd = translate_stderr(cmd)
    cmd = translate_env_var(cmd)
    cmd = translate_and(cmd)
    cmd = translate_or(cmd)

    if is_windows():
        if cmd.strip().startswith("command -v"):
            tool = cmd.strip().replace("command -v", "").strip()
            cmd = f"Get-Command {tool}"

    return cmd


def main() -> None:
    args = sys.argv[1:]
    if not args:
        print("Usage: just_shell.py <command> [args...]", file=sys.stderr)
        sys.exit(1)

    raw_cmd = " ".join(shlex.quote(a) if " " in a else a for a in args)
    translated = translate_command(raw_cmd)

    shell_cmd: list[str]
    if is_windows():
        shell_cmd = ["powershell", "-NoProfile", "-Command", translated]
    else:
        shell_cmd = ["bash", "-c", translated]

    proc = subprocess.run(shell_cmd)
    sys.exit(proc.returncode)


if __name__ == "__main__":
    main()

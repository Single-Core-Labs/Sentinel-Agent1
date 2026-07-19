#!/usr/bin/env pwsh
# ---------------------------------------------------------------------------
# Optimise the Windows PATH for Bazel CI tasks.
#
# Bazel on Windows is sensitive to PATH ordering — tools like `patch`, `sh`,
# and `python3` must resolve consistently so that BuildBuddy remote cache
# keys are stable across CI runs.  This script prunes user-local entries
# that can change between runners and ensures the system PATH is canonical.
#
# Usage:
#   .github/scripts/compute-bazel-windows-path.ps1
# ---------------------------------------------------------------------------
$ErrorActionPreference = "Stop"

Write-Host "[bazel-windows-path] Computing stable Windows PATH for Bazel CI…"

# Collect paths to keep — system directories first
$keep = @()

# System directories (always stable across runners)
$systemDirs = @(
    "$env:SystemRoot",
    "$env:SystemRoot\System32",
    "$env:SystemRoot\System32\WindowsPowerShell\v1.0",
    "$env:SystemRoot\System32\Wbem",
    "$env:SystemRoot\System32\OpenSSH"
)

# Git and common tool locations installed by GitHub Actions runners
$toolDirs = @(
    "$env:ProgramFiles\Git\cmd",
    "$env:ProgramFiles\Git\bin",
    "$env:ProgramFiles\Git\usr\bin",
    "${env:ProgramFiles(x86)}\Git\cmd",
    "${env:ProgramFiles(x86)}\Git\bin",
    "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer",
    "$env:LOCALAPPDATA\Microsoft\WinGet\Packages"
)

# Bazelisk / Bazelisk-managed Bazel
$bazelDirs = @(
    "$env:USERPROFILE\.bazelisk",
    "$env:ProgramFiles\Bazelisk"
)

# Rust toolchain (stable only)
$rustDirs = @(
    "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin",
    "$env:USERPROFILE\.cargo\bin"
)

# Python (from setup-python action — consistent path as set by GHA)
$pythonDirs = @(
    "$env:RUNNER_TOOL_CACHE\Python\3.12.*\x64",
    "$env:RUNNER_TOOL_CACHE\Python\3.12.*\x64\Scripts"
)

$allCandidates = $systemDirs + $toolDirs + $bazelDirs + $rustDirs + $pythonDirs

foreach ($dir in $allCandidates) {
    # Expand wildcards (e.g. "3.12.*") and check existence
    $resolved = Resolve-Path $dir -ErrorAction SilentlyContinue
    if ($resolved) {
        foreach ($r in $resolved) {
            $path = $r.Path
            if ($keep -notcontains $path -and (Test-Path $path)) {
                $keep += $path
            }
        }
    }
}

# Append original PATH entries that look like system/tool paths
$originalPath = $env:PATH -split ";"
foreach ($entry in $originalPath) {
    $normalized = $entry.Trim()
    if (-not $normalized) { continue }
    # Only keep entries that look stable (no AppData\Local\Temp, no user-local random paths)
    $isStable = $normalized -notmatch "AppData\\Local\\Temp" -and
                $normalized -notmatch "AppData\\Roaming\\npm" -and
                $normalized -notmatch "\.vscode" -and
                $normalized -match "^[A-Z]:\\"
    if ($isStable -and $keep -notcontains $normalized -and (Test-Path $normalized)) {
        $keep += $normalized
    }
}

# Deduplicate and join
$newPath = ($keep | Select-Object -Unique) -join ";"

Write-Host "[bazel-windows-path] PATH length: $($newPath.Length) chars over $($keep.Count) entries"

# Export so GitHub Actions steps that follow inherit it
Write-Host "[bazel-windows-path] Setting GITHUB_ENV…"
"PATH=${newPath}" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8

Write-Host "[bazel-windows-path] Done"

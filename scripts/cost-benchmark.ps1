# cost-benchmark.ps1 — Measurable work is free: Sentinel local slash commands vs an LLM-only agent.
#
# For each task, runs two paths headless:
#   1. Sentinel local REPL  (`sentinel local <model>` + piped slash command) — 0 LLM tokens by construction.
#   2. LLM agent            (`sentinel ai --prompt <task> --yolo`) — token counts parsed from the
#      `[sentinel] session summary: prompt_tokens=.. completion_tokens=.. total_tokens=..` line.
#
# Emits docs/design/cost-results.md (Markdown table).
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/cost-benchmark.ps1
#   powershell -ExecutionPolicy Bypass -File scripts/cost-benchmark.ps1 -Tasks emulate,gpu,anomaly,sweep,ssh
#   powershell -ExecutionPolicy Bypass -File scripts/cost-benchmark.ps1 -SkipLLM          # local path only
#   powershell -ExecutionPolicy Bypass -File scripts/cost-benchmark.ps1 -SkipLocal
#
# Requirements: built sentinel binary (cargo build --bin sentinel), a configured
# provider for the LLM path (sentinel auth login), Ollama for the local path.
param(
    [string]$Model = "qwen3:8b",
    [string]$LLMModel = "",
    [string[]]$Tasks = @("emulate", "gpu", "anomaly", "sweep", "ssh"),
    [switch]$SkipLLM,
    [switch]$SkipLocal,
    [double]$DollarsPerMTok = 2.0,
    [string]$SSHHost = $env:SENTINEL_SSH_HOST
)

$ErrorActionPreference = "Continue"
$Repo = Split-Path -Parent $PSScriptRoot
$Bin = Join-Path $Repo "target\debug\sentinel.exe"
$ResultsFile = Join-Path $Repo "docs\design\cost-results.md"
$Tasks = $Tasks | ForEach-Object { $_ -split "," } | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }

if (-not (Test-Path $Bin)) {
    Write-Host "Building sentinel binary..."
    Push-Location $Repo
    cargo build --bin sentinel 2>&1 | Out-Null
    Pop-Location
}
if (-not (Test-Path $Bin)) { Write-Error "sentinel.exe not found after build"; exit 1 }

$KernelDir = Join-Path $Repo "test-kernels"
$KernelFiles = Get-ChildItem -Path $KernelDir -Filter *.cu -ErrorAction SilentlyContinue
if (-not $KernelFiles) { $KernelFiles = Get-ChildItem -Path $KernelDir -Filter *.py -ErrorAction SilentlyContinue }
if (-not $KernelFiles) { Write-Error "no test kernels in test-kernels\"; exit 1 }
$KernelA = $KernelFiles[0].FullName
$KernelB = if ($KernelFiles.Count -gt 1) { $KernelFiles[1].FullName } else { $KernelA }

$env:SENTINEL_NON_INTERACTIVE = "1"

function Invoke-LocalCommand {
    param([string]$Command)
    $psi = @($Command, "exit") -join "`n"
    $psi = $psi + "`n"
    $out = $psi | & $Bin local $Model 2>&1
    return ($out -join "`n")
}

function Invoke-LLMAgent {
    param([string]$Prompt)
    $args = @("ai")
    if ($LLMModel) { $args += $LLMModel }
    $args += @("--prompt", $Prompt, "--yolo")
    $out = & $Bin @args 2>&1
    $text = ($out -join "`n")
    $tokens = @{ prompt = 0; completion = 0; total = 0; ok = $false }
    if ($text -match "\[sentinel\] session summary: prompt_tokens=(\d+) completion_tokens=(\d+) total_tokens=(\d+)") {
        $tokens.prompt = [int]$Matches[1]
        $tokens.completion = [int]$Matches[2]
        $tokens.total = [int]$Matches[3]
        $tokens.ok = $true
    } elseif ($text -match "needs setup|No provider configured|✖") {
        $tokens.ok = $false
    }
    return @{ text = $text; tokens = $tokens }
}

function Get-LocalOutput {
    param([string]$Name)
    switch ($Name) {
        "emulate"   { return Invoke-LocalCommand "/emulate $KernelA --sweep" }
        "gpu"       { return Invoke-LocalCommand "/gpu" }
        "anomaly"   { return Invoke-LocalCommand "/profile dmon 2" }
        "sweep"     { return Invoke-LocalCommand "/emulate $KernelB --sweep" }
        "ssh"       { if ($SSHHost) { return Invoke-LocalCommand "/ssh profile $SSHHost 2" } else { return $null } }
        default     { return $null }
    }
}

function Get-LLMPrompt {
    param([string]$Name)
    switch ($Name) {
        "emulate"   { return "Analyze the GPU kernel in test-kernels\$($KernelFiles[0].Name) and recommend the best block size and shared memory configuration with reasoning." }
        "gpu"       { return "Report the current GPU stats: name, VRAM, utilization, temperature. Use any available tool." }
        "anomaly"   { return "Explain how to detect GPU anomalies (compute/memory/thermal) from nvidia-smi dmon output and produce the exact command for a 2-second profile." }
        "sweep"     { foreach ($f in $KernelFiles) { if ($f.FullName -eq $KernelB) { return "Sweep launch configurations for the GPU kernel $($f.Name) and report the best config with reasoning." } }; return "Sweep launch configurations for the GPU kernel and report the best config with reasoning." }
        "ssh"       { if ($SSHHost) { return "Profile GPU utilization on remote host $SSHHost for 2 seconds and summarize anomalies." } else { return $null } }
        default     { return $null }
    }
}

$rows = @()
foreach ($task in $Tasks) {
    Write-Host ("== task: {0}" -f $task)
    $row = [ordered]@{ Task = $task; LocalTokens = "n/a"; LLMTokens = "n/a"; Delta = "n/a"; EstCost = "n/a"; LocalMs = "n/a"; LLMMs = "n/a"; Note = "" }

    $t0 = Get-Date
    if (-not $SkipLocal) {
        $out = Get-LocalOutput $task
        $row.LocalMs = [int]((Get-Date) - $t0).TotalMilliseconds
        if ($null -eq $out) {
            $row.LocalTokens = "skipped"
            $row.Note = "no SSH host configured (set SENTINEL_SSH_HOST)"
        } elseif ($out -match "Unknown command|Ollama not found|Error") {
            $row.LocalTokens = "error"
            $row.Note = "local REPL failed (Ollama running?)"
        } else {
            $row.LocalTokens = "0"
        }
    }

    $t1 = Get-Date
    if (-not $SkipLLM) {
        $prompt = Get-LLMPrompt $task
        if ($null -eq $prompt) {
            $row.LLMTokens = "skipped"
            if ($row.Note -eq "") { $row.Note = "no SSH host configured" }
        } else {
            $r = Invoke-LLMAgent $prompt
            $row.LLMMs = [int]((Get-Date) - $t1).TotalMilliseconds
            if ($r.tokens.ok) {
                $row.LLMTokens = $r.tokens.total
                $row.Delta = $r.tokens.total
                $row.EstCost = ("{0:N4}" -f (($r.tokens.total / 1e6) * $DollarsPerMTok))
            } else {
                $row.LLMTokens = "error"
                $row.Note = "LLM path failed (provider configured? `sentinel auth login`)"
            }
        }
    }

    $rows += [pscustomobject]$row
}

$md = @()
$md += "# Cost Results (measured)"
$md += ""
$md += "Run: $(Get-Date -Format 'yyyy-MM-dd HH:mm') | Local model: $Model | LLM pricing: $DollarsPerMTok USD/Mtok (input, illustrative)"
$md += ""
$md += "| Task | Local tokens | LLM tokens | Delta | Est. cost | Local wall | LLM wall |"
$md += "|---|---|---|---|---|---|---|"
foreach ($r in $rows) {
    $md += "| $($r.Task) | $($r.LocalTokens) | $($r.LLMTokens) | $($r.Delta) | $($r.EstCost) | $($r.LocalMs) ms | $($r.LLMMs) ms |"
}
if (($rows | Where-Object { $_.Note -ne "" } | Measure-Object).Count -gt 0) {
    $md += ""
    $md += "Notes:"
    foreach ($r in $rows) { if ($r.Note -ne "") { $md += "- $($r.Task): $($r.Note)" } }
}
$md | Set-Content -Path $ResultsFile -Encoding UTF8
Write-Host "Wrote $ResultsFile"

param(
    [string]$Model = "qwen3:8b",
    [string]$Tasks = "info,models,backends,recommend",
    [switch]$SkipLLM,
    [switch]$SkipLocal,
    [string]$SSHHost = "",
    [double]$DollarsPerMTok = 2.0
)

# Cost harness: same measurable task, two execution paths.
#   Local path:  sentinel local <model> /<cmd>     -> 0 LLM tokens by construction
#   LLM path:    sentinel ai --prompt "<task>"      -> tokens parsed from the
#                [sentinel] session summary line
# Emits docs/design/cost-results.md.

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $repo "target\debug\sentinel.exe"
$outDoc = Join-Path $repo "docs\design\cost-results.md"

if (-not (Test-Path $bin)) {
    Push-Location $repo
    try { cargo build -q --bin sentinel } finally { Pop-Location }
}

$env:SENTINEL_NON_INTERACTIVE = "1"

$taskDefs = @{
    bench      = @{ Local = "/bench"; LLM = "Benchmark the current LLM model's token throughput (tokens per second) using your tools, and report the result." }
    info       = @{ Local = "/info"; LLM = "Report the current machine's OS, CPU cores, and RAM by using your tools." }
    models     = @{ Local = "/models"; LLM = "List the models currently pulled in local Ollama, using your tools." }
    backends   = @{ Local = "/backends"; LLM = "Detect which local LLM backends are available (Ollama, vLLM, LM Studio) using your tools." }
    recommend  = @{ Local = "/recommend"; LLM = "Recommend a suitable local LLM model for this machine's hardware (RAM, cores) using your tools." }
    ssh        = @{ Local = "/ssh $SSHHost hostname"; LLM = "Run 'hostname' on remote host $SSHHost via SSH using your tools." }
}

function Run-And-Capture([string]$file, [string[]]$argsArr) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $out = & $file @argsArr 2>&1
    } catch {
        $out = "error: $_"
    }
    $sw.Stop()
    return @{ Out = ($out | Out-String); WallMs = $sw.ElapsedMilliseconds }
}

function Get-Tokens([string]$out) {
    $m = [regex]::Match($out, "\[sentinel\] session summary: prompt_tokens=(\d+) completion_tokens=(\d+) total_tokens=(\d+)")
    if ($m.Success) { return [int]$m.Groups[3].Value }
    return -1
}

$rows = @()
foreach ($task in $Tasks.Split(',')) {
    $task = $task.Trim()
    if (-not $taskDefs.ContainsKey($task)) {
        Write-Warning "Unknown task '$task' - skipping"
        continue
    }
    $def = $taskDefs[$task]
    Write-Host "== Task: $task =="

    $localTokens = 0; $localWall = $null
    if (-not $SkipLocal) {
        if ($task -eq "ssh" -and [string]::IsNullOrEmpty($SSHHost)) {
            Write-Host "  local : skipped (no SENTINEL_SSH_HOST)"
            $localTokens = -2
        } else {
            $r = Run-And-Capture $bin @("local", $Model, $def.Local)
            $localWall = $r.WallMs
            Write-Host ("  local : {0} ms (0 LLM tokens by construction)" -f $localWall)
        }
    }

    $llmTokens = $null; $llmWall = $null
    if (-not $SkipLLM) {
        if ($task -eq "ssh" -and [string]::IsNullOrEmpty($SSHHost)) {
            Write-Host "  llm   : skipped (no SENTINEL_SSH_HOST)"
            $llmTokens = -2
        } else {
            $r = Run-And-Capture $bin @("ai", "--model", $Model, "--yolo", "--prompt", $def.LLM)
            $llmWall = $r.WallMs
            $llmTokens = Get-Tokens $r.Out
            Write-Host ("  llm   : {0} ms, total_tokens={1}" -f $llmWall, $llmTokens)
        }
    }

    $rows += [pscustomobject]@{
        Task    = $task
        Local   = $localTokens
        LLM     = $llmTokens
        LocalW  = $localWall
        LLMW    = $llmWall
    }
}

$now = Get-Date -Format "yyyy-MM-dd HH:mm"
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine("# Cost Results (measured)")
[void]$sb.AppendLine("")
[void]$sb.AppendLine("Run: $now | Local model: $Model | LLM pricing: $DollarsPerMTok USD/Mtok (input, illustrative)")
[void]$sb.AppendLine("")
[void]$sb.AppendLine("| Task | Local tokens | LLM tokens | Delta | Est. cost | Local wall | LLM wall |")
[void]$sb.AppendLine("|---|---|---|---|---|---|---|")
foreach ($r in $rows) {
    if ($r.Local -eq -2) { $lt = "skipped" } elseif ($null -eq $r.Local) { $lt = "n/a" } else { $lt = $r.Local }
    if ($r.LLM -eq -2) { $mt = "skipped" } elseif ($null -eq $r.LLM) { $mt = "n/a" } elseif ($r.LLM -lt 0) { $mt = "error" } else { $mt = $r.LLM }
    $delta = "n/a"
    if ($r.Local -ge 0 -and $r.LLM -ge 0) { $delta = $r.LLM - $r.Local }
    $cost = "n/a"
    if ($r.LLM -ge 0) { $cost = ("{0:N4}" -f ($r.LLM / 1e6 * $DollarsPerMTok)) }
    $lw = if ($null -eq $r.LocalW) { "n/a" } else { "$($r.LocalW) ms" }
    $mw = if ($null -eq $r.LLMW) { "n/a" } else { "$($r.LLMW) ms" }
    [void]$sb.AppendLine("| $($r.Task) | $lt | $mt | $delta | $cost | $lw | $mw |")
}
[void]$sb.AppendLine('Notes:')
[void]$sb.AppendLine('- Local path is `sentinel local <model> /<cmd>` (one-shot); zero LLM tokens by construction.')
[void]$sb.AppendLine('- LLM path is `sentinel ai --prompt "<task>" --yolo`; tokens parsed from the `[sentinel] session summary:` line.')
[void]$sb.AppendLine('- Rerun: `powershell -ExecutionPolicy Bypass -File scripts/cost-benchmark.ps1`')
[void]$sb.AppendLine('- ssh task requires -SSHHost <host> (or the SENTINEL_SSH_HOST env var).')

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($outDoc, $sb.ToString(), $utf8NoBom)
Write-Host ""
Write-Host "Wrote $outDoc"

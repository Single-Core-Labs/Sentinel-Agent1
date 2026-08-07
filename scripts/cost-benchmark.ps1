param(
    [string]$Model = "qwen3:8b",
    [string]$LLMModels = "openrouter/openai/gpt-oss-20b:free,openrouter/poolside/laguna-s-2.1:free,openrouter/nvidia/nemotron-3-super-120b-a12b:free,openrouter/cohere/north-mini-code:free",
    [string]$BaselineModel = "openrouter/openai/gpt-4o-mini",
    [string]$Tasks = "info,models,backends,recommend,bench",
    [switch]$SkipLLM,
    [switch]$SkipLocal,
    [switch]$SkipBaseline,
    [string]$SSHHost = "",
    [double]$DollarsPerMTok = 2.0
)

# Cost harness: same measurable task, two execution paths, N models.
#   Local path:  sentinel local <model> /<cmd>        -> 0 LLM tokens by construction
#   LLM path:    sentinel ai --model <id> --yolo --prompt "<task>" (sandboxed)
#                     -> tokens parsed from the [sentinel] session summary line
#   Baseline:    one paid model (default gpt-4o-mini) runs the same tasks so the
#                dashboard can show "$ saved" per model per task.
# Every output is validated (task-specific pattern); failing runs are excluded
# from the optimizer leaderboard. Emits docs/design/bench-results.json (drives
# the cost-lab dashboard) + docs/design/cost-results.md.

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $repo "target\debug\sentinel.exe"
$outDoc = Join-Path $repo "docs\design\cost-results.md"
$outJson = Join-Path $repo "docs\design\bench-results.json"

if (-not (Test-Path $bin)) {
    Push-Location $repo
    try { cargo build -q --bin sentinel } finally { Pop-Location }
}

$env:SENTINEL_NON_INTERACTIVE = "1"
$env:SENTINEL_MAX_TOKENS = "4096"
# Confine agent tool calls (write/edit/run_shell) to a temp scratch sandbox.
$env:SENTINEL_SANDBOX = "1"

if (-not $SkipLLM -and -not $env:OPENROUTER_API_KEY) {
    Write-Host "W OPENROUTER_API_KEY is not set — LLM/baseline runs will fail. Set it or pass -SkipLLM -SkipBaseline."
}

$taskDefs = @{
    bench      = @{ Local = "/bench"; LLM = "Benchmark the current LLM model's token throughput (tokens per second) using your tools, and report the result." }
    info       = @{ Local = "/info"; LLM = "Report the current machine's OS, CPU cores, and RAM by using your tools." }
    models     = @{ Local = "/models"; LLM = "List the models currently pulled in local Ollama, using your tools." }
    backends   = @{ Local = "/backends"; LLM = "Detect which local LLM backends are available (Ollama, vLLM, LM Studio) using your tools." }
    recommend  = @{ Local = "/recommend"; LLM = "Recommend a suitable local LLM model for this machine's hardware (RAM, cores) using your tools." }
    ssh        = @{ Local = "/ssh $SSHHost hostname"; LLM = "Run 'hostname' on remote host $SSHHost via SSH using your tools." }
}

# Output validation: is the task's answer actually present? (lenient on purpose)
function Test-Answer([string]$task, [string]$output) {
    if ([string]::IsNullOrWhiteSpace($output)) { return $false }
    switch ($task) {
        "info"      { return ($output -match "windows|linux|macos|darwin") -and ($output -match "core|cpu") -and ($output -match "\d+\s*(gb|gib|ram|memory)") }
        "models"    { return $output -match "\b(qwen|mistral|llama|gemma|phi|deepseek|nemotron|gpt)\b" }
        "backends"  { return $output -match "ollama|vllm|lm\s?studio|lms" }
        "recommend" { return $output -match "\d+\s*(gb|gib)" -or $output -match "\b(qwen|mistral|llama|gemma|phi|deepseek|nemotron|gpt)\b" }
        "bench"     { return $output -match "\d+(\.\d+)?\s*(tok|tokens?)?\s*/?\s*(s|sec|second)" -or $output -match "tokens? per second" }
        "ssh"       { return $output -notmatch "error|failed|denied" }
        default     { return $true }
    }
}

function Run-And-Capture([string]$file, [string[]]$argsArr, [string]$workdir) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        if ($workdir) {
            $null = Push-Location $workdir
            try { $cap = & $file @argsArr 2>&1 } finally { Pop-Location }
        } else {
            $cap = & $file @argsArr 2>&1
        }
    } catch {
        $cap = "error: $_"
    }
    $sw.Stop()
    return @{ Out = ($cap | Out-String); WallMs = $sw.ElapsedMilliseconds }
}

function Get-Tokens([string]$out) {
    $m = [regex]::Match($out, "\[sentinel\] session summary: prompt_tokens=(\d+) completion_tokens=(\d+) total_tokens=(\d+)")
    if ($m.Success) { return [int]$m.Groups[3].Value }
    return -1
}

# Run one LLM-path invocation with 429/5xx backoff. Returns @{Tokens; WallMs; Status; Output}
function Invoke-LLMTask([string]$model, [string]$prompt, [string]$scratch) {
    for ($attempt = 1; $attempt -le 4; $attempt++) {
        $r = Run-And-Capture $bin @("ai", "--model", $model, "--yolo", "--prompt", $prompt) $scratch
        $tokens = Get-Tokens $r.Out
        $transient = $r.Out -match "Rate limited|429|ServiceUnavailable|502|503|504|temporarily rate-limited"
        if ($tokens -ge 0) {
            return @{ Tokens = $tokens; WallMs = $r.WallMs; Output = $r.Out }
        }
        if ($transient) {
            $backoff = 15 * $attempt
            Write-Host ("    retry {0}: transient, backing off {1}s" -f $attempt, $backoff)
            Start-Sleep -Seconds $backoff
        } else {
            return @{ Tokens = -1; WallMs = $r.WallMs; Output = $r.Out }
        }
    }
    return @{ Tokens = -1; WallMs = $null; Output = "" }
}

function New-Scratch() {
    $dir = Join-Path $env:TEMP ("sentinel-bench-{0}" -f ([guid]::NewGuid().ToString("N")))
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    return $dir
}

$models = @($LLMModels.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$scratch = New-Scratch

# results[task][model] = @{ tokens; wall_ms; status; cost_usd; output }
$results = @{}
$baseline = @{}

foreach ($task in $Tasks.Split(',')) {
    $task = $task.Trim()
    if (-not $taskDefs.ContainsKey($task)) {
        Write-Warning "Unknown task '$task' - skipping"
        continue
    }
    $def = $taskDefs[$task]
    Write-Host "== Task: $task =="
    $taskResults = @{}

    if (-not $SkipLocal) {
        if ($task -eq "ssh" -and [string]::IsNullOrEmpty($SSHHost)) {
            Write-Host "  local : skipped (no SENTINEL_SSH_HOST)"
        } else {
            $r = Run-And-Capture $bin @("local", $Model, $def.Local) ""
            $ok = Test-Answer $task $r.Out
            $taskResults["local"] = @{
                tokens = 0; wall_ms = $r.WallMs; status = if ($ok) { "pass" } else { "fail" };
                cost_usd = 0.0; output = $r.Out
            }
            Write-Host ("  local : {0} ms, status={1}" -f $r.WallMs, $taskResults["local"].status)
        }
    }

    if (-not $SkipBaseline -and $env:OPENROUTER_API_KEY) {
        if ($task -eq "ssh" -and [string]::IsNullOrEmpty($SSHHost)) {
            Write-Host "  base  : skipped (no SENTINEL_SSH_HOST)"
        } else {
            Write-Host ("  base  : {0}" -f $BaselineModel)
            $r = Invoke-LLMTask $BaselineModel $def.LLM $scratch
            $ok = $r.Tokens -ge 0 -and (Test-Answer $task $r.Output)
            $base = @{
                tokens = $r.Tokens; wall_ms = $r.WallMs; status = if ($ok) { "pass" } else { "fail" };
                cost_usd = if ($r.Tokens -gt 0) { [math]::Round($r.Tokens / 1e6 * $DollarsPerMTok, 6) } else { 0.0 };
                output = $r.Output
            }
            $baseline[$task] = $base
            Write-Host ("  base  : {0} ms, tokens={1}, status={2}" -f $r.WallMs, $r.Tokens, $base.status)
        }
    }

    if (-not $SkipLLM -and $env:OPENROUTER_API_KEY) {
        foreach ($m in $models) {
            if ($task -eq "ssh" -and [string]::IsNullOrEmpty($SSHHost)) { break }
            Write-Host ("  llm   : {0}" -f $m)
            $r = Invoke-LLMTask $m $def.LLM $scratch
            $ok = $r.Tokens -ge 0 -and (Test-Answer $task $r.Output)
            $taskResults[$m] = @{
                tokens = $r.Tokens; wall_ms = $r.WallMs; status = if ($ok) { "pass" } else { "fail" };
                cost_usd = if ($r.Tokens -gt 0) { [math]::Round($r.Tokens / 1e6 * $DollarsPerMTok, 6) } else { 0.0 };
                output = $r.Output
            }
            Write-Host ("         {0} ms, tokens={1}, status={2}" -f $r.WallMs, $r.Tokens, $taskResults[$m].status)
        }
    }

    $results[$task] = $taskResults
}

# ── Emit JSON (dashboard input) ────────────────────────────────────────────
$jsonDoc = @{
    generated_at = (Get-Date -Format "yyyy-MM-dd HH:mm:ss")
    pricing_per_mtok = $DollarsPerMTok
    local_model = $Model
    baseline_model = if ($SkipBaseline) { $null } else { $BaselineModel }
    models = $models
    tasks = @($Tasks.Split(',') | ForEach-Object { $_.Trim() })
    results = $results
    baseline = $baseline
} | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText($outJson, $jsonDoc, (New-Object System.Text.UTF8Encoding($false)))
Write-Host ""
Write-Host "Wrote $outJson"

# ── Emit Markdown table ────────────────────────────────────────────────────
$now = Get-Date -Format "yyyy-MM-dd HH:mm"
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine("# Cost Results (measured)")
[void]$sb.AppendLine("")
[void]$sb.AppendLine("Run: $now | Local model: $Model | LLM models: $($models -join ', ') | Baseline: $BaselineModel | Pricing: $DollarsPerMTok USD/Mtok (illustrative)")
[void]$sb.AppendLine("")
[void]$sb.AppendLine("| Task | Path | Tokens | Status | Est. cost | vs baseline | Wall |")
[void]$sb.AppendLine("|---|---|---|---|---|---|---|")
foreach ($task in $results.Keys) {
    $tr = $results[$task]
    $base = $baseline[$task]
    foreach ($path in @("local") + $models) {
        if (-not $tr.ContainsKey($path)) { continue }
        $r = $tr[$path]
        $tok = if ($r.tokens -lt 0) { "error" } else { $r.tokens }
        $cost = if ($r.tokens -ge 0) { ("{0:N4}" -f $r.cost_usd) } else { "n/a" }
        $vs = "n/a"
        if ($r.tokens -ge 0 -and $base -and $base.tokens -ge 0) {
            $vs = ("{0:N4}" -f [math]::Max(0.0, $base.cost_usd - $r.cost_usd))
        }
        $wall = if ($null -eq $r.wall_ms) { "n/a" } else { "$($r.wall_ms) ms" }
        [void]$sb.AppendLine("| $task | $path | $tok | $($r.status) | `$$cost | `$$vs | $wall |")
    }
}
[void]$sb.AppendLine('Notes:')
[void]$sb.AppendLine('- Local path is `sentinel local <model> /<cmd>` (one-shot); zero LLM tokens by construction.')
[void]$sb.AppendLine('- LLM path is `sentinel ai --yolo --prompt "<task>"` (sandboxed, `SENTINEL_SANDBOX=1`); tokens parsed from the `[sentinel] session summary:` line.')
[void]$sb.AppendLine('- "vs baseline" = baseline $ - model $ (0 = as cheap as baseline). Baseline: one paid model run per task.')
[void]$sb.AppendLine('- `status` = pass/fail — the output is validated per task (e.g. `info` must mention OS + cores + RAM); failing runs are excluded from the dashboard optimizer.')
[void]$sb.AppendLine('- Dashboard: `scripts/run-bench-lab.ps1` (serves charts at http://localhost:PORT/cost-lab/).')
[void]$sb.AppendLine('- Rerun: `powershell -ExecutionPolicy Bypass -File scripts/cost-benchmark.ps1`')
[void]$sb.AppendLine('- ssh task requires -SSHHost <host> (or the SENTINEL_SSH_HOST env var).')

[System.IO.File]::WriteAllText($outDoc, $sb.ToString(), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "Wrote $outDoc"

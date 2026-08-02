# CLI Agent Test Suite for Sentinel
# Triggers the real headless single-shot agent (`sentinel ai ... --prompt`) against a local Ollama backend.
# Usage:  powershell -ExecutionPolicy Bypass -File tests/cli-agent/test-agent.ps1
# Requires: Ollama running (`ollama serve`) with the model referenced in sentinel.toml (qwen3:8b).

param(
    [string]$Binary = "..\..\target\debug\sentinel.exe",
    [string]$Model   = "qwen3:8b"
)

$ErrorActionPreference = "Continue"
$pass = 0
$fail = 0

function Assert-True([string]$Name, [string[]]$Stdout, [string[]]$Matchers) {
    $joined = $Stdout -join "`n"
    $ok = $true
    foreach ($m in $Matchers) {
        if ($joined -notmatch $m) { $ok = $false }
    }
    if ($ok) {
        Write-Host "PASS  $Name" -ForegroundColor Green
        $script:pass++
    } else {
        Write-Host "FAIL  $Name (missing: $($Matchers -join ', '))" -ForegroundColor Red
        Write-Host "      --- output ---"
        $Stdout | ForEach-Object { Write-Host "      $_" }
        $script:fail++
    }
}

# Sanity: Ollama must be reachable + model must exist
try {
    $tags = Invoke-RestMethod "http://localhost:11434/api/tags" -TimeoutSec 5
    if (-not ($tags.models.name -contains $Model)) {
        Write-Host "WARN  model '$Model' not in 'ollama pull $Model'" -ForegroundColor Yellow
    }
} catch {
    Write-Host "ERROR Ollama not reachable on :11434. Run: ollama serve" -ForegroundColor Red
    exit 1
}

$env:SENTINEL_NON_INTERACTIVE = "1"

Write-Host "=== Sentinel CLI agent tests (model=$Model) ===" -ForegroundColor Cyan

# T1 basic single-turn
$out1 = & $Bin ai $Model --prompt "Reply with exactly: AGENT_OK" --yolo 2>&1
Assert-True "single-turn reply" $out1 @("AGENT_OK", "session summary")

# T2 unknown model -> actionable list, no silent fallback (#49/#52)
$out2 = & $Bin ai "nonexistent-model-x" --prompt "hi" --yolo 2>&1
Assert-True "unknown model rejected with model list" $out2 @("not recognized", "ollama-local")

# T3 remote model missing key -> preflight error (still requires OPENAI_API_KEY not set)
$out3 = & $Bin ai gpt-4o-mini --prompt "hi" --yolo 2>&1
Assert-True "remote preflight failure is actionable" $out3 @("not recognized")

# T4 tool use: glob lists real dirs
$out4 = & $Bin ai $Model --prompt "Use a tool to list the files in the current directory, then tell me the names of the first 3 crates directories you see. Do not guess." --yolo 2>&1
Assert-True "tool use glob returns real directories" $out4 @("glob", "crates\\", "session summary")

# T5 policy hook deny blocks tool call
$deny = New-TemporaryFile
Set-Content -Path $deny -Value 'Write-Output "deny test-deny"'
$out5 = & $Bin ai $Model --prompt "Use the glob tool with pattern '*', then reply DONE" --yolo --hook-command "powershell -File $deny" 2>&1
Assert-True "policy deny blocks tool call" $out5 @("denied")
Remove-Item $deny -ErrorAction SilentlyContinue

# T6 policy hook allow -> tool call executes
$allow = New-Item
Set-Content -Path $allow -Value 'Write-Output "allow"'
$out6 = & $Bin ai $Model --prompt "Use the glob tool with pattern 'crates/*', then reply ONLY-DONE" --yolo --hook-command "powershell -File $allow" 2>&1
Assert-True "policy allow fetches real glob result" $out6 @("crates\\", "ONLY-DONE")
Remove-Item $allow -ErrorAction SilentlyContinue

Write-Host "" -ForegroundColor Cyan
Write-Host "=== RESULT: $pass passed, $fail failed ===" -ForegroundColor $(if ($fail -eq 0) { "Green" } else { "Red" })
exit $fail
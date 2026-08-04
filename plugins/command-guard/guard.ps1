# command-guard (Windows) verdict logic. Reads the full event JSON from stdin,
# prints "allow" or "veto <reason>". Fail-closed on destructive patterns.
$ErrorActionPreference = 'SilentlyContinue'
$raw = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($raw)) { Write-Output "allow"; exit 0 }
try { $j = $raw | ConvertFrom-Json } catch { Write-Output "allow"; exit 0 }
if ([string]$j.tool_name -ne 'run_shell_command') { Write-Output "allow"; exit 0 }
$cmd = [string]$j.args.command
if ([string]::IsNullOrWhiteSpace($cmd)) { Write-Output "allow"; exit 0 }

$patterns = @(Get-Content (Join-Path $PSScriptRoot 'patterns.txt') -ErrorAction SilentlyContinue |
    ForEach-Object { $_.Trim() } |
    Where-Object { $_ -and -not $_.StartsWith('#') })

foreach ($p in $patterns) {
    $pNet = $p -replace '\[\[:space:\]\]', '\s'
    if ($cmd -match $pNet) {
        Write-Output ("veto destructive command: " + $cmd)
        exit 0
    }
}
Write-Output "allow"
exit 0

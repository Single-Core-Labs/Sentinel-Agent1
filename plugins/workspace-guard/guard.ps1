# workspace-guard (Windows) verdict logic. Reads the full event JSON from
# stdin, prints "allow" or "veto <reason>" as the first stdout line.
$ErrorActionPreference = 'SilentlyContinue'
$raw = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($raw)) { Write-Output "allow"; exit 0 }
try { $j = $raw | ConvertFrom-Json } catch { Write-Output "allow"; exit 0 }
$tool = [string]$j.tool_name
if ($tool -notin @('write', 'edit', 'apply_patch')) { Write-Output "allow"; exit 0 }

$cwd = [Environment]::CurrentDirectory
if ([string]::IsNullOrWhiteSpace($cwd)) { Write-Output "allow"; exit 0 }
$cwd = $cwd.TrimEnd('\')

function Test-Allowed([string]$p) {
    if ([string]::IsNullOrWhiteSpace($p)) { return $true }
    $full = [IO.Path]::GetFullPath($p).TrimEnd('\')
    if ($full -eq $cwd) { return $true }
    return $full.StartsWith($cwd + '\', [StringComparison]::OrdinalIgnoreCase)
}

if ($tool -in @('write', 'edit')) {
    $fp = [string]$j.args.file_path
    if (-not (Test-Allowed $fp)) {
        Write-Output ("veto file escapes workspace: " + $fp)
        exit 0
    }
    Write-Output "allow"
    exit 0
}

foreach ($line in (([string]$j.args.diff) -split "`n")) {
    if ($line -match '^\+\+\+\s+(.+)$') {
        $p = $Matches[1].Trim()
        if ($p -eq '/dev/null') { continue }
        $p = $p -replace '^b/', ''
        if ([IO.Path]::IsPathRooted($p) -and -not (Test-Allowed $p)) {
            Write-Output ("veto apply_patch target escapes workspace: " + $p)
            exit 0
        }
    }
}
Write-Output "allow"
exit 0

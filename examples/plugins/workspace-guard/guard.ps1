# workspace-guard (Windows) verdict logic. Reads the full event JSON from
# stdin, prints "allow" or "veto <reason>" as the first stdout line.
$j = [Console]::In.ReadToEnd() | ConvertFrom-Json
$p = $j.args.file_path
if (-not $p) { Write-Output "allow"; exit 0 }
$cwd = (Get-Location).Path.TrimEnd('\')
$full = [IO.Path]::GetFullPath($p).TrimEnd('\')
if ($p -match '\.\.') {
    Write-Output ("veto path traversal: " + $p)
} elseif ($full -ne $cwd -and -not $full.StartsWith($cwd + '\')) {
    Write-Output ("veto file escapes workspace: " + $p)
} else {
    Write-Output "allow"
}

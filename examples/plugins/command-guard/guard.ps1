# command-guard (Windows) verdict logic. Reads the full event JSON from stdin,
# prints "allow" or "veto <reason>". Fail-closed on destructive patterns.
$j = [Console]::In.ReadToEnd() | ConvertFrom-Json
$cmd = [string]$j.args.command
$patterns = '(?i)\brm\s+-rf\s+(/|~|\*|[a-z]:\\?)|format\s+[a-z]:|del\s+/[fqs]*\s*/[sq]|rmdir\s+/[sq]|Remove-Item[^\r\n]*\b-Recurse\b|mkfs|diskpart|dd\s+if=|shutdown\s+/[rs]|>\s*/dev/sd[a-z]|:\(\)\s*\{.*\|\s*bash'
if ($cmd -match $patterns) {
    Write-Output ("veto destructive command: " + $cmd)
} else {
    Write-Output "allow"
}

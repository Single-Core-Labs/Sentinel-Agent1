# web-guard (Windows) verdict logic. Reads the full event JSON from stdin,
# prints "allow" or "veto <reason>". Default posture: deny.
$j = [Console]::In.ReadToEnd() | ConvertFrom-Json
$a = ""
if ($j.args.url) {
    try { $a = ([uri]$j.args.url).Host } catch { $a = "invalid-url" }
} elseif ($j.args.query) {
    $a = "search:*"
}
if (-not $a) { Write-Output "allow"; exit 0 }
$allow = @("github.com", "docs.rs", "en.wikipedia.org", "opencode.ai")
if ($allow -contains $a -or ($a -eq "search:*" -and $allow -contains "search:*")) {
    Write-Output "allow"
} else {
    Write-Output ("veto domain not allowlisted: " + $a)
}

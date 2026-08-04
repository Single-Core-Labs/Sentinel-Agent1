# web-guard (Windows) verdict logic. Reads the full event JSON from stdin,
# prints "allow" or "veto <reason>". Default posture: deny.
$ErrorActionPreference = 'SilentlyContinue'
$raw = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($raw)) { Write-Output "allow"; exit 0 }
try { $j = $raw | ConvertFrom-Json } catch { Write-Output "allow"; exit 0 }
$tool = [string]$j.tool_name
if ($tool -notin @('web_fetch', 'web_search')) { Write-Output "allow"; exit 0 }

$url = [string]$j.args.url
$query = [string]$j.args.query
if (-not $url -and -not $query) { Write-Output "allow"; exit 0 }

$listPath = Join-Path $PSScriptRoot 'allowlist.txt'
$allow = @(Get-Content $listPath -ErrorAction SilentlyContinue |
    ForEach-Object { $_.Trim() } |
    Where-Object { $_ -and -not $_.StartsWith('#') })

if ($url) {
    $urlHost = ""
    try { $urlHost = ([uri]$url).Host } catch { }
    if (-not $urlHost) {
        Write-Output ("veto web request without a resolvable host: " + $url)
        exit 0
    }
    foreach ($a in $allow) {
        if ($urlHost -ieq $a -or $urlHost -ilike "*.$a") {
            Write-Output "allow"
            exit 0
        }
    }
    Write-Output ("veto domain not allowlisted: " + $urlHost)
    exit 0
}

if ($query) {
    if ($allow -contains 'search:*') {
        Write-Output "allow"
    } else {
        Write-Output "veto web_search is not allowlisted (add 'search:*' to the allowlist to enable)"
    }
    exit 0
}
Write-Output "allow"
exit 0

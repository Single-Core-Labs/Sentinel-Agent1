# Sentinel AI Installer for Windows (PowerShell 5.1+)
# Usage:
#   One-liner (latest release):      irm https://raw.githubusercontent.com/Single-Core-Labs/Sentinel-Agent1/master/install.ps1 | iex
#   Pinned version:                  powershell -ExecutionPolicy Bypass -File install.ps1 -Version v0.1.0
#   Dev install (local cargo build): powershell -ExecutionPolicy Bypass -File install.ps1 -LocalBuild target\release\sentinel.exe
param(
    [string]$Repo = "Single-Core-Labs/Sentinel-Agent1",
    [string]$Version = "",
    [string]$InstallDir = "",
    [string]$LocalBuild = "",
    [switch]$InstallVSCode,
    [string]$VsixPath = "",
    [switch]$SkipConfig,
    [switch]$SkipPath,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"

function Write-Step($Msg) { Write-Host $Msg -ForegroundColor Cyan }
function Write-Ok($Msg)    { Write-Host $Msg -ForegroundColor Green }
function Write-Warn($Msg)  { Write-Host $Msg -ForegroundColor Yellow }
function Write-Fail($Msg)  { Write-Host $Msg -ForegroundColor Red }

$ApiBase = "https://api.github.com/repos/$Repo"

if ([string]::IsNullOrEmpty($InstallDir)) {
    $InstallDir = Join-Path $env:USERPROFILE ".sentinel\bin"
}

if ([string]::IsNullOrEmpty($LocalBuild)) {
    if (-not [Environment]::Is64BitProcess) {
        Write-Fail "Error: only 64-bit x86_64 releases are published for Windows."
        Write-Fail "       For 32-bit, build from source: cargo build --release --path crates/interfaces/sentinel-cli"
        exit 1
    }
    $Target = "x86_64-pc-windows-msvc"
}

Write-Step "Sentinel AI installer for Windows"
Write-Host "  repo:       $Repo"
if (-not [string]::IsNullOrEmpty($LocalBuild)) {
    Write-Host "  source:     local build ($LocalBuild)"
} elseif (-not [string]::IsNullOrEmpty($Version)) {
    Write-Host "  version:    $Version"
} else {
    Write-Host "  version:    latest"
}
Write-Host "  install to: $InstallDir"

# -- 1. Resolve the binary --------------------------------------------------
$LocalBuild = $LocalBuild.Trim()
if ([string]::IsNullOrEmpty($LocalBuild)) {
    if (-not (Test-Path $InstallDir)) { New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null }

    if ([string]::IsNullOrEmpty($Version)) {
        Write-Step "Querying GitHub for the latest release..."
        try {
            $Release = Invoke-RestMethod -Uri "$ApiBase/releases/latest" -Headers @{ "User-Agent" = "sentinel-installer" }
        } catch {
            Write-Fail "Error: could not find any GitHub release for '$Repo'."
            Write-Warn "       Build from source instead, then use -LocalBuild:"
            Write-Warn "         cargo build --release --path crates/interfaces/sentinel-cli"
            Write-Warn "         powershell -File install.ps1 -LocalBuild target\release\sentinel.exe"
            exit 1
        }
        $Tag = $Release.tag_name
    } else {
        $Tag = $Version
    }
    $AssetName = "sentinel-$Tag-$Target.zip"
    $Asset = $Release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
    if ($null -eq $Asset) {
        if (-not [string]::IsNullOrEmpty($Version)) {
            try {
                $Release = Invoke-RestMethod -Uri "$ApiBase/releases/tags/$Tag" -Headers @{ "User-Agent" = "sentinel-installer" }
            } catch {
                Write-Fail "Error: release tag '$Tag' not found on '$Repo'."
                exit 1
            }
            $Asset = $Release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
        }
    }
    if ($null -eq $Asset) {
        Write-Fail "Error: asset '$AssetName' not found in release '$Tag'."
        if ($Release.assets) {
            Write-Fail "Available assets:"
            $Release.assets | ForEach-Object { Write-Fail "  $($_.name)" }
        }
        Write-Warn "Tip: no GitHub release yet? Use a local build:"
        Write-Warn "  powershell -File install.ps1 -LocalBuild target\release\sentinel.exe"
        exit 1
    }

    Write-Step "Downloading $($Asset.name)..."
    $ZipPath = Join-Path $InstallDir "sentinel.zip"
    Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ZipPath
    Expand-Archive -Path $ZipPath -DestinationPath $InstallDir -Force
    Remove-Item -Path $ZipPath -Force
} else {
    if (-not (Test-Path $LocalBuild)) {
        Write-Fail "Error: local build not found at '$LocalBuild'."
        Write-Fail "       Build it first:  cargo build --release --path crates/interfaces/sentinel-cli"
        exit 1
    }
    if (-not (Test-Path $InstallDir)) { New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null }
    Copy-Item -Path $LocalBuild -Destination (Join-Path $InstallDir "sentinel.exe") -Force
    Write-Ok "Copied local build to $InstallDir\sentinel.exe"
}

$Exe = Join-Path $InstallDir "sentinel.exe"
if (-not (Test-Path $Exe)) {
    Write-Fail "Error: $Exe not found after install."
    exit 1
}

# -- 2. Write default global config (~/.sentinel/sentinel.toml) -------------
if (-not $SkipConfig) {
    $ConfigDir = Join-Path $env:USERPROFILE ".sentinel"
    $ConfigPath = Join-Path $ConfigDir "sentinel.toml"
    if (-not (Test-Path $ConfigPath)) {
        New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
        $DefaultConfig = @'
# Sentinel global configuration (created by install.ps1).
# Config priority: ./sentinel.toml > ./config.toml > ./.sentinel.toml,
# then this global file ($SENTINEL_HOME/sentinel.toml or ~/.sentinel/sentinel.toml).

[agent]
default_model = "gpt-4o-mini"
max_turns = 50
max_iterations = 100
yolo_mode = false
verbose = false

# Providers auto-enable from environment variables (.env file or your shell):
#   OPENAI_API_KEY, ANTHROPIC_API_KEY, GOOGLE_AI_STUDIO_API_KEY,
#   DEEPSEEK_API_KEY, OPENROUTER_API_KEY, NVIDIA_NIM_API_KEY
'@
        [System.IO.File]::WriteAllText($ConfigPath, $DefaultConfig, (New-Object System.Text.UTF8Encoding($false)))
        Write-Ok "Wrote default config: $ConfigPath"
    } else {
        Write-Warn "Config already exists, leaving it untouched: $ConfigPath"
    }
}

# -- 3. Add install dir to user PATH ----------------------------------------
if (-not $SkipPath) {
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        $NewPath = if ([string]::IsNullOrEmpty($UserPath)) { $InstallDir } else { "$UserPath;$InstallDir" }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        Write-Ok "Added $InstallDir to user PATH (new terminals only; current session unchanged)."
    } else {
        Write-Warn "$InstallDir already on user PATH."
    }
} else {
    Write-Warn "PATH update skipped (-SkipPath)."
}

# -- 4. Optional VS Code extension registration ----------------------------
if ($InstallVSCode) {
    if ([string]::IsNullOrEmpty($VsixPath)) {
        $VsixCandidates = @(Join-Path $env:USERPROFILE ".sentinel\extensions\*.vsix")
        if (-not [string]::IsNullOrEmpty($PSScriptRoot)) {
            $VsixCandidates += Join-Path $PSScriptRoot "extensions\*.vsix"
        }
        $VsixPath = $VsixCandidates | ForEach-Object { Get-ChildItem -Path $_ -ErrorAction SilentlyContinue } |
                    Select-Object -First 1 -ExpandProperty FullName
    }
    $Code = Get-Command code -ErrorAction SilentlyContinue
    if (-not [string]::IsNullOrEmpty($VsixPath) -and $null -ne $Code) {
        Write-Step "Installing VS Code extension $VsixPath ..."
        & $Code.Source --install-extension $VsixPath
        if ($LASTEXITCODE -eq 0) { Write-Ok "VS Code extension installed." } else { Write-Warn "VS Code extension install failed (exit $LASTEXITCODE)." }
    } else {
        Write-Warn "No VS Code extension found, skipping (extension ships in a later release)."
    }
} else {
    Write-Warn "VS Code extension registration skipped (pass -InstallVSCode [-VsixPath <file.vsix>] to enable)."
}

# -- 5. Verify --------------------------------------------------------------
Write-Step "Verifying install..."
& $Exe --version
if ($LASTEXITCODE -ne 0) {
    Write-Fail "Error: installed binary failed to run (exit $LASTEXITCODE)."
    exit 1
}

Write-Ok "Sentinel installed successfully."
Write-Host "  binary: $Exe"
Write-Host "  next:   open a new terminal, then run 'sentinel ai'"

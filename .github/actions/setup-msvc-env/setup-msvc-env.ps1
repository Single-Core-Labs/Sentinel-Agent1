#!/usr/bin/env pwsh
# ---------------------------------------------------------------------------
# Setup MSVC build environment for Windows CI.
#
# Installs Visual Studio 2025 Build Tools with the C++ workload via
# winget-cli, then invokes vcvars64.bat to set up the environment
# variables that Rust's MSVC targets require (PATH, INCLUDE, LIB).
#
# This script is idempotent — safe to run on self-hosted runners that
# already have VS installed.
# ---------------------------------------------------------------------------
$ErrorActionPreference = "Stop"

Write-Host "[setup-msvc-env] Configuring MSVC environment…"

# Detect existing VS installation
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) {
    Write-Host "[setup-msvc-env] vswhere not found; installing VS Build Tools…"

    # Install VS 2025 Build Tools with C++ workload
    # Uses the official vs_BuildTools.exe bootstrapper
    $vsBootstrapper = "$env:TEMP\vs_BuildTools.exe"
    $vsUrl = "https://aka.ms/vs/17/release/vs_BuildTools.exe"

    Write-Host "[setup-msvc-env] Downloading VS Build Tools bootstrapper…"
    Invoke-WebRequest -Uri $vsUrl -OutFile $vsBootstrapper

    Write-Host "[setup-msvc-env] Installing VS Build Tools (C++ workload)…"
    $args = @(
        "--quiet",
        "--norestart",
        "--wait",
        "--add", "Microsoft.VisualStudio.Workload.VCTools",
        "--includeRecommended"
    )
    $proc = Start-Process -FilePath $vsBootstrapper -ArgumentList $args -NoNewWindow -Wait -PassThru
    if ($proc.ExitCode -ne 0 -and $proc.ExitCode -ne 3010) {
        throw "VS Build Tools installation failed with exit code $($proc.ExitCode)"
    }

    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
}

# Locate the VS installation path
$vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsPath) {
    throw "No Visual Studio installation with C++ tools found"
}

Write-Host "[setup-msvc-env] VS installation: $vsPath"

# Invoke vcvars64.bat and capture the environment
$vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vcvars)) {
    throw "vcvars64.bat not found at $vcvars"
}

# Use cmd to run vcvars then print the environment
$envDump = & cmd /c "`"$vcvars`" > nul && set"
foreach ($line in $envDump) {
    $parts = $line.Split('=', 2)
    if ($parts.Length -eq 2) {
        $name = $parts[0]
        $value = $parts[1]
        # Update PATH, INCLUDE, LIB, and any other environment variables
        if ($name -in @('PATH', 'INCLUDE', 'LIB', 'LIBPATH', 'UCRTVersion', 'WindowsSdkVersion', 'VCToolsVersion')) {
            [System.Environment]::SetEnvironmentVariable($name, $value, [System.EnvironmentVariableTarget]::Process)
        }
    }
}

Write-Host "[setup-msvc-env] MSVC environment configured successfully"

# Verify
$cl = Get-Command cl.exe -ErrorAction SilentlyContinue
if ($cl) {
    Write-Host "[setup-msvc-env] cl.exe: $($cl.Source)"
} else {
    Write-Warning "[setup-msvc-env] cl.exe not found in PATH after vcvars"
}

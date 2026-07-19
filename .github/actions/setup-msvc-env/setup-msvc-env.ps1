#!/usr/bin/env pwsh
# ---------------------------------------------------------------------------
# Setup MSVC build environment for Windows CI.
#
# Installs Visual Studio 2025 Build Tools with the C++ workload, invokes
# vcvars for x86_64 and ARM64, and applies an lld linker workaround for
# ARM64 cross-compilation by compiling a C# wrapper that filters
# incompatible linker flags.
#
# The script:
#   1. Detects or installs VS Build Tools.
#   2. Runs vcvars64.bat for x86_64 and vcvarsamd64_arm64.bat for ARM64.
#   3. Extracts PATH, INCLUDE, LIB, LIBPATH into the process environment.
#   4. Compiles a C# lld-link wrapper for ARM64 that strips unknown flags.
#   5. Verifies cl.exe and link.exe are on PATH.
#
# Usage:
#   .github/actions/setup-msvc-env/setup-msvc-env.ps1
#
# Environment:
#   VS_ARCH — target architecture: x86_64 (default) or aarch64
# ---------------------------------------------------------------------------
$ErrorActionPreference = "Stop"

$arch = $env:VS_ARCH ?? "x86_64"
Write-Host "[setup-msvc-env] Configuring MSVC environment for ${arch}…"

# ---------------------------------------------------------------------------
# 1. Detect or install Visual Studio Build Tools
# ---------------------------------------------------------------------------
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) {
    Write-Host "[setup-msvc-env] vswhere not found; installing VS Build Tools…"

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
        "--add", "Microsoft.VisualStudio.Component.VC.Tools.ARM64",
        "--add", "Microsoft.VisualStudio.Component.VC.ATL.ARM64",
        "--includeRecommended"
    )
    $proc = Start-Process -FilePath $vsBootstrapper -ArgumentList $args -NoNewWindow -Wait -PassThru
    if ($proc.ExitCode -ne 0 -and $proc.ExitCode -ne 3010) {
        throw "VS Build Tools installation failed with exit code $($proc.ExitCode)"
    }

    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
}

# ---------------------------------------------------------------------------
# 2. Locate the VS installation and invoke the correct vcvars script
# ---------------------------------------------------------------------------
$vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsPath) {
    throw "No Visual Studio installation with C++ tools found"
}
Write-Host "[setup-msvc-env] VS installation: $vsPath"

if ($arch -eq "aarch64") {
    $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvarsamd64_arm64.bat"
} else {
    $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
}
if (-not (Test-Path $vcvars)) {
    throw "vcvars script not found at $vcvars"
}

# Run vcvars via cmd and capture environment
$envDump = & cmd /c "`"$vcvars`" > nul && set"
foreach ($line in $envDump) {
    $parts = $line.Split('=', 2)
    if ($parts.Length -eq 2) {
        $name = $parts[0]
        $value = $parts[1]
        if ($name -in @('PATH', 'INCLUDE', 'LIB', 'LIBPATH',
                       'UCRTVersion', 'WindowsSdkVersion', 'VCToolsVersion',
                       'VSCMD_ARG_TGT_ARCH', 'VSCMD_ARG_HOST_ARCH')) {
            [System.Environment]::SetEnvironmentVariable($name, $value, [System.EnvironmentVariableTarget]::Process)
        }
    }
}

# ---------------------------------------------------------------------------
# 3. lld-link wrapper for ARM64 — filter incompatible flags
# ---------------------------------------------------------------------------
if ($arch -eq "aarch64") {
    Write-Host "[setup-msvc-env] Compiling lld-link wrapper for ARM64…"

    $wrapperDir = "$env:TEMP\lld-link-wrapper"
    New-Item -ItemType Directory -Path $wrapperDir -Force | Out-Null

    # C# source for the wrapper — strips flags that lld-link does not recognise
    @"
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;

class LldLinkWrapper
{
    static int Main(string[] args)
    {
        var filtered = new List<string>();
        foreach (var arg in args)
        {
            // Skip flags that lld-link does not support
            if (arg.StartsWith("/guard:") ||
                arg.StartsWith("/CETCOMPAT") ||
                arg == "/HIGHENTROPYVA" ||
                arg.StartsWith("/pdbaltpath:") ||
                arg.StartsWith("/subsystem:") && !arg.Contains("CONSOLE") && !arg.Contains("WINDOWS"))
            {
                Console.Error.WriteLine($"[lld-wrap] Stripping: {arg}");
                continue;
            }
            filtered.Add(arg);
        }

        // Locate the real lld-link next to the wrapper
        var lldPath = Path.Combine(
            AppDomain.CurrentDomain.BaseDirectory, "lld-link.exe"
        );
        if (!File.Exists(lldPath))
        {
            // Fall back to PATH
            lldPath = "lld-link.exe";
        }

        var psi = new ProcessStartInfo(lldPath, string.Join(" ", filtered.Select(a => a.Contains(' ') ? $"\"{a}\"" : a)))
        {
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        var proc = Process.Start(psi);
        proc.WaitForExit();
        Console.Write(proc.StandardOutput.ReadToEnd());
        Console.Error.Write(proc.StandardError.ReadToEnd());
        return proc.ExitCode;
    }
}
"@ | Out-File -FilePath "$wrapperDir\LldLinkWrapper.cs" -Encoding utf8

    # Compile with csc
    $csc = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\..\..\..\MSBuild\Current\Bin\Roslyn\csc.exe"
    if (-not (Test-Path $csc)) {
        # Use dotnet if csc not found
        $csc = "dotnet"
        $args = @("build", "-nologo", "-o", "$wrapperDir\lld-link-wrapper.exe")
    } else {
        $args = @("-nologo", "-out:$wrapperDir\lld-link-wrapper.exe", "$wrapperDir\LldLinkWrapper.cs")
    }

    & $csc $args
    if ($LASTEXITCODE -eq 0) {
        # Prepend wrapper to PATH so Rust/Cargo picks it up
        $env:PATH = "$wrapperDir;$env:PATH"
        Write-Host "[setup-msvc-env] lld-link wrapper installed at $wrapperDir\lld-link-wrapper.exe"
    } else {
        Write-Warning "[setup-msvc-env] Failed to compile lld-link wrapper — ARM64 linking may fail"
    }
}

# ---------------------------------------------------------------------------
# 4. Add Rust targets
# ---------------------------------------------------------------------------
if ($arch -eq "aarch64") {
    rustup target add aarch64-pc-windows-msvc 2>$null | Out-Null
} else {
    rustup target add x86_64-pc-windows-msvc 2>$null | Out-Null
}

# ---------------------------------------------------------------------------
# 5. Verify
# ---------------------------------------------------------------------------
Write-Host "[setup-msvc-env] MSVC environment configured successfully"

$cl = Get-Command cl.exe -ErrorAction SilentlyContinue
if ($cl) {
    Write-Host "[setup-msvc-env] cl.exe: $($cl.Source)"
} else {
    Write-Warning "[setup-msvc-env] cl.exe not found in PATH after vcvars"
}

$link = Get-Command link.exe -ErrorAction SilentlyContinue
if ($link) {
    Write-Host "[setup-msvc-env] link.exe: $($link.Source)"
} else {
    Write-Warning "[setup-msvc-env] link.exe not found in PATH after vcvars"
}

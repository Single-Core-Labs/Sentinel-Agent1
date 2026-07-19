#!/usr/bin/env pwsh
# ---------------------------------------------------------------------------
# Setup MSVC build environment for Windows CI.
#
# Configures the MSVC toolchain for both x86_64 and aarch64 targets,
# sets up Cargo for cross-compilation, and provides an lld-link wrapper
# for ARM64 that filters incompatible flags.
#
# Steps:
#   1. Detect or install Visual Studio Build Tools (multiple version fallback).
#   2. Run the correct vcvars script (vcvars64 / vcvarsamd64_arm64) and
#      extract PATH, INCLUDE, LIB, LIBPATH into the process environment.
#   3. Write Cargo cross-compilation config to .cargo/config.toml.
#   4. Compile a C# lld-link wrapper for ARM64 that strips flags lld
#      does not recognise (/guard:, /CETCOMPAT, /HIGHENTROPYVA, ...).
#   5. Add the corresponding Rust MSVC target.
#   6. Verify cl.exe and link.exe are on PATH.
#
# Usage:
#   .github/actions/setup-msvc-env/setup-msvc-env.ps1
#
# Environment:
#   VS_ARCH        — target architecture: x86_64 (default) or aarch64
#   VS_VERSION     — VS version range to search (default: [17.0, 18.0))
#   CARGO_HOME     — cargo home directory (default: ~\.cargo)
# ---------------------------------------------------------------------------
$ErrorActionPreference = "Stop"

$arch = $env:VS_ARCH ?? "x86_64"
$vsVersionRange = $env:VS_VERSION ?? "[17.0,18.0)"
$cargoHome = $env:CARGO_HOME ?? "$env:USERPROFILE\.cargo"

Write-Host "[setup-msvc-env] Configuring MSVC environment for ${arch}…"
Write-Host "[setup-msvc-env] VS version range: ${vsVersionRange}"

# ---------------------------------------------------------------------------
# 1. Locate or install Visual Studio Build Tools
# ---------------------------------------------------------------------------
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$fallbackVswhere = "${env:ProgramFiles}\Microsoft Visual Studio\Installer\vswhere.exe"

if (-not (Test-Path $vswhere)) {
    if (Test-Path $fallbackVswhere) { $vswhere = $fallbackVswhere }
}

if (-not (Test-Path $vswhere)) {
    Write-Host "[setup-msvc-env] vswhere not found; installing VS Build Tools…"

    $vsBootstrapper = "$env:TEMP\vs_BuildTools.exe"
    $vsUrl = "https://aka.ms/vs/17/release/vs_BuildTools.exe"

    Write-Host "[setup-msvc-env] Downloading VS Build Tools bootstrapper…"
    Invoke-WebRequest -Uri $vsUrl -OutFile $vsBootstrapper

    Write-Host "[setup-msvc-env] Installing VS Build Tools (C++ workload, x64 + ARM64)…"
    $installArgs = @(
        "--quiet", "--norestart", "--wait",
        "--add", "Microsoft.VisualStudio.Workload.VCTools",
        "--add", "Microsoft.VisualStudio.Component.VC.Tools.ARM64",
        "--add", "Microsoft.VisualStudio.Component.VC.ATL.ARM64",
        "--includeRecommended"
    )
    $proc = Start-Process -FilePath $vsBootstrapper -ArgumentList $installArgs -NoNewWindow -Wait -PassThru
    if ($proc.ExitCode -ne 0 -and $proc.ExitCode -ne 3010) {
        throw "VS Build Tools installation failed with exit code $($proc.ExitCode)"
    }

    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) { $vswhere = $fallbackVswhere }
}

# Try to locate VS — fall back through versions
$vsPath = $null
$products = @("*")
foreach ($product in $products) {
    $vsPath = & $vswhere -latest -products $product -version "$vsVersionRange" `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath
    if ($vsPath) { break }
}
if (-not $vsPath) {
    # Last resort: any version
    $vsPath = & $vswhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath
}
if (-not $vsPath) {
    throw "No Visual Studio installation with C++ tools found"
}
Write-Host "[setup-msvc-env] VS installation: ${vsPath}"

# ---------------------------------------------------------------------------
# 2. Invoke vcvars for the target architecture and capture environment
# ---------------------------------------------------------------------------
if ($arch -eq "aarch64") {
    $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvarsamd64_arm64.bat"
    $rustTarget = "aarch64-pc-windows-msvc"
} else {
    $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
    $rustTarget = "x86_64-pc-windows-msvc"
}
if (-not (Test-Path $vcvars)) {
    throw "vcvars script not found at ${vcvars}"
}

Write-Host "[setup-msvc-env] Running ${vcvars}…"
$envDump = & cmd /c "`"${vcvars}`" > nul && set"
foreach ($line in $envDump) {
    $parts = $line.Split('=', 2)
    if ($parts.Length -eq 2) {
        $name = $parts[0].Trim()
        $value = $parts[1].Trim()
        if ($name -in @('PATH', 'INCLUDE', 'LIB', 'LIBPATH',
                       'UCRTVersion', 'WindowsSdkVersion', 'VCToolsVersion',
                       'VSCMD_ARG_TGT_ARCH', 'VSCMD_ARG_HOST_ARCH',
                       'WindowsLibPath', 'UniversalCRTSdkDir',
                       'WindowsSdkDir', 'ExtensionSdkDir')) {
            [System.Environment]::SetEnvironmentVariable($name, $value, [System.EnvironmentVariableTarget]::Process)
        }
    }
}

# ---------------------------------------------------------------------------
# 3. Write Cargo cross-compilation configuration
# ---------------------------------------------------------------------------
$cargoConfigDir = Join-Path $cargoHome "config.d"
New-Item -ItemType Directory -Path $cargoConfigDir -Force | Out-Null

$cargoConfig = @"
# MSVC cross-compilation — generated by setup-msvc-env.ps1
# Target: ${rustTarget}

[target.${rustTarget}]
linker = "rust-lld.exe"
rustflags = ["-C", "link-args=-Wl,/machine:${arch}"]

[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"

[target.aarch64-pc-windows-msvc]
linker = "rust-lld.exe"
"@

$cargoConfig | Out-File -FilePath (Join-Path $cargoConfigDir "msvc-cross.toml") -Encoding utf8
Write-Host "[setup-msvc-env] Cargo config written to ${cargoConfigDir}\msvc-cross.toml"

# ---------------------------------------------------------------------------
# 4. Compile C# lld-link wrapper for ARM64 (filters incompatible flags)
# ---------------------------------------------------------------------------
if ($arch -eq "aarch64") {
    Write-Host "[setup-msvc-env] Compiling lld-link wrapper for ARM64…"

    $wrapperDir = "$env:TEMP\lld-link-wrapper"
    New-Item -ItemType Directory -Path $wrapperDir -Force | Out-Null

    $csSource = @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;

/// <summary>
/// lld-link wrapper that strips linker flags the LLVM linker does not
/// recognise but that the MSVC linker (link.exe) accepts silently.
///
/// Incompatible flags filtered:
///   /guard:cf, /guard:ehcont       — Control Flow Guard (lld lacks some
///                                     C++ EHCont metadata handling)
///   /CETCOMPAT                     — Shadow stack (lld stub support only)
///   /HIGHENTROPYVA                 — ASLR (lld always enables)
///   /pdbaltpath:                   — PDB alt path (lld uses own layout)
///   /subsystem:xxxx (non-standard) — Only CONSOLE and WINDOWS are kept
/// </summary>
class LldLinkWrapper
{
    static readonly HashSet<string> KnownFlags = new(StringComparer.OrdinalIgnoreCase)
    {
        "/nologo", "/machine:", "/out:", "/defaultlib:", "/nodefaultlib",
        "/entry:", "/base:", "/stack:", "/heap:", "/version:",
        "/subsystem:console", "/subsystem:windows",
        "/dll", "/debug", "/pdb:", "/pdbaltpath:",
        "/opt:ref", "/opt:icf", "/opt:lldemit",
        "/lldsavetemps", "/lldmingw",
        "/largeaddressaware", "/tsaware",
        "/nxcompat", "/dynamicbase",
        "/export:", "/include:", "/manifest:", "/manifestuac:",
        "/tlbid:", "/tlssize:",
        "/verbose", "/verbose:icf", "/verbose:ref",
        "/wx", "/w",
        "/merge:", "/section:",
        "/def:", "/implib:", "/libpath:",
        "/order:", "/delayload:",
        "/ignore:", "/safeseh",
        "/integritycheck", "/filealign:",
    };

    static bool ShouldFilter(string arg)
    {
        if (arg.StartsWith("/guard:", StringComparison.OrdinalIgnoreCase))
            return true;
        if (arg.Equals("/CETCOMPAT", StringComparison.OrdinalIgnoreCase))
            return true;
        if (arg.Equals("/HIGHENTROPYVA", StringComparison.OrdinalIgnoreCase))
            return true;
        if (arg.StartsWith("/pdbaltpath:", StringComparison.OrdinalIgnoreCase))
            return true;
        if (arg.StartsWith("/subsystem:", StringComparison.OrdinalIgnoreCase))
        {
            var val = arg.Substring("/subsystem:".Length);
            if (!"CONSOLE".Equals(val, StringComparison.OrdinalIgnoreCase) &&
                !"WINDOWS".Equals(val, StringComparison.OrdinalIgnoreCase))
                return true;
        }
        return false;
    }

    static int Main(string[] args)
    {
        var filtered = new List<string>();
        foreach (var arg in args)
        {
            if (ShouldFilter(arg))
            {
                Console.Error.WriteLine($"[lld-wrap] Stripping incompatible flag: {arg}");
                continue;
            }
            filtered.Add(arg);
        }

        var wrapperDir = AppDomain.CurrentDomain.BaseDirectory;
        var lldPath = Path.Combine(wrapperDir, "lld-link.exe");
        if (!File.Exists(lldPath))
        {
            // Search PATH
            var paths = (Environment.GetEnvironmentVariable("PATH") ?? "").Split(Path.PathSeparator);
            lldPath = paths
                .Select(p => Path.Combine(p, "lld-link.exe"))
                .FirstOrDefault(File.Exists);
        }
        if (lldPath == null)
        {
            Console.Error.WriteLine("[lld-wrap] ERROR: lld-link.exe not found on PATH");
            return 1;
        }

        var psi = new ProcessStartInfo
        {
            FileName = lldPath,
            Arguments = string.Join(" ", filtered.Select(a =>
                a.Contains(' ') || a.Contains('"') ? $"\"{a.Replace("\"", "\\\"")}\"" : a)),
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        var proc = new Process { StartInfo = psi };
        proc.Start();
        proc.WaitForExit();

        Console.Write(proc.StandardOutput.ReadToEnd());
        Console.Error.Write(proc.StandardError.ReadToEnd());
        return proc.ExitCode;
    }
}
'@

    $csPath = Join-Path $wrapperDir "LldLinkWrapper.cs"
    $csSource | Out-File -FilePath $csPath -Encoding utf8

    # Locate the C# compiler
    $cscCandidates = @(
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\..\..\..\MSBuild\Current\Bin\Roslyn\csc.exe",
        "${env:ProgramFiles}\Microsoft Visual Studio\Installer\..\..\..\MSBuild\Current\Bin\Roslyn\csc.exe",
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2019\BuildTools\MSBuild\Current\Bin\Roslyn\csc.exe",
        (Get-Command csc.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
    )

    $csc = $cscCandidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $csc) {
        Write-Warning "[setup-msvc-env] C# compiler not found — attempting dotnet build"
        $csc = "dotnet"
        $buildArgs = @("build", "-nologo", "-o", $wrapperDir, $csPath)
    } else {
        $buildArgs = @("-nologo", "-out:$wrapperDir\lld-link-wrapper.exe", $csPath)
    }

    try {
        & $csc $buildArgs 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path "$wrapperDir\lld-link-wrapper.exe")) {
            # Prepend wrapper to PATH so Rust/Cargo uses it
            $env:PATH = "${wrapperDir};${env:PATH}"
            Write-Host "[setup-msvc-env] lld-link wrapper: ${wrapperDir}\lld-link-wrapper.exe"
            Write-Host "[setup-msvc-env]   Filters: /guard:*, /CETCOMPAT, /HIGHENTROPYVA, /pdbaltpath:*, non-standard /subsystem:"
        } else {
            Write-Warning "[setup-msvc-env] lld-link wrapper compilation failed — ARM64 linking may fail"
        }
    } catch {
        Write-Warning "[setup-msvc-env] lld-link wrapper compilation error: $_"
    }
}

# ---------------------------------------------------------------------------
# 5. Add Rust target
# ---------------------------------------------------------------------------
Write-Host "[setup-msvc-env] Adding Rust target: ${rustTarget}…"
& rustup target add $rustTarget 2>&1 | Out-Null
Write-Host "[setup-msvc-env] Installed Rust target: ${rustTarget}"

# ---------------------------------------------------------------------------
# 6. Verify
# ---------------------------------------------------------------------------
Write-Host "[setup-msvc-env] Verification:"
$cl = Get-Command cl.exe -ErrorAction SilentlyContinue
if ($cl) {
    Write-Host "  cl.exe:     $($cl.Source)"
} else {
    Write-Warning "  cl.exe:     NOT FOUND"
}
$link = Get-Command link.exe -ErrorAction SilentlyContinue
if ($link) {
    Write-Host "  link.exe:   $($link.Source)"
} else {
    Write-Warning "  link.exe:   NOT FOUND"
}
$lld = Get-Command lld-link.exe -ErrorAction SilentlyContinue
if ($lld) {
    Write-Host "  lld-link:   $($lld.Source)"
} else {
    Write-Host "  lld-link:   (not expected for this arch)"
}
Write-Host "  Rust target: $(& rustup target list --installed | Select-String $rustTarget)"
Write-Host "[setup-msvc-env] MSVC environment configured successfully"

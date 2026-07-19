#!/usr/bin/env pwsh
# ---------------------------------------------------------------------------
# Provision a fast Dev Drive on Windows CI for I/O-intensive build processes.
#
# Dev Drive uses the ReFS filesystem with optimisations for developer
# workloads (4 KB allocation unit, integrity streams disabled, large FRS).
# Build caches for Rust (cargo), Bazel, npm, pip/uv, and sccache are
# relocated onto the Dev Drive via directory junctions.
#
# If a VHDX already exists from a previous run, it is re-attached instead
# of created from scratch (preserving cached build artifacts across CI
# workflow runs where the runner is reused).
#
# Usage:
#   .github/scripts/setup-dev-drive.ps1
#
# Environment:
#   DEV_DRIVE_LETTER  — drive letter (default: D)
#   DEV_DRIVE_SIZE_GB — size in GB (default: 50)
#   VHDX_PATH         — path to VHDX file (default: C:\DevDrive_D.vhdx)
#   CARGO_HOME        — cargo home directory (default: ~\.cargo)
#   SCCACHE_DIR       — sccache directory (default: ~\AppData\Local\sccache)
# ---------------------------------------------------------------------------
$ErrorActionPreference = "Stop"

$driveLetter    = $env:DEV_DRIVE_LETTER ?? "D"
$sizeGB         = $env:DEV_DRIVE_SIZE_GB ?? 50
$vhdxPath       = $env:VHDX_PATH ?? "C:\DevDrive_${driveLetter}.vhdx"
$cargoHome      = $env:CARGO_HOME ?? "$env:USERPROFILE\.cargo"
$sccacheDir     = $env:SCCACHE_DIR ?? "$env:LOCALAPPDATA\sccache"
$mountPoint     = "${driveLetter}:\"

Write-Host "[setup-dev-drive] === Dev Drive Provisioning ==="
Write-Host "  Drive letter: ${driveLetter}:"
Write-Host "  Size:         ${sizeGB} GB"
Write-Host "  VHDX path:    ${vhdxPath}"
Write-Host "  Mount point:  ${mountPoint}"

# ---------------------------------------------------------------------------
# 1. Create or attach the Dev Drive VHDX
# ---------------------------------------------------------------------------
$driveReady = $false

if (Test-Path $mountPoint) {
    Write-Host "[setup-dev-drive] Drive ${driveLetter}: already mounted — reusing"
    $driveReady = $true
}
elseif (Test-Path $vhdxPath) {
    Write-Host "[setup-dev-drive] VHDX exists at ${vhdxPath} — attaching…"
    try {
        $disk = Mount-VHD -Path $vhdxPath -PassThru -ErrorAction Stop
        $partition = $disk | Get-Disk | Where-Object { $_.OperationalStatus -eq "OK" } | Get-Partition -DriveLetter $driveLetter -ErrorAction SilentlyContinue
        if (-not $partition) {
            $disk | Get-Disk | Where-Object { $_.OperationalStatus -eq "OK" } | Get-Partition | Set-Partition -NewDriveLetter $driveLetter
        }
        $driveReady = $true
        Write-Host "[setup-dev-drive] VHDX attached at ${mountPoint}"
    } catch {
        Write-Warning "[setup-dev-drive] Failed to attach existing VHDX: $_"
        Write-Host "[setup-dev-drive] Will create a new one instead"
    }
}

if (-not $driveReady) {
    Write-Host "[setup-dev-drive] Creating new Dev Drive VHDX (${sizeGB} GB)…"
    try {
        $volume = New-VirtualDisk `
            -FriendlyName "DevDrive" `
            -Path $vhdxPath `
            -SizeBytes ($sizeGB * 1GB) `
            -BlockSizeBytes 4KB `
            -PhysicalSectorSizeBytes 4KB | `
            Initialize-Disk -PartitionStyle GPT -PassThru | `
            New-Partition -UseMaximumSize -AssignDriveLetter -DriveLetter $driveLetter | `
            Format-Volume `
                -FileSystem ReFS `
                -AllocationUnitSize 4KB `
                -SetIntegrityStreams $false `
                -UseLargeFRS `
                -Force
        $driveReady = $true
        Write-Host "[setup-dev-drive] Dev Drive created at ${mountPoint}"
    } catch {
        Write-Error "[setup-dev-drive] Failed to create Dev Drive: $_"
        exit 1
    }
}

# Verify mount
if (-not (Test-Path $mountPoint)) {
    Write-Error "[setup-dev-drive] Drive ${driveLetter}: not accessible after setup"
    exit 1
}

# Verify ReFS
$fsInfo = Get-Volume -DriveLetter $driveLetter
Write-Host "[setup-dev-drive] Filesystem: $($fsInfo.FileSystem) — $([math]::Round($fsInfo.Size / 1GB, 1)) GB total, $([math]::Round($fsInfo.SizeRemaining / 1GB, 1)) GB free"

# ---------------------------------------------------------------------------
# 2. Relocate build caches to Dev Drive
# ---------------------------------------------------------------------------
$targets = @(
    @{Source = "$cargoHome\registry";   Name = "Cargo registry";    DestDir = "${mountPoint}cargo-registry"}
    @{Source = "$cargoHome\git";        Name = "Cargo git";         DestDir = "${mountPoint}cargo-git"}
    @{Source = "$cargoHome\index";      Name = "Cargo index";       DestDir = "${mountPoint}cargo-index"}
    @{Source = "$env:USERPROFILE\_builder\cache"; Name = "Bazel cache"; DestDir = "${mountPoint}bazel-cache"}
    @{Source = "$sccacheDir";           Name = "sccache";           DestDir = "${mountPoint}sccache"}
)

# npm/pip/uv caches — only if directories exist
if (Test-Path "$env:APPDATA\npm-cache") {
    $targets += @{Source = "$env:APPDATA\npm-cache"; Name = "npm cache"; DestDir = "${mountPoint}npm-cache"}
}
if (Test-Path "$env:LOCALAPPDATA\pip\cache") {
    $targets += @{Source = "$env:LOCALAPPDATA\pip\cache"; Name = "pip cache"; DestDir = "${mountPoint}pip-cache"}
}
if (Test-Path "$env:USERPROFILE\.cache\uv") {
    $targets += @{Source = "$env:USERPROFILE\.cache\uv"; Name = "uv cache"; DestDir = "${mountPoint}uv-cache"}
}

foreach ($t in $targets) {
    $srcDir = $t.Source
    $dstDir = $t.DestDir
    $name   = $t.Name

    New-Item -ItemType Directory -Path $dstDir -Force | Out-Null

    # If source is a junction already pointing to our destination, skip
    if (Test-Path $srcDir) {
        $item = Get-Item $srcDir -Force -ErrorAction SilentlyContinue
        if ($item.LinkType -eq "Junction" -and $item.Target -eq $dstDir) {
            Write-Host "[setup-dev-drive]  ✓ ${name}: already linked"
            continue
        }
        Remove-Item -Path $srcDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    New-Item -ItemType Junction -Path $srcDir -Target $dstDir -Force | Out-Null
    Write-Host "[setup-dev-drive]  ✓ ${name}: ${srcDir} → ${dstDir}"
}

# ---------------------------------------------------------------------------
# 3. Set environment variables pointing to Dev Drive paths
# ---------------------------------------------------------------------------
$env:CARGO_HOME = $cargoHome
$env:SCCACHE_DIR = $sccacheDir
$env:BAZEL_DISK_CACHE = "${mountPoint}bazel-cache"

# Ensure Bazel uses the Dev Drive for its disk cache
$bazelRcPath = "$env:USERPROFILE\.bazelrc"
if (-not (Select-String -Path $bazelRcPath -Pattern "disk_cache" -Quiet -ErrorAction SilentlyContinue)) {
    "build --disk_cache=${mountPoint}bazel-cache" | Out-File -FilePath $bazelRcPath -Append -Encoding utf8
    Write-Host "[setup-dev-drive]  ✓ Bazel disk_cache written to ~\.bazelrc"
}

# ---------------------------------------------------------------------------
# 4. Summary
# ---------------------------------------------------------------------------
$totalSize = Get-ChildItem -Path $mountPoint -Recurse -Force -ErrorAction SilentlyContinue |
    Measure-Object -Property Length -Sum -ErrorAction SilentlyContinue
$cachedGB = [math]::Round(($totalSize.Sum / 1GB), 2)

Write-Host "[setup-dev-drive] === Dev Drive Setup Complete ==="
Write-Host "  Mount:  ${mountPoint}"
Write-Host "  VHDX:   ${vhdxPath}"
Write-Host "  Cached: ${cachedGB} GB across $($targets.Count) locations"

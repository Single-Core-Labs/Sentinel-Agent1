#!/usr/bin/env pwsh
# ---------------------------------------------------------------------------
# Set up a fast Dev Drive on Windows CI for improved build performance.
#
# Dev Drive uses the ReFS filesystem and is optimised for developer
# workloads.  This script creates a Dev Drive on the CI runner and
# relocates the Rust and Bazel build caches onto it.
#
# Usage:
#   .github/scripts/setup-dev-drive.ps1
#
# Environment:
#   DEV_DRIVE_LETTER — drive letter (default: D)
#   DEV_DRIVE_SIZE_GB — size in GB (default: 50)
# ---------------------------------------------------------------------------
$ErrorActionPreference = "Stop"

$driveLetter = $env:DEV_DRIVE_LETTER ?? "D"
$sizeGB = $env:DEV_DRIVE_SIZE_GB ?? 50
$mountPoint = "${driveLetter}:\"

Write-Host "[setup-dev-drive] Creating Dev Drive ${driveLetter}: (${sizeGB} GB)…"

# Check if Dev Drive already exists
if (Test-Path $mountPoint) {
    Write-Host "[setup-dev-drive] Drive ${driveLetter}: already exists — reusing"
} else {
    # Create VHDX and format as Dev Drive (ReFS with dev drive optimizations)
    $vhdxPath = "C:\DevDrive_${driveLetter}.vhdx"
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

    Write-Host "[setup-dev-drive] Dev Drive created at ${mountPoint}"
}

# Relocate build caches to Dev Drive
$targets = @(
    @{Source = "$env:USERPROFILE\.cargo\registry"; Destination = "${mountPoint}cargo-registry"},
    @{Source = "$env:USERPROFILE\.cargo\git";     Destination = "${mountPoint}cargo-git"},
    @{Source = "$env:USERPROFILE\_builder\cache";  Destination = "${mountPoint}bazel-cache"}
)

foreach ($t in $targets) {
    $dstDir = $t.Destination
    if (-not (Test-Path $dstDir)) {
        New-Item -ItemType Directory -Path $dstDir -Force | Out-Null
    }
    if (Test-Path $t.Source) {
        Remove-Item -Path $t.Source -Recurse -Force -ErrorAction SilentlyContinue
    }
    New-Item -ItemType Junction -Path $t.Source -Target $dstDir -Force | Out-Null
    Write-Host "[setup-dev-drive] Linked $($t.Source) → $dstDir"
}

Write-Host "[setup-dev-drive] Dev Drive setup complete"

# Sentinel AI Installer for Windows (PowerShell)
$ErrorActionPreference = "Stop"

$Repo = "sentinel-ai/sentinel"
$InstallDir = "$env:USERPROFILE\.sentinel\bin"

Write-Host "Downloading and installing Sentinel AI for Windows..." -ForegroundColor Cyan

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$Arch = if ([Environment]::Is64BitProcess) { "x86_64" } else { "x86" }
$Target = "$Arch-pc-windows-msvc"
$Url = "https://github.com/$Repo/releases/latest/download/sentinel-$Target.zip"
$ZipPath = "$InstallDir\sentinel.zip"

Invoke-WebRequest -Uri $Url -OutFile $ZipPath
Expand-Archive -Path $ZipPath -DestinationPath $InstallDir -Force
Remove-Item -Path $ZipPath -Force

Write-Host "✅ Installed sentinel to $InstallDir\sentinel.exe" -ForegroundColor Green

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to User PATH." -ForegroundColor Yellow
}

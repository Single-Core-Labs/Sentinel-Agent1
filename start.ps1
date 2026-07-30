# PowerShell launcher script
Write-Host "Starting Sentinel AI agent (Solid.js + OpenTUI)..." -ForegroundColor Cyan
bun run "--jsx-import-source=@opentui/solid" packages/cli-agent/src/index.tsx



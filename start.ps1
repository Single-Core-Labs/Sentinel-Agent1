# PowerShell launcher script
Write-Host "Starting Rust Sentinel AI agent and Frontend Ink TUI..." -ForegroundColor Cyan
npx concurrently "cargo run --bin sentinel -- ai" "npm run --prefix frontend cli:dev"

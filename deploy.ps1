# deploy.ps1 - Build and deploy TaskPilot to D:\apps\taskpilot\
param(
    [string]$DeployDir = "D:\apps\taskpilot"
)

$ErrorActionPreference = "Stop"

Write-Host "`n=== TaskPilot Deploy ===" -ForegroundColor Cyan

# Build release
Write-Host "`nBuilding release..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}
Write-Host "Build succeeded." -ForegroundColor Green

# Ensure deploy directory exists
if (-not (Test-Path $DeployDir)) {
    New-Item -ItemType Directory -Path $DeployDir -Force | Out-Null
    Write-Host "Created deploy directory: $DeployDir"
}

# Copy the executables
$source = "target\release\taskpilot.exe"
$dest = Join-Path $DeployDir "taskpilot.exe"

Copy-Item -Path $source -Destination $dest -Force
Write-Host "Copied taskpilot.exe -> $dest" -ForegroundColor Green

$cliSource = "target\release\taskpilot-cli.exe"
$cliDest = Join-Path $DeployDir "taskpilot-cli.exe"

Copy-Item -Path $cliSource -Destination $cliDest -Force
Write-Host "Copied taskpilot-cli.exe -> $cliDest" -ForegroundColor Green

# Copy sample config if no config exists yet
$configDir = Join-Path $DeployDir ".taskpilot"
$configPath = Join-Path $configDir "config.toml"
if (Test-Path $configPath) {
    Write-Host "Config preserved: $configPath (not overwritten)" -ForegroundColor Yellow
} else {
    if (-not (Test-Path $configDir)) {
        New-Item -ItemType Directory -Path $configDir -Force | Out-Null
    }
    Copy-Item -Path "config.example.toml" -Destination $configPath
    Write-Host "Created starter config: $configPath" -ForegroundColor Green
}

Write-Host "`n=== Deploy complete ===" -ForegroundColor Cyan
Write-Host "Run: $dest`n"

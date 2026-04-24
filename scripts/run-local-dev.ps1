[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $projectRoot '.env'))) {
    Copy-Item (Join-Path $projectRoot '.env.example') (Join-Path $projectRoot '.env') -Force
    Write-Host 'Created .env from .env.example'
}

. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
$dockerExe = Get-CexDockerExe
$env:PATH = (Split-Path $dockerExe -Parent) + ';' + $env:PATH
Push-Location $projectRoot
try {
    & $dockerExe compose up -d
} finally {
    Pop-Location
}

& (Join-Path $PSScriptRoot 'apply-migrations.ps1')
& (Join-Path $PSScriptRoot 'seed-local-dev.ps1')

Write-Host ''
Write-Host 'Infra is up, migrations are applied, and local dev identity data is seeded.'
Write-Host 'Start services with:'
Write-Host '  powershell -ExecutionPolicy Bypass -File scripts\start-rust-service.ps1 -Package identity-service'
Write-Host '  powershell -ExecutionPolicy Bypass -File scripts\start-rust-service.ps1 -Package ledger-service'
Write-Host '  powershell -ExecutionPolicy Bypass -File scripts\start-rust-service.ps1 -Package execution-service'
Write-Host '  powershell -ExecutionPolicy Bypass -File scripts\start-rust-service.ps1 -Package audit-service'
Write-Host '  powershell -ExecutionPolicy Bypass -File scripts\start-rust-service.ps1 -Package gateway-service'


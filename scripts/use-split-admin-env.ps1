[CmdletBinding()]
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$source = Join-Path $projectRoot '.env.split-admin.example'
$target = Join-Path $projectRoot '.env'

if (-not (Test-Path $source)) {
    throw "split-admin env example not found: $source"
}

if ((Test-Path $target) -and -not $Force) {
    $timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $backup = Join-Path $projectRoot (".env.backup-{0}" -f $timestamp)
    Copy-Item -LiteralPath $target -Destination $backup -Force
    Write-Host "Backed up existing .env to $backup"
}

Copy-Item -LiteralPath $source -Destination $target -Force
Write-Host "Applied split-admin env template: $source -> $target"
Write-Host "Next step: run the full local gate on the Windows host, for example:"
Write-Host '  powershell -ExecutionPolicy Bypass -File .\gate-local.ps1'

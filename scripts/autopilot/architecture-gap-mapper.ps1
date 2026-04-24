[CmdletBinding()]
param(
    [string]$ProjectRoot,
    [string]$JobId = ''
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot 'worker-common.ps1')
$runtimeRoot = Initialize-AutopilotRuntime -ProjectRoot $ProjectRoot
$reportsDir = Join-Path $runtimeRoot 'reports'

$presentServices = @(Get-ChildItem -LiteralPath (Join-Path $ProjectRoot 'services') -Directory | Select-Object -ExpandProperty Name | Sort-Object)
$targetServices = @('identity-service','ledger-service','gateway-service','execution-service','audit-service','capability-service','policy-risk-service','marketplace-service')
$missingServices = @($targetServices | Where-Object { $_ -notin $presentServices })

$presentCrates = @(Get-ChildItem -LiteralPath (Join-Path $ProjectRoot 'crates') -Directory | Select-Object -ExpandProperty Name | Sort-Object)
$targetCrates = @('shared-types','shared-config','shared-errors','shared-tracing')
$missingCrates = @($targetCrates | Where-Object { $_ -notin $presentCrates })

$report = [pscustomobject]@{
    generatedAt = Get-UtcText
    jobId = $JobId
    presentServices = $presentServices
    missingTargetServices = $missingServices
    presentCrates = $presentCrates
    missingTargetCrates = $missingCrates
    stage = if ($missingServices.Count -eq 0) { 'platform-shape-present' } else { 'mvp-core-with-platform-gaps' }
}
Write-JsonFile -Path (Join-Path $reportsDir 'architecture-gaps.json') -Value $report

$lines = @()
$lines += '# Architecture gap report'
$lines += ''
$lines += '## Present services'
foreach ($name in $presentServices) { $lines += ('- ' + $name) }
$lines += ''
$lines += '## Missing target services'
if ($missingServices.Count -eq 0) { $lines += '- none' } else { foreach ($name in $missingServices) { $lines += ('- ' + $name) } }
$lines += ''
$lines += '## Present shared crates'
foreach ($name in $presentCrates) { $lines += ('- ' + $name) }
$lines += ''
$lines += '## Missing target shared crates'
if ($missingCrates.Count -eq 0) { $lines += '- none' } else { foreach ($name in $missingCrates) { $lines += ('- ' + $name) } }
Write-TextFile -Path (Join-Path $reportsDir 'architecture-gaps.md') -Content ($lines -join "`r`n")
Write-Host ('architecture-gap: missingServices=' + $missingServices.Count)

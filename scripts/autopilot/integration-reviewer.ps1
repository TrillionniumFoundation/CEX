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
$planPath = Join-Path $runtimeRoot 'dispatch-plan.json'
$executionPath = Join-Path $runtimeRoot 'dispatch-execution.json'
$leasesDir = Join-Path $runtimeRoot 'leases'
$locksDir = Join-Path $runtimeRoot 'locks'

$plan = if (Test-Path $planPath) { Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json } else { $null }
$execution = if (Test-Path $executionPath) { Get-Content -LiteralPath $executionPath -Raw | ConvertFrom-Json } else { $null }
$leaseCount = if (Test-Path $leasesDir) { @(Get-ChildItem -LiteralPath $leasesDir -Filter '*.json' -File).Count } else { 0 }
$lockCount = if (Test-Path $locksDir) { @(Get-ChildItem -LiteralPath $locksDir -Filter '*.json' -File).Count } else { 0 }

$risks = @()
if ($plan -and $plan.summary -and [int]$plan.summary.plannedWriteCount -gt 5) {
    $risks += 'planned write count exceeded intended safe budget'
}
if ($execution -and $execution.results) {
    $failed = @($execution.results | Where-Object { $_.status -ne 'ok' -and $_.status -ne 'no-worker' })
    if ($failed.Count -gt 0) {
        $risks += ('worker failures present: ' + $failed.Count)
    }
}
if ($leaseCount -gt 12) {
    $risks += 'lease set is large enough that starvation or stale-lock cleanup should be watched'
}
if ($risks.Count -eq 0) {
    $risks += 'no critical integration risks detected in the current runtime snapshot'
}

$report = [pscustomobject]@{
    generatedAt = Get-UtcText
    jobId = $JobId
    selectedCount = if ($plan) { [int]$plan.summary.selected } else { 0 }
    plannedWriteCount = if ($plan) { [int]$plan.summary.plannedWriteCount } else { 0 }
    leaseCount = $leaseCount
    lockCount = $lockCount
    risks = $risks
}
Write-JsonFile -Path (Join-Path $reportsDir 'integration-review.json') -Value $report

$lines = @()
$lines += '# Integration review'
$lines += ''
$lines += ('- selectedCount: ' + $report.selectedCount)
$lines += ('- plannedWriteCount: ' + $report.plannedWriteCount)
$lines += ('- leaseCount: ' + $report.leaseCount)
$lines += ('- lockCount: ' + $report.lockCount)
$lines += ''
$lines += '## Risks'
foreach ($risk in $risks) { $lines += ('- ' + $risk) }
Write-TextFile -Path (Join-Path $reportsDir 'integration-review.md') -Content ($lines -join "`r`n")
Write-Host ('integration-review: risks=' + $risks.Count)

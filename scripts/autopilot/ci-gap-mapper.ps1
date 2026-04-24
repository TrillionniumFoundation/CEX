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

$requiredFiles = @(
    '.github\workflows\rust-service-gate.yml',
    '.github\workflows\rust-full-gate-self-hosted.yml',
    '.github\workflows\rust-self-hosted-preflight-hygiene.yml',
    'scripts\self-hosted-runner-preflight.ps1',
    'scripts\collect-self-hosted-artifacts.ps1',
    'scripts\register-openclaw-autopilot-cron.ps1'
)
$present = @()
$missing = @()
foreach ($path in $requiredFiles) {
    if (Test-Path (Join-Path $ProjectRoot $path)) { $present += $path } else { $missing += $path }
}

$report = [pscustomobject]@{
    generatedAt = Get-UtcText
    jobId = $JobId
    present = $present
    missing = $missing
    maturityNote = 'CI/gate scaffolding is ahead of business-feature completeness; next gap is real worker execution and promotion policy.'
}
Write-JsonFile -Path (Join-Path $reportsDir 'ci-gaps.json') -Value $report

$lines = @()
$lines += '# CI gap report'
$lines += ''
$lines += '## Present expected files'
foreach ($name in $present) { $lines += ('- ' + $name) }
$lines += ''
$lines += '## Missing expected files'
if ($missing.Count -eq 0) { $lines += '- none' } else { foreach ($name in $missing) { $lines += ('- ' + $name) } }
$lines += ''
$lines += ('- note: ' + $report.maturityNote)
Write-TextFile -Path (Join-Path $reportsDir 'ci-gaps.md') -Content ($lines -join "`r`n")
Write-Host ('ci-gap: missing=' + $missing.Count)

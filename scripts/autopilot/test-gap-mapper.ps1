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

$serviceDirs = @(Get-ChildItem -LiteralPath (Join-Path $ProjectRoot 'services') -Directory | Sort-Object Name)
$serviceSummaries = @()
foreach ($serviceDir in $serviceDirs) {
    $testFiles = @()
    $testsDir = Join-Path $serviceDir.FullName 'tests'
    if (Test-Path $testsDir) {
        $testFiles = @(Get-ChildItem -LiteralPath $testsDir -Filter '*.rs' -File)
    }
    $srcFiles = @(Get-ChildItem -LiteralPath (Join-Path $serviceDir.FullName 'src') -Recurse -Filter '*.rs' -File -ErrorAction SilentlyContinue)
    $tokioTests = 0
    $plainTests = 0
    foreach ($file in @($testFiles + $srcFiles)) {
        $tokioTests += @(Select-String -Path $file.FullName -Pattern '#\[tokio::test\]' -ErrorAction SilentlyContinue).Count
        $plainTests += @(Select-String -Path $file.FullName -Pattern '#\[test\]' -ErrorAction SilentlyContinue).Count
    }
    $serviceSummaries += [pscustomobject]@{
        service = $serviceDir.Name
        testFiles = $testFiles.Count
        tokioTests = $tokioTests
        plainTests = $plainTests
    }
}

$report = [pscustomobject]@{
    generatedAt = Get-UtcText
    jobId = $JobId
    services = $serviceSummaries
    obviousGaps = @('identity-service has no meaningful test surface yet','no capability/policy/provider service tests exist because those services are not present')
}
Write-JsonFile -Path (Join-Path $reportsDir 'test-gaps.json') -Value $report

$lines = @()
$lines += '# Test gap report'
$lines += ''
foreach ($entry in $serviceSummaries) {
    $lines += ('- ' + $entry.service + ': testFiles=' + $entry.testFiles + ', tokio=' + $entry.tokioTests + ', plain=' + $entry.plainTests)
}
$lines += ''
$lines += '## Obvious gaps'
foreach ($name in $report.obviousGaps) { $lines += ('- ' + $name) }
Write-TextFile -Path (Join-Path $reportsDir 'test-gaps.md') -Content ($lines -join "`r`n")
Write-Host ('test-gap: services=' + $serviceSummaries.Count)

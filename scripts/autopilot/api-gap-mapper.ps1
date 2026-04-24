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

$files = @(
    @{ service = 'identity-service'; path = 'services\identity-service\src\main.rs' },
    @{ service = 'ledger-service'; path = 'services\ledger-service\src\api.rs' },
    @{ service = 'gateway-service'; path = 'services\gateway-service\src\interfaces\http.rs' },
    @{ service = 'execution-service'; path = 'services\execution-service\src\api.rs' },
    @{ service = 'audit-service'; path = 'services\audit-service\src\api.rs' }
)

$routesByService = @()
foreach ($entry in $files) {
    $fullPath = Join-Path $ProjectRoot $entry.path
    if (-not (Test-Path $fullPath)) {
        $routesByService += [pscustomobject]@{ service = $entry.service; routes = @(); file = $entry.path }
        continue
    }
    $lines = @(Select-String -Path $fullPath -Pattern '"/health"|"/v1/[^"]+"' -AllMatches | ForEach-Object {
        foreach ($m in $_.Matches) {
            $m.Value.Trim('"')
        }
    } | Select-Object -Unique)
    $routesByService += [pscustomobject]@{ service = $entry.service; routes = $lines; file = $entry.path }
}

$missingFamilies = @('identity management APIs','capability registry APIs','policy-risk evaluate APIs','provider admin/execution APIs')
$report = [pscustomobject]@{
    generatedAt = Get-UtcText
    jobId = $JobId
    routesByService = $routesByService
    missingApiFamilies = $missingFamilies
}
Write-JsonFile -Path (Join-Path $reportsDir 'api-gaps.json') -Value $report

$lines = @()
$lines += '# API gap report'
$lines += ''
foreach ($entry in $routesByService) {
    $lines += ('## ' + $entry.service)
    if (@($entry.routes).Count -eq 0) {
        $lines += '- no routes detected'
    } else {
        foreach ($route in @($entry.routes)) { $lines += ('- ' + $route) }
    }
    $lines += ''
}
$lines += '## Missing API families'
foreach ($name in $missingFamilies) { $lines += ('- ' + $name) }
Write-TextFile -Path (Join-Path $reportsDir 'api-gaps.md') -Content ($lines -join "`r`n")
Write-Host ('api-gap: services=' + $routesByService.Count)

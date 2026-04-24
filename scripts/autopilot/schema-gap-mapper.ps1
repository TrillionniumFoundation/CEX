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

$migrationFiles = @(Get-ChildItem -LiteralPath (Join-Path $ProjectRoot 'migrations') -Filter '*.sql' -File | Sort-Object Name)
$tables = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
foreach ($file in $migrationFiles) {
    foreach ($match in @(Select-String -Path $file.FullName -Pattern 'create\s+table\s+(if\s+not\s+exists\s+)?(?<name>[a-zA-Z0-9_]+)' -AllMatches)) {
        foreach ($m in $match.Matches) {
            [void]$tables.Add($m.Groups['name'].Value)
        }
    }
}
$presentTables = @($tables | Sort-Object)
$targetTables = @('organizations','users','api_keys','capabilities','capability_versions','accounts','ledger_entries','invocations','executions','approvals','audit_events','policy_rules','provider_runs','async_events')
$missingTables = @($targetTables | Where-Object { $_ -notin $presentTables })

$report = [pscustomobject]@{
    generatedAt = Get-UtcText
    jobId = $JobId
    migrationFiles = @($migrationFiles | ForEach-Object { $_.Name })
    presentTables = $presentTables
    missingTargetTables = $missingTables
}
Write-JsonFile -Path (Join-Path $reportsDir 'schema-gaps.json') -Value $report

$lines = @()
$lines += '# Schema gap report'
$lines += ''
$lines += '## Migration files'
foreach ($name in @($migrationFiles | ForEach-Object { $_.Name })) { $lines += ('- ' + $name) }
$lines += ''
$lines += '## Present tables'
foreach ($name in $presentTables) { $lines += ('- ' + $name) }
$lines += ''
$lines += '## Missing target tables'
if ($missingTables.Count -eq 0) { $lines += '- none' } else { foreach ($name in $missingTables) { $lines += ('- ' + $name) } }
Write-TextFile -Path (Join-Path $reportsDir 'schema-gaps.md') -Content ($lines -join "`r`n")
Write-Host ('schema-gap: presentTables=' + $presentTables.Count + ' missingTarget=' + $missingTables.Count)

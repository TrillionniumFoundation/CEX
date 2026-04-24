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

$extensions = @('.rs','.ps1','.md','.toml','.json','.yml','.yaml','.sql')
$files = @(Get-AutopilotRepoFiles -ProjectRoot $ProjectRoot -Extensions $extensions)
$largeFiles = @($files | Sort-Object Length -Descending | Select-Object -First 15 | ForEach-Object {
    [pscustomobject]@{
        path = Get-RelativePath -ProjectRoot $ProjectRoot -Path $_.FullName
        length = [int64]$_.Length
    }
})

$todoHits = @()
foreach ($file in $files) {
    $matches = @(Select-String -Path $file.FullName -Pattern 'TODO|FIXME' -ErrorAction SilentlyContinue)
    foreach ($match in $matches) {
        $todoHits += [pscustomobject]@{
            path = Get-RelativePath -ProjectRoot $ProjectRoot -Path $file.FullName
            line = [int]$match.LineNumber
            text = $match.Line.Trim()
        }
        if ($todoHits.Count -ge 40) { break }
    }
    if ($todoHits.Count -ge 40) { break }
}

$report = [pscustomobject]@{
    generatedAt = Get-UtcText
    jobId = $JobId
    hasGitMetadata = Test-Path (Join-Path $ProjectRoot '.git')
    scannedFileCount = $files.Count
    todoCountSampled = $todoHits.Count
    largeFiles = $largeFiles
    todoHits = $todoHits
}
Write-JsonFile -Path (Join-Path $reportsDir 'repo-drift.json') -Value $report

$lines = @()
$lines += '# Repo drift report'
$lines += ''
$lines += ('- scanned files: ' + $files.Count)
$lines += ('- git metadata present: ' + (Test-Path (Join-Path $ProjectRoot '.git')))
$lines += ('- sampled TODO/FIXME hits: ' + $todoHits.Count)
$lines += ''
$lines += '## Largest source-like files'
foreach ($entry in $largeFiles) {
    $lines += ('- ' + $entry.path + ' (' + $entry.length + ' bytes)')
}
$lines += ''
$lines += '## Sampled TODO/FIXME hits'
if ($todoHits.Count -eq 0) {
    $lines += '- none found in scanned subset'
} else {
    foreach ($entry in $todoHits) {
        $lines += ('- ' + $entry.path + ':' + $entry.line + ' - ' + $entry.text)
    }
}
Write-TextFile -Path (Join-Path $reportsDir 'repo-drift.md') -Content ($lines -join "`r`n")
Write-Host ('repo-drift: scanned=' + $files.Count + ' todoSample=' + $todoHits.Count)

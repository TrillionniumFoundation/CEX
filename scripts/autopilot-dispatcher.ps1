[CmdletBinding()]
param(
    [string]$ProjectRoot = '',
    [switch]$Dispatch,
    [string]$OnlyJobId,
    [int]$MaxPlanned = 0
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

if ([string]::IsNullOrWhiteSpace($ProjectRoot)) {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $ProjectRoot = Split-Path -Parent $scriptDir
}

function Ensure-Dir {
    param([string]$Path)
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

function Read-JsonFile {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return $null }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) { return $null }
    return $raw | ConvertFrom-Json
}

function Write-JsonFile {
    param([string]$Path, $Value)
    $json = $Value | ConvertTo-Json -Depth 12
    Set-Content -LiteralPath $Path -Value $json -Encoding UTF8
}

function Get-UtcText {
    param([datetime]$Value = (Get-Date))
    return $Value.ToUniversalTime().ToString('o')
}

function Get-ActiveRecordMap {
    param([string]$Dir, [datetime]$Now)
    $map = @{}
    if (-not (Test-Path $Dir)) { return $map }

    foreach ($file in Get-ChildItem -LiteralPath $Dir -Filter '*.json' -File) {
        try {
            $record = Read-JsonFile -Path $file.FullName
            if ($null -eq $record) { continue }
            $expiresAtUtc = ([datetimeoffset]::Parse([string]$record.expiresAt)).UtcDateTime
            $nowUtc = $Now.ToUniversalTime()
            if ($expiresAtUtc -le $nowUtc) {
                Remove-Item -LiteralPath $file.FullName -Force -ErrorAction SilentlyContinue
                continue
            }
            $map[[string]$record.id] = $record
        }
        catch {
            Remove-Item -LiteralPath $file.FullName -Force -ErrorAction SilentlyContinue
        }
    }

    return $map
}

function Get-GroupOrderIndex {
    param($Config, [string]$Group)
    $index = 0
    foreach ($name in @($Config.groupOrder)) {
        if ([string]$name -eq $Group) {
            return $index
        }
        $index += 1
    }
    return 999
}

function Get-GroupLimit {
    param($Config, [string]$Group)
    $prop = $Config.groupLimits.PSObject.Properties | Where-Object { $_.Name -eq $Group } | Select-Object -First 1
    if ($null -eq $prop) { return 0 }
    return [int]$prop.Value
}

$autopilotRoot = Join-Path $ProjectRoot 'ops\autopilot'
$configPath = Join-Path $autopilotRoot 'config\dispatcher-config.json'
$jobsDir = Join-Path $autopilotRoot 'jobs'
$runtimeRoot = Join-Path $autopilotRoot 'runtime'
$reportsDir = Join-Path $runtimeRoot 'reports'
$resultsDir = Join-Path $runtimeRoot 'results'
$leasesDir = Join-Path $runtimeRoot 'leases'
$locksDir = Join-Path $runtimeRoot 'locks'
$statePath = Join-Path $runtimeRoot 'job-state.json'
$planJsonPath = Join-Path $runtimeRoot 'dispatch-plan.json'
$planMdPath = Join-Path $runtimeRoot 'dispatch-plan.md'
$executionJsonPath = Join-Path $runtimeRoot 'dispatch-execution.json'
$executionMdPath = Join-Path $runtimeRoot 'dispatch-execution.md'

Ensure-Dir -Path $runtimeRoot
Ensure-Dir -Path $reportsDir
Ensure-Dir -Path $resultsDir
Ensure-Dir -Path $leasesDir
Ensure-Dir -Path $locksDir

$config = Read-JsonFile -Path $configPath
if ($null -eq $config) {
    throw "Missing dispatcher config: $configPath"
}

$jobs = @()
foreach ($file in Get-ChildItem -LiteralPath $jobsDir -Filter '*.json' -File) {
    $job = Read-JsonFile -Path $file.FullName
    if ($null -eq $job) { continue }
    if ($OnlyJobId -and [string]$job.id -ne $OnlyJobId) { continue }
    $jobs += $job
}
if ($jobs.Count -eq 0) {
    throw 'No autopilot jobs matched the current filter.'
}

$jobs = @($jobs | Where-Object { ($null -eq $_.enabled) -or [bool]$_.enabled })
$jobs = @($jobs | Sort-Object `
    @{ Expression = { Get-GroupOrderIndex -Config $config -Group ([string]$_.group) } }, `
    @{ Expression = { [int]$_.priority }; Descending = $true }, `
    @{ Expression = { [string]$_.id } })

$now = Get-Date
$nowUtcText = Get-UtcText -Value $now
$maxPlannedPerTick = if ($MaxPlanned -gt 0) { $MaxPlanned } else { [int]$config.maxPlannedPerTick }
$maxWritePerTick = [int]$config.maxWritePerTick
$maxImplementPerTick = [int]$config.maxImplementPerTick
$maxVerifyWritePerTick = [int]$config.maxVerifyWritePerTick
$maxDocsWritePerTick = [int]$config.maxDocsWritePerTick
$defaultLeaseMinutes = [int]$config.defaultLeaseMinutes

$state = Read-JsonFile -Path $statePath
$stateById = @{}
if ($state -and $state.jobs) {
    foreach ($entry in @($state.jobs)) {
        $stateById[[string]$entry.id] = $entry
    }
}

$activeLeases = Get-ActiveRecordMap -Dir $leasesDir -Now $now
$activeLocks = Get-ActiveRecordMap -Dir $locksDir -Now $now

$selected = @()
$skipped = @()
$countsByGroup = @{}
$reservedLockGroups = @{}
$plannedWriteCount = 0
$plannedImplementCount = 0
$plannedVerifyWriteCount = 0
$plannedDocsWriteCount = 0

foreach ($job in $jobs) {
    $jobId = [string]$job.id
    $group = [string]$job.group
    $intent = [string]$job.intent
    $lockGroup = [string]$job.lockGroup

    if (-not $countsByGroup.ContainsKey($group)) {
        $countsByGroup[$group] = 0
    }

    $jobState = if ($stateById.ContainsKey($jobId)) { $stateById[$jobId] } else { $null }
    $lastPlannedAt = $null
    if ($jobState -and $jobState.lastPlannedAt) {
        $lastPlannedAt = [datetime]::Parse([string]$jobState.lastPlannedAt)
    }

    $isDue = $true
    if ($lastPlannedAt) {
        $delta = $now.ToUniversalTime() - $lastPlannedAt.ToUniversalTime()
        $isDue = ($delta.TotalMinutes -ge [int]$job.cadenceMinutes)
    }

    if (-not $isDue) {
        $skipped += [pscustomobject]@{ id = $jobId; reason = 'not-due'; group = $group }
        continue
    }
    if ($selected.Count -ge $maxPlannedPerTick) {
        $skipped += [pscustomobject]@{ id = $jobId; reason = 'max-planned-reached'; group = $group }
        continue
    }
    if ($activeLeases.ContainsKey($jobId)) {
        $skipped += [pscustomobject]@{ id = $jobId; reason = 'active-lease-present'; group = $group }
        continue
    }

    $groupLimit = Get-GroupLimit -Config $config -Group $group
    if ($countsByGroup[$group] -ge $groupLimit) {
        $skipped += [pscustomobject]@{ id = $jobId; reason = 'group-limit'; group = $group }
        continue
    }

    if ($intent -eq 'write') {
        if ($plannedWriteCount -ge $maxWritePerTick) {
            $skipped += [pscustomobject]@{ id = $jobId; reason = 'write-budget'; group = $group }
            continue
        }
        if ($group -eq 'implement' -and $plannedImplementCount -ge $maxImplementPerTick) {
            $skipped += [pscustomobject]@{ id = $jobId; reason = 'implement-budget'; group = $group }
            continue
        }
        if ($group -eq 'verify' -and $plannedVerifyWriteCount -ge $maxVerifyWritePerTick) {
            $skipped += [pscustomobject]@{ id = $jobId; reason = 'verify-write-budget'; group = $group }
            continue
        }
        if ($lockGroup -eq 'docs' -and $plannedDocsWriteCount -ge $maxDocsWritePerTick) {
            $skipped += [pscustomobject]@{ id = $jobId; reason = 'docs-write-budget'; group = $group }
            continue
        }
        if ($reservedLockGroups.ContainsKey($lockGroup)) {
            $skipped += [pscustomobject]@{ id = $jobId; reason = 'reserved-lock-group'; group = $group }
            continue
        }
        if ($activeLocks.ContainsKey($lockGroup)) {
            $skipped += [pscustomobject]@{ id = $jobId; reason = 'active-lock-group'; group = $group }
            continue
        }
    }

    $leaseMinutes = if ($job.leaseMinutes) { [int]$job.leaseMinutes } else { $defaultLeaseMinutes }
    $expiresAtText = Get-UtcText -Value ($now.AddMinutes($leaseMinutes))

    $selected += [pscustomobject]@{
        id = $jobId
        name = [string]$job.name
        group = $group
        intent = $intent
        lockGroup = $lockGroup
        milestone = [string]$job.milestone
        cadenceMinutes = [int]$job.cadenceMinutes
        priority = [int]$job.priority
        leaseMinutes = $leaseMinutes
        expiresAt = $expiresAtText
        goal = [string]$job.goal
        outputs = @($job.outputs)
        workerScript = if ($job.workerScript) { [string]$job.workerScript } else { '' }
        workerArgs = if ($job.workerArgs) { $job.workerArgs } else { $null }
    }

    $countsByGroup[$group] += 1
    if ($intent -eq 'write') {
        $plannedWriteCount += 1
        $reservedLockGroups[$lockGroup] = $true
        if ($group -eq 'implement') { $plannedImplementCount += 1 }
        if ($group -eq 'verify') { $plannedVerifyWriteCount += 1 }
        if ($lockGroup -eq 'docs') { $plannedDocsWriteCount += 1 }
    }
}

$summary = [pscustomobject]@{
    totalJobsConsidered = $jobs.Count
    selected = $selected.Count
    skipped = $skipped.Count
    plannedWriteCount = $plannedWriteCount
    plannedImplementCount = $plannedImplementCount
    plannedVerifyWriteCount = $plannedVerifyWriteCount
    plannedDocsWriteCount = $plannedDocsWriteCount
}

$plan = [pscustomobject]@{
    generatedAt = $nowUtcText
    projectRoot = $ProjectRoot
    dispatch = [bool]$Dispatch
    onlyJobId = $OnlyJobId
    summary = $summary
    selected = @($selected)
    skipped = @($skipped)
}
Write-JsonFile -Path $planJsonPath -Value $plan

$md = @()
$md += '# Autopilot Dispatch Plan'
$md += ''
$md += "- generatedAt: $nowUtcText"
$md += "- dispatch: $([bool]$Dispatch)"
$md += "- selected: $($selected.Count)"
$md += "- plannedWriteCount: $plannedWriteCount"
$md += ''
$md += '## Selected jobs'
if ($selected.Count -eq 0) {
    $md += '- none'
} else {
    foreach ($entry in $selected) {
        $md += ("- [{0}] {1} ({2}/{3}) -> {4}" -f $entry.id, $entry.name, $entry.group, $entry.lockGroup, $entry.goal)
    }
}
$md += ''
$md += '## Skipped jobs'
if ($skipped.Count -eq 0) {
    $md += '- none'
} else {
    foreach ($entry in $skipped) {
        $md += ("- [{0}] {1}" -f $entry.id, $entry.reason)
    }
}
Set-Content -LiteralPath $planMdPath -Value ($md -join "`r`n") -Encoding UTF8

$executionResults = @()

if ($Dispatch) {
    foreach ($entry in $selected) {
        $leasePath = Join-Path $leasesDir ($entry.id + '.json')
        Write-JsonFile -Path $leasePath -Value ([pscustomobject]@{
            id = [string]$entry.id
            name = [string]$entry.name
            group = [string]$entry.group
            intent = [string]$entry.intent
            lockGroup = [string]$entry.lockGroup
            acquiredAt = $nowUtcText
            expiresAt = [string]$entry.expiresAt
        })

        if ([string]$entry.intent -eq 'write') {
            $lockPath = Join-Path $locksDir (([string]$entry.lockGroup) + '.json')
            Write-JsonFile -Path $lockPath -Value ([pscustomobject]@{
                id = [string]$entry.lockGroup
                holderJobId = [string]$entry.id
                acquiredAt = $nowUtcText
                expiresAt = [string]$entry.expiresAt
            })
        }
    }

    foreach ($entry in $selected) {
        if ([string]::IsNullOrWhiteSpace([string]$entry.workerScript)) {
            $executionResults += [pscustomobject]@{ id = $entry.id; status = 'no-worker'; output = '' }
            continue
        }

        $workerPath = if ([System.IO.Path]::IsPathRooted([string]$entry.workerScript)) { [string]$entry.workerScript } else { Join-Path $ProjectRoot ([string]$entry.workerScript) }
        if (-not (Test-Path $workerPath)) {
            $executionResults += [pscustomobject]@{ id = $entry.id; status = 'missing-worker'; output = $workerPath }
            continue
        }

        $invokeParams = @{
            ProjectRoot = $ProjectRoot
            JobId = [string]$entry.id
        }
        if ($entry.workerArgs) {
            foreach ($prop in $entry.workerArgs.PSObject.Properties) {
                $invokeParams[[string]$prop.Name] = [string]$prop.Value
            }
        }

        try {
            $output = (& $workerPath @invokeParams 2>&1 | Out-String -Width 4096).Trim()
            $executionResults += [pscustomobject]@{ id = $entry.id; status = 'ok'; output = $output }
        }
        catch {
            $message = $_.Exception.Message
            $details = ($_ | Out-String -Width 4096).Trim()
            $executionResults += [pscustomobject]@{ id = $entry.id; status = 'failed'; output = (($message + "`n" + $details).Trim()) }
        }
    }

    $execution = [pscustomobject]@{
        generatedAt = $nowUtcText
        dispatch = $true
        results = @($executionResults)
    }
    Write-JsonFile -Path $executionJsonPath -Value $execution

    $execMd = @()
    $execMd += '# Autopilot Dispatch Execution'
    $execMd += ''
    foreach ($entry in $executionResults) {
        $execMd += ('- [' + $entry.id + '] ' + $entry.status)
    }
    Set-Content -LiteralPath $executionMdPath -Value ($execMd -join "`r`n") -Encoding UTF8
}

$stateRecords = @()
foreach ($job in $jobs) {
    $jobId = [string]$job.id
    $existing = if ($stateById.ContainsKey($jobId)) { $stateById[$jobId] } else { $null }
    $selectedEntry = $selected | Where-Object { $_.id -eq $jobId } | Select-Object -First 1
    $skippedEntry = $skipped | Where-Object { $_.id -eq $jobId } | Select-Object -First 1
    $resultEntry = $executionResults | Where-Object { $_.id -eq $jobId } | Select-Object -First 1

    $stateRecords += [pscustomobject]@{
        id = $jobId
        group = [string]$job.group
        intent = [string]$job.intent
        lockGroup = [string]$job.lockGroup
        enabled = if ($null -eq $job.enabled) { $true } else { [bool]$job.enabled }
        lastSeenAt = $nowUtcText
        lastDecision = if ($selectedEntry) { if ($Dispatch) { 'dispatched' } else { 'planned' } } elseif ($skippedEntry) { [string]$skippedEntry.reason } else { 'unseen' }
        lastPreviewAt = if ($selectedEntry -and -not $Dispatch) { $nowUtcText } elseif ($existing -and $existing.lastPreviewAt) { [string]$existing.lastPreviewAt } else { $null }
        lastPlannedAt = if ($selectedEntry -and $Dispatch) { $nowUtcText } elseif ($existing -and $existing.lastPlannedAt) { [string]$existing.lastPlannedAt } else { $null }
        lastRunStatus = if ($resultEntry) { [string]$resultEntry.status } elseif ($existing -and $existing.lastRunStatus) { [string]$existing.lastRunStatus } else { $null }
        lastRunAt = if ($resultEntry) { $nowUtcText } elseif ($existing -and $existing.lastRunAt) { [string]$existing.lastRunAt } else { $null }
    }
}
Write-JsonFile -Path $statePath -Value ([pscustomobject]@{ generatedAt = $nowUtcText; jobs = @($stateRecords) })

Write-Host '==> autopilot-dispatcher.ps1'
Write-Host ("project_root={0}" -f $ProjectRoot)
Write-Host ("dispatch={0}" -f ([bool]$Dispatch))
Write-Host ("selected={0}" -f $selected.Count)
Write-Host ("write_jobs={0}" -f $plannedWriteCount)
if ($Dispatch) {
    $okCount = @($executionResults | Where-Object { $_.status -eq 'ok' }).Count
    $failedCount = @($executionResults | Where-Object { $_.status -eq 'failed' -or $_.status -eq 'missing-worker' }).Count
    Write-Host ("worker_ok={0}" -f $okCount)
    Write-Host ("worker_failed={0}" -f $failedCount)
}
Write-Host ("plan_json={0}" -f $planJsonPath)
Write-Host ("plan_md={0}" -f $planMdPath)

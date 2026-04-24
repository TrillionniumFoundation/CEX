function Ensure-Dir {
    param([Parameter(Mandatory = $true)][string]$Path)
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Value
    )
    $json = $Value | ConvertTo-Json -Depth 12
    Set-Content -LiteralPath $Path -Value $json -Encoding UTF8
}

function Write-TextFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )
    Set-Content -LiteralPath $Path -Value $Content -Encoding UTF8
}

function Get-UtcText {
    param([datetime]$Value = (Get-Date))
    return $Value.ToUniversalTime().ToString('o')
}

function Get-AutopilotRuntimeRoot {
    param([Parameter(Mandatory = $true)][string]$ProjectRoot)
    return Join-Path $ProjectRoot 'ops\autopilot\runtime'
}

function Initialize-AutopilotRuntime {
    param([Parameter(Mandatory = $true)][string]$ProjectRoot)
    $runtimeRoot = Get-AutopilotRuntimeRoot -ProjectRoot $ProjectRoot
    Ensure-Dir -Path $runtimeRoot
    Ensure-Dir -Path (Join-Path $runtimeRoot 'reports')
    Ensure-Dir -Path (Join-Path $runtimeRoot 'queue')
    Ensure-Dir -Path (Join-Path $runtimeRoot 'results')
    return $runtimeRoot
}

function Get-RelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$ProjectRoot,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $rootFull = [System.IO.Path]::GetFullPath($ProjectRoot)
    $pathFull = [System.IO.Path]::GetFullPath($Path)
    if ($pathFull.StartsWith($rootFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $pathFull.Substring($rootFull.Length).TrimStart('\\')
    }
    return $pathFull
}

function Test-AutopilotExcludedPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    $normalized = $Path.Replace('/', '\\')
    $fragments = @(
        '\\target\\',
        '\\.git\\',
        '\\ops\\autopilot\\runtime\\',
        '\\ci-artifacts\\',
        '\\logs\\',
        '\\run\\',
        '\\node_modules\\'
    )
    foreach ($fragment in $fragments) {
        if ($normalized -like ('*' + $fragment + '*')) {
            return $true
        }
    }
    return $false
}

function Get-AutopilotRepoFiles {
    param(
        [Parameter(Mandatory = $true)][string]$ProjectRoot,
        [string[]]$Extensions = @()
    )

    $all = @(Get-ChildItem -LiteralPath $ProjectRoot -Recurse -File | Where-Object { -not (Test-AutopilotExcludedPath -Path $_.FullName) })
    if ($Extensions.Count -eq 0) {
        return $all
    }

    $wanted = @{}
    foreach ($ext in $Extensions) {
        $wanted[$ext.ToLowerInvariant()] = $true
    }

    return @($all | Where-Object {
        $ext = [System.IO.Path]::GetExtension($_.FullName).ToLowerInvariant()
        $wanted.ContainsKey($ext)
    })
}

function New-QueueTask {
    param(
        [Parameter(Mandatory = $true)][string]$Domain,
        [Parameter(Mandatory = $true)][string]$Id,
        [Parameter(Mandatory = $true)][string]$Title,
        [Parameter(Mandatory = $true)][string]$Goal,
        [string[]]$DependsOn = @(),
        [string[]]$Acceptance = @(),
        [string[]]$TargetPaths = @()
    )

    return [pscustomobject]@{
        domain = $Domain
        id = $Id
        title = $Title
        goal = $Goal
        dependsOn = @($DependsOn)
        acceptance = @($Acceptance)
        targetPaths = @($TargetPaths)
    }
}

function Write-QueueBundle {
    param(
        [Parameter(Mandatory = $true)][string]$ProjectRoot,
        [Parameter(Mandatory = $true)][string]$Domain,
        [Parameter(Mandatory = $true)]$Tasks
    )

    $runtimeRoot = Initialize-AutopilotRuntime -ProjectRoot $ProjectRoot
    $queueDir = Join-Path (Join-Path $runtimeRoot 'queue') $Domain
    Ensure-Dir -Path $queueDir

    foreach ($task in @($Tasks)) {
        $path = Join-Path $queueDir ($task.id + '.json')
        Write-JsonFile -Path $path -Value $task
    }

    $index = [pscustomobject]@{
        generatedAt = Get-UtcText
        domain = $Domain
        count = @($Tasks).Count
        tasks = @($Tasks)
    }
    Write-JsonFile -Path (Join-Path $queueDir 'index.json') -Value $index

    $lines = @()
    $lines += ('# ' + $Domain + ' roadmap queue')
    $lines += ''
    foreach ($task in @($Tasks)) {
        $lines += ('- [' + $task.id + '] ' + $task.title + ' - ' + $task.goal)
    }
    Write-TextFile -Path (Join-Path $queueDir 'index.md') -Content ($lines -join "`r`n")
}

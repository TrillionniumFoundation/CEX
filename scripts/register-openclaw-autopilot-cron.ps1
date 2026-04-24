[CmdletBinding()]
param(
    [ValidateSet('install', 'show', 'remove', 'run-now')]
    [string]$Action = 'install',
    [switch]$Recreate
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$jobName = 'CEX Autopilot Dispatcher'
$jobDescription = 'Run the repo-local autopilot dispatcher every 5 minutes with OpenClaw cron.'
$message = @'
In E:\CEX, run exactly this command from the repo root:

powershell -ExecutionPolicy Bypass -File .\scripts\autopilot-dispatcher.ps1 -Dispatch

Rules:
- Do not edit source files.
- This run is only for dispatcher tick execution.
- If the command succeeds, give a short internal summary: selected count, write_jobs count, and whether leases/locks were written.
- If it fails, report the exact failure briefly.
'@

function Get-CronListJson {
    $raw = openclaw cron list --json
    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw 'openclaw cron list returned empty output'
    }
    return $raw | ConvertFrom-Json
}

function Get-MatchingJobs {
    $list = Get-CronListJson
    return @($list.jobs | Where-Object { $_.name -eq $jobName })
}

function Remove-JobById {
    param([Parameter(Mandatory = $true)][string]$Id)
    openclaw cron rm $Id --json | Out-Null
}

function Add-Job {
    $raw = openclaw cron add --json `
        --name $jobName `
        --description $jobDescription `
        --every 5m `
        --session isolated `
        --agent main `
        --message $message `
        --tools 'exec,read' `
        --thinking minimal `
        --light-context `
        --no-deliver

    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw 'openclaw cron add returned empty output'
    }

    return $raw | ConvertFrom-Json
}

$matches = @(Get-MatchingJobs)

switch ($Action) {
    'show' {
        if ($matches.Count -eq 0) {
            Write-Host 'No matching OpenClaw autopilot dispatcher cron job found.'
            exit 0
        }

        $matches | ConvertTo-Json -Depth 10
        exit 0
    }

    'remove' {
        if ($matches.Count -eq 0) {
            Write-Host 'No matching OpenClaw autopilot dispatcher cron job found.'
            exit 0
        }

        foreach ($job in $matches) {
            Remove-JobById -Id ([string]$job.id)
        }

        Write-Host ("Removed {0} cron job(s) named '{1}'." -f $matches.Count, $jobName)
        exit 0
    }

    'install' {
        if ($Recreate -and $matches.Count -gt 0) {
            foreach ($job in $matches) {
                Remove-JobById -Id ([string]$job.id)
            }
            $matches = @()
        }

        if ($matches.Count -gt 1) {
            throw ("Found multiple matching cron jobs named '{0}'. Run with -Action remove or -Recreate first." -f $jobName)
        }

        if ($matches.Count -eq 1) {
            Write-Host 'OpenClaw autopilot dispatcher cron job already exists:'
            $matches[0] | ConvertTo-Json -Depth 10
            exit 0
        }

        $job = Add-Job
        Write-Host 'Installed OpenClaw autopilot dispatcher cron job:'
        $job | ConvertTo-Json -Depth 10
        exit 0
    }

    'run-now' {
        if ($matches.Count -eq 0) {
            throw 'No matching cron job found. Install it first.'
        }
        if ($matches.Count -gt 1) {
            throw ("Found multiple matching cron jobs named '{0}'. Clean them up first." -f $jobName)
        }

        openclaw cron run ([string]$matches[0].id)
        exit $LASTEXITCODE
    }
}

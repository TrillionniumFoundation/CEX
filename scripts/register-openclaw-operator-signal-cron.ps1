[CmdletBinding()]
param(
    [ValidateSet('install', 'show', 'remove', 'run-now')]
    [string]$Action = 'install',
    [switch]$Recreate,
    [switch]$UseEntryIdentityPolicyExample,
    [switch]$UseMonitoringDeployPolicyExample,
    [ValidateSet('entry-identity', 'monitoring-deploy', 'baseline')]
    [string[]]$PolicyBundle,
    [ValidateSet('default', 'identity', 'deploy')]
    [string[]]$PolicyProfile
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$jobName = 'CEX Operator Signal Monitor'
$baseJobDescription = 'Run the repo-local operator signal wrapper every 5 minutes with OpenClaw cron.'
$runCommand = './scripts/run-operator-signal-check.sh --compact'

$policyLabels = @()
if ($UseEntryIdentityPolicyExample) {
    $policyLabels += 'entry-identity'
}
if ($UseMonitoringDeployPolicyExample) {
    $policyLabels += 'monitoring-deploy'
}
if ($PolicyBundle) {
    $policyLabels += $PolicyBundle
}
$policyLabels = @($policyLabels | Sort-Object -Unique)
$policyProfiles = @()
if ($PolicyProfile) {
    $policyProfiles += $PolicyProfile
}
$policyProfiles = @($policyProfiles | Sort-Object -Unique)
if ($policyProfiles.Count -gt 0) {
    $profileValue = ($policyProfiles | Sort-Object -Unique) -join ','
    $runCommand = 'OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE="' + $profileValue + '" ./scripts/run-operator-signal-check.sh --compact'
} elseif ($policyLabels.Count -gt 0) {
    $bundleValue = ($policyLabels | Sort-Object -Unique) -join ','
    $runCommand = 'OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE="' + $bundleValue + '" ./scripts/run-operator-signal-check.sh --compact'
}

$jobDescription = $baseJobDescription
if ($policyProfiles.Count -gt 0) {
    $jobDescription += ' Uses the repo-local policy profiles: ' + (($policyProfiles | Sort-Object) -join ', ') + '.'
} elseif ($policyLabels.Count -gt 0) {
    $jobDescription += ' Uses the repo-local policy bundles: ' + (($policyLabels | Sort-Object) -join ', ') + '.'
}

$message = @"
In /data/home-data/CEX, run exactly this command from the repo root:

$runCommand

Rules:
- Do not edit source files.
- This run is only for operator-signal monitoring.
- If the result is ok, keep the internal summary short.
- If the result is warn or critical, summarize the triggered signals briefly.
- If the script fails, report the exact failure briefly.
"@

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
            Write-Host 'No matching OpenClaw operator signal cron job found.'
            exit 0
        }

        $matches | ConvertTo-Json -Depth 10
        exit 0
    }

    'remove' {
        if ($matches.Count -eq 0) {
            Write-Host 'No matching OpenClaw operator signal cron job found.'
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
            Write-Host 'OpenClaw operator signal cron job already exists:'
            $matches[0] | ConvertTo-Json -Depth 10
            exit 0
        }

        $job = Add-Job
        Write-Host 'Installed OpenClaw operator signal cron job:'
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

[CmdletBinding()]
param(
    [switch]$BootstrapEnv
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
$checks = [System.Collections.Generic.List[object]]::new()
$failed = $false

function Add-CheckResult {
    param(
        [string]$Name,
        [string]$Status,
        [string]$Detail
    )

    $script:checks.Add([pscustomobject]@{
        name = $Name
        status = $Status
        detail = $Detail
    }) | Out-Null

    if ($Status -eq 'FAIL') {
        $script:failed = $true
    }
}

function Invoke-Check {
    param(
        [string]$Name,
        [scriptblock]$Script
    )

    try {
        $detail = & $Script
        if ($null -eq $detail -or [string]::IsNullOrWhiteSpace([string]$detail)) {
            $detail = 'ok'
        }
        Add-CheckResult -Name $Name -Status 'PASS' -Detail ([string]$detail)
    }
    catch {
        Add-CheckResult -Name $Name -Status 'FAIL' -Detail $_.Exception.Message
    }
}

function Get-ListeningPortOwners {
    param(
        [Parameter(Mandatory = $true)][int]$Port
    )

    $listeners = @(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
    if ($listeners.Count -eq 0) {
        return @()
    }

    $owners = foreach ($processId in ($listeners | Select-Object -ExpandProperty OwningProcess -Unique)) {
        $processName = 'UNKNOWN'
        try {
            $processName = (Get-Process -Id $processId -ErrorAction Stop).ProcessName
        }
        catch {}

        [pscustomobject]@{
            pid = [int]$processId
            name = [string]$processName
        }
    }

    return @($owners)
}

function Test-PortExpectation {
    param(
        [Parameter(Mandatory = $true)][int]$Port,
        [Parameter(Mandatory = $true)][string[]]$AllowedProcessNames
    )

    $owners = @(Get-ListeningPortOwners -Port $Port)
    if ($owners.Count -eq 0) {
        return "port $Port is free"
    }

    $unexpected = @($owners | Where-Object { $_.name -notin $AllowedProcessNames })
    $rendered = ($owners | ForEach-Object { "{0} pid={1}" -f $_.name, $_.pid }) -join ', '

    if ($unexpected.Count -gt 0) {
        throw "port $Port is occupied by unexpected listener(s): $rendered"
    }

    return "port $Port is already owned by expected listener(s): $rendered"
}

Write-Host '==> self-hosted-runner-preflight.ps1'
Write-Host ("project_root={0}" -f $projectRoot)
Write-Host ("bootstrap_env={0}" -f ([bool]$BootstrapEnv))

Invoke-Check 'platform_windows' {
    if ($env:OS -ne 'Windows_NT') {
        throw 'this preflight is intended for Windows self-hosted runners'
    }

    return 'Windows runner detected'
}

Invoke-Check 'required_repo_files' {
    $required = @(
        'gate-local.ps1',
        'docker-compose.yml',
        '.env.example',
        'scripts\rust-regression-check.ps1',
        'scripts\start-local-runtime-detached.ps1',
        'scripts\stop-local-runtime.ps1',
        'scripts\status-local-runtime.ps1',
        'scripts\collect-self-hosted-artifacts.ps1',
        '.github\workflows\rust-full-gate-self-hosted.yml'
    )

    $missing = @($required | Where-Object { -not (Test-Path (Join-Path $projectRoot $_)) })
    if ($missing.Count -gt 0) {
        throw ("missing required repo file(s): {0}" -f ($missing -join ', '))
    }

    return 'all required gate files are present'
}

Invoke-Check 'env_file' {
    $envPath = Join-Path $projectRoot '.env'
    $envExamplePath = Join-Path $projectRoot '.env.example'

    if (Test-Path $envPath) {
        return '.env already present'
    }

    if (-not (Test-Path $envExamplePath)) {
        throw '.env is missing and .env.example is unavailable'
    }

    if (-not $BootstrapEnv) {
        throw '.env is missing; copy .env.example to .env or rerun with -BootstrapEnv'
    }

    Copy-Item -LiteralPath $envExamplePath -Destination $envPath -Force
    return 'created .env from .env.example'
}

Invoke-Check 'env_keys' {
    $envPath = Join-Path $projectRoot '.env'
    if (-not (Test-Path $envPath)) {
        throw '.env is still missing after bootstrap check'
    }

    $requiredKeys = @(
        'RUST_LOG',
        'APP_ENV',
        'GATEWAY_HOST',
        'GATEWAY_PORT',
        'LEDGER_BASE_URL',
        'EXECUTION_BASE_URL',
        'IDENTITY_BASE_URL',
        'AUDIT_BASE_URL',
        'DATABASE_URL',
        'REDIS_URL',
        'NATS_URL'
    )

    $keys = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    Get-Content -LiteralPath $envPath | ForEach-Object {
        $line = $_.Trim()
        if (-not $line -or $line.StartsWith('#')) {
            return
        }

        $parts = $line -split '=', 2
        if ($parts.Count -eq 2) {
            [void]$keys.Add($parts[0].Trim())
        }
    }

    $missing = @($requiredKeys | Where-Object { -not $keys.Contains($_) })
    if ($missing.Count -gt 0) {
        throw (".env is missing required key(s): {0}" -f ($missing -join ', '))
    }

    return 'all required .env keys are present'
}

Invoke-Check 'cargo_available' {
    $version = (cargo --version).Trim()
    if (-not $version) {
        throw 'cargo returned an empty version string'
    }

    return $version
}

Invoke-Check 'docker_available' {
    $dockerExe = Get-CexDockerExe
    $version = (& $dockerExe version --format '{{.Server.Version}}').Trim()
    if (-not $version) {
        throw 'docker server version was empty; engine may not be reachable'
    }

    return ("docker server {0} via {1}" -f $version, $dockerExe)
}

Invoke-Check 'docker_compose_available' {
    $dockerExe = Get-CexDockerExe
    $version = (& $dockerExe compose version --short).Trim()
    if (-not $version) {
        throw 'docker compose version was empty'
    }

    return ("docker compose {0} via {1}" -f $version, $dockerExe)
}

$portExpectations = @(
    @{ Name = 'port_7001_identity'; Port = 7001; Allowed = @('identity-service') },
    @{ Name = 'port_7002_ledger'; Port = 7002; Allowed = @('ledger-service') },
    @{ Name = 'port_7003_execution'; Port = 7003; Allowed = @('execution-service') },
    @{ Name = 'port_7004_audit'; Port = 7004; Allowed = @('audit-service') },
    @{ Name = 'port_8080_gateway'; Port = 8080; Allowed = @('gateway-service') },
    @{ Name = 'port_5432_postgres'; Port = 5432; Allowed = @('com.docker.backend') },
    @{ Name = 'port_6379_redis'; Port = 6379; Allowed = @('com.docker.backend') },
    @{ Name = 'port_4222_nats'; Port = 4222; Allowed = @('com.docker.backend') }
)

foreach ($rule in $portExpectations) {
    Invoke-Check $rule.Name {
        Test-PortExpectation -Port $rule.Port -AllowedProcessNames $rule.Allowed
    }
}

Write-Host ''
$checks | Format-Table -AutoSize
Write-Host ''

if ($failed) {
    throw 'self-hosted runner preflight failed'
}

Write-Host 'self-hosted runner preflight passed.'

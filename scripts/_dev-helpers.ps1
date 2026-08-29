$script:ProjectRoot = Split-Path -Parent $PSScriptRoot

function Import-CexDotEnv {
    param(
        [string]$Path = $null
    )

    if (-not $Path) {
        $explicitPath = [System.Environment]::GetEnvironmentVariable('CEX_ENV_FILE', 'Process')
        if ($explicitPath) {
            $Path = $explicitPath
        }
        else {
            $envPath = Join-Path $script:ProjectRoot '.env'
            $examplePath = Join-Path $script:ProjectRoot '.env.example'
            if (Test-Path $envPath) {
                $Path = $envPath
            }
            elseif (Test-Path $examplePath) {
                $Path = $examplePath
            }
        }
    }

    if (-not $Path -or -not (Test-Path $Path)) {
        throw ".env not found; set CEX_ENV_FILE or create .env/.env.example"
    }

    Get-Content -LiteralPath $Path | ForEach-Object {
        $line = $_.Trim()
        if (-not $line -or $line.StartsWith('#')) { return }
        $parts = $line -split '=', 2
        if ($parts.Count -ne 2) { return }
        [System.Environment]::SetEnvironmentVariable($parts[0], $parts[1], 'Process')
    }
}

function Get-CexEnvValue {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    $value = [System.Environment]::GetEnvironmentVariable($Name, 'Process')
    if ($null -eq $value) {
        return $null
    }

    $trimmed = $value.Trim()
    if ([string]::IsNullOrWhiteSpace($trimmed)) {
        return $null
    }

    return $trimmed
}

function Get-CexScopedAdminToken {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Scope
    )

    $bundleEnvNames = @('IDENTITY_ADMIN_TOKENS_JSON')
    $singleEnvNames = @('IDENTITY_ADMIN_TOKEN')

    if ($Scope -eq 'audit:read') {
        $bundleEnvNames = @('AUDIT_ADMIN_TOKENS_JSON', 'IDENTITY_ADMIN_TOKENS_JSON')
        $singleEnvNames = @('AUDIT_ADMIN_TOKEN', 'IDENTITY_ADMIN_TOKEN')
    }
    elseif ($Scope -like 'executions:*') {
        $bundleEnvNames = @('EXECUTION_ADMIN_TOKENS_JSON', 'IDENTITY_ADMIN_TOKENS_JSON')
        $singleEnvNames = @('EXECUTION_ADMIN_TOKEN', 'IDENTITY_ADMIN_TOKEN')
    }
    elseif ($Scope -like 'ledger:*') {
        $bundleEnvNames = @('LEDGER_ADMIN_TOKENS_JSON', 'IDENTITY_ADMIN_TOKENS_JSON')
        $singleEnvNames = @('LEDGER_ADMIN_TOKEN', 'IDENTITY_ADMIN_TOKEN')
    }

    foreach ($envName in $bundleEnvNames) {
        $raw = Get-CexEnvValue -Name $envName
        if (-not $raw) {
            continue
        }

        try {
            $records = @($raw | ConvertFrom-Json -Depth 16)
        }
        catch {
            throw "$envName is not valid JSON: $($_.Exception.Message)"
        }

        foreach ($record in $records) {
            $token = ([string]$record.token).Trim()
            if ([string]::IsNullOrWhiteSpace($token)) {
                continue
            }

            $scopes = @($record.scopes)
            foreach ($scopeValue in $scopes) {
                if (([string]$scopeValue).Trim() -eq $Scope) {
                    return $token
                }
            }
        }
    }

    foreach ($envName in $singleEnvNames) {
        $token = Get-CexEnvValue -Name $envName
        if ($token) {
            return $token
        }
    }

    $appEnv = Get-CexEnvValue -Name 'APP_ENV'
    if ($appEnv -and $appEnv.ToLowerInvariant() -eq 'dev') {
        return 'local-dev-admin-token'
    }

    throw "No admin token configured for scope '$Scope'"
}

function Get-CexAdminHeaders {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Scope
    )

    return @{ 'x-admin-token' = (Get-CexScopedAdminToken -Scope $Scope) }
}

function Get-CexGatewayApiKey {
    $token = Get-CexEnvValue -Name 'CEX_GATEWAY_API_KEY'
    if (-not $token) {
        $token = Get-CexEnvValue -Name 'LOCAL_DEV_API_KEY'
    }
    if (-not $token) {
        $token = 'local-dev-key'
    }

    return $token
}

function Get-CexGatewayApiHeaders {
    return @{ 'x-api-key' = (Get-CexGatewayApiKey) }
}

function New-CexLedgerAccount {
    param(
        [Parameter(Mandatory = $true)]
        [string]$OrgId,
        [Parameter(Mandatory = $true)]
        [double]$InitialBalance,
        [hashtable]$Headers
    )

    if (-not $PSBoundParameters.ContainsKey('Headers')) {
        $Headers = Get-CexAdminHeaders -Scope 'ledger:manage'
    }

    $body = @{
        org_id = $OrgId
        account_type = 'org_wallet'
        currency_unit = 'credit'
        initial_balance = $InitialBalance
    } | ConvertTo-Json

    return Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:7002/v1/accounts' -Headers $Headers -ContentType 'application/json' -Body $body
}

function New-CexGatewayInvocation {
    param(
        [Parameter(Mandatory = $true)]
        [string]$OrgId,
        [Parameter(Mandatory = $true)]
        [string]$AccountId,
        [Parameter(Mandatory = $true)]
        [string]$Prompt,
        [Parameter(Mandatory = $true)]
        [double]$ReserveAmount,
        [hashtable]$Headers,
        [switch]$AllowErrorResponse
    )

    if (-not $PSBoundParameters.ContainsKey('Headers')) {
        $Headers = Get-CexGatewayApiHeaders
    }

    $body = @{
        org_id = $OrgId
        account_id = $AccountId
        prompt = $Prompt
        reserve_amount = $ReserveAmount
    } | ConvertTo-Json

    try {
        return Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:8080/v1/invocations' -Headers $Headers -ContentType 'application/json' -Body $body
    }
    catch {
        if ($AllowErrorResponse -and $_.ErrorDetails.Message) {
            return $_.ErrorDetails.Message | ConvertFrom-Json
        }
        throw
    }
}

function Restart-CexDetachedRuntime {
    param(
        [switch]$SkipBuild = $true
    )

    $args = @('-ExecutionPolicy', 'Bypass', '-File', (Join-Path $script:ProjectRoot 'scripts/start-local-runtime-detached.ps1'))
    if ($SkipBuild) {
        $args += '-SkipBuild'
    }
    $args += '-Restart'

    & powershell @args | Out-Null
}

function Assert-CexInvocationExecutionSnapshot {
    param(
        [Parameter(Mandatory = $true)]
        $Invocation,
        [Parameter(Mandatory = $true)]
        $Execution
    )

    if (-not $Invocation.execution) {
        throw 'Invocation execution snapshot missing'
    }

    if ($Invocation.execution.dispatch_mode -ne $Execution.dispatch_mode) {
        throw "Invocation execution dispatch_mode expected $($Execution.dispatch_mode), got $($Invocation.execution.dispatch_mode)"
    }
    if ([int]$Invocation.execution.attempt_count -ne [int]$Execution.attempt_count) {
        throw "Invocation execution attempt_count expected $($Execution.attempt_count), got $($Invocation.execution.attempt_count)"
    }
    if ([int]$Invocation.execution.max_attempts -ne [int]$Execution.max_attempts) {
        throw "Invocation execution max_attempts expected $($Execution.max_attempts), got $($Invocation.execution.max_attempts)"
    }

    $expectedRemaining = [math]::Max([int]$Execution.max_attempts - [int]$Execution.attempt_count, 0)
    if ([int]$Invocation.execution.attempts_remaining -ne $expectedRemaining) {
        throw "Invocation execution attempts_remaining expected $expectedRemaining, got $($Invocation.execution.attempts_remaining)"
    }

    $expectedExhausted = ($expectedRemaining -eq 0)
    if ([bool]$Invocation.execution.retry_budget_exhausted -ne $expectedExhausted) {
        throw "Invocation execution retry_budget_exhausted expected $expectedExhausted, got $($Invocation.execution.retry_budget_exhausted)"
    }
}

function Get-CexInvocationExecutionSnapshotObject {
    param(
        [Parameter(Mandatory = $true)]
        $Invocation,
        $Execution
    )

    if ($null -ne $Execution) {
        Assert-CexInvocationExecutionSnapshot -Invocation $Invocation -Execution $Execution
    }
    elseif (-not $Invocation.execution) {
        throw 'Invocation execution snapshot missing'
    }

    return [ordered]@{
        dispatch_mode = $Invocation.execution.dispatch_mode
        attempt_count = [int]$Invocation.execution.attempt_count
        max_attempts = [int]$Invocation.execution.max_attempts
        attempts_remaining = [int]$Invocation.execution.attempts_remaining
        retry_budget_exhausted = [bool]$Invocation.execution.retry_budget_exhausted
    }
}

function New-CexLegacyResultObject {
    param(
        [Parameter(Mandatory = $true)]
        [System.Collections.Specialized.OrderedDictionary]$Fields
    )

    $result = [ordered]@{ ok = $true }
    foreach ($key in $Fields.Keys) {
        $result[$key] = $Fields[$key]
    }

    return $result
}

function Get-CexAccountStateObject {
    param(
        [Parameter(Mandatory = $true)]
        [string]$AccountId,
        [Parameter(Mandatory = $true)]
        $Account
    )

    return [ordered]@{
        account_id = $AccountId
        balance = [double]$Account.balance
        reserved = [double]$Account.reserved
    }
}

function Get-CexInvocationExecutionStateObject {
    param(
        [Parameter(Mandatory = $true)]
        $Invocation,
        [Parameter(Mandatory = $true)]
        $Execution
    )

    return [ordered]@{
        invocation_id = $Invocation.invocation_id
        invocation_status = $Invocation.status
        execution_id = $Invocation.execution_id
        execution_status = $Execution.status
        execution_snapshot = Get-CexInvocationExecutionSnapshotObject -Invocation $Invocation -Execution $Execution
    }
}

function Get-CexHealthStateObject {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable]$Health
    )

    $result = [ordered]@{}
    foreach ($name in @('identity', 'ledger', 'execution', 'audit', 'gateway')) {
        if ($Health.ContainsKey($name)) {
            $result[$name] = [int]$Health[$name]
        }
    }

    foreach ($name in ($Health.Keys | Sort-Object)) {
        if (-not $result.Contains($name)) {
            $result[$name] = [int]$Health[$name]
        }
    }

    return $result
}

function Get-CexAuditEventTypeList {
    param(
        [Parameter(Mandatory = $true)]
        $AuditEvents
    )

    return @($AuditEvents | ForEach-Object { $_.event_type })
}

function Get-CexApprovalDbStateObject {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Status,
        [string]$ResolverId,
        [string]$Resolution
    )

    return [ordered]@{
        status = $Status
        resolver_id = $ResolverId
        resolution = $Resolution
    }
}

function Get-CexApprovalDbStateFromPsqlLine {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Line
    )

    $parts = $Line -split '\|', 3
    if ($parts.Count -lt 3) {
        throw "Unexpected approval row output: $Line"
    }

    return Get-CexApprovalDbStateObject -Status $parts[0] -ResolverId $parts[1] -Resolution $parts[2]
}

function Get-CexFlowCheckpointState {
    param(
        [Parameter(Mandatory = $true)]
        [string]$InvocationId,
        [Parameter(Mandatory = $true)]
        [string]$ExecutionId,
        [hashtable]$GatewayHeaders,
        [hashtable]$ExecutionHeaders,
        [string]$AccountId,
        [hashtable]$LedgerHeaders
    )

    if (-not $PSBoundParameters.ContainsKey('GatewayHeaders')) {
        $GatewayHeaders = Get-CexGatewayApiHeaders
    }
    if (-not $PSBoundParameters.ContainsKey('ExecutionHeaders')) {
        $ExecutionHeaders = Get-CexAdminHeaders -Scope 'executions:manage'
    }

    $invocation = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $InvocationId) -Headers $GatewayHeaders
    $execution = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7003/v1/executions/{0}" -f $ExecutionId) -Headers $ExecutionHeaders

    $account = $null
    if ($PSBoundParameters.ContainsKey('AccountId') -and -not [string]::IsNullOrWhiteSpace($AccountId)) {
        if (-not $PSBoundParameters.ContainsKey('LedgerHeaders')) {
            $LedgerHeaders = Get-CexAdminHeaders -Scope 'ledger:manage'
        }
        $account = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $AccountId) -Headers $LedgerHeaders
    }

    $result = [ordered]@{
        invocation = $invocation
        execution = $execution
    }
    if ($null -ne $account) {
        $result.account = $account
    }

    $result.checkpoint = Get-CexFlowCheckpointObject -Invocation $invocation -Execution $execution -AccountId $AccountId -Account $account
    return $result
}

function Get-CexFlowCheckpointObject {
    param(
        [Parameter(Mandatory = $true)]
        $Invocation,
        [Parameter(Mandatory = $true)]
        $Execution,
        [string]$AccountId,
        $Account
    )

    $checkpoint = [ordered]@{
        invocation_execution = Get-CexInvocationExecutionStateObject -Invocation $Invocation -Execution $Execution
    }

    if ($PSBoundParameters.ContainsKey('Account') -and $null -ne $Account) {
        $checkpoint.account = Get-CexAccountStateObject -AccountId $AccountId -Account $Account
    }

    return $checkpoint
}

function Enter-CexVsDevShell {
    if (-not $IsWindows) {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            throw 'cargo not found in PATH'
        }
        return
    }

    $launchScript = 'D:\VSstudio\Common7\Tools\Launch-VsDevShell.ps1'
    if (-not (Test-Path $launchScript)) {
        throw "Launch-VsDevShell.ps1 not found at $launchScript"
    }

    . $launchScript -Arch amd64 -HostArch amd64
    $cargoBin = Join-Path $HOME '.cargo\bin'
    if (Test-Path $cargoBin) {
        $env:PATH = "$cargoBin;$env:PATH"
    }
}

function Get-CexDockerExe {
    if (-not $IsWindows) {
        $dockerCommand = Get-Command docker -ErrorAction SilentlyContinue
        if ($dockerCommand) {
            & $dockerCommand.Source info *> $null
            if ($LASTEXITCODE -eq 0) {
                return $dockerCommand.Source
            }

            $sudoCommand = Get-Command sudo -ErrorAction SilentlyContinue
            if ($sudoCommand) {
                & $sudoCommand.Source -n $dockerCommand.Source info *> $null
                if ($LASTEXITCODE -eq 0) {
                    $shimDir = Join-Path $script:ProjectRoot 'run/powershell-shims'
                    New-Item -ItemType Directory -Force -Path $shimDir | Out-Null
                    $shimPath = Join-Path $shimDir 'docker.exe'
                    @('#!/usr/bin/env bash', 'exec sudo -n docker "$@"') -join "`n" |
                        Set-Content -LiteralPath $shimPath -Encoding utf8NoBOM
                    chmod +x $shimPath
                    return $shimPath
                }
            }

            return $dockerCommand.Source
        }
    }

    foreach ($candidate in @(
        'D:\Docker\DockerDesktop\resources\bin\docker.exe',
        'C:\Program Files\Docker\Docker\resources\bin\docker.exe'
    )) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    throw 'docker.exe not found'
}

function Get-CexPowerShellExe {
    foreach ($candidate in @('powershell.exe', 'powershell', 'pwsh')) {
        $command = Get-Command $candidate -ErrorAction SilentlyContinue
        if ($command) {
            return $command.Source
        }
    }

    throw 'PowerShell executable not found'
}

function Wait-CexPostgresReady {
    param(
        [int]$TimeoutSeconds = 60
    )

    $dockerExe = Get-CexDockerExe
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        $result = & $dockerExe exec cex-postgres-1 pg_isready -U postgres -d cex_ai 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Host 'Postgres is ready.'
            return
        }
        Start-Sleep -Seconds 2
    } while ((Get-Date) -lt $deadline)

    throw 'Postgres did not become ready in time.'
}

[CmdletBinding()]
param(
    [string]$Path = (Join-Path (Split-Path -Parent $PSScriptRoot) '.env')
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv -Path $Path

function Get-EnvValue {
    param([Parameter(Mandatory = $true)][string]$Name)

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

function Parse-AdminBundle {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Raw
    )

    try {
        $parsed = $Raw | ConvertFrom-Json -Depth 16
    }
    catch {
        throw "$Name is not valid JSON: $($_.Exception.Message)"
    }

    $records = @($parsed)
    if ($records.Count -eq 0) {
        throw "$Name must contain at least one admin token record"
    }

    return $records
}

function Get-RecordsWithScope {
    param(
        [Parameter(Mandatory = $true)]$Records,
        [Parameter(Mandatory = $true)][string]$Scope
    )

    $matches = @()
    foreach ($record in @($Records)) {
        $token = [string]$record.token
        if ([string]::IsNullOrWhiteSpace($token)) {
            continue
        }

        $scopes = @($record.scopes)
        $hasScope = $false
        foreach ($scopeValue in $scopes) {
            if (([string]$scopeValue).Trim() -eq $Scope) {
                $hasScope = $true
                break
            }
        }

        if ($hasScope) {
            $matches += $record
        }
    }

    return @($matches)
}

function Get-UniqueTokens {
    param([Parameter(Mandatory = $true)]$Records)

    return @(
        $Records |
            ForEach-Object { ([string]$_.token).Trim() } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            Select-Object -Unique
    )
}

function Test-OptionalOrgIds {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)]$Records
    )

    foreach ($record in @($Records)) {
        if ($null -eq $record.org_ids) {
            continue
        }

        $orgIds = @($record.org_ids)
        if ($orgIds.Count -eq 0) {
            throw "$Name contains a record with org_ids present but empty; omit org_ids for unrestricted access or provide at least one org id"
        }

        foreach ($orgId in $orgIds) {
            if ([string]::IsNullOrWhiteSpace(([string]$orgId).Trim())) {
                throw "$Name contains a record with a blank org_ids entry"
            }
        }
    }
}

$identityBundleRaw = Get-EnvValue -Name 'IDENTITY_ADMIN_TOKENS_JSON'
$auditBundleRaw = Get-EnvValue -Name 'AUDIT_ADMIN_TOKENS_JSON'
$executionBundleRaw = Get-EnvValue -Name 'EXECUTION_ADMIN_TOKENS_JSON'
$ledgerBundleRaw = Get-EnvValue -Name 'LEDGER_ADMIN_TOKENS_JSON'
$legacyIdentityToken = Get-EnvValue -Name 'IDENTITY_ADMIN_TOKEN'
$legacyAuditToken = Get-EnvValue -Name 'AUDIT_ADMIN_TOKEN'
$legacyExecutionToken = Get-EnvValue -Name 'EXECUTION_ADMIN_TOKEN'
$legacyLedgerToken = Get-EnvValue -Name 'LEDGER_ADMIN_TOKEN'

if (-not $identityBundleRaw) {
    throw 'IDENTITY_ADMIN_TOKENS_JSON must be set for split-admin rehearsal'
}

if (-not $auditBundleRaw) {
    throw 'AUDIT_ADMIN_TOKENS_JSON must be set for split-admin rehearsal'
}

if (-not $executionBundleRaw) {
    throw 'EXECUTION_ADMIN_TOKENS_JSON must be set for split-admin rehearsal'
}

if (-not $ledgerBundleRaw) {
    throw 'LEDGER_ADMIN_TOKENS_JSON must be set for split-admin rehearsal'
}

if ($legacyIdentityToken) {
    throw 'IDENTITY_ADMIN_TOKEN should be unset/empty during split-admin rehearsal'
}

if ($legacyAuditToken) {
    throw 'AUDIT_ADMIN_TOKEN should be unset/empty during split-admin rehearsal'
}

if ($legacyExecutionToken) {
    throw 'EXECUTION_ADMIN_TOKEN should be unset/empty during split-admin rehearsal'
}

if ($legacyLedgerToken) {
    throw 'LEDGER_ADMIN_TOKEN should be unset/empty during split-admin rehearsal'
}

$identityBundle = Parse-AdminBundle -Name 'IDENTITY_ADMIN_TOKENS_JSON' -Raw $identityBundleRaw
$auditBundle = Parse-AdminBundle -Name 'AUDIT_ADMIN_TOKENS_JSON' -Raw $auditBundleRaw
$executionBundle = Parse-AdminBundle -Name 'EXECUTION_ADMIN_TOKENS_JSON' -Raw $executionBundleRaw
$ledgerBundle = Parse-AdminBundle -Name 'LEDGER_ADMIN_TOKENS_JSON' -Raw $ledgerBundleRaw

Test-OptionalOrgIds -Name 'IDENTITY_ADMIN_TOKENS_JSON' -Records $identityBundle
Test-OptionalOrgIds -Name 'AUDIT_ADMIN_TOKENS_JSON' -Records $auditBundle
Test-OptionalOrgIds -Name 'EXECUTION_ADMIN_TOKENS_JSON' -Records $executionBundle
Test-OptionalOrgIds -Name 'LEDGER_ADMIN_TOKENS_JSON' -Records $ledgerBundle

$identityManagers = Get-RecordsWithScope -Records $identityBundle -Scope 'api_keys:manage'
if ($identityManagers.Count -eq 0) {
    throw 'IDENTITY_ADMIN_TOKENS_JSON must contain at least one token with api_keys:manage'
}

$auditReaders = Get-RecordsWithScope -Records $auditBundle -Scope 'audit:read'
if ($auditReaders.Count -eq 0) {
    throw 'AUDIT_ADMIN_TOKENS_JSON must contain at least one token with audit:read'
}

$executionManagers = Get-RecordsWithScope -Records $executionBundle -Scope 'executions:manage'
if ($executionManagers.Count -eq 0) {
    throw 'EXECUTION_ADMIN_TOKENS_JSON must contain at least one token with executions:manage'
}

$ledgerManagers = Get-RecordsWithScope -Records $ledgerBundle -Scope 'ledger:manage'
if ($ledgerManagers.Count -eq 0) {
    throw 'LEDGER_ADMIN_TOKENS_JSON must contain at least one token with ledger:manage'
}

$executionReaders = Get-RecordsWithScope -Records $executionBundle -Scope 'executions:read'
$ledgerReaders = Get-RecordsWithScope -Records $ledgerBundle -Scope 'ledger:read'
$identityTokens = Get-UniqueTokens -Records $identityManagers
$auditTokens = Get-UniqueTokens -Records $auditReaders
$executionManageTokens = Get-UniqueTokens -Records $executionManagers
$executionReadTokens = Get-UniqueTokens -Records $executionReaders
$ledgerManageTokens = Get-UniqueTokens -Records $ledgerManagers
$ledgerReadTokens = Get-UniqueTokens -Records $ledgerReaders
$overlap = @($identityTokens | Where-Object { $auditTokens -contains $_ })
if ($overlap.Count -gt 0) {
    throw "split-admin rehearsal expects distinct key-management and audit-read tokens; overlapping token(s): $($overlap -join ', ')"
}

Write-Host 'Split-admin env validation passed.'
Write-Host "  .env path: $Path"
Write-Host "  identity manage tokens: $($identityTokens -join ', ')"
Write-Host "  audit read tokens: $($auditTokens -join ', ')"
Write-Host "  execution manage tokens: $($executionManageTokens -join ', ')"
if ($executionReadTokens.Count -gt 0) {
    Write-Host "  execution read tokens: $($executionReadTokens -join ', ')"
}
else {
    Write-Host '  execution read tokens: none declared (execution-manage token will also satisfy read paths)'
}
Write-Host "  ledger manage tokens: $($ledgerManageTokens -join ', ')"
if ($ledgerReadTokens.Count -gt 0) {
    Write-Host "  ledger read tokens: $($ledgerReadTokens -join ', ')"
}
else {
    Write-Host '  ledger read tokens: none declared (ledger-manage token will also satisfy read paths)'
}
Write-Host '  note: identity, audit, execution, and ledger principals may optionally declare org_ids to constrain service-local admin access'
Write-Host 'Next step:'
Write-Host '  powershell -ExecutionPolicy Bypass -File .\gate-local.ps1'

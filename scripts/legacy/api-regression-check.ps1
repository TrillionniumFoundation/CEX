# DEPRECATED REGRESSION SCRIPT
# Regression coverage for this script now exists in Rust tests and the unified gate.
# Preferred entrypoints:
#   powershell -ExecutionPolicy Bypass -File .\gate-local.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\rust-regression-check.ps1
# Coverage details: docs/REGRESSION-COVERAGE-MATRIX.md
# This script is retained only as a compatibility/manual probe.
[CmdletBinding()]
param(
    [string]$OrgId = $(if ($env:DEV_ORG_ID) { $env:DEV_ORG_ID } else { '00000000-0000-0000-0000-00000000ce01' }),
    [double]$InitialBalance = 100,
    [double]$ReserveAmount = 5
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$gatewayHeaders = Get-CexGatewayApiHeaders
$ledgerHeaders = Get-CexAdminHeaders -Scope 'ledger:manage'
$executionHeaders = Get-CexAdminHeaders -Scope 'executions:manage'
$auditHeaders = Get-CexAdminHeaders -Scope 'audit:read'

$healthEndpoints = @(
    @{ name = 'identity';  url = 'http://127.0.0.1:7001/health' },
    @{ name = 'ledger';    url = 'http://127.0.0.1:7002/health' },
    @{ name = 'execution'; url = 'http://127.0.0.1:7003/health' },
    @{ name = 'audit';     url = 'http://127.0.0.1:7004/health' },
    @{ name = 'gateway';   url = 'http://127.0.0.1:8080/health' }
)

$health = @{}
foreach ($ep in $healthEndpoints) {
    try {
        $resp = Invoke-WebRequest -UseBasicParsing -Uri $ep.url -TimeoutSec 5
        $health[$ep.name] = [int]$resp.StatusCode
    }
    catch {
        throw "Health check failed for $($ep.name): $($_.Exception.Message)"
    }
}

$account = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders

$accountFetched1 = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $account.account_id) -Headers $ledgerHeaders
if ([double]$accountFetched1.balance -ne $InitialBalance) {
    throw "Unexpected initial balance: $($accountFetched1.balance)"
}
if ([double]$accountFetched1.reserved -ne 0) {
    throw "Unexpected initial reserved: $($accountFetched1.reserved)"
}

$invocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $account.account_id -Prompt 'api regression test' -ReserveAmount $ReserveAmount -Headers $gatewayHeaders -AllowErrorResponse

if ($invocation.status -ne 'Queued') {
    throw "Invocation status expected Queued, got $($invocation.status). failure_reason=$($invocation.failure_reason)"
}
if (-not $invocation.ledger_reserved) {
    throw 'Invocation expected ledger_reserved=true'
}
if (-not $invocation.execution_id) {
    throw 'Invocation expected a real execution_id'
}

$invocationFetched = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $invocation.invocation_id) -Headers $gatewayHeaders
$executionFetched = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7003/v1/executions/{0}" -f $invocation.execution_id) -Headers $executionHeaders
$accountFetched2 = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $account.account_id) -Headers $ledgerHeaders
$auditFetched = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7004/v1/audit/events/trace/{0}" -f $invocation.trace.trace_id) -Headers $auditHeaders

if ($invocationFetched.status -ne 'Queued') {
    throw "Fetched invocation status expected Queued, got $($invocationFetched.status)"
}
if ($executionFetched.status -ne 'Queued') {
    throw "Execution status expected Queued, got $($executionFetched.status)"
}
$checkpoint = Get-CexFlowCheckpointObject -Invocation $invocationFetched -Execution $executionFetched -AccountId $account.account_id -Account $accountFetched2
if ([math]::Abs([double]$accountFetched2.reserved - $ReserveAmount) -gt 0.000001) {
    throw "Reserved balance expected $ReserveAmount, got $($accountFetched2.reserved)"
}

New-CexLegacyResultObject -Fields ([ordered]@{
    org_id = $OrgId
    health = Get-CexHealthStateObject -Health $health
    checkpoint = $checkpoint
    audit_event_types = Get-CexAuditEventTypeList -AuditEvents $auditFetched
}) | ConvertTo-Json -Depth 6


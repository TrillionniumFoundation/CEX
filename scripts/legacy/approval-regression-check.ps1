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
    [double]$ReserveAmount = 25
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$gatewayHeaders = Get-CexGatewayApiHeaders
$ledgerHeaders = Get-CexAdminHeaders -Scope 'ledger:manage'
$executionHeaders = Get-CexAdminHeaders -Scope 'executions:manage'
$auditHeaders = Get-CexAdminHeaders -Scope 'audit:read'

$account = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders

$invocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $account.account_id -Prompt 'publish deployment package to external users' -ReserveAmount $ReserveAmount -Headers $gatewayHeaders -AllowErrorResponse

if ($invocation.status -ne 'AwaitingApproval') {
    throw "Expected invocation AwaitingApproval, got $($invocation.status)"
}
if (-not $invocation.execution_id) {
    throw 'Expected execution_id for approval flow'
}

$execution1 = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7003/v1/executions/{0}" -f $invocation.execution_id) -Headers $executionHeaders
if ($execution1.status -ne 'AwaitingApproval') {
    throw "Expected execution AwaitingApproval, got $($execution1.status)"
}
$initialCheckpoint = Get-CexFlowCheckpointObject -Invocation $invocation -Execution $execution1

$auditBefore = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7004/v1/audit/events/trace/{0}" -f $invocation.trace.trace_id) -Headers $auditHeaders

$approveReq = @{ approved_by = 'local-dev-approver'; note = 'approval regression test' } | ConvertTo-Json
$execution2 = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/approve" -f $invocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $approveReq
if ($execution2.status -ne 'Queued') {
    throw "Expected execution Queued after approval, got $($execution2.status)"
}
$invocationAfter = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $invocation.invocation_id) -Headers $gatewayHeaders
$finalCheckpoint = Get-CexFlowCheckpointObject -Invocation $invocationAfter -Execution $execution2

$auditAfter = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7004/v1/audit/events/trace/{0}" -f $invocation.trace.trace_id) -Headers $auditHeaders

New-CexLegacyResultObject -Fields ([ordered]@{
    trace_id = $invocation.trace.trace_id
    initial_checkpoint = $initialCheckpoint
    final_checkpoint = $finalCheckpoint
    policy_reason = $execution1.policy_reason
    audit_events_before = Get-CexAuditEventTypeList -AuditEvents $auditBefore
    audit_events_after = Get-CexAuditEventTypeList -AuditEvents $auditAfter
}) | ConvertTo-Json -Depth 6


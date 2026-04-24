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
$dockerExe = Get-CexDockerExe
Wait-CexPostgresReady

$account = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders

$invocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $account.account_id -Prompt 'publish deployment package to external users' -ReserveAmount $ReserveAmount -Headers $gatewayHeaders
if ($invocation.status -ne 'AwaitingApproval') {
    throw "Expected AwaitingApproval before reject, got $($invocation.status)"
}

$accountBefore = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $account.account_id) -Headers $ledgerHeaders
if ([math]::Abs([double]$accountBefore.reserved - $ReserveAmount) -gt 0.000001) {
    throw "Expected reserved=$ReserveAmount before reject, got $($accountBefore.reserved)"
}

$rejectReq = @{ rejected_by = 'local-dev-reviewer'; reason = 'policy denied in reject-refund regression' } | ConvertTo-Json
$executionAfterReject = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/reject" -f $invocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $rejectReq
if ($executionAfterReject.status -ne 'Cancelled') {
    throw "Expected execution Cancelled after reject, got $($executionAfterReject.status)"
}

$invocationAfterReject = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $invocation.invocation_id) -Headers $gatewayHeaders
$accountAfterReject = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $account.account_id) -Headers $ledgerHeaders
$executionAfterGet = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7003/v1/executions/{0}" -f $invocation.execution_id) -Headers $executionHeaders

if ($invocationAfterReject.status -ne 'Refunded') {
    throw "Expected invocation Refunded after reject, got $($invocationAfterReject.status)"
}
if (-not $invocationAfterReject.ledger_refunded) {
    throw 'Expected ledger_refunded=true after reject'
}
if ([math]::Abs([double]$accountAfterReject.balance - $InitialBalance) -gt 0.000001) {
    throw "Expected balance restored to $InitialBalance after reject, got $($accountAfterReject.balance)"
}
if ([math]::Abs([double]$accountAfterReject.reserved - 0) -gt 0.000001) {
    throw "Expected reserved=0 after reject, got $($accountAfterReject.reserved)"
}
if ($executionAfterGet.status -ne 'Cancelled') {
    throw "Expected execution Cancelled on GET after reject, got $($executionAfterGet.status)"
}

$approvalRow = & $dockerExe exec cex-postgres-1 psql -U postgres -d cex_ai -At -F '|' -c "select status, coalesce(resolver_id, ''), coalesce(resolution, '') from approvals where execution_id = '$($invocation.execution_id)' order by requested_at desc limit 1"
if ($LASTEXITCODE -ne 0) {
    throw 'Approval row query failed'
}
$approvalLine = ($approvalRow | Select-Object -Last 1).Trim()
$approvalDbState = Get-CexApprovalDbStateFromPsqlLine -Line $approvalLine
if ($approvalDbState.status -ne 'rejected') {
    throw "Expected approval row rejected, got $($approvalDbState.status)"
}
if ($approvalDbState.resolver_id -ne 'local-dev-reviewer') {
    throw "Expected resolver local-dev-reviewer, got $($approvalDbState.resolver_id)"
}

Restart-CexDetachedRuntime

$restartState = Get-CexFlowCheckpointState -InvocationId $invocation.invocation_id -ExecutionId $invocation.execution_id -AccountId $account.account_id -GatewayHeaders $gatewayHeaders -ExecutionHeaders $executionHeaders -LedgerHeaders $ledgerHeaders
if ($restartState.invocation.status -ne 'Refunded') { throw "Expected invocation Refunded after restart, got $($restartState.invocation.status)" }
if ([math]::Abs([double]$restartState.account.reserved - 0) -gt 0.000001) { throw "Expected reserved=0 after restart, got $($restartState.account.reserved)" }
if ($restartState.execution.status -ne 'Cancelled') { throw "Expected execution Cancelled after restart, got $($restartState.execution.status)" }

$checkpointAfterReject = Get-CexFlowCheckpointObject -Invocation $invocationAfterReject -Execution $executionAfterGet -AccountId $account.account_id -Account $accountAfterReject
$checkpointAfterRestart = $restartState.checkpoint

New-CexLegacyResultObject -Fields ([ordered]@{
    checkpoint_after_reject = $checkpointAfterReject
    checkpoint_after_restart = $checkpointAfterRestart
    approval_db = $approvalDbState
}) | ConvertTo-Json -Depth 6


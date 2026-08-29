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
    [double]$CancelReserveAmount = 8,
    [double]$TimeoutReserveAmount = 9
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$gatewayHeaders = Get-CexGatewayApiHeaders
$ledgerHeaders = Get-CexAdminHeaders -Scope 'ledger:manage'
$executionHeaders = Get-CexAdminHeaders -Scope 'executions:manage'

$accountCancel = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders
$cancelInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountCancel.account_id -Prompt 'lifecycle cancel regression' -ReserveAmount $CancelReserveAmount -Headers $gatewayHeaders
if ($cancelInvocation.status -ne 'Queued') { throw "Expected cancel invocation Queued, got $($cancelInvocation.status)" }

$dispatchReq = @{ dispatched_by = 'local-dev-runner'; note = 'lifecycle dispatch regression' } | ConvertTo-Json
$startReq = @{ started_by = 'local-dev-runner'; note = 'lifecycle start regression' } | ConvertTo-Json
$cancelReq = @{ cancelled_by = 'local-dev-runner'; reason = 'lifecycle cancel regression path' } | ConvertTo-Json

$cancelDispatch = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/dispatch" -f $cancelInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $dispatchReq
if ($cancelDispatch.status -ne 'Dispatching') { throw "Expected Dispatching, got $($cancelDispatch.status)" }
$cancelInvocationAfterDispatch = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $cancelInvocation.invocation_id) -Headers $gatewayHeaders
if ($cancelInvocationAfterDispatch.status -ne 'Dispatching') { throw "Expected invocation Dispatching, got $($cancelInvocationAfterDispatch.status)" }
$cancelCheckpointAfterDispatch = Get-CexFlowCheckpointObject -Invocation $cancelInvocationAfterDispatch -Execution $cancelDispatch

$cancelStart = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/start" -f $cancelInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $startReq
if ($cancelStart.status -ne 'Running') { throw "Expected Running, got $($cancelStart.status)" }
$cancelInvocationAfterStart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $cancelInvocation.invocation_id) -Headers $gatewayHeaders
if ($cancelInvocationAfterStart.status -ne 'Running') { throw "Expected invocation Running, got $($cancelInvocationAfterStart.status)" }
$cancelCheckpointAfterStart = Get-CexFlowCheckpointObject -Invocation $cancelInvocationAfterStart -Execution $cancelStart

$cancelFinal = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/cancel" -f $cancelInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $cancelReq
if ($cancelFinal.status -ne 'Cancelled') { throw "Expected Cancelled, got $($cancelFinal.status)" }
$cancelInvocationAfterFinal = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $cancelInvocation.invocation_id) -Headers $gatewayHeaders
$cancelAccountAfterFinal = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $accountCancel.account_id) -Headers $ledgerHeaders
if ($cancelInvocationAfterFinal.status -ne 'Refunded') { throw "Expected cancel invocation Refunded, got $($cancelInvocationAfterFinal.status)" }
$cancelCheckpointAfterFinal = Get-CexFlowCheckpointObject -Invocation $cancelInvocationAfterFinal -Execution $cancelFinal -AccountId $accountCancel.account_id -Account $cancelAccountAfterFinal
if ([math]::Abs([double]$cancelAccountAfterFinal.balance - $InitialBalance) -gt 0.000001) { throw "Expected cancel balance restored to $InitialBalance, got $($cancelAccountAfterFinal.balance)" }
if ([math]::Abs([double]$cancelAccountAfterFinal.reserved - 0) -gt 0.000001) { throw "Expected cancel reserved 0, got $($cancelAccountAfterFinal.reserved)" }

$accountTimeout = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders
$timeoutInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountTimeout.account_id -Prompt 'lifecycle timeout regression' -ReserveAmount $TimeoutReserveAmount -Headers $gatewayHeaders
if ($timeoutInvocation.status -ne 'Queued') { throw "Expected timeout invocation Queued, got $($timeoutInvocation.status)" }

$timeoutReq = @{ timed_out_by = 'local-dev-runner'; reason = 'lifecycle timeout regression path' } | ConvertTo-Json
$timeoutDispatch = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/dispatch" -f $timeoutInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $dispatchReq
if ($timeoutDispatch.status -ne 'Dispatching') { throw "Expected timeout Dispatching, got $($timeoutDispatch.status)" }
$timeoutStart = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/start" -f $timeoutInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $startReq
if ($timeoutStart.status -ne 'Running') { throw "Expected timeout Running, got $($timeoutStart.status)" }
$timeoutFinal = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/timeout" -f $timeoutInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $timeoutReq
if ($timeoutFinal.status -ne 'TimedOut') { throw "Expected TimedOut, got $($timeoutFinal.status)" }
$timeoutInvocationAfterFinal = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $timeoutInvocation.invocation_id) -Headers $gatewayHeaders
$timeoutAccountAfterFinal = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $accountTimeout.account_id) -Headers $ledgerHeaders
if ($timeoutInvocationAfterFinal.status -ne 'Refunded') { throw "Expected timeout invocation Refunded, got $($timeoutInvocationAfterFinal.status)" }
$timeoutCheckpointAfterFinal = Get-CexFlowCheckpointObject -Invocation $timeoutInvocationAfterFinal -Execution $timeoutFinal -AccountId $accountTimeout.account_id -Account $timeoutAccountAfterFinal
if ([math]::Abs([double]$timeoutAccountAfterFinal.balance - $InitialBalance) -gt 0.000001) { throw "Expected timeout balance restored to $InitialBalance, got $($timeoutAccountAfterFinal.balance)" }
if ([math]::Abs([double]$timeoutAccountAfterFinal.reserved - 0) -gt 0.000001) { throw "Expected timeout reserved 0, got $($timeoutAccountAfterFinal.reserved)" }

Restart-CexDetachedRuntime

$cancelRestartState = Get-CexFlowCheckpointState -InvocationId $cancelInvocation.invocation_id -ExecutionId $cancelInvocation.execution_id -AccountId $accountCancel.account_id -GatewayHeaders $gatewayHeaders -ExecutionHeaders $executionHeaders -LedgerHeaders $ledgerHeaders
$timeoutRestartState = Get-CexFlowCheckpointState -InvocationId $timeoutInvocation.invocation_id -ExecutionId $timeoutInvocation.execution_id -AccountId $accountTimeout.account_id -GatewayHeaders $gatewayHeaders -ExecutionHeaders $executionHeaders -LedgerHeaders $ledgerHeaders

if ($cancelRestartState.invocation.status -ne 'Refunded') { throw "Expected cancel invocation Refunded after restart, got $($cancelRestartState.invocation.status)" }
if ($cancelRestartState.execution.status -ne 'Cancelled') { throw "Expected cancel execution Cancelled after restart, got $($cancelRestartState.execution.status)" }
$cancelCheckpointAfterRestart = $cancelRestartState.checkpoint
if ([math]::Abs([double]$cancelRestartState.account.balance - $InitialBalance) -gt 0.000001) { throw "Expected cancel balance after restart $InitialBalance, got $($cancelRestartState.account.balance)" }
if ([math]::Abs([double]$cancelRestartState.account.reserved - 0) -gt 0.000001) { throw "Expected cancel reserved after restart 0, got $($cancelRestartState.account.reserved)" }
if ($timeoutRestartState.invocation.status -ne 'Refunded') { throw "Expected timeout invocation Refunded after restart, got $($timeoutRestartState.invocation.status)" }
if ($timeoutRestartState.execution.status -ne 'TimedOut') { throw "Expected timeout execution TimedOut after restart, got $($timeoutRestartState.execution.status)" }
$timeoutCheckpointAfterRestart = $timeoutRestartState.checkpoint
if ([math]::Abs([double]$timeoutRestartState.account.balance - $InitialBalance) -gt 0.000001) { throw "Expected timeout balance after restart $InitialBalance, got $($timeoutRestartState.account.balance)" }
if ([math]::Abs([double]$timeoutRestartState.account.reserved - 0) -gt 0.000001) { throw "Expected timeout reserved after restart 0, got $($timeoutRestartState.account.reserved)" }

New-CexLegacyResultObject -Fields ([ordered]@{
    cancel_checkpoint_after_dispatch = $cancelCheckpointAfterDispatch
    cancel_checkpoint_after_start = $cancelCheckpointAfterStart
    cancel_checkpoint_after_final = $cancelCheckpointAfterFinal
    cancel_checkpoint_after_restart = $cancelCheckpointAfterRestart
    timeout_checkpoint_after_final = $timeoutCheckpointAfterFinal
    timeout_checkpoint_after_restart = $timeoutCheckpointAfterRestart
}) | ConvertTo-Json -Depth 6


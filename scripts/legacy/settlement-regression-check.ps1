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
    [double]$SuccessReserveAmount = 5,
    [double]$FailReserveAmount = 6
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$gatewayHeaders = Get-CexGatewayApiHeaders
$ledgerHeaders = Get-CexAdminHeaders -Scope 'ledger:manage'
$executionHeaders = Get-CexAdminHeaders -Scope 'executions:manage'

$accountSuccess = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders
$successInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountSuccess.account_id -Prompt 'settlement success regression' -ReserveAmount $SuccessReserveAmount -Headers $gatewayHeaders
if ($successInvocation.status -ne 'Queued') { throw "Expected success invocation Queued, got $($successInvocation.status)" }

$successSettleReq = @{ settled_by = 'local-dev-runner'; note = 'settlement regression success path' } | ConvertTo-Json
$successExecutionAfter = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/succeed" -f $successInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $successSettleReq
if ($successExecutionAfter.status -ne 'Succeeded') { throw "Expected execution Succeeded, got $($successExecutionAfter.status)" }

$successInvocationAfter = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $successInvocation.invocation_id) -Headers $gatewayHeaders
$successAccountAfter = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $accountSuccess.account_id) -Headers $ledgerHeaders
if ($successInvocationAfter.status -ne 'Succeeded') { throw "Expected invocation Succeeded, got $($successInvocationAfter.status)" }
if ([math]::Abs([double]$successAccountAfter.balance - ($InitialBalance - $SuccessReserveAmount)) -gt 0.000001) { throw "Expected success balance $($InitialBalance - $SuccessReserveAmount), got $($successAccountAfter.balance)" }
if ([math]::Abs([double]$successAccountAfter.reserved - 0) -gt 0.000001) { throw "Expected success reserved 0, got $($successAccountAfter.reserved)" }

$accountFail = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders
$failInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountFail.account_id -Prompt 'settlement fail regression' -ReserveAmount $FailReserveAmount -Headers $gatewayHeaders
if ($failInvocation.status -ne 'Queued') { throw "Expected fail invocation Queued, got $($failInvocation.status)" }

$failExecutionReq = @{ failed_by = 'local-dev-runner'; reason = 'settlement regression failure path' } | ConvertTo-Json
$failExecutionAfter = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/fail" -f $failInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $failExecutionReq
if ($failExecutionAfter.status -ne 'Failed') { throw "Expected execution Failed, got $($failExecutionAfter.status)" }

$failInvocationAfter = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $failInvocation.invocation_id) -Headers $gatewayHeaders
$failAccountAfter = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $accountFail.account_id) -Headers $ledgerHeaders
if ($failInvocationAfter.status -ne 'Refunded') { throw "Expected failed invocation Refunded, got $($failInvocationAfter.status)" }
if (-not $failInvocationAfter.ledger_refunded) { throw 'Expected ledger_refunded=true for failed invocation' }
if ([math]::Abs([double]$failAccountAfter.balance - $InitialBalance) -gt 0.000001) { throw "Expected failed balance restored to $InitialBalance, got $($failAccountAfter.balance)" }
if ([math]::Abs([double]$failAccountAfter.reserved - 0) -gt 0.000001) { throw "Expected failed reserved 0, got $($failAccountAfter.reserved)" }

Restart-CexDetachedRuntime

$successRestartState = Get-CexFlowCheckpointState -InvocationId $successInvocation.invocation_id -ExecutionId $successInvocation.execution_id -AccountId $accountSuccess.account_id -GatewayHeaders $gatewayHeaders -ExecutionHeaders $executionHeaders -LedgerHeaders $ledgerHeaders
$failRestartState = Get-CexFlowCheckpointState -InvocationId $failInvocation.invocation_id -ExecutionId $failInvocation.execution_id -AccountId $accountFail.account_id -GatewayHeaders $gatewayHeaders -ExecutionHeaders $executionHeaders -LedgerHeaders $ledgerHeaders

if ($successRestartState.invocation.status -ne 'Succeeded') { throw "Expected success invocation Succeeded after restart, got $($successRestartState.invocation.status)" }
if ($successRestartState.execution.status -ne 'Succeeded') { throw "Expected success execution Succeeded after restart, got $($successRestartState.execution.status)" }
if ([math]::Abs([double]$successRestartState.account.balance - ($InitialBalance - $SuccessReserveAmount)) -gt 0.000001) { throw "Expected success balance after restart $($InitialBalance - $SuccessReserveAmount), got $($successRestartState.account.balance)" }
if ([math]::Abs([double]$successRestartState.account.reserved - 0) -gt 0.000001) { throw "Expected success reserved after restart 0, got $($successRestartState.account.reserved)" }
if ($failRestartState.invocation.status -ne 'Refunded') { throw "Expected failed invocation Refunded after restart, got $($failRestartState.invocation.status)" }
if ($failRestartState.execution.status -ne 'Failed') { throw "Expected failed execution Failed after restart, got $($failRestartState.execution.status)" }
if ([math]::Abs([double]$failRestartState.account.balance - $InitialBalance) -gt 0.000001) { throw "Expected failed balance after restart $InitialBalance, got $($failRestartState.account.balance)" }
if ([math]::Abs([double]$failRestartState.account.reserved - 0) -gt 0.000001) { throw "Expected failed reserved after restart 0, got $($failRestartState.account.reserved)" }

New-CexLegacyResultObject -Fields ([ordered]@{
    success_invocation_id = $successInvocation.invocation_id
    success_execution_id = $successInvocation.execution_id
    success_invocation_status_after_restart = $successRestartState.invocation.status
    success_execution_status_after_restart = $successRestartState.execution.status
    success_balance_after_restart = [double]$successRestartState.account.balance
    success_reserved_after_restart = [double]$successRestartState.account.reserved
    fail_invocation_id = $failInvocation.invocation_id
    fail_execution_id = $failInvocation.execution_id
    fail_invocation_status_after_restart = $failRestartState.invocation.status
    fail_execution_status_after_restart = $failRestartState.execution.status
    fail_balance_after_restart = [double]$failRestartState.account.balance
    fail_reserved_after_restart = [double]$failRestartState.account.reserved
}) | ConvertTo-Json -Depth 6


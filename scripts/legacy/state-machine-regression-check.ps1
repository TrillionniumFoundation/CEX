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
    [double]$SuccessReserveAmount = 4,
    [double]$CancelReserveAmount = 6,
    [double]$TimeoutReserveAmount = 7
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$gatewayHeaders = Get-CexGatewayApiHeaders
$ledgerHeaders = Get-CexAdminHeaders -Scope 'ledger:manage'
$executionHeaders = Get-CexAdminHeaders -Scope 'executions:manage'

function Invoke-CexJson {
    param(
        [string]$Method,
        [string]$Uri,
        [object]$Body = $null
    )

    $params = @{
        Method = $Method
        Uri = $Uri
        UseBasicParsing = $true
        ErrorAction = 'Stop'
    }
    if ($Uri -like 'http://127.0.0.1:8080/*') {
        $params.Headers = $gatewayHeaders
    }
    elseif ($Uri -like 'http://127.0.0.1:7002/*') {
        $params.Headers = $ledgerHeaders
    }
    elseif ($Uri -like 'http://127.0.0.1:7003/*') {
        $params.Headers = $executionHeaders
    }
    if ($null -ne $Body) {
        $params.ContentType = 'application/json'
        $params.Body = ($Body | ConvertTo-Json -Depth 8)
    }

    try {
        $resp = Invoke-WebRequest @params
        $parsed = $null
        if ($resp.Content) {
            try { $parsed = $resp.Content | ConvertFrom-Json } catch {}
        }
        [pscustomobject]@{
            statusCode = [int]$resp.StatusCode
            body = $parsed
            raw = $resp.Content
        }
    }
    catch {
        $statusCode = 0
        if ($_.Exception.Response -and $_.Exception.Response.StatusCode) {
            $statusCode = [int]$_.Exception.Response.StatusCode.value__
        }
        $raw = $_.ErrorDetails.Message
        $parsed = $null
        if ($raw) {
            try { $parsed = $raw | ConvertFrom-Json } catch {}
        }
        [pscustomobject]@{
            statusCode = $statusCode
            body = $parsed
            raw = $raw
        }
    }
}

$dispatchBody = @{ dispatched_by = 'local-dev-runner'; note = 'state machine regression dispatch' }
$startBody = @{ started_by = 'local-dev-runner'; note = 'state machine regression start' }
$cancelBody = @{ cancelled_by = 'local-dev-runner'; reason = 'state machine regression cancel' }
$timeoutBody = @{ timed_out_by = 'local-dev-runner'; reason = 'state machine regression timeout' }
$succeedBody = @{ settled_by = 'local-dev-runner'; note = 'state machine regression succeed' }
$failBody = @{ failed_by = 'local-dev-runner'; reason = 'state machine regression fail' }

# Success path: replay dispatch/start/succeed, reject invalid fail after succeed
$accountSuccess = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders
$successInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountSuccess.account_id -Prompt 'state machine success regression' -ReserveAmount $SuccessReserveAmount -Headers $gatewayHeaders
if ($successInvocation.status -ne 'Queued') { throw "Expected success invocation Queued, got $($successInvocation.status)" }

$successDispatch1 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/dispatch" -f $successInvocation.execution_id) -Body $dispatchBody
$successDispatch2 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/dispatch" -f $successInvocation.execution_id) -Body $dispatchBody
if ($successDispatch1.statusCode -ne 200 -or $successDispatch1.body.status -ne 'Dispatching') { throw 'Expected first dispatch 200/Dispatching' }
if ($successDispatch2.statusCode -ne 200 -or $successDispatch2.body.status -ne 'Dispatching') { throw 'Expected replay dispatch 200/Dispatching' }

$successStart1 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/start" -f $successInvocation.execution_id) -Body $startBody
$successStart2 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/start" -f $successInvocation.execution_id) -Body $startBody
if ($successStart1.statusCode -ne 200 -or $successStart1.body.status -ne 'Running') { throw 'Expected first start 200/Running' }
if ($successStart2.statusCode -ne 200 -or $successStart2.body.status -ne 'Running') { throw 'Expected replay start 200/Running' }

$successSettle1 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/succeed" -f $successInvocation.execution_id) -Body $succeedBody
$successSettle2 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/succeed" -f $successInvocation.execution_id) -Body $succeedBody
$successFailAfter = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/fail" -f $successInvocation.execution_id) -Body $failBody
if ($successSettle1.statusCode -ne 200 -or $successSettle1.body.status -ne 'Succeeded') { throw 'Expected first succeed 200/Succeeded' }
if ($successSettle2.statusCode -ne 200 -or $successSettle2.body.status -ne 'Succeeded') { throw 'Expected replay succeed 200/Succeeded' }
if ($successFailAfter.statusCode -ne 409) { throw "Expected fail after succeed to return 409, got $($successFailAfter.statusCode)" }

# Cancel path: replay cancel, reject invalid start after cancel
$accountCancel = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders
$cancelInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountCancel.account_id -Prompt 'state machine cancel regression' -ReserveAmount $CancelReserveAmount -Headers $gatewayHeaders
if ($cancelInvocation.status -ne 'Queued') { throw "Expected cancel invocation Queued, got $($cancelInvocation.status)" }
Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/dispatch" -f $cancelInvocation.execution_id) -Body $dispatchBody | Out-Null
Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/start" -f $cancelInvocation.execution_id) -Body $startBody | Out-Null
$cancel1 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/cancel" -f $cancelInvocation.execution_id) -Body $cancelBody
$cancel2 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/cancel" -f $cancelInvocation.execution_id) -Body $cancelBody
$cancelStartAfter = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/start" -f $cancelInvocation.execution_id) -Body $startBody
if ($cancel1.statusCode -ne 200 -or $cancel1.body.status -ne 'Cancelled') { throw 'Expected first cancel 200/Cancelled' }
if ($cancel2.statusCode -ne 200 -or $cancel2.body.status -ne 'Cancelled') { throw 'Expected replay cancel 200/Cancelled' }
if ($cancelStartAfter.statusCode -ne 409) { throw "Expected start after cancel to return 409, got $($cancelStartAfter.statusCode)" }

# Timeout path: replay timeout, reject invalid dispatch after timeout
$accountTimeout = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders
$timeoutInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountTimeout.account_id -Prompt 'state machine timeout regression' -ReserveAmount $TimeoutReserveAmount -Headers $gatewayHeaders
if ($timeoutInvocation.status -ne 'Queued') { throw "Expected timeout invocation Queued, got $($timeoutInvocation.status)" }
Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/dispatch" -f $timeoutInvocation.execution_id) -Body $dispatchBody | Out-Null
Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/start" -f $timeoutInvocation.execution_id) -Body $startBody | Out-Null
$timeout1 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/timeout" -f $timeoutInvocation.execution_id) -Body $timeoutBody
$timeout2 = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/timeout" -f $timeoutInvocation.execution_id) -Body $timeoutBody
$timeoutDispatchAfter = Invoke-CexJson -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/dispatch" -f $timeoutInvocation.execution_id) -Body $dispatchBody
if ($timeout1.statusCode -ne 200 -or $timeout1.body.status -ne 'TimedOut') { throw 'Expected first timeout 200/TimedOut' }
if ($timeout2.statusCode -ne 200 -or $timeout2.body.status -ne 'TimedOut') { throw 'Expected replay timeout 200/TimedOut' }
if ($timeoutDispatchAfter.statusCode -ne 409) { throw "Expected dispatch after timeout to return 409, got $($timeoutDispatchAfter.statusCode)" }

Restart-CexDetachedRuntime

$successExecutionAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7003/v1/executions/{0}" -f $successInvocation.execution_id) -Headers $executionHeaders
$successInvocationAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $successInvocation.invocation_id) -Headers $gatewayHeaders
$successAccountAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $accountSuccess.account_id) -Headers $ledgerHeaders
$cancelExecutionAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7003/v1/executions/{0}" -f $cancelInvocation.execution_id) -Headers $executionHeaders
$cancelInvocationAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $cancelInvocation.invocation_id) -Headers $gatewayHeaders
$cancelAccountAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $accountCancel.account_id) -Headers $ledgerHeaders
$timeoutExecutionAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7003/v1/executions/{0}" -f $timeoutInvocation.execution_id) -Headers $executionHeaders
$timeoutInvocationAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:8080/v1/invocations/{0}" -f $timeoutInvocation.invocation_id) -Headers $gatewayHeaders
$timeoutAccountAfterRestart = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $accountTimeout.account_id) -Headers $ledgerHeaders

if ($successExecutionAfterRestart.status -ne 'Succeeded') { throw "Expected success execution Succeeded after restart, got $($successExecutionAfterRestart.status)" }
if ($successInvocationAfterRestart.status -ne 'Succeeded') { throw "Expected success invocation Succeeded after restart, got $($successInvocationAfterRestart.status)" }
if ([math]::Abs([double]$successAccountAfterRestart.balance - ($InitialBalance - $SuccessReserveAmount)) -gt 0.000001) { throw 'Unexpected success balance after restart' }
if ([math]::Abs([double]$successAccountAfterRestart.reserved - 0) -gt 0.000001) { throw 'Unexpected success reserved after restart' }
if ($cancelExecutionAfterRestart.status -ne 'Cancelled') { throw "Expected cancel execution Cancelled after restart, got $($cancelExecutionAfterRestart.status)" }
if ($cancelInvocationAfterRestart.status -ne 'Refunded') { throw "Expected cancel invocation Refunded after restart, got $($cancelInvocationAfterRestart.status)" }
if ([math]::Abs([double]$cancelAccountAfterRestart.balance - $InitialBalance) -gt 0.000001) { throw 'Unexpected cancel balance after restart' }
if ([math]::Abs([double]$cancelAccountAfterRestart.reserved - 0) -gt 0.000001) { throw 'Unexpected cancel reserved after restart' }
if ($timeoutExecutionAfterRestart.status -ne 'TimedOut') { throw "Expected timeout execution TimedOut after restart, got $($timeoutExecutionAfterRestart.status)" }
if ($timeoutInvocationAfterRestart.status -ne 'Refunded') { throw "Expected timeout invocation Refunded after restart, got $($timeoutInvocationAfterRestart.status)" }
if ([math]::Abs([double]$timeoutAccountAfterRestart.balance - $InitialBalance) -gt 0.000001) { throw 'Unexpected timeout balance after restart' }
if ([math]::Abs([double]$timeoutAccountAfterRestart.reserved - 0) -gt 0.000001) { throw 'Unexpected timeout reserved after restart' }

$result = [ordered]@{
    ok = $true
    success_replay_dispatch_status = $successDispatch2.body.status
    success_replay_start_status = $successStart2.body.status
    success_replay_succeed_status = $successSettle2.body.status
    success_invalid_fail_status_code = $successFailAfter.statusCode
    cancel_replay_cancel_status = $cancel2.body.status
    cancel_invalid_start_status_code = $cancelStartAfter.statusCode
    timeout_replay_timeout_status = $timeout2.body.status
    timeout_invalid_dispatch_status_code = $timeoutDispatchAfter.statusCode
    success_execution_status_after_restart = $successExecutionAfterRestart.status
    cancel_execution_status_after_restart = $cancelExecutionAfterRestart.status
    timeout_execution_status_after_restart = $timeoutExecutionAfterRestart.status
}

$result | ConvertTo-Json -Depth 6


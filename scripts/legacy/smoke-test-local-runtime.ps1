# DEPRECATED REGRESSION SCRIPT
# Regression coverage for this script now exists in Rust tests and the unified gate.
# Preferred entrypoints:
#   powershell -ExecutionPolicy Bypass -File .\gate-local.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\rust-regression-check.ps1
# Coverage details: docs/REGRESSION-COVERAGE-MATRIX.md
# This script is retained only as a compatibility/manual probe.
[CmdletBinding()]
param(
    [string]$OrgId = '00000000-0000-0000-0000-00000000ce01',
    [double]$InitialBalance = 100,
    [double]$ReserveAmount = 5
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$gatewayHeaders = Get-CexGatewayApiHeaders
$ledgerHeaders = Get-CexAdminHeaders -Scope 'ledger:manage'

$accountResp = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders

$invResp = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountResp.account_id -Prompt 'local detached runtime smoke test' -ReserveAmount $ReserveAmount -Headers $gatewayHeaders -AllowErrorResponse

$acct = Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7002/v1/accounts/{0}" -f $accountResp.account_id) -Headers $ledgerHeaders

[pscustomobject]@{
    org_id = $OrgId
    account_id = $accountResp.account_id
    invocation_id = $invResp.invocation_id
    invocation_status = $invResp.status
    execution_id = $invResp.execution_id
    ledger_reserved = $invResp.ledger_reserved
    ledger_refunded = $invResp.ledger_refunded
    failure_reason = $invResp.failure_reason
    balance = $acct.balance
    reserved = $acct.reserved
} | ConvertTo-Json -Depth 5




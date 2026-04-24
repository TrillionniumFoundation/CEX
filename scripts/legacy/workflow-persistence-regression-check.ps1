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
    [double]$AutoReserveAmount = 5,
    [double]$ApprovalReserveAmount = 25
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

$accountAuto = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders
$accountApproval = New-CexLedgerAccount -OrgId $OrgId -InitialBalance $InitialBalance -Headers $ledgerHeaders

$autoInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountAuto.account_id -Prompt 'workflow persistence auto path' -ReserveAmount $AutoReserveAmount -Headers $gatewayHeaders
if ($autoInvocation.status -ne 'Queued') { throw "Expected auto invocation Queued, got $($autoInvocation.status)" }

$approvalInvocation = New-CexGatewayInvocation -OrgId $OrgId -AccountId $accountApproval.account_id -Prompt 'publish deployment package to external users' -ReserveAmount $ApprovalReserveAmount -Headers $gatewayHeaders
if ($approvalInvocation.status -ne 'AwaitingApproval') { throw "Expected approval invocation AwaitingApproval, got $($approvalInvocation.status)" }

$approveBody = @{ approved_by = 'local-dev-approver'; note = 'workflow persistence regression' } | ConvertTo-Json
$approvedExecution = Invoke-RestMethod -Method Post -Uri ("http://127.0.0.1:7003/v1/executions/{0}/approve" -f $approvalInvocation.execution_id) -Headers $executionHeaders -ContentType 'application/json' -Body $approveBody
if ($approvedExecution.status -ne 'Queued') { throw "Expected approved execution Queued, got $($approvedExecution.status)" }

Restart-CexDetachedRuntime

$autoRestartState = Get-CexFlowCheckpointState -InvocationId $autoInvocation.invocation_id -ExecutionId $autoInvocation.execution_id -GatewayHeaders $gatewayHeaders -ExecutionHeaders $executionHeaders
$approvalRestartState = Get-CexFlowCheckpointState -InvocationId $approvalInvocation.invocation_id -ExecutionId $approvalInvocation.execution_id -GatewayHeaders $gatewayHeaders -ExecutionHeaders $executionHeaders

$approvalRow = & $dockerExe exec cex-postgres-1 psql -U postgres -d cex_ai -At -F '|' -c "select status, coalesce(resolver_id, ''), coalesce(resolution, '') from approvals where execution_id = '$($approvalInvocation.execution_id)' order by requested_at desc limit 1"
if ($LASTEXITCODE -ne 0) {
    throw 'Approval row query failed'
}
$approvalLine = ($approvalRow | Select-Object -Last 1).Trim()
$approvalDbState = Get-CexApprovalDbStateFromPsqlLine -Line $approvalLine

if ($autoRestartState.invocation.status -ne 'Queued') { throw "Auto invocation after restart expected Queued, got $($autoRestartState.invocation.status)" }
if ($autoRestartState.execution.status -ne 'Queued') { throw "Auto execution after restart expected Queued, got $($autoRestartState.execution.status)" }
$autoCheckpointAfter = $autoRestartState.checkpoint
if ($approvalRestartState.invocation.status -ne 'Queued') { throw "Approval invocation after restart expected Queued, got $($approvalRestartState.invocation.status)" }
if ($approvalRestartState.execution.status -ne 'Queued') { throw "Approval execution after restart expected Queued, got $($approvalRestartState.execution.status)" }
$approvalCheckpointAfter = $approvalRestartState.checkpoint
if ($approvalDbState.status -ne 'approved') { throw "Approval row status expected approved, got $($approvalDbState.status)" }
if ($approvalDbState.resolver_id -ne 'local-dev-approver') { throw "Approval resolver expected local-dev-approver, got $($approvalDbState.resolver_id)" }

New-CexLegacyResultObject -Fields ([ordered]@{
    auto_checkpoint_after = $autoCheckpointAfter
    approval_checkpoint_after = $approvalCheckpointAfter
    approval_db = $approvalDbState
}) | ConvertTo-Json -Depth 6


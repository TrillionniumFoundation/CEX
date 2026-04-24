# DEPRECATED REGRESSION SCRIPT
# Regression coverage for this script now exists in Rust tests and the unified gate.
# Preferred entrypoints:
#   powershell -ExecutionPolicy Bypass -File .\gate-local.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\rust-regression-check.ps1
# Coverage details: docs/REGRESSION-COVERAGE-MATRIX.md
# This script is retained only as a compatibility/manual probe.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$auditHeaders = Get-CexAdminHeaders -Scope 'audit:read'

$traceId = [guid]::NewGuid().ToString()
$nonce = [guid]::NewGuid().ToString()
$createReq = @{
    trace_id = $traceId
    actor_type = 'audit-persistence-regression-check'
    actor_id = 'local-dev'
    event_type = 'audit.persistence.probe'
    payload = @{
        probe = 'restart-survival'
        nonce = $nonce
    }
} | ConvertTo-Json -Depth 8

$created = Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:7004/v1/audit/events' -ContentType 'application/json' -Body $createReq
$before = @(Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7004/v1/audit/events/trace/{0}" -f $traceId) -Headers $auditHeaders)
if ($before.Count -lt 1) {
    throw 'Expected at least one audit event before restart'
}

Restart-CexDetachedRuntime

$after = @(Invoke-RestMethod -Method Get -Uri ("http://127.0.0.1:7004/v1/audit/events/trace/{0}" -f $traceId) -Headers $auditHeaders)
$persisted = @($after | Where-Object { $_.event_id -eq $created.event_id }).Count -eq 1
if (-not $persisted) {
    throw 'Audit event did not survive runtime restart'
}

$result = [ordered]@{
    ok = $true
    trace_id = $traceId
    event_id = $created.event_id
    before_count = $before.Count
    after_count = $after.Count
    persisted = $persisted
    event_types_after = @($after | ForEach-Object { $_.event_type })
}

$result | ConvertTo-Json -Depth 6


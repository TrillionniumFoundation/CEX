[CmdletBinding()]
param(
    [switch]$ServiceLocalOnly,
    [switch]$SkipBlackbox,
    [switch]$KeepRuntimeDown,
    [switch]$SkipStatusCheck
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$rustGate = Join-Path $projectRoot 'scripts\rust-regression-check.ps1'

if (-not (Test-Path $rustGate)) {
    throw "Missing script: $rustGate"
}

$effectiveSkipBlackbox = $SkipBlackbox -or $ServiceLocalOnly
$effectiveKeepRuntimeDown = $KeepRuntimeDown -or $ServiceLocalOnly

Write-Host '==> gate-local.ps1'
Write-Host ("project_root={0}" -f $projectRoot)
Write-Host ("service_local_only={0}" -f ([bool]$ServiceLocalOnly))
Write-Host ("skip_blackbox={0}" -f ([bool]$effectiveSkipBlackbox))
Write-Host ("keep_runtime_down={0}" -f ([bool]$effectiveKeepRuntimeDown))
Write-Host ("skip_status_check={0}" -f ([bool]$SkipStatusCheck))

$params = @{}
if ($effectiveSkipBlackbox) { $params['SkipBlackbox'] = $true }
if ($effectiveKeepRuntimeDown) { $params['KeepRuntimeDown'] = $true }
if ($SkipStatusCheck) { $params['SkipStatusCheck'] = $true }

& $rustGate @params
exit $LASTEXITCODE

[CmdletBinding()]
param(
    [string[]]$Packages = @('identity-service','ledger-service','execution-service','audit-service','capability-service','gateway-service')
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
Enter-CexVsDevShell
Set-Location (Split-Path -Parent $PSScriptRoot)

$args = @('build')
foreach ($pkg in $Packages) {
    $args += @('-p', $pkg)
}

cargo @args

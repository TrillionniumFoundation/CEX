[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Package
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
Enter-CexVsDevShell
Set-Location (Split-Path -Parent $PSScriptRoot)

cargo run -p $Package

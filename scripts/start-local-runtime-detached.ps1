[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$Restart
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$runDir = Join-Path $projectRoot 'run'
New-Item -ItemType Directory -Force -Path $runDir | Out-Null

if ($Restart) {
    & (Join-Path $PSScriptRoot 'stop-local-runtime.ps1')
    Start-Sleep -Seconds 2
}

& (Join-Path $PSScriptRoot 'run-local-dev.ps1')
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'build-services.ps1')
}

$services = @('identity-service','ledger-service','execution-service','audit-service','capability-service','gateway-service')
foreach ($pkg in $services) {
    $pidFile = Join-Path $runDir ("{0}.pid" -f $pkg)
    $existingProc = $null

    if (Test-Path $pidFile) {
        try {
            $procId = [int](Get-Content -LiteralPath $pidFile -Raw)
            $existingProc = Get-Process -Id $procId -ErrorAction SilentlyContinue
        }
        catch {}
    }

    if (-not $existingProc) {
        $existingProc = Get-Process -Name $pkg -ErrorAction SilentlyContinue | Select-Object -First 1
    }

    if ($existingProc) {
        Write-Host "$pkg already running pid=$($existingProc.Id)"
        continue
    }

    Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue

    Start-Process -FilePath 'powershell.exe' `
        -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'service-host.ps1'),'-Package',$pkg) `
        -WorkingDirectory $projectRoot `
        -WindowStyle Hidden

    Start-Sleep -Seconds 1
}

Start-Sleep -Seconds 4
& (Join-Path $PSScriptRoot 'status-local-runtime.ps1')

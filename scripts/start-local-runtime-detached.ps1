[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$Restart
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
$runDir = Join-Path $projectRoot 'run'
$logDir = Join-Path $projectRoot 'logs'
New-Item -ItemType Directory -Force -Path $runDir | Out-Null
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

if ($Restart) {
    & (Join-Path $PSScriptRoot 'stop-local-runtime.ps1')
    Start-Sleep -Seconds 2
}

& (Join-Path $PSScriptRoot 'run-local-dev.ps1')
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'build-services.ps1')
}

$services = @('identity-service','ledger-service','execution-service','audit-service','capability-service','gateway-service')
$powerShellExe = Get-CexPowerShellExe
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

    $startProcessArgs = @{
        FilePath = $powerShellExe
        ArgumentList = @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'service-host.ps1'),'-Package',$pkg)
        WorkingDirectory = $projectRoot
        RedirectStandardOutput = (Join-Path $logDir ("{0}.host.out.log" -f $pkg))
        RedirectStandardError = (Join-Path $logDir ("{0}.host.err.log" -f $pkg))
    }
    if ($IsWindows) {
        $startProcessArgs['WindowStyle'] = 'Hidden'
    }
    Start-Process @startProcessArgs

    Start-Sleep -Seconds 1
}

Start-Sleep -Seconds 4
& (Join-Path $PSScriptRoot 'status-local-runtime.ps1')

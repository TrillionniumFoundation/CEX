[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$runDir = Join-Path $projectRoot 'run'
$services = @(
    @{ Package='identity-service'; Url='http://127.0.0.1:7001/health' },
    @{ Package='ledger-service'; Url='http://127.0.0.1:7002/health' },
    @{ Package='execution-service'; Url='http://127.0.0.1:7003/health' },
    @{ Package='audit-service'; Url='http://127.0.0.1:7004/health' },
    @{ Package='capability-service'; Url='http://127.0.0.1:7005/health' },
    @{ Package='gateway-service'; Url='http://127.0.0.1:8080/health' }
)

$rows = foreach ($svc in $services) {
    $pidFile = Join-Path $runDir ("{0}.pid" -f $svc.Package)
    $proc = $null

    if (Test-Path $pidFile) {
        try {
            $procIdFromFile = [int](Get-Content -LiteralPath $pidFile -Raw)
            $proc = Get-Process -Id $procIdFromFile -ErrorAction SilentlyContinue
        }
        catch {}
    }

    if (-not $proc) {
        $proc = Get-Process -Name $svc.Package -ErrorAction SilentlyContinue | Select-Object -First 1
    }

    $procId = if ($proc) { $proc.Id } else { $null }
    $alive = [bool]$proc

    $health = 'DOWN'
    try {
        $resp = Invoke-WebRequest -UseBasicParsing -Uri $svc.Url -TimeoutSec 5
        $health = [string]$resp.StatusCode
    }
    catch {}

    [pscustomobject]@{
        package = $svc.Package
        pid = $procId
        alive = $alive
        health = $health
    }
}

$rows | Format-Table -AutoSize

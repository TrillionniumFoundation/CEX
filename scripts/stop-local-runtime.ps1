[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$projectRoot = Split-Path -Parent $PSScriptRoot
$runDir = Join-Path $projectRoot 'run'
$services = @('identity-service','ledger-service','execution-service','audit-service','capability-service','gateway-service')

foreach ($pkg in $services) {
    $pidFile = Join-Path $runDir ("{0}.pid" -f $pkg)
    $metaFile = Join-Path $runDir ("{0}.json" -f $pkg)
    $stopped = $false

    if (Test-Path $pidFile) {
        try {
            $procId = [int](Get-Content -LiteralPath $pidFile -Raw)
            $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
            if ($proc) {
                Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
                Write-Host "$pkg : stopped pid=$procId"
                $stopped = $true
            }
            else {
                Write-Host "$pkg : stale pid file pid=$procId"
            }
        }
        catch {
            Write-Host "$pkg : invalid pid file"
        }
    }

    if (-not $stopped) {
        $procs = @(Get-Process -Name $pkg -ErrorAction SilentlyContinue)
        if ($procs.Count -gt 0) {
            foreach ($proc in $procs) {
                Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
                Write-Host "$pkg : stopped pid=$($proc.Id) (by name)"
            }
            $stopped = $true
        }
    }

    if (-not $stopped) {
        Write-Host "$pkg : no tracked process"
    }

    Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $metaFile -Force -ErrorAction SilentlyContinue
}

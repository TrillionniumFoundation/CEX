[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Package
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv

$logDir = Join-Path $projectRoot 'logs'
$runDir = Join-Path $projectRoot 'run'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
New-Item -ItemType Directory -Force -Path $runDir | Out-Null

$exeName = if ($IsWindows) { "{0}.exe" -f $Package } else { $Package }
$exe = Join-Path $projectRoot (Join-Path 'target/debug' $exeName)
if (-not (Test-Path $exe)) {
    throw "Built executable not found: $exe"
}

$logFile = Join-Path $logDir ("{0}.log" -f $Package)
$pidFile = Join-Path $runDir ("{0}.pid" -f $Package)
$metaFile = Join-Path $runDir ("{0}.json" -f $Package)

Set-Content -LiteralPath $pidFile -Value $PID -Encoding ascii
@{
    package = $Package
    pid = $PID
    started_at = (Get-Date).ToString('s')
    exe = $exe
    log = $logFile
} | ConvertTo-Json | Set-Content -LiteralPath $metaFile -Encoding UTF8

Add-Content -LiteralPath $logFile -Value ("[{0}] starting {1}" -f (Get-Date).ToString('s'), $Package)

try {
    Set-Location $projectRoot
    & $exe *>> $logFile
    $exitCode = $LASTEXITCODE
    if ($null -eq $exitCode) { $exitCode = 0 }
    Add-Content -LiteralPath $logFile -Value ("[{0}] exited code={1}" -f (Get-Date).ToString('s'), $exitCode)
    exit $exitCode
}
catch {
    Add-Content -LiteralPath $logFile -Value ("[{0}] exception {1}" -f (Get-Date).ToString('s'), $_)
    throw
}
finally {
    Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
}

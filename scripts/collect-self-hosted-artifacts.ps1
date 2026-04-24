[CmdletBinding()]
param(
    [string]$OutputDir
)

$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$projectRoot = Split-Path -Parent $PSScriptRoot

if (-not $OutputDir) {
    $OutputDir = Join-Path $projectRoot 'ci-artifacts\self-hosted-full-gate'
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$helperPath = Join-Path $PSScriptRoot '_dev-helpers.ps1'
if (Test-Path $helperPath) {
    . $helperPath
}

function Write-CaptureFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][scriptblock]$Script
    )

    try {
        $content = (& $Script 2>&1 | Out-String -Width 4096)
        if ([string]::IsNullOrWhiteSpace($content)) {
            $content = '<no output>'
        }
        Set-Content -LiteralPath $Path -Value $content -Encoding UTF8
    }
    catch {
        Set-Content -LiteralPath $Path -Value ("capture failed: {0}" -f $_.Exception.Message) -Encoding UTF8
    }
}

function Copy-TreeIfPresent {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    if (Test-Path $Destination) {
        Remove-Item -LiteralPath $Destination -Recurse -Force -ErrorAction SilentlyContinue
    }

    if (Test-Path $Source) {
        Copy-Item -LiteralPath $Source -Destination $Destination -Recurse -Force
    }
}

$meta = [ordered]@{
    collected_at = (Get-Date).ToString('s')
    computer_name = $env:COMPUTERNAME
    project_root = $projectRoot
    output_dir = $OutputDir
}
$meta | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $OutputDir 'collection-meta.json') -Encoding UTF8

Copy-TreeIfPresent -Source (Join-Path $projectRoot 'logs') -Destination (Join-Path $OutputDir 'logs')
Copy-TreeIfPresent -Source (Join-Path $projectRoot 'run') -Destination (Join-Path $OutputDir 'run')

Write-CaptureFile -Path (Join-Path $OutputDir 'tool-versions.txt') -Script {
    "captured_at=$(Get-Date -Format s)"
    try { cargo --version } catch { "cargo: $($_.Exception.Message)" }
    try { rustc --version } catch { "rustc: $($_.Exception.Message)" }
    try { powershell -NoProfile -Command "$PSVersionTable.PSVersion.ToString()" } catch { "powershell: $($_.Exception.Message)" }
}

Write-CaptureFile -Path (Join-Path $OutputDir 'status-local-runtime.txt') -Script {
    & (Join-Path $PSScriptRoot 'status-local-runtime.ps1')
}

Write-CaptureFile -Path (Join-Path $OutputDir 'port-listeners.txt') -Script {
    Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
        Where-Object { $_.LocalPort -in 7001,7002,7003,7004,8080,5432,6379,4222 } |
        Select-Object LocalAddress, LocalPort, OwningProcess |
        Sort-Object LocalPort |
        Format-Table -AutoSize
}

Write-CaptureFile -Path (Join-Path $OutputDir 'logs-dir.txt') -Script {
    if (Test-Path (Join-Path $projectRoot 'logs')) {
        Get-ChildItem -LiteralPath (Join-Path $projectRoot 'logs') -Force |
            Select-Object Name, Length, LastWriteTime |
            Sort-Object Name |
            Format-Table -AutoSize
    }
    else {
        'logs directory not present'
    }
}

Write-CaptureFile -Path (Join-Path $OutputDir 'run-dir.txt') -Script {
    if (Test-Path (Join-Path $projectRoot 'run')) {
        Get-ChildItem -LiteralPath (Join-Path $projectRoot 'run') -Force |
            Select-Object Name, Length, LastWriteTime |
            Sort-Object Name |
            Format-Table -AutoSize
    }
    else {
        'run directory not present'
    }
}

Write-CaptureFile -Path (Join-Path $OutputDir 'docker-version.txt') -Script {
    if (-not (Get-Command Get-CexDockerExe -ErrorAction SilentlyContinue)) {
        throw '_dev-helpers.ps1 / Get-CexDockerExe is unavailable'
    }

    $dockerExe = Get-CexDockerExe
    & $dockerExe version
}

Write-CaptureFile -Path (Join-Path $OutputDir 'docker-ps.txt') -Script {
    if (-not (Get-Command Get-CexDockerExe -ErrorAction SilentlyContinue)) {
        throw '_dev-helpers.ps1 / Get-CexDockerExe is unavailable'
    }

    $dockerExe = Get-CexDockerExe
    & $dockerExe ps -a
}

Write-CaptureFile -Path (Join-Path $OutputDir 'docker-compose-ps.txt') -Script {
    if (-not (Get-Command Get-CexDockerExe -ErrorAction SilentlyContinue)) {
        throw '_dev-helpers.ps1 / Get-CexDockerExe is unavailable'
    }

    $dockerExe = Get-CexDockerExe
    Push-Location $projectRoot
    try {
        & $dockerExe compose ps
    }
    finally {
        Pop-Location
    }
}

Write-Host ("Collected self-hosted gate artifacts at {0}" -f $OutputDir)

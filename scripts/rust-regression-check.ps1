[CmdletBinding()]
param(
    [switch]$SkipBlackbox,
    [switch]$KeepRuntimeDown,
    [switch]$SkipStatusCheck
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv

$projectRoot = Split-Path -Parent $PSScriptRoot
$steps = [System.Collections.Generic.List[object]]::new()
$currentStep = $null
$runtimeWasStopped = $false
$runtimeRestarted = $false

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][scriptblock]$Script
    )

    $script:currentStep = $Name
    Write-Host "==> $Name"
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $Script
    $sw.Stop()
    $script:steps.Add([ordered]@{
        name = $Name
        ok = $true
        seconds = [math]::Round($sw.Elapsed.TotalSeconds, 2)
    }) | Out-Null
}

function Ensure-RuntimeStarted {
    if ($KeepRuntimeDown) {
        return
    }

    Invoke-Step 'restart_runtime_detached' {
        & (Join-Path $PSScriptRoot 'start-local-runtime-detached.ps1') -SkipBuild -Restart
    }
    $script:runtimeRestarted = $true

    if (-not $SkipStatusCheck) {
        Invoke-Step 'status_runtime_after_restart' {
            & (Join-Path $PSScriptRoot 'status-local-runtime.ps1')
        }
    }
}

Push-Location $projectRoot
try {
    Invoke-Step 'stop_runtime_for_cargo_tests' {
        & (Join-Path $PSScriptRoot 'stop-local-runtime.ps1')
        Get-Process identity-service,ledger-service,execution-service,audit-service,gateway-service -ErrorAction SilentlyContinue |
            Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 2
    }
    $runtimeWasStopped = $true

    Invoke-Step 'cargo_test_execution_service' {
        cargo test -p execution-service
    }

    Invoke-Step 'cargo_test_identity_service' {
        cargo test -p identity-service
    }

    Invoke-Step 'cargo_test_gateway_service' {
        cargo test -p gateway-service
    }

    Invoke-Step 'cargo_test_ledger_service' {
        cargo test -p ledger-service
    }

    Invoke-Step 'cargo_test_audit_service' {
        cargo test -p audit-service
    }

    Ensure-RuntimeStarted

    if (-not $SkipBlackbox) {
        Invoke-Step 'cargo_test_audit_runtime_blackbox' {
            cargo test -p audit-service --test runtime_blackbox -- --ignored --test-threads=1
        }

        Invoke-Step 'cargo_test_identity_runtime_blackbox' {
            cargo test -p identity-service --test runtime_blackbox -- --ignored --test-threads=1
        }

        Invoke-Step 'cargo_test_gateway_runtime_blackbox' {
            cargo test -p gateway-service --test runtime_blackbox -- --ignored --test-threads=1
        }

        Invoke-Step 'cargo_test_gateway_runtime_approval_probe' {
            cargo test -p gateway-service --test runtime_approval_probe -- --ignored --test-threads=1
        }

        if (-not $SkipStatusCheck) {
            Invoke-Step 'status_runtime_after_blackbox' {
                & (Join-Path $PSScriptRoot 'status-local-runtime.ps1')
            }
        }
    }

    $result = [ordered]@{
        ok = $true
        blackbox = (-not $SkipBlackbox)
        runtime_restarted = $runtimeRestarted
        keep_runtime_down = [bool]$KeepRuntimeDown
        steps = @($steps)
    }

    $result | ConvertTo-Json -Depth 6
}
catch {
    $failure = $_
    Write-Error "rust-regression-check failed at step '$currentStep': $($failure.Exception.Message)"

    if ($runtimeWasStopped -and -not $runtimeRestarted -and -not $KeepRuntimeDown) {
        try {
            & (Join-Path $PSScriptRoot 'start-local-runtime-detached.ps1') -SkipBuild -Restart | Out-Host
            $runtimeRestarted = $true
        }
        catch {
            Write-Warning "failed to restart runtime during recovery: $($_.Exception.Message)"
        }
    }

    $result = [ordered]@{
        ok = $false
        failed_step = $currentStep
        error = $failure.Exception.Message
        blackbox = (-not $SkipBlackbox)
        runtime_restarted = $runtimeRestarted
        keep_runtime_down = [bool]$KeepRuntimeDown
        steps = @($steps)
    }

    $result | ConvertTo-Json -Depth 6
    exit 1
}
finally {
    Pop-Location
}


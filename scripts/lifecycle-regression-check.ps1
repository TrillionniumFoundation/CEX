# COMPATIBILITY SHIM
# Original script moved to scripts\\legacy\\lifecycle-regression-check.ps1.
# Preferred entrypoints:
#   powershell -ExecutionPolicy Bypass -File .\\gate-local.ps1
#   powershell -ExecutionPolicy Bypass -File .\\scripts\\rust-regression-check.ps1
# Admin-token precedence / split-admin guidance:
#   docs\admin-token-model.md
$legacyPath = Join-Path $PSScriptRoot 'legacy\\lifecycle-regression-check.ps1'
& $legacyPath @args
exit $LASTEXITCODE

# legacy shim: forward to parent runtime starter
& (Join-Path (Split-Path -Parent $PSScriptRoot) 'start-local-runtime-detached.ps1') @args
exit $LASTEXITCODE

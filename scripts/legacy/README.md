# Legacy regression scripts

This folder contains the original PowerShell regression scripts that were superseded by Rust coverage.

Original entrypoints under `scripts\` remain as compatibility shims and forward here.

Preferred modern entrypoints:

```powershell
powershell -ExecutionPolicy Bypass -File .\gate-local.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\rust-regression-check.ps1
```

# CI Gate

This repo now has two Rust validation layers plus optional legacy PowerShell probes.

## 1. Full local gate

Use this on a Windows dev machine that can run the detached local runtime and Docker-backed infra.

```powershell
powershell -ExecutionPolicy Bypass -File .\gate-local.ps1
```

What it does:

- stops detached local runtime first to avoid Windows `.exe` lock failures during `cargo test`
- runs service-local Rust tests for:
  - `execution-service`
  - `identity-service`
  - `gateway-service`
  - `ledger-service`
  - `audit-service`
- restarts detached local runtime
- verifies runtime health
- runs ignored live-runtime HTTP black-box tests
  - `audit-service/tests/runtime_blackbox.rs`
  - `identity-service/tests/runtime_blackbox.rs`
  - `gateway-service/tests/runtime_blackbox.rs`
- runs ignored live-runtime approval DB probe tests
- verifies runtime health again

This is the strongest local regression gate currently available.

If you want to rehearse split admin principals instead of the shared dev token, start from:

```text
docs/admin-token-model.md
.env.split-admin.example
```

Read the token model doc first, then copy `.env.split-admin.example` to `.env` before running the full gate on the Windows host, or use:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\use-split-admin-env.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\validate-split-admin-env.ps1
```

Practical expectation for that Windows-only rehearsal:
- `identity-service` management endpoints should succeed with the key-management token
- `audit-service` trace reads should succeed with the audit-read token
- direct `execution-service` admin/operator endpoints should succeed with the execution-manage token, and direct execution reads should accept execution-read or execution-manage
- direct `ledger-service` account and credit-mutation endpoints should succeed with the ledger-manage token, and direct account reads should accept ledger-read or ledger-manage
- the ignored runtime blackbox suite should no longer rely on a single shared `local-dev-admin-token`
- until this full gate is actually run on the Windows host, treat split-admin runtime support as **ready for validation**, not already end-to-end proven

## 2. Service-local-only gate

Use this when the environment does **not** have Docker/runtime support, but you still want the crate-local Rust checks.

```powershell
powershell -ExecutionPolicy Bypass -File .\gate-local.ps1 -ServiceLocalOnly
```

This mode:

- skips live runtime black-box / DB probe tests
- keeps runtime down after the cargo test phase
- is suitable for CI runners that only need service-local coverage

## 3. Linux equivalent full gate

Use this on a Linux host when you want the same practical validation shape as the Windows full gate, but you do not have `pwsh`/`powershell` available.

```bash
./scripts/gate-local-linux.sh
```

What it does:

- loads `.env` when present, otherwise falls back to `.env.example`
- applies SQL migrations and seeds the default local-dev org/key when DB bootstrap is available
- runs `cargo test --workspace`
- starts a repo-local detached Linux runtime through `scripts/runtime-manager-linux.sh`
- explicitly disables the detached queued-worker loop during the gate (`CEX_ENABLE_QUEUED_WORKER=0`) so the ignored runtime suites remain deterministic
- if `run/openclaw-cex/openclaw.json` exists, the runtime manager auto-scopes OpenClaw bridge/import calls to that isolated repo-local state instead of the default `~/.openclaw/main` scope
- injects a temporary `powershell` compatibility shim so the existing ignored Rust runtime suites can still call the expected repo scripts
- runs the ignored runtime black-box and approval DB-probe tests for:
  - `audit-service/tests/runtime_blackbox.rs`
  - `identity-service/tests/runtime_blackbox.rs`
  - `gateway-service/tests/runtime_blackbox.rs`
  - `gateway-service/tests/runtime_approval_probe.rs`
- verifies runtime health again at the end

Helpful flags:

```bash
./scripts/gate-local-linux.sh --service-local-only
./scripts/gate-local-linux.sh --skip-db-bootstrap
./scripts/gate-local-linux.sh --skip-workspace
```

Use `--skip-db-bootstrap` when the database is already provisioned but the current user does not have a usable `psql` client or Docker access for applying migrations/seeding.

## Under the hood

`gate-local.ps1` is the friendly root entrypoint.

It delegates to:

- `scripts\rust-regression-check.ps1`

The self-hosted workflows also use:

- `scripts\self-hosted-runner-preflight.ps1`
- `scripts\collect-self-hosted-artifacts.ps1`

Those lower-level scripts handle runner prerequisite validation and GitHub Actions artifact collection.

## Current test coverage behind the gate

### execution-service
- state-machine unit tests
- router-level HTTP integration tests
- admin/operator auth coverage for direct execution reads and lifecycle transitions

### identity-service
- helper/unit tests in `src/lib.rs`
- router-level HTTP integration tests
- opt-in live-runtime HTTP black-box tests for:
  - issue/list/revoke/resolve flow over detached runtime
  - expired key rejection over detached runtime

### gateway-service
- router-level HTTP integration tests
- opt-in live-runtime HTTP black-box tests for:
  - happy path invocation flow
  - approval flow
  - reject/refund flow
  - settlement success/failure flow
  - lifecycle cancel/timeout flow
  - state-machine replay/conflict flow
  - workflow persistence after restart
  - direct audit persistence after restart
- opt-in live-runtime approval DB probe tests for:
  - reject/refund approval row persistence
  - workflow approval row persistence

### ledger-service
- router-level HTTP integration tests
- admin/operator auth coverage for direct account reads and ledger mutations

### audit-service
- router-level HTTP integration tests
- opt-in live-runtime HTTP black-box tests for:
  - protected audit trace reads require admin auth
  - local dev admin token can still read persisted trace events

## Legacy PowerShell regressions

Legacy PowerShell regression scripts still exist as compatibility shims, but the major runtime behaviors they covered now have Rust equivalents. The real retained PowerShell bodies now live under `scripts\legacy\`.

See:

- `docs/REGRESSION-COVERAGE-MATRIX.md`

That matrix tracks which PowerShell scripts are now covered by Rust tests and therefore can be treated as retirement candidates or optional manual probes.

## GitHub Actions

### Hosted workflow

The standard hosted workflow now runs **service-local-only** validation on both `windows-latest` and `ubuntu-latest`:

- file: `.github/workflows/rust-service-gate.yml`
- Windows job runs `./gate-local.ps1 -ServiceLocalOnly`
- Linux job runs `bash ./scripts/gate-local-linux.sh --service-local-only --skip-db-bootstrap`; full runtime mode starts core plus entry services, seeds repo-local entry identity governance defaults under `run/linux-runtime/entry-config/`, and runs `scripts/smoke-runtime-metrics.sh` after runtime blackbox/status to assert native core/entry `/metrics` endpoints are live; `scripts/check-production-readiness.sh` is intentionally stricter and should stay red until operator signals, provider dead letters, and live provider probe success are clear
- validates the Rust service-local tests on relevant pushes / pull requests across both OS families
- does **not** attempt to boot the full detached local runtime or provision DB/infra on the hosted runner

### Self-hosted full gate workflow

A separate self-hosted full-gate workflow is also available:

- file: `.github/workflows/rust-full-gate-self-hosted.yml`
- runner labels:
  - `self-hosted`
  - `windows`
  - `x64`
  - `cex-full-gate`
- trigger: `workflow_dispatch` only
- inputs:
  - `mode`: `full`, `service-local-only`, or `preflight-only`
  - `bootstrap_env`: whether preflight may create `.env` from `.env.example`
  - `upload_artifacts`: whether to upload the diagnostic bundle at the end
  - `note`: optional operator note recorded into the artifact bundle
- always runs repo-local preflight first:

```powershell
./scripts/self-hosted-runner-preflight.ps1 -BootstrapEnv
```

- then behaves according to `mode`:
  - `full` -> `./gate-local.ps1`
  - `service-local-only` -> `./gate-local.ps1 -ServiceLocalOnly`
  - `preflight-only` -> skip gate execution after prerequisite validation

- then always collects diagnostics under:

```text
ci-artifacts/self-hosted-full-gate
```

- and uploads that bundle when `upload_artifacts=true`

That bundle is intended to keep the first failure investigation lightweight. It includes the preflight transcript, gate transcript, copied `logs/` and `run/` trees, runtime status output, Docker snapshots, tool-version metadata, and the recorded dispatch inputs.

### Scheduled self-hosted hygiene workflow

There is now also a lighter periodic self-hosted workflow for runner hygiene:

- file: `.github/workflows/rust-self-hosted-preflight-hygiene.yml`
- triggers:
  - weekly schedule: `15 2 * * 1` (Monday 02:15 UTC)
  - manual `workflow_dispatch`
- runner labels:
  - `self-hosted`
  - `windows`
  - `x64`
  - `cex-full-gate`
- action: runs `./scripts/self-hosted-runner-preflight.ps1 -BootstrapEnv`, skips the gate intentionally, then uploads a diagnostic bundle from:

```text
ci-artifacts/self-hosted-preflight-hygiene
```

Use this workflow as a cheap recurring signal that the dedicated self-hosted runner still has the expected tools, ports, and local-runtime prerequisites.

This split is intentional:

- hosted CI = portable service-local validation
- self-hosted full gate = strongest local-runtime validation
- scheduled hygiene = cheap recurring runner-health signal

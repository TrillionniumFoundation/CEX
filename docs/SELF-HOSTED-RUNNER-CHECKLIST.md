# Self-Hosted Runner Checklist

Use this when bringing a machine online for the self-hosted Rust gate workflows.

## Goal

The dedicated self-hosted runner must be able to:

- run PowerShell on Windows
- install/use Rust toolchains through Actions
- talk to a live Docker engine
- stop/start the detached local runtime when the full gate needs it
- tolerate fixed localhost ports used by the runtime and Docker-backed infra

## One-time machine setup

1. Install a GitHub Actions **self-hosted Windows x64** runner.
2. Add labels:
   - `self-hosted`
   - `windows`
   - `x64`
   - `cex-full-gate`
3. Install Docker Desktop (or an equivalent Docker engine reachable by `docker` on PATH).
4. Keep the runner **dedicated or low-contention**. The full gate intentionally stops services, starts Docker-backed infra, and binds fixed localhost ports.

## Repo-level bootstrap

From the repo root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\self-hosted-runner-preflight.ps1 -BootstrapEnv
```

That preflight does three practical things:

- validates the repo has the full-gate entrypoints it expects
- creates `.env` from `.env.example` when `.env` is missing
- checks Docker / cargo reachability and fixed-port contention before the expensive gate starts

## Port expectations

The preflight treats these states as **healthy**:

### Service ports

- `7001` free **or** owned by `identity-service`
- `7002` free **or** owned by `ledger-service`
- `7003` free **or** owned by `execution-service`
- `7004` free **or** owned by `audit-service`
- `8080` free **or** owned by `gateway-service`

### Infra ports

- `5432` free **or** owned by Docker backend
- `6379` free **or** owned by Docker backend
- `4222` free **or** owned by Docker backend

If one of those ports is occupied by an unexpected process, fix that first. Otherwise the gate can fail later in a much less obvious way.

## Recommended bring-up sequence

1. Run local preflight manually.
2. Dispatch `.github/workflows/rust-full-gate-self-hosted.yml` in `preflight-only` mode.
3. Dispatch `.github/workflows/rust-full-gate-self-hosted.yml` again in `service-local-only` mode.
4. Use `full` mode once the runner has already proven it can safely stop/start runtime dependencies.
5. Leave `.github/workflows/rust-self-hosted-preflight-hygiene.yml` enabled so the machine keeps proving it has not drifted.

Manual local full-gate command:

```powershell
powershell -ExecutionPolicy Bypass -File .\gate-local.ps1
```

What success looks like in `full` mode:

- service-local Rust tests pass
- detached runtime restarts cleanly
- live runtime black-box tests pass
- live runtime approval DB probe tests pass
- final `status-local-runtime.ps1` health check is green

## In GitHub Actions

### Manual full-gate workflow

Available dispatch inputs:

- `mode`: `full`, `service-local-only`, `preflight-only`
- `bootstrap_env`: allow `.env` bootstrap during preflight
- `upload_artifacts`: upload the diagnostic bundle
- `note`: optional operator note

### Scheduled hygiene workflow

There is also a lighter workflow that runs preflight only:

- file: `.github/workflows/rust-self-hosted-preflight-hygiene.yml`
- schedule: Monday 02:15 UTC (`15 2 * * 1`)
- also supports manual `workflow_dispatch`

The hygiene workflow uploads a diagnostic artifact bundle from `ci-artifacts/self-hosted-preflight-hygiene`, including the preflight transcript and copied runtime/log snapshots.

## Recommended operator habits

- Use the hosted `rust-service-gate.yml` workflow as the default PR signal.
- Use `preflight-only` for new-runner bring-up and hygiene checks.
- Use `service-local-only` for quick self-hosted validation without runtime-heavy phases.
- Use the self-hosted `full` gate for release candidates, persistence changes, approval/state-machine work, or failures that only reproduce in the real local runtime.
- Treat unexpected port conflicts as a runner hygiene problem, not as a flaky test to ignore.

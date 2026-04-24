# Self-Hosted Full Gate Runner

This repo now has **three GitHub Actions layers** relevant to Rust validation:

1. `rust-service-gate.yml`
   - hosted `windows-latest`
   - service-local Rust coverage only
2. `rust-full-gate-self-hosted.yml`
   - self-hosted Windows runner
   - selectable `full` / `service-local-only` / `preflight-only` modes
3. `rust-self-hosted-preflight-hygiene.yml`
   - self-hosted Windows runner
   - scheduled preflight-only hygiene checks

## Why keep the full gate separate from hosted CI?

The full gate needs capabilities that hosted runners are not guaranteed to provide safely or consistently:

- Docker / Compose for local infra
- the ability to bind and reuse local ports:
  - `7001`
  - `7002`
  - `7003`
  - `7004`
  - `8080`
- permission to stop/start the detached local runtime
- enough time to run service-local tests + live runtime tests

So the split is intentional:

- hosted workflow = portable service-local validation
- self-hosted full gate = strongest end-to-end local gate
- scheduled hygiene workflow = cheap recurring runner-health check

## Runner labels

Both self-hosted workflows target this label set:

- `self-hosted`
- `windows`
- `x64`
- `cex-full-gate`

Add the custom label `cex-full-gate` only to machines that are meant to run the dedicated local-runtime gate/hygiene workloads.

## Runner prerequisites

Recommended machine profile:

- Windows x64
- GitHub Actions self-hosted runner installed and online
- Docker Desktop or equivalent Docker engine available from PATH
- PowerShell available from PATH
- Rust toolchain installable by Actions
- enough free disk for `target/`, Docker images, and test artifacts

The runner should be **dedicated or low-contention** because the full gate intentionally:

- stops and restarts local services
- uses Docker-backed infra
- binds fixed localhost ports
- may take a while to finish

## Bootstrap / preflight

Before spending time on the expensive full gate, run the repo-local preflight:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\self-hosted-runner-preflight.ps1 -BootstrapEnv
```

That script:

- bootstraps `.env` from `.env.example` when needed
- validates Docker and `cargo` reachability
- checks that the fixed service/infra ports are either free or owned by the expected processes
- fails early with targeted prerequisite messages instead of letting `gate-local.ps1` fail later in a noisier way

The GitHub Actions self-hosted workflows now use this same preflight for both manual full-gate runs and scheduled hygiene checks.

For a human-readable bring-up checklist, see:

- `docs/SELF-HOSTED-RUNNER-CHECKLIST.md`

## Manual self-hosted full-gate workflow

The manual self-hosted full-gate workflow accepts these inputs:

- `mode`
  - `full`
  - `service-local-only`
  - `preflight-only`
- `bootstrap_env`
  - when `true`, preflight may create `.env` from `.env.example`
- `upload_artifacts`
  - when `true`, the diagnostic bundle is uploaded to GitHub Actions artifacts
- `note`
  - optional operator note recorded into the artifact bundle

Recommended usage:

- `preflight-only` when bringing a new runner online
- `service-local-only` when you want a quick self-hosted sanity pass without touching live runtime phases
- `full` when you want the strongest local-runtime confidence signal

Workflow file:

- `.github/workflows/rust-full-gate-self-hosted.yml`

## Scheduled hygiene workflow

There is also a separate periodic hygiene workflow:

- `.github/workflows/rust-self-hosted-preflight-hygiene.yml`

Trigger policy:

- weekly on Monday 02:15 UTC via cron `15 2 * * 1`
- manual `workflow_dispatch` when you want an on-demand hygiene-only check

This workflow intentionally does **not** run `gate-local.ps1`. It only:

1. checks out the repo
2. installs Rust
3. runs `self-hosted-runner-preflight.ps1 -BootstrapEnv`
4. marks the gate as intentionally skipped
5. collects + uploads the artifact bundle

That keeps the scheduled signal cheap while still catching common self-hosted drift: Docker missing, cargo missing, wrong port owners, missing `.env`, or broken repo prerequisites.

## Workflow artifacts

Every self-hosted workflow run collects a diagnostic bundle under a repo-local artifact root before upload.

### Full-gate workflow artifact root

```text
ci-artifacts/self-hosted-full-gate
```

Includes:

- `dispatch-inputs.txt`
- `operator-note.txt` when provided
- `preflight-output.txt`
- `gate-output.txt` (or a skip marker in `preflight-only` mode)
- copied `logs/`
- copied `run/`
- `status-local-runtime.txt`
- `docker-version.txt`
- `docker-ps.txt`
- `docker-compose-ps.txt`
- `tool-versions.txt`
- basic collection metadata

### Scheduled hygiene artifact root

```text
ci-artifacts/self-hosted-preflight-hygiene
```

Includes:

- `hygiene-context.txt`
- `preflight-output.txt`
- `gate-output.txt` explaining the intentional skip
- copied `logs/`
- copied `run/`
- `status-local-runtime.txt`
- Docker snapshots
- tool versions
- collection metadata

## What the full gate runs

`./gate-local.ps1` in full mode currently drives:

- service-local Rust tests for:
  - `execution-service`
  - `gateway-service`
  - `ledger-service`
  - `audit-service`
- live runtime HTTP black-box tests
- live runtime approval DB probe tests
- runtime health checks before/after the live-runtime phases

## Suggested operating policy

Use the hosted workflow as the default PR signal.

Use the scheduled hygiene workflow to detect runner drift cheaply.

Use the self-hosted full gate when you want high confidence for:

- release candidates
- major runtime / persistence changes
- migration changes
- state-machine / approval / settlement rewrites
- debugging a failure that only shows up in the real local runtime

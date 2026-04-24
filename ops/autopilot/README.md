# Autopilot Scaffold

This folder holds the first repo-local scaffold for a 30-role autopilot system.

## What is included

- `config/dispatcher-config.json` - quotas, group limits, and milestone metadata
- `jobs/*.json` - 30 logical job specifications
- `../scripts/autopilot-dispatcher.ps1` - a dispatcher prototype that plans safe work for a tick

## What is NOT included yet

- no real worker execution harness
- no 30-worker OS-level scheduler fan-out; one OpenClaw cron dispatcher is the intended shape
- no automatic patch application or branch promotion

That is intentional. The scaffold is meant to prove:

1. the job catalog
2. the lock model
3. the tick planner
4. the repo/runtime file layout

before any always-on automation starts mutating the source tree.

## Runtime files

The dispatcher writes generated files under `ops/autopilot/runtime/`:

- `dispatch-plan.json`
- `dispatch-plan.md`
- `job-state.json`
- `leases/*.json`
- `locks/*.json`
- `reports/*.json`

This directory is git-ignored.

## Example usage

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\autopilot-dispatcher.ps1
```

Plan + persist leases/locks:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\autopilot-dispatcher.ps1 -Dispatch
```

Plan a single job only:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\autopilot-dispatcher.ps1 -OnlyJobId 07-identity-roadmap-slicer
```

## Current active phase

The currently enabled autopilot batch is intentionally **runtime-only**:

- enabled groups: `control`, `scout`, `design`, `verify` (`29-integration-reviewer`)
- currently disabled: source-mutating implementers and write-verifiers (`13-28`)
- current dispatcher quotas favor safe visibility + backlog generation:
  - `maxPlannedPerTick = 12`
  - `maxWritePerTick = 4`
  - `maxImplementPerTick = 0`

This means the 5-minute OpenClaw cron tick now produces:

- gap reports under `ops/autopilot/runtime/reports/`
- roadmap queue packets under `ops/autopilot/runtime/queue/<domain>/`
- leases / locks / dispatch metadata under `ops/autopilot/runtime/`

before any source-mutating worker is allowed into the loop.
## OpenClaw cron integration

The recommended always-on trigger is **one OpenClaw cron job**, not 30 separate schedulers.

Install or show the dispatcher cron job:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-autopilot-cron.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-autopilot-cron.ps1 -Action show
```

Force a clean reinstall:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-autopilot-cron.ps1 -Recreate
```

Run the dispatcher immediately for debugging:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-autopilot-cron.ps1 -Action run-now
```


# CEX Autopilot - 30 Jobs Design
This document defines a first repo-local scaffold for a 30-role autopilot that can wake every 5 minutes **without** letting 30 independent writers corrupt the repo. The design assumes a **single dispatcher / owner flow**, lease files, domain locks, and a write-budget per tick.
## Why not 30 blind concurrent writers?
Running 30 code-writing jobs every 5 minutes against the same Rust monorepo would create lock contention, migration races, broken CI, and unreadable history. The correct model is:
- 30 logical jobs
- 1 dispatcher
- domain locks
- low write concurrency
- high read/analysis concurrency
- milestone-based promotion
## Runtime model
Every 5 minutes all 30 **logical** jobs are considered due. The dispatcher then selects a safe subset for the current tick under these rules:
- max planned per tick: **10**
- max write jobs per tick: **5**
- max implement jobs per tick: **4**
- max verify-write jobs per tick: **2**
- max docs-write jobs per tick: **1**
- one active lock holder per domain (`identity`, `gateway`, `execution`, `capability`, `policy`, `provider`, `async`, `docs`, etc.)
- no direct write to `main` without a higher-level promotion step
## Milestones
- **M0** - Core stabilization + visibility
- **M1** - Identity foundation
- **M2** - Capability registry
- **M3** - Policy-risk service extraction
- **M4** - Provider-backed execution
- **M5** - Async bus and event-driven runtime
- **M6** - Service decomposition and control-plane hardening
## File layout
```text
ops/autopilot/
  README.md
  config/dispatcher-config.json
  jobs/*.json
  runtime/           # generated; ignored by git
scripts/autopilot-dispatcher.ps1
docs/CEX-AUTOPILOT-30-JOBS.md
```
## Job roster
| ID | Group | Lock | Milestone | Purpose |
|---|---|---|---|---|
| `01-repo-drift-scanner` | `scout` | `repo-read` | `M0` | Detect repo drift, TODO hotspots, oversized files, and code/doc mismatch candidates. |
| `02-architecture-gap-mapper` | `scout` | `repo-read` | `M0` | Compare current source tree with target platform architecture and record missing domains. |
| `03-schema-gap-mapper` | `scout` | `repo-read` | `M0` | Map schema/runtime gaps across migrations, services, and persistence behavior. |
| `04-api-gap-mapper` | `scout` | `repo-read` | `M0` | Inventory current service APIs and identify missing platform-facing endpoints. |
| `05-test-gap-mapper` | `scout` | `repo-read` | `M0` | Track service-local, runtime blackbox, DB probe, and migration coverage gaps. |
| `06-ci-gap-mapper` | `scout` | `repo-read` | `M0` | Track hosted CI, self-hosted gate, artifact, and runner hygiene gaps. |
| `07-identity-roadmap-slicer` | `design` | `identity-design` | `M1` | Slice identity foundation into org/user/api-key/auth milestones and task packets. |
| `08-capability-roadmap-slicer` | `design` | `capability-design` | `M2` | Slice capability registry work into model, persistence, and API packets. |
| `09-policy-roadmap-slicer` | `design` | `policy-design` | `M3` | Slice policy-risk extraction into service boundary, contract, and rollout steps. |
| `10-provider-roadmap-slicer` | `design` | `provider-design` | `M4` | Slice provider execution into adapter, handoff contract, and persistence tasks. |
| `11-async-roadmap-slicer` | `design` | `async-design` | `M5` | Slice Redis/NATS/event-driven evolution into publish/consume/retry milestones. |
| `12-refactor-roadmap-slicer` | `design` | `refactor-design` | `M6` | Slice large-file / mixed-responsibility refactors into safe domain-locked patches. |
| `13-identity-schema-writer` | `implement` | `identity` | `M1` | Add identity persistence/migration pieces for orgs, users, and API keys. |
| `14-identity-api-writer` | `implement` | `identity` | `M1` | Implement identity CRUD/auth endpoints beyond health-only stub behavior. |
| `15-gateway-auth-writer` | `implement` | `gateway` | `M1` | Integrate gateway with identity/auth context and inject org/actor provenance. |
| `16-capability-schema-writer` | `implement` | `capability` | `M2` | Add capability registry schema/storage model and supporting types. |
| `17-capability-api-writer` | `implement` | `capability` | `M2` | Add capability registry CRUD/query APIs and metadata endpoints. |
| `18-policy-service-scaffolder` | `implement` | `policy` | `M3` | Create policy-risk service skeleton and decision contract. |
| `19-execution-policy-extractor` | `implement` | `execution` | `M3` | Replace in-process execution policy heuristic with remote/service-backed policy evaluation. |
| `20-provider-adapter-scaffolder` | `implement` | `provider` | `M4` | Create provider adapter trait, execution handoff abstraction, and stub adapter support. |
| `21-provider-first-impl` | `implement` | `provider` | `M4` | Implement the first real provider-backed execution path. |
| `22-async-bus-writer` | `implement` | `async` | `M5` | Wire first NATS/Redis-backed publish-consume path for runtime events. |
| `23-execution-decomposer` | `implement` | `execution` | `M6` | Split execution-service responsibilities out of oversized api.rs into smaller modules. |
| `24-gateway-decomposer` | `implement` | `gateway` | `M6` | Further separate gateway orchestration, persistence, and client-side concerns. |
| `25-service-local-test-expander` | `verify` | `tests-service-local` | `M0` | Expand service-local test coverage for newly added APIs and invariants. |
| `26-runtime-blackbox-expander` | `verify` | `tests-runtime` | `M0` | Extend runtime blackbox scenarios for auth, provider, and async milestones. |
| `27-db-probe-expander` | `verify` | `tests-db` | `M0` | Add DB probe assertions for approvals, persistence, migrations, and idempotency. |
| `28-docs-sync-writer` | `verify` | `docs` | `M0` | Keep CI/gate/runner/roadmap docs aligned with actual source and runtime behavior. |
| `29-integration-reviewer` | `verify` | `repo-read` | `M0` | Review patch interaction risk, domain collisions, and promotion readiness before heavy gates. |
| `30-dispatcher-controller` | `control` | `control` | `M0` | Own tick scheduling, lease assignment, lock arbitration, and milestone advancement. |
## Safety rails
- `migrations/` and shared crates should never be edited by more than one active writer at a time.
- Consecutive gate failures should freeze implementers and leave only scout / verify / repair roles active.
- Hosted service-local CI can run often; self-hosted full gates should be milestone-triggered.
- The recommended scheduler shape is **one OpenClaw cron dispatcher every 5 minutes**, not 30 OS-level writers or 30 independent cron coders.
## Recommended next step after this scaffold
1. Wire real worker commands into selected job specs.
2. Add a promotion policy that decides when to run hosted CI vs self-hosted preflight vs full gate.
3. Add a persistent owner-flow state bag if you want detached multi-turn autonomy.
4. Start with milestone `M1` (identity foundation) rather than trying to build the whole platform in one burst.


## OpenClaw cron connection

The scaffold is now designed to connect to **OpenClaw's built-in cron**, not to 30 separate OS schedulers.

Recommended trigger shape:

- exactly **one** recurring cron job
- cadence: every 5 minutes
- session target: `isolated`
- delivery mode: `none`
- allowed tools: `exec,read`
- job body: run `scripts\autopilot-dispatcher.ps1 -Dispatch`

That keeps timing inside the Gateway while preserving repo-local control over:

- leases
- domain locks
- per-tick write budgets
- milestone ordering
- later worker fan-out

Repo-local helper script:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-autopilot-cron.ps1
```

This helper installs, shows, removes, or debug-runs the single dispatcher cron job without hard-coding host-local cron ids into the repository.

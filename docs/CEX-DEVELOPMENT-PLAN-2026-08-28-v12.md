# CEX Development Plan — 2026-08-28 v12

## Status and authority

- Status: active all-blocker closure candidate; **not production-ready**.
- Supersedes: `CEX-DEVELOPMENT-PLAN-2026-08-28-v11.md` for the exact-money, durable-settlement, provider-dispatch and release-evidence workstream.
- Candidate migration head: `0083_fix_audit_baseline_uuid_cursor.sql`.
- Default rollout posture: shadow or fail-closed; no production cutover is authorized by this document.
- Acceptance rule: source presence, a static marker, a template manifest or an unexecuted script is not evidence. A gap closes only when the exact commit/tree has the required hosted execution evidence and the evidence is bound to the release candidate.

## 1. Objective

Close every repository-actionable gap between the legacy compatibility control plane and an exact,
auditable, recoverable Ledger authority without converting, rounding or inferring historical money.
The target state is:

1. exact minor units are authoritative for every new value-bearing operation;
2. Invocation, Gateway, Execution, Ledger, Audit and provider evidence share stable operation identity;
3. all remote side effects are driven by durable commands whose claim transaction commits before network I/O;
4. ambiguous outcomes fail to `reconcile_required`, never to an invented success or failure;
5. restart, replay, lease expiry, retry exhaustion and operator recovery are executable and independently observable;
6. compatibility writes are retired or explicitly break-glass governed;
7. release qualification is based on immutable evidence, not prose or mutable CI status alone.

## 2. Candidate scope

### P0-N5 durable Execution settlement

Migrations `0067`–`0072` provide immutable settlement commands, transition history, bounded claims,
verified Ledger receipts, exact replay, retry/dead-letter semantics and operator recovery.
Execution terminal transitions enqueue consume/refund settlement durably rather than performing a
remote Ledger call inside the business transaction.

### P0-N6 exact Gateway reserve

**P0-N6 delivered by this candidate** means the repository contains the exact reserve registration
contract, durable reserve command, fail-closed worker, receipt validation, operator recovery and
hosted executable gate. It does not mean production cutover has happened.

The Gateway exact path accepts canonical string minor units, rejects dual exact/legacy monetary
intent and binds one immutable Invocation Ledger contract to one reserve operation. Claims and
outcomes run in separate transactions around the Ledger v2 network call.

### Genesis-as-entry and exact account authority

**Genesis-as-entry** is the only supported non-zero account opening model. The account is opened
through `cex_open_account_v2`; the opening value is represented by the first append-only Ledger
entry and bound to an immutable opening contract. Direct non-zero account insertion fails closed.

Migrations `0075`–`0077` add exact opening, signed inventory, projection parity, deterministic repair
evidence and append-only controls. HTTP regression coverage uses `/v2/accounts` and
`/v2/ledger/effects`; retired v1 monetary routes are asserted to return `410 Gone`.

### Provider-dispatch authority and unknown outcomes

Migrations `0078`, `0081` and `0082` separate provider dispatch from Execution state mutation,
require exact authority, classify lease expiry after dispatch as an unknown outcome, require
immutable reconciliation artifacts and permit requeue only after fresh confirmed-not-executed
evidence plus acknowledgement.

### Audit and release evidence

Migrations `0059`–`0065`, `0080` and `0083` provide authenticated Audit v2, durable outbox delivery,
source baseline backfill, operation provenance, append-only checkpoints and release evidence.
`0083` makes the bounded baseline cursor executable on PostgreSQL by replacing unsupported UUID
aggregate usage while preserving an additive `0082 -> 0083` upgrade path.

## 3. Non-negotiable invariants

1. No `f32`/`f64`, decimal rounding, truncation or major-unit compatibility conversion is permitted in a new authoritative money write path.
2. Every exact effect carries account, organization/tenancy, trace, operation, currency unit, currency scale, signed minor-unit amount, reference and scoped idempotency identity.
3. Identical immutable input replays the original result; different content under the same identity collides.
4. No SQL transaction remains open while calling Ledger, a provider or another remote service.
5. A claim lease that may have crossed a remote side-effect boundary never implies automatic retry.
6. Append-only evidence cannot be updated or deleted through normal service roles.
7. New writes may not use compatibility-only Ledger columns or retired v1 monetary endpoints.
8. A release candidate cannot be qualified from the template manifest; all placeholder hashes, URIs, run IDs, artifacts and approvals must be replaced and verified.

## 4. Repository-actionable closure blocks

### Block A — migration correctness and upgrade safety

Required evidence:

- numbered migration sequence is contiguous and the release template points at the actual head;
- all P0 migrations are transactional and functions pin `search_path`;
- fresh PostgreSQL 16 apply succeeds through the current head;
- representative existing-row upgrade paths execute, including Audit baseline, Ledger operation identity and Invocation exact contract lifecycle;
- views preserve existing column names/order unless an explicit compatible migration is used;
- function replacement preserves signatures and input parameter names where PostgreSQL requires it;
- migration scripts exercise current exact APIs rather than bypassing closed guards.

Closure gate: `.github/workflows/p0-migration-gate.yml` must complete successfully on the exact
candidate commit.

### Block B — cross-platform Rust and API regression

Required evidence:

- `cargo fmt --all -- --check`;
- Linux and Windows service-local tests;
- all-target workspace compile;
- exact Ledger account/effect lifecycle, replay and insufficient-funds recovery;
- retired v1 value routes fail closed without in-memory mutation;
- no placeholder repository mode silently accepts value writes.

Closure gate: `.github/workflows/rust-service-gate.yml` must complete successfully on the exact
candidate commit.

### Block C — Gateway exact reserve

Required evidence:

- static architecture and legacy-money exclusion;
- Gateway package tests and clippy with warnings denied;
- fresh migration lifecycle;
- Invocation exact contract lifecycle;
- Gateway command shadow exclusion, claim ownership, lease expiry, retry exhaustion, receipt
  validation, acknowledgement and exact requeue lifecycle.

Closure gate: `.github/workflows/p0-gateway-exact-reserve-gate.yml` must complete successfully on
the exact candidate commit.

### Block D — Execution terminal settlement

Required evidence:

- static architecture and no network-in-transaction path;
- Execution package tests and all-target compile;
- fresh migrations plus Invocation lifecycle and terminal mutual exclusion;
- settlement command claim, consume/refund success, exact replay, retry, dead-letter,
  acknowledgement, requeue and crash/lease recovery.

Closure gate: `.github/workflows/p0-execution-settlement-gate.yml` must complete successfully on
the exact candidate commit.

### Block E — provider reconciliation

Required evidence:

- provider dispatch authority is bound to the exact Invocation/Execution contract;
- transport ambiguity and expired active claims become `reconcile_required`;
- reconciliation artifacts have immutable URI and SHA-256 evidence;
- confirmed execution closes the command exactly once;
- confirmed-not-executed evidence is required before requeue;
- indeterminate evidence cannot authorize retry;
- transition and reconciliation evidence are append-only.

A hosted PostgreSQL lifecycle must execute these branches; static SQL markers alone are not
sufficient.

### Block F — Audit baseline and delivery

Required evidence:

- authenticated Audit v2 append and exact replay;
- outbox enqueue collision, bounded claim, retry and verified acknowledgement;
- existing Execution and API-key rows backfill in bounded batches;
- baseline restart is a no-op after completion and never duplicates intents;
- backlog pressure blocks the baseline rather than overrunning delivery capacity;
- UUID source cursors execute on PostgreSQL without lossy conversion;
- source revision advancement and Audit intent creation are atomic.

### Block G — release evidence hygiene

Before qualification:

- remove temporary self-patch workflows, patch scripts and CI trigger files;
- generate a non-template release manifest for the final commit/tree;
- bind Cargo.lock digest, migration digest, workflow run IDs, artifacts, image digests, SBOM and
  provenance;
- bind fresh and upgrade migration results, restore/rollback drill, fault-injection matrix and soak
  result;
- record explicit approvals and revocation procedure;
- prove branch/ruleset policy requires the authoritative checks.

## 5. External gates that repository edits cannot self-certify

These remain blockers until their independent evidence exists. They must not be relabelled as
closed merely because code or hosted CI is green.

1. production-like backup and restore rehearsal against representative data volume;
2. deployment/cutover and rollback rehearsal with real service identities, network policy and secret custody;
3. real provider reconciliation artifacts for success, definite non-execution and indeterminate outcome;
4. credential issuance, rotation, revocation and break-glass custody review;
5. sustained soak/endurance run with queue age, retry, reconciliation, parity and Audit delivery SLOs;
6. independent security, operations and financial-control review;
7. legal, commercial or provider approvals where the production integration requires them;
8. final human go/no-go decision bound to the immutable release candidate.

## 6. Evidence ledger

The following statuses are intentionally fail-closed until the final exact-tree runs finish and are
bound to a non-template manifest.

| Evidence | Required state | Current plan state |
|---|---|---|
| Fresh migration through current head | hosted success | pending exact-tree binding |
| Existing-row Audit baseline | hosted success | pending exact-tree binding |
| Ledger operation identity/replay | hosted success | pending exact-tree binding |
| Invocation lifecycle/exclusivity | hosted success | pending exact-tree binding |
| Linux service-local gate | hosted success | pending exact-tree binding |
| Windows service-local gate | hosted success | pending exact-tree binding |
| Gateway exact reserve gate | hosted success | pending exact-tree binding |
| Execution settlement gate | hosted success | pending exact-tree binding |
| Provider unknown-outcome lifecycle | hosted success | pending dedicated binding |
| Backup/restore and rollback | immutable external artifact | open external blocker |
| Soak/SLO qualification | immutable external artifact | open external blocker |
| Independent approvals | signed/recorded approval | open external blocker |

## 7. Definition of repository closure

Repository-actionable gaps are closed only when all of the following are true on one final commit:

1. all authoritative hosted workflows are green;
2. no temporary patch/trigger artifact remains;
3. migration and manifest heads agree;
4. fresh and existing-row upgrade paths both execute;
5. exact replay, collision, crash/lease and operator-recovery branches execute;
6. compatibility writes and v1 value routes remain fail-closed;
7. the v12 evidence ledger names the final run IDs and conclusions;
8. the generated release manifest contains no placeholder value.

Even after repository closure, the release remains **not production-ready** until every external
gate in Section 5 is independently satisfied and the final go/no-go approval is recorded.

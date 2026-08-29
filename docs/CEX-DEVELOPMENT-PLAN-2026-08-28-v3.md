# CEX Development Plan v3 — P0 Durable Audit and Value Integrity

- Status: Active
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Parent fact baseline: `003ed50282f2c4209ac88715614737e013dee556`
- Migration head after this batch: `0062_add_identity_transactional_audit_outbox.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. Why this revision exists

Development Plan v2 established the production-baseline principles: explicit production
profiles, authenticated internal services, exact money, durable saga/outbox, authenticated
tamper-evident Audit events and machine-bound release evidence.

A remote branch fact check on 2026-08-28 showed that the branch still ended at migration
0059 even though a prior PR description had described 0060–0062 as complete. Plan v3
therefore resets status to repository truth and makes `pushed source + executable evidence`
the only completion standard.

No local narrative, PR body or historical PASS table can advance a work item.

## 2. Current batch

This batch closes the first Audit delivery/source-atomicity slice.

### 2.1 Migration 0060 — delivery lifecycle

Deliverables:

- generic exact-replay source enqueue;
- deterministic event IDs;
- claim ownership and lease checks;
- verified ACK receipt;
- transient retry scheduling;
- permanent/budget-exhausted dead letter;
- delivery/retry/dead-letter evidence columns;
- operator summary view;
- dedicated authenticated dispatcher.

Invariants:

- a different receipt cannot overwrite a delivered row;
- wrong/expired worker claims cannot ACK or fail a row;
- exact enqueue replay never resets lifecycle state;
- remote success plus local ACK crash is recovered by exact replay.

### 2.2 Migration 0061 — Execution transaction binding

Deliverables:

- database-owned monotonic `executions.audit_revision`;
- same-transaction intent for insert and meaningful updates;
- no intent for timestamp-only/no-op updates;
- authoritative invocation-org validation;
- deterministic event identity;
- result payload digest rather than raw provider output;
- rollback atomicity probes.

### 2.3 Migration 0062 — Identity transaction binding

Deliverables:

- database-owned monotonic `api_keys.audit_revision`;
- same-transaction issue/revoke/expiry/material/metadata intent;
- no event for `last_used_at-only`;
- no raw key or key hash in Audit payload;
- transaction-local admin actor hook;
- rollback atomicity probes.

## 3. Evidence status for this batch

### Source evidence

Required source files, migrations, runtime unit, environment contract, static wiring gate and
fresh-database probes are committed together on the integration branch.

### Executable evidence

Until GitHub runner capacity is restored, the following remain **pending** rather than
implicitly passed:

- `cargo fmt --all --check`;
- Audit dispatcher compile and unit tests;
- full workspace `--all-targets` compile;
- fresh PostgreSQL migration/lifecycle test;
- upgrade/restore migration test;
- workload token rotation;
- Audit outage and ACK-crash fault injection.

PR #1 must remain Draft and unmerged while these are pending.

## 4. P0 workstream state

| Workstream | State after this batch | Exit condition |
|---|---|---|
| Repository/release baseline | In progress | Protected canonical trunk and executed required checks |
| Runtime fail-closed | Implemented candidate | Runtime/negative tests executed in CI |
| Workload identity v1 | Implemented candidate | Rotation, revocation and wrong-caller tests |
| Identity authority | Partial | No fail-open DB fallback and stable sanitized errors |
| Money v2 | Expand/dual-write | Backfill, dual-read parity and `f64` removal |
| Durable saga | Shadow foundation | Crash/replay/concurrency qualification |
| Audit v2 append | Implemented candidate | DB roles, signed checkpoints, restore proof |
| Audit delivery | Implemented in this batch | Dispatcher compile/integration/fault evidence |
| Execution source enqueue | Implemented in this batch | Fresh/upgrade DB and lifecycle probes |
| Identity source enqueue | Implemented in this batch | Fresh/upgrade DB and actor propagation probes |
| SRE/DR | Partial | SLO, restore and incident drill bound to RC |

## 5. Next locked slice

Work must continue in this order.

### P0-N0 Existing-row Audit baseline

Add a bounded, resumable backfill for existing `executions` and `api_keys` rows whose
`audit_revision = 0`. Do not bulk-flood the outbox inside a schema migration.

Acceptance:

- cursor/restart-safe;
- exact replay does not duplicate intent;
- source revision and baseline intent commit together;
- residual revision-0 count reaches zero or has an explicit reviewed waiver;
- outbox age/backlog remains within rollout limits.

### P0-N1 Ledger operation identity

Add immutable fields to every financial effect:

- `trace_id`;
- `operation_id`;
- operation kind;
- idempotency scope;
- source service/principal;
- schema version.

Do not infer cross-service trace identity from arbitrary text after cutover.

Acceptance:

- one business operation maps to one effective ledger mutation;
- exact replay returns the original receipt;
- a key collision with different immutable content fails closed;
- every entry is queryable by trace and operation.

### P0-N2 Genesis-as-entry

Remove implicit value creation through account summary initialization.

Acceptance:

- nonzero initial value creates an explicit immutable genesis entry;
- account summary can be rebuilt from entries;
- direct summary mutation is denied outside controlled repair;
- migration reconciles existing initial balances with explicit evidence.

### P0-N3 Money v2 backfill and read cutover

Sequence:

1. inventory every value-bearing `f64`/numeric boundary;
2. backfill minor units;
3. dual-read and compare;
4. alert on mismatch;
5. switch authoritative reads;
6. remove legacy value-bearing floating paths;
7. contract old columns only after the rollback window.

Acceptance:

- no value-bearing API/repository path uses binary floating point;
- scale/currency mismatches fail closed;
- overflow and rounding behavior are property-tested;
- release evidence contains zero unresolved parity drift.

### P0-N4 Ledger projection rebuild/reconciliation

Deliver:

- rebuild from authoritative entries/reservations;
- drift report;
- dry-run repair plan;
- operator-authorized repair;
- immutable repair event and before/after digest;
- recurring reconciliation metrics.

Acceptance:

- a disposable database can reconstruct account state;
- intentionally corrupted projection is detected;
- repair requires scoped operator authority;
- repair never mutates authoritative historical entries.

### P0-N5 Network calls outside SQL transactions

Execution currently has paths that hold database transactions/row locks across provider and
ledger HTTP calls. Replace with durable intents and receipts:

```text
short transaction persists command
worker claims and commits claim
network side effect
receipt persists
short transaction advances state
```

Acceptance:

- no provider/ledger request occurs while an authoritative row lock is held;
- timeout with unknown remote result becomes `reconcile_required`;
- blind retry cannot duplicate charge or provider effect;
- fault injection covers every persistence/network boundary.

### P0-N6 Saga promotion qualification

Before shadow commands become authoritative:

- crash after claim;
- crash after provider success;
- receipt insert failure;
- duplicate worker;
- lease expiry;
- consume/refund unknown result;
- DB failover;
- property/model tests.

Promotion requires a written semantic guarantee: at-least-once delivery with idempotent
effects and explicit reconciliation, not an unsupported exactly-once claim.

## 6. Security hardening parallel track

Run in parallel only when it does not delay N0–N6:

- least-privilege PostgreSQL roles for each source trigger and dispatcher transition;
- separate dispatcher credential and rotation window;
- operator dead-letter read/requeue with immutable evidence;
- payload redaction/schema registry;
- externally signed Audit chain-head checkpoints;
- provider child-process environment allow-list and non-argv prompt transport;
- route/auth/OpenAPI parity.

## 7. Required CI matrix

### Fast gate

- `cargo fmt --all --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- unit/property tests;
- static wiring and migration governance;
- OpenAPI/config/doc drift.

### Persistence gate

- fresh migrations 0001 through 0062;
- supported-version upgrade;
- constraints and trigger behavior;
- concurrent claim/replay;
- rollback atomicity;
- backup/restore and chain verification.

### Integration/fault gate

- authenticated dispatcher to real Audit service;
- token rotation;
- outage/recovery;
- remote append success/local ACK crash;
- wrong worker and lease expiry;
- Execution/Identity source mutation rollback;
- provider/ledger saga crash matrix.

### Supply-chain gate

- locked release build;
- audit/deny/license;
- secret scan;
- SBOM;
- image scan;
- signed provenance.

## 8. Merge and release rules

The branch stays Draft until all required checks actually execute.

A release candidate must bind exact commit/tree, Cargo.lock hash, full migration filename,
workflow run/job IDs, test/fault artifacts, image digest, SBOM/provenance, deployment
profile, approval and revocation metadata.

`Source present` means implementation candidate. `Production ready` requires the complete
machine-verifiable evidence chain.

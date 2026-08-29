# CEX Development Plan v5 — Existing-Row Audit Baseline Implemented Candidate

- Status: Active
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Canonical migration head: `0064_add_audit_source_baseline_backfill.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. Completed in this revision

### Canonical migration repair

Duplicate 0060–0062 files were removed. The canonical chain is now:

1. `0060_close_audit_outbox_delivery_lifecycle.sql`
2. `0061_close_audit_outbox_delivery_transitions.sql`
3. `0062_add_execution_transactional_audit_outbox.sql`
4. `0063_add_identity_transactional_audit_outbox.sql`
5. `0064_add_audit_source_baseline_backfill.sql`

The static gate fails if a superseded duplicate path returns.

### P0-N0 existing-row Audit baseline

The repository now contains an implementation candidate for bounded baseline processing of
historical `executions` and `api_keys` rows with `audit_revision = 0`.

Delivered:

- durable per-source progress;
- one advisory transaction lock per source;
- batch size and backlog ceilings;
- revision-0 authoritative selection;
- deterministic baseline event identity;
- revision/intent/progress atomicity;
- restart-safe exact replay;
- `execution.persisted.baseline`;
- `identity.api_key.persisted.baseline`;
- provider-output digest rather than raw output;
- no raw API key or key hash;
- completion/blocked/live-residual status view;
- bounded operator CLI;
- dedicated fresh-PostgreSQL lifecycle probe.

This is source-complete only. It is not production-qualified until CI, upgrade and runtime
evidence execute.

## 2. Required P0-N0 evidence still pending

- GitHub required checks must allocate a runner and execute;
- fresh 0001–0064 migration gate;
- upgrade from a real pre-0062/0063 database;
- partial batch/restart/completion;
- backlog block/resume;
- transaction rollback on tenant mismatch;
- dispatcher throughput and oldest-age observation;
- least-privilege execution role;
- staging cardinality report and residual-zero proof.

PR #1 remains Draft and unmerged.

## 3. Next locked slice — P0-N1 Ledger operation identity

Every financial effect must gain immutable:

- `trace_id`;
- `operation_id`;
- operation kind;
- idempotency scope and key;
- source service;
- source principal;
- schema version.

### Design constraints

- do not infer a trustworthy cross-service trace from arbitrary legacy text;
- legacy entries receive explicitly labelled entry-scoped identities;
- new operations use stable caller/business identity;
- immutable collision must fail closed;
- exact replay must return the original effect/receipt;
- operation scope replaces the current global idempotency namespace;
- reserve, consume, refund, grant and future genesis effects require explicit semantics;
- value paths remain dual-write until Money v2 cutover.

### Acceptance

- every entry is queryable by trace and operation;
- one operation ID maps to one immutable effect;
- same operation/key plus identical content returns the original effect;
- same operation/key plus different content is rejected;
- production-like writes cannot omit provenance;
- migration/backfill publishes residual and collision evidence.

## 4. P0-N2 Genesis-as-entry

Account creation with nonzero value must create an immutable genesis effect in the same
transaction. Existing balances are not retroactively invented as history; they require
reconciliation evidence and an explicit migration disposition.

## 5. P0-N3 Money v2 cutover

- inventory `f64`;
- exact minor-unit request/response types;
- backfill;
- dual-read comparison;
- mismatch metrics;
- authoritative read switch;
- binary floating-point removal;
- rollback window;
- legacy-column contract only after evidence.

## 6. P0-N4 Projection rebuild

Reconstruct account summaries from authoritative effects/reservations, report drift, create
a dry-run repair plan, require scoped approval, and append immutable repair evidence.

## 7. P0-N5 Network calls outside SQL transactions

Persist command, commit claim, perform network effect, persist receipt, then advance state in
a short transaction. Unknown remote outcomes enter `reconcile_required`.

## 8. P0-N6 Saga qualification

Execute crash/replay/concurrency/model tests before shadow promotion. The supported semantic
is at-least-once delivery with idempotent effects and explicit reconciliation.

## 9. Merge and release rule

No source or document statement counts as a PASS without machine-verifiable evidence bound
to the exact commit/tree and full migration filename.

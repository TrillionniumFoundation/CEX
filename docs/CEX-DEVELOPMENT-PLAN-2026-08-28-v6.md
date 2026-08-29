# CEX Development Plan v6 — Ledger Operation Identity Database Contract

- Status: Active
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Canonical migration head: `0065_add_ledger_operation_identity.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. Completed candidates

### P0-N0 Existing-row Audit baseline

Source code now includes bounded, restart-safe backfill for revision-0 Execution and API-key
rows, durable progress, backlog protection and PostgreSQL probes. Production execution and
upgrade evidence are still pending.

### P0-N1 Ledger operation identity — database half

Migration 0065 adds immutable trace/operation provenance, scoped idempotency, request
fingerprints, append-only enforcement, exact replay/collision semantics, minor-unit account
mutation and transactional Audit intent creation.

Existing entries receive deterministic entry-scoped legacy identities rather than fabricated
cross-service traces. Legacy v1 inserts remain available but are explicitly labelled
`operation_scoped_compatibility`.

## 2. Evidence still pending

- GitHub Actions runner allocation;
- fresh and supported upgrade migrations;
- concurrent exact replay;
- real legacy-entry cardinality/collision report;
- compatibility traffic observation;
- authenticated HTTP surface compile/test;
- caller cutover;
- least-privilege database role;
- backup/restore and reconciliation.

The current Actions failures contain no executed steps and therefore prove neither source
failure nor success. PR #1 remains Draft and unmerged.

## 3. Next locked slice — P0-N1 HTTP and caller contract

Deliver:

- canonical `POST /v2/ledger/effects`;
- `GET /v2/ledger/effects/:operation_id`;
- `GET /v2/ledger/traces/:trace_id`;
- authenticated scoped admin principal;
- stable operation ID derived from scoped idempotency when omitted;
- explicit trace requirement in production-like configuration;
- generic client-safe error codes without SQL detail leakage;
- exact replay response with original effect;
- org-boundary enforcement;
- route/static/HTTP tests;
- production environment contract.

The HTTP implementation must call `cex_apply_ledger_effect_v1`; it must not reproduce ledger
state transition logic in Rust.

## 4. P0-N1 caller migration

After the HTTP surface compiles and passes database probes:

1. Gateway reserve/refund uses explicit invocation trace and deterministic operation IDs.
2. Execution consume/refund uses execution trace and operation IDs.
3. Compatibility v1 traffic is measured.
4. Production configuration rejects compatibility writes.
5. Legacy global route deprecation receives an ADR and telemetry gate.

## 5. P0-N2 Genesis-as-entry

Nonzero account creation must create a genesis effect in the same transaction. Existing
balances require reconciliation evidence, not fabricated history.

## 6. P0-N3 Money v2 cutover

Inventory value-bearing `f64`, backfill, dual-read, compare, publish mismatch metrics, switch
reads, remove binary floating point and retain a tested rollback window.

## 7. P0-N4 Projection rebuild

Rebuild summaries from authoritative effects/reservations, report drift, generate dry-run
repair, require scoped approval and append immutable before/after evidence.

## 8. P0-N5 Network calls outside SQL transactions

Persist command, commit claim, perform network effect, persist receipt, then advance state in
a short transaction. Unknown outcomes become `reconcile_required`.

## 9. P0-N6 Saga qualification

Execute crash/replay/concurrency/model tests before shadow promotion. Supported semantics are
at-least-once delivery with idempotent effects and explicit reconciliation.

## 10. Merge/release rule

No source-complete candidate is a PASS without machine evidence bound to exact commit/tree,
full migration filename, lockfile, workflow jobs, artifacts and approvals.

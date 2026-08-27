# CEX Development Plan v8 — Exact Ledger Caller Contract Before Cutover

- Status: Active
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Canonical migration head: `0065_add_ledger_operation_identity.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. Current completed candidates

- canonical 0060–0065 migration chain;
- fail-closed runtime and internal workload identity;
- Audit v2/hash chain/outbox/dispatcher;
- Execution and Identity same-transaction Audit intents;
- bounded existing-row Audit baseline;
- immutable Ledger operation identity and scoped idempotency;
- exact minor-unit Ledger database authority;
- canonical Ledger apply/read HTTP routes.

## 2. P0-N1 caller contract delivered in this revision

A shared wire contract now represents Ledger effects without binary floating point:

- `amount_minor` is an `i64` serialized as a JSON string;
- currency unit and scale are explicit;
- operation kind is an enum;
- trace/operation IDs are UUIDs;
- reference binding and scoped idempotency are validated centrally.

Ledger service consumes the shared contract and checks currency/scale against the account.
Gateway has an exact v2 client, rollout-mode parser and deterministic Invocation effect
builder that accepts `MoneyAmount`, never `f64`.

## 3. caller migration blocked until exact reserve contract

Invocation ingress, stored request payload and Execution creation still carry
`reserve_amount: f64`. Therefore Gateway Invocation orchestration is intentionally not wired
to `apply_ledger_effect_v2` yet.

The safety gate fails if v2 orchestration is enabled before the authoritative exact reserve
field exists, or if the client contains multiplication/cast/rounding bridges.

The dependency order is stricter than a simple Gateway switch:

1. persist an exact Invocation ledger contract;
2. make Execution consume/refund read the same durable contract;
3. only then allow Gateway reserve/refund to use v2;
4. preserve v1 behind an explicit rollback mode;
5. reject dual legacy/exact input;
6. add compatibility counters and production rejection;
7. execute HTTP, persistence, settlement, restart and rollback tests.

Opening Gateway v2 before Execution can settle the same exact reservation would create a
reserved-value leak, so the safety gate must continue to report `cutover_ready=false`.

## 4. P0-N2 Genesis-as-entry

After caller cutover is an implemented candidate, nonzero account creation must create a
genesis effect in the same transaction. Existing balances require reconciliation evidence,
not fabricated history.

## 5. P0-N3 Money v2 read cutover

Inventory value-bearing `f64`, backfill, dual-read/compare, publish mismatch metrics, switch
authoritative reads, remove binary floating point and retain a tested rollback window.

## 6. P0-N4 Projection rebuild and repair

Rebuild account summaries from effects/reservations, report drift, generate a dry-run repair
plan, require scoped approval and append immutable before/after repair evidence.

## 7. P0-N5 Network calls outside SQL transactions

Persist command, commit claim, perform network effect, persist receipt, then advance state in
a short transaction. Unknown outcomes enter `reconcile_required`.

## 8. P0-N6 Saga qualification

Execute crash/replay/concurrency/model tests before shadow promotion. Supported semantics are
at-least-once delivery with idempotent effects and explicit reconciliation.

## 9. Evidence rule

Source presence is not PASS evidence. GitHub jobs still complete before runner allocation with
no steps, so PR #1 remains Draft and unmerged. Every promotion claim must bind exact source,
migration filename, executed jobs and artifacts.

# CEX Development Plan v9 — Durable Invocation Ledger Contract

- Status: Active
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Canonical migration head: `0066_add_invocation_ledger_contract.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. Current implementation candidates

- canonical migrations 0060–0066;
- fail-closed runtime/workload identity;
- Audit v2/hash chain/outbox/dispatcher;
- Execution and Identity transactional Audit intents;
- bounded revision-0 Audit baseline;
- exact Ledger operation identity/scoped idempotency;
- canonical exact Ledger database and HTTP authority;
- shared exact caller wire contract and Gateway client;
- durable Invocation Ledger contract.

## 2. P0-N1 settlement bridge delivered in this revision

Migration 0066 creates a single durable source of truth shared by Gateway and Execution for
reserve/consume/refund:

- exact minor units and currency contract;
- Invocation/account/org/trace binding;
- deterministic operation IDs matching Ledger HTTP derivation;
- exact replay and immutable collision rejection;
- contract registration Audit intent;
- canonical request projection for each operation;
- state machine `registered -> reserved -> consumed/refunded`;
- Ledger-entry trigger that validates and atomically advances the contract;
- last operation/entry evidence and drift status view.

This removes the architectural excuse to pass floating-point settlement amounts between
Gateway and Execution. It does not yet activate the caller switch.

## 3. Next locked slice — Execution exact settlement adapter

Execution must gain a small adapter that:

1. queries `cex_invocation_ledger_effect_request_v1` by invocation ID;
2. sends the returned shared request to canonical `POST /v2/ledger/effects`;
3. exact-replays safely after timeout/restart;
4. distinguishes missing contract from compatibility Invocation;
5. supports `legacy_v1`, `dual`, `require_v2` modes;
6. never converts `StoredInvocationRequest.reserve_amount: f64` to minor units;
7. emits stable settlement errors without SQL/upstream detail;
8. records `reconcile_required` for unknown remote outcomes.

Because `services/execution-service/src/api.rs` still performs provider and Ledger HTTP calls
inside database transactions, the adapter must first remain dual/compatibility. Authoritative
promotion waits for P0-N5 command/receipt separation.

## 4. Following slice — Gateway exact registration/reserve

- exact Invocation ingress field;
- reject simultaneous legacy/exact forms;
- register 0066 contract before reserve;
- query canonical reserve request from the contract;
- dual/require-v2 behavior;
- exact refund on execution-create failure;
- persist/restart tests;
- compatibility counters and production rejection.

## 5. P0-N2 Genesis-as-entry

Nonzero account creation must create a genesis effect in one transaction. Existing balances
require reconciliation evidence, not fabricated history.

## 6. P0-N3 Money v2 read cutover

Inventory value-bearing `f64`, backfill, dual-read/compare, publish mismatch metrics, switch
reads, remove binary floating point and preserve a tested rollback window.

## 7. P0-N4 Projection rebuild and repair

Rebuild summaries from effects/reservations, report drift, generate dry-run repair, require
scoped approval and append immutable before/after evidence.

## 8. P0-N5 Network calls outside SQL transactions

Persist command, commit claim, perform network effect, persist receipt, then advance state in
a short transaction. Unknown outcomes enter `reconcile_required`.

## 9. P0-N6 Saga qualification

Run crash/replay/concurrency/model tests before shadow promotion. Supported semantics are
at-least-once delivery with idempotent effects and explicit reconciliation.

## 10. Evidence rule

The current Actions infrastructure still returns jobs with no runner and no steps. Source
presence is not PASS evidence. PR #1 remains Draft and unmerged until exact commit/tree,
canonical migration filename and executed artifacts are bound in the release manifest.

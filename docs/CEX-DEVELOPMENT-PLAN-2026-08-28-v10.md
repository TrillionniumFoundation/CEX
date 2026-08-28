# CEX Development Plan v10 — Isolated Execution Exact Settlement Adapter

- Status: Active
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Canonical migration head: `0066_add_invocation_ledger_contract.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. Current implementation candidates

- canonical migrations 0060–0066;
- runtime fail-closed and workload identity;
- Audit v2/outbox/dispatcher and transactional source intents;
- bounded historical Audit baseline;
- immutable Ledger operation identity and exact-effect authority;
- shared exact caller contract;
- durable Invocation settlement contract;
- independent terminal-mutual-exclusion probe;
- isolated Execution exact settlement adapter.

## 2. Execution adapter delivered

The adapter:

- reads consume/refund requests only from the 0066 durable contract;
- never reads or converts `reserve_amount: f64`;
- supports `legacy_v1`, `dual`, `require_v2`;
- sends the shared exact request to canonical Ledger v2;
- performs bounded response reads;
- verifies immutable success receipts;
- distinguishes retryable exact replay, permanent rejection and unknown remote outcome;
- treats timeout/ambiguous transport/invalid success receipt as `ReconcileRequired`.

It is exported for compile/test coverage but deliberately not activated in `api.rs`.

## 3. P0-N5 transaction separation remains the activation blocker

The current Execution implementation performs provider and Ledger HTTP calls while a business
SQL transaction is open. Activating the new adapter there would preserve the same crash and
lock hazards.

P0-N5 transaction separation must deliver:

1. durable settlement command;
2. short claim transaction;
3. network call after commit;
4. verified receipt or explicit unknown outcome;
5. short state-advance transaction;
6. exact replay/reconciliation on restart.

The static gate fails if `api.rs` begins calling `settle_invocation` before this work lands.

## 4. Next locked slice — settlement command and receipt schema

Add an expand-only migration for:

- consume/refund settlement commands;
- claim lease and bounded attempts;
- operation/Invocation/Execution bindings;
- command payload fingerprint;
- verified Ledger receipt;
- `reconcile_required` unknown-outcome state;
- retry/dead-letter/operator acknowledgement;
- outbox/status metrics.

Then implement a worker that invokes the isolated adapter outside all business SQL
transactions.

## 5. Following slice — Gateway exact registration/reserve

Only after Execution can durably settle:

- add exact Invocation ingress;
- register the 0066 contract;
- reserve via canonical v2;
- exact refund on create failure;
- dual/require-v2 behavior;
- compatibility telemetry and production rejection.

## 6. P0-N2 Genesis-as-entry

Nonzero account creation becomes one same-transaction genesis effect. Existing balances need
reconciliation evidence, not fabricated history.

## 7. P0-N3 Money v2 read cutover

Backfill, dual-read/compare, mismatch metrics, authoritative read switch, binary-floating-point
removal and rollback evidence.

## 8. Evidence rule

No source-complete candidate is a PASS. GitHub jobs still fail before runner allocation with
no executed steps. PR #1 remains Draft and unmerged until exact source, migration head and
executed artifacts are bound in a non-template release manifest.

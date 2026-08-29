# CEX Development Plan v7 — Canonical Ledger Effect API Candidate

- Status: Active
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Canonical migration head: `0065_add_ledger_operation_identity.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. P0-N0 existing-row Audit baseline

The bounded baseline source, durable progress, backlog guard and test harness are present.
Runner execution, real upgrade data and staging residual-zero evidence remain pending.

## 2. P0-N1 Ledger operation identity

Database and HTTP implementation candidates are now present.

Delivered:

- immutable trace and operation identity;
- scoped idempotency and exact replay;
- immutable-content collision rejection;
- exact minor-unit write path;
- source service/principal and request fingerprint;
- append-only ledger entries;
- same-transaction `ledger.effect.persisted` intent;
- legacy and compatibility provenance labels;
- canonical apply/read-by-operation/read-by-trace routes;
- stable operation ID derivation from scoped key;
- production explicit-trace configuration;
- org-boundary enforcement;
- generic client-safe errors without SQL details.

This remains an implementation candidate. No CI runner has executed the code or migrations.

## 3. Next locked slice — P0-N1 caller migration and cutover controls

1. Add a Gateway client for `/v2/ledger/effects`.
2. Reserve uses invocation trace and deterministic `reserve` operation identity.
3. Execution consume/refund uses execution trace and deterministic operation identity.
4. Preserve current v1 calls behind an explicit rollback feature flag.
5. Count compatibility writes and reject them in production after staging reaches zero.
6. Add concurrent same-operation/same-key probes.
7. Add token rotation and org-crossing negative tests.

No caller may convert through binary floating point on the canonical path. Until callers move,
legacy v1 remains compatibility-only and N1 is not promotion-complete.

## 4. P0-N2 Genesis-as-entry

Nonzero account creation must create a genesis effect in the same transaction. Existing
balances require reconciliation evidence, not fabricated history.

## 5. P0-N3 Money v2 read cutover

Inventory value-bearing `f64`, backfill, dual-read, compare, publish mismatch metrics, switch
reads, remove binary floating point and preserve a tested rollback window.

## 6. P0-N4 Projection rebuild

Rebuild summaries from authoritative effects/reservations, report drift, generate dry-run
repair, require scoped approval and append immutable before/after evidence.

## 7. P0-N5 Network calls outside SQL transactions

Persist command, commit claim, perform network effect, persist receipt, then advance state in
a short transaction. Unknown outcomes become `reconcile_required`.

## 8. P0-N6 Saga qualification

Execute crash/replay/concurrency/model tests before shadow promotion. Supported semantics are
at-least-once delivery with idempotent effects and explicit reconciliation.

## 9. Evidence rule

Source presence is not PASS evidence. PR #1 remains Draft and unmerged until required checks
run and machine artifacts bind the exact commit/tree and migration head.

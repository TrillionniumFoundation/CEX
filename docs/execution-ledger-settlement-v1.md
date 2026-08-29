# Execution Exact Ledger Settlement Adapter v1

- Status: isolated implementation candidate
- Module: `execution_service::ledger_settlement`
- Durable request source: `cex_invocation_ledger_effect_request_v1`
- Canonical target: `POST /v2/ledger/effects`
- Safety gate: `scripts/check-execution-ledger-settlement.py`

## Purpose

Execution settles an Invocation reservation without reading or converting the legacy
`reserve_amount: f64`. The adapter loads the exact consume/refund request from the durable
Invocation Ledger contract introduced by migration 0066. Terminal API transitions now leave the
legacy side-effect path disabled and rely on the 0074 database trigger to enqueue a durable v2
command; the settlement worker invokes this adapter only after its claim transaction commits.

## Modes

`CEX_EXECUTION_LEDGER_MODE` supports:

- `legacy_v1` — retained as a compatibility parser value, but the durable worker rejects it at
  startup and no API terminal path calls Ledger v1;
- `dual` — use the exact contract when present and fail closed for a missing contract;
- `require_v2` — reject a missing exact contract (the production-like default).

The module is exported for compilation and tests and is called by the durable settlement worker;
it is **not yet called by api.rs** because the API owns only the short status transaction. This is
intentional: the old API-side provider/Ledger network path has been removed, and provider-backed
requests must enter through `provider_dispatch` before the settlement worker is eligible.

## Flow

1. validate non-nil Invocation ID;
2. query the durable consume/refund request by Invocation ID;
3. deserialize the shared exact-money request;
4. require explicit trace and operation identity;
5. call canonical Ledger v2 with the scoped admin principal;
6. read at most 64 KiB using bounded chunks;
7. verify success receipt against account, trace, operation, kind, amount, scale, currency and
   scoped idempotency;
8. return an explicit settlement classification.

No `f64`, multiplication, cast or rounding bridge is allowed.

## Outcome classes

- `UseLegacyV1` — only in legacy/dual compatibility posture;
- `Applied` — verified first application or exact replay;
- `RetryableExactReplay` — no accepted effect is known, or a received retryable HTTP response
  can be safely replayed with the exact same operation identity;
- `ReconcileRequired` — timeout, ambiguous transport, invalid success receipt, or another
  unknown remote outcome;
- `Rejected` — contract-state, auth, tenancy, validation or immutable collision failure.

A timeout is not blindly marked failed. The stable operation ID permits a later exact replay,
but the execution lifecycle must first persist `reconcile_required` outside the provider SQL
transaction.

## Activation blocker

Activation waits for P0-N5 transaction separation:

```text
persist settlement command
commit claim
perform Ledger HTTP
persist verified receipt or unknown outcome
advance Execution in a short transaction
```

The repository implementation now follows this sequence. Production activation still requires the
hosted exact-SHA gates and the independent external controls listed in the v12 plan.

## Required next evidence

- Rust compile and unit tests;
- mocked HTTP first-apply/exact-replay/collision/timeout/oversize tests;
- workload/admin token rotation;
- missing-contract dual/require-v2 behavior;
- Execution restart after remote success/local receipt failure;
- concurrent settlement workers;
- durable `reconcile_required` operator surface;
- transaction separation and crash injection;
- hosted exact-SHA worker evidence and production-like credential/lease review.

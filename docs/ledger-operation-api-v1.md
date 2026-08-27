# Ledger Operation HTTP API v1

- Status: implementation candidate
- Database authority: `cex_apply_ledger_effect_v1`
- Effect schema: `cex.ledger.effect.v1`
- Audit event: `ledger.effect.persisted`

## Canonical routes

```text
POST /v2/ledger/effects
GET  /v2/ledger/effects/:operation_id
GET  /v2/ledger/traces/:trace_id
```

The routes use scoped ledger admin principals and enforce the principal's optional org list.
Rust performs authentication, tenancy checks, validation and response shaping. All monetary
state transitions remain in the single PostgreSQL function.

## Apply request

```json
{
  "account_id": "uuid",
  "trace_id": "uuid",
  "operation_id": "uuid-or-null",
  "operation_kind": "reserve",
  "amount_minor": 1250000,
  "currency_scale": 6,
  "reference_type": "invocation",
  "reference_id": "uuid",
  "idempotency_scope": "org:example:reserve",
  "idempotency_key": "reserve:business-operation"
}
```

Allowed operation kinds are reserve, consume, refund and grant. The request never accepts a
binary floating-point value.

When `operation_id` is omitted, the service derives a stable UUID from the scoped
idempotency pair. Exact retries therefore resolve to the same operation. A client-supplied
operation ID is preserved and verified.

When `trace_id` is omitted in compatibility mode, the operation ID is used and the entry is
labelled `operation_scoped_compatibility`. Production configuration must set:

```text
LEDGER_V2_REQUIRE_EXPLICIT_TRACE=true
```

## Response

A first effect returns `201`; an exact replay returns `200`. Both contain the same immutable
effect receipt. The replay does not apply the account mutation again.

```json
{
  "replayed": false,
  "account": {
    "account_id": "uuid",
    "org_id": "uuid",
    "currency_unit": "credit",
    "currency_scale": 6,
    "balance_minor": 10000000,
    "reserved_minor": 1250000
  },
  "effect": {
    "entry_id": "uuid",
    "trace_id": "uuid",
    "operation_id": "uuid",
    "operation_kind": "reserve",
    "idempotency_scope": "org:example:reserve",
    "idempotency_key": "reserve:business-operation",
    "request_fingerprint": "sha256:..."
  }
}
```

## Stable error categories

The API does not return SQL or driver messages. It maps failures to stable categories:

- `ledger_operation_collision` — same operation/scoped key, different immutable content;
- `ledger_account_not_found`;
- `ledger_insufficient_funds`;
- `ledger_operation_invalid`;
- `ledger_operation_persistence_unavailable`;
- `ledger_org_forbidden`;
- `explicit_trace_required`.

Detailed database errors remain server-side.

## Reads

Operation lookup returns one effect. Trace lookup is ordered by creation time and capped at
200 effects in this first contract. Every returned row passes org-boundary enforcement.

## Rollout boundary

The canonical API is expand-only. Existing `/v1/ledger/*` routes remain compatibility paths.
Production cutover still requires Gateway/Execution caller migration, traffic telemetry,
compiled HTTP tests, concurrent database tests, and a configuration gate rejecting residual
compatibility writes.

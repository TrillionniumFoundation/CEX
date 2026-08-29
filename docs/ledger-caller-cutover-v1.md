# Ledger Caller Cutover v1

- Status: shared contract and Gateway client implementation candidate; canonical legacy reserve is fail-closed
- Shared wire contract: `shared_types::ledger_v2::LedgerEffectRequestV1`
- Gateway client: `infrastructure::ledger_v2_client`
- Safety gate: `scripts/check-ledger-caller-cutover.py`

## Exact-money boundary

The canonical Ledger API accepts only:

- integer `amount_minor` serialized as a JSON string;
- explicit `currency_unit`;
- explicit `currency_scale`;
- operation kind;
- scoped idempotency;
- trace and optional operation UUIDs.

No canonical caller may derive minor units from an in-memory `f64`. Legacy floating-point
fields remain compatibility inputs only until their API contracts are expanded and migrated.

## Gateway client modes

`CEX_GATEWAY_LEDGER_MODE` defines rollout intent:

- `legacy_v1`: canonical effects are disabled; default and rollback mode;
- `dual`: exact requests may use v2 while legacy requests remain on v1;
- `require_v2`: legacy writes are forbidden.

The mode parser and exact client are implemented, but Invocation orchestration is not yet
switched. This is deliberate: the current `CreateInvocationBody.reserve_amount` and stored
`InvocationRequest.reserve_amount` remain compatibility `f64` fields while the exact contract is
introduced.

## Canonical Invocation fail-closed boundary

`POST /v1/invocations` rejects a request that supplies the legacy `reserve_amount` field before
API-key resolution, capability lookup, Invocation persistence or any other upstream call. The
response uses the stable code `legacy_reserve_requires_exact_ingress` and points callers to the
two-step migration:

1. create a non-monetary Invocation skeleton without `reserve_amount`;
2. register the exact reserve with `POST /v2/invocations/:invocation_id/exact-reserve`, using a
   canonical string `amount_minor` and explicit currency metadata.

When `reserve_amount` is absent, the stored request omits the compatibility key as well, so the
0066/0073 dual-money guard can distinguish an exact attachment from legacy intent. A non-production
rollback may set `CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS=true`; production-like profiles ignore the
switch and remain fail-closed.

Machine-readable rollout posture:

```text
CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS=false
legacy_reserve_fail_closed=true
canonical_reserve_guard=before API-key resolution
```

## Stable Invocation effect builder

`invocation_ledger_effect` builds reserve/refund requests from:

- authenticated org;
- account ID;
- invocation ID;
- invocation trace ID;
- exact `MoneyAmount`;
- explicit operation kind.

It does not accept `f64`. Operation ID may be omitted because Ledger deterministically derives
it from the scoped idempotency pair.

## Safety gate

`check-ledger-caller-cutover.py` fails if Invocation orchestration begins calling Ledger v2
while the authoritative Invocation reserve contract remains floating point. It also rejects
common ad-hoc conversion patterns such as multiplication, casts and rounding in the v2 client.
It additionally verifies that the canonical Invocation boundary is fail-closed before the legacy
implementation and that the break-glass switch defaults to false.

The current expected status is:

```text
legacy_f64_contract_present=true
canonical_invocation_call_enabled=false
cutover_ready=false
legacy_reserve_fail_closed=true
legacy_break_glass_default=false
```

This is a blocker report for the exact-field caller cutover, while the legacy value route itself is
closed by default. It is not production authorization.

## Next implementation

1. Add an exact `reserve_money` field to the Invocation ingress and stored request.
2. Reject requests that supply both legacy and exact reserve forms.
3. Preserve missing exact fields when reading historical request payloads.
4. In `dual`, route exact reserve/refund through Ledger v2 and legacy input through v1.
5. In `require_v2`, reject legacy reserve input before any durable mutation or external call.
6. Pass the exact reserve contract into Execution for consume/refund.
7. Add HTTP, persistence, restart and rollback tests.
8. Measure compatibility traffic before making `require_v2` the production default.

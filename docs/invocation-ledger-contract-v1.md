# Invocation Ledger Contract v1

- Status: database implementation candidate
- Migration: `0066_add_invocation_ledger_contract.sql`
- Schema: `cex.invocation.ledger-contract.v1`
- Lifecycle gate: `scripts/check-invocation-ledger-contract-postgres.sh`
- Independent terminal gate: `scripts/check-invocation-ledger-terminal-postgres.sh`

## Problem

Gateway cannot safely switch reserve/refund to the canonical exact Ledger API while Execution
still settles from the legacy `reserve_amount: f64` request. Doing so can create a reserved-value
leak: reserve succeeds with an exact amount, Execution sees no equivalent exact contract, and
the terminal consume/refund is skipped or uses a different value.

## Authority

`cex_invocation_ledger_contracts_v1` binds one Invocation to:

- account and tenant;
- trace ID;
- exact currency unit, scale and minor units;
- scoped idempotency namespace;
- deterministic reserve, consume and refund operation IDs;
- source service/principal;
- immutable contract hash;
- current settlement state and last effect evidence.

The allowed lifecycle is:

```text
registered -> reserved -> consumed/refunded
```

Terminal states cannot cross: a consumed contract cannot refund and a refunded contract
cannot consume.

## Registration

`cex_register_invocation_ledger_contract_v1` validates authoritative Invocation/account tenant
and currency bindings, serializes concurrent registration with an advisory lock, returns the
original row for exact replay, and rejects immutable-content collision with SQLSTATE 23505.

Registration also writes `invocation.ledger_contract.registered` to the Audit outbox in the
same transaction.

## Effect request projection

`cex_invocation_ledger_effect_request_v1(invocation_id, operation_kind)` returns the canonical
Ledger request document shared by Gateway and Execution:

- `amount_minor` is a JSON string;
- operation ID is deterministic and explicit;
- trace, account, currency and reference are taken from the durable contract;
- idempotency key is the operation kind within the Invocation scope.

The function refuses consume/refund before reserve and refuses a terminally incompatible
request.

## Effect binding

An after-insert trigger on `ledger_entries` validates every Invocation-bound reserve, consume
or refund against the contract. Account, trace, amount, scale, operation ID, scope and key
must match. A mismatch aborts the entire Ledger transaction, including account projection,
entry and Audit intent.

The same trigger advances contract state and stores the last operation/entry evidence.

## Terminal exclusivity evidence

The independent terminal-exclusivity probe uses separate consumed and refunded contracts. It
sets its rejection flag only inside an actual database exception handler, then verifies:

- refund request after consume is rejected;
- consume request after refund is rejected;
- rejected requests add no ledger entry;
- account balance/reservation values do not change;
- contract states remain `consumed` and `refunded`;
- status view reports no missing effect evidence.

This separate probe avoids the false-positive pattern where a test throws its own failure
exception and then catches that same exception as if the system had rejected the transition.

## Rollout order

1. deploy 0066 and run fresh/upgrade probes;
2. register contracts from exact Invocation ingress;
3. make Execution query the durable request for consume/refund;
4. make Gateway query the durable request for reserve/refund;
5. enable `dual` mode;
6. observe residual compatibility traffic;
7. enable `require_v2` only after settlement and rollback drills.

## Required evidence

- exact registration replay and collision rejection;
- tenant/currency mismatch denial;
- reserve/consume and reserve/refund paths;
- independent terminal-mutual-exclusion denial;
- wrong operation ID rollback;
- account projection, entry, Audit intent and contract-state atomicity;
- concurrent registration and settlement;
- provider-success/local-receipt failure behavior;
- Execution restart settlement;
- Gateway rollback mode;
- status view with zero missing effect evidence;
- backup/restore and least-privilege role tests.

## Security boundary

The functions are initially public schema candidates because current services share one
PostgreSQL database. Production qualification must revoke direct execution from broad roles
and grant registration, request projection and Ledger effect application only to dedicated
service roles.

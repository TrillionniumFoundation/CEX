# Ledger Operation Identity v1

- Status: database contract implementation candidate
- Migration: `0065_add_ledger_operation_identity.sql`
- PostgreSQL gate: `scripts/check-ledger-operation-identity-postgres.sh`
- Canonical schema: `cex.ledger.effect.v1`

## Goal

Every immutable financial effect must be attributable to one operation and one trace without
relying on arbitrary free-text references.

The new contract adds:

- `trace_id`;
- `operation_id`;
- `operation_kind`;
- scoped idempotency;
- source service and principal;
- schema version;
- provenance mode;
- immutable request fingerprint.

## Legacy truth

Existing rows are not assigned invented cross-service history. They receive deterministic,
entry-scoped identities and `provenance_mode=legacy_entry_scoped`. Historical reference text
is retained but is not promoted into a trusted trace.

Existing global idempotency uniqueness is replaced by:

```text
(idempotency_scope, idempotency_key)
```

This allows the same textual key in different business scopes while preserving one effect
per scope/key pair.

## Exact effect function

`cex_apply_ledger_effect_v1` is the new database authority for reserve, consume, refund and
grant effects. In one transaction it:

1. validates identifiers, amount, scale, source and reference shape;
2. serializes by operation ID and scoped idempotency key;
3. returns the original effect for exact replay;
4. rejects immutable-content collision with SQLSTATE `23505`;
5. locks the account;
6. applies exact minor-unit balance/reservation semantics;
7. inserts one immutable ledger entry;
8. writes one canonical Audit v2 outbox intent through an INSERT trigger;
9. returns the resulting account projection and effect receipt.

The operation uses `amount_minor` and the account's configured scale. This is an expand path,
not yet the global Money v2 read cutover; legacy v1 routes still exist.

## Compatibility writers

During rollout, an INSERT from a legacy v1 writer is not silently treated as explicit
provenance. A BEFORE INSERT trigger supplies deterministic operation/trace identity and
labels it:

```text
provenance_mode=operation_scoped_compatibility
source_principal=legacy-v1-api
```

This preserves availability while making residual compatibility traffic measurable. Public
production promotion requires the compatibility count and traffic telemetry to reach zero or
have a reviewed waiver.

## Audit durability

Every new ledger entry enqueues `ledger.effect.persisted` in the same transaction. The Audit
payload records exact minor units, operation identity, scoped idempotency, source principal,
reference binding, request fingerprint and post-effect account summary. It does not replace
the ledger entry as the value authority.

## Append-only rule

UPDATE and DELETE on `ledger_entries` are rejected. Corrections must be new compensating
entries with their own operation identity.

## Read and evidence

The migration adds indexes for operation ID, trace and operation kind, plus
`cex_ledger_operation_identity_status_v1` for:

- total entries;
- explicit entries;
- compatibility entries;
- entry-scoped legacy entries;
- missing provenance;
- operation-ID uniqueness;
- scoped idempotency cardinality.

## Required rollout evidence

- fresh 0001–0065 migration;
- upgrade from a database containing real legacy entries;
- exact replay and collision rejection;
- concurrent same-operation/same-key callers;
- same key in different scopes;
- source-row/Audit-intent rollback atomicity;
- append-only denial;
- compatibility traffic count;
- Gateway and Execution caller migration;
- least-privilege SQL roles;
- backup/restore and reconciliation.

## Remaining cutover

The database contract is not the final API cutover. The next commit must expose a canonical
authenticated HTTP surface, derive a stable operation ID when a caller supplies only a scoped
key, enforce explicit trace in production-like profiles, and provide reads by operation and
trace. Existing v1 routes remain compatibility-only until their callers migrate.

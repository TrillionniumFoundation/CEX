# Ledger Operation Identity v1

- Status: database and HTTP implementation candidate
- Migration: `0065_add_ledger_operation_identity.sql`
- PostgreSQL gate: `scripts/check-ledger-operation-identity-postgres.sh`
- Canonical schema: `cex.ledger.effect.v1`
- HTTP contract: `docs/ledger-operation-api-v1.md`

## Goal

Every immutable financial effect must be attributable to one operation and one trace without
relying on arbitrary free-text references.

The contract adds trace/operation IDs, operation kind, scoped idempotency, source service and
principal, schema version, provenance mode and immutable request fingerprint.

## Legacy truth

Existing rows are not assigned invented cross-service history. They receive deterministic,
entry-scoped identities and `provenance_mode=legacy_entry_scoped`. Historical reference text
is retained but is not promoted into a trusted trace.

Existing global idempotency uniqueness is replaced by:

```text
(idempotency_scope, idempotency_key)
```

## Exact effect authority

`cex_apply_ledger_effect_v1` validates, serializes, exact-replays, rejects collision, locks the
account, applies exact minor-unit semantics, inserts one immutable entry and writes one Audit
intent in one transaction.

The HTTP service exposes this authority through:

```text
POST /v2/ledger/effects
GET  /v2/ledger/effects/:operation_id
GET  /v2/ledger/traces/:trace_id
```

Rust does not reproduce balance state transitions.

## Compatibility writers

Legacy v1 INSERTs receive deterministic identity and are labelled
`operation_scoped_compatibility`, with source principal `legacy-v1-api`. This preserves the
expand phase while making residual compatibility traffic measurable.

## Audit and append-only behavior

Every new ledger entry enqueues `ledger.effect.persisted` in the same transaction. UPDATE and
DELETE are rejected; corrections are new compensating effects.

## Evidence and remaining cutover

`cex_ledger_operation_identity_status_v1` exposes explicit, compatibility, legacy and missing
provenance counts. Production promotion still requires:

- executed fresh/upgrade/concurrency tests;
- authenticated HTTP compile and route tests;
- Gateway and Execution caller migration;
- explicit-trace production configuration;
- compatibility traffic reaching zero or a reviewed waiver;
- least-privilege roles;
- backup/restore and reconciliation evidence.

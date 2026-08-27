# Audit Source Transactional Enqueue v1

- Status: implementation candidate
- Execution migration: `0061_add_execution_transactional_audit_outbox.sql`
- Identity migration: `0062_add_identity_transactional_audit_outbox.sql`
- Dependency: `0060_close_audit_outbox_delivery_lifecycle.sql`

## Invariant

For a critical authoritative mutation, the source row and Audit outbox intent must commit
or roll back in the same PostgreSQL transaction.

```text
authoritative source mutation
        +
canonical Audit v2 intent
        =
one database commit
```

A synchronous HTTP Audit call remains useful as compatibility telemetry, but it is not the
durability authority and must not be used as evidence that every committed state
transition was recorded.

## Execution

`executions.audit_revision` is database-owned:

- insert sets revision 1;
- a meaningful update increments it;
- an `updated_at`-only/no-op update keeps the existing revision.

The trigger covers lifecycle status, invocation/tenant binding, provider target, attempt
budget, worker lease, start/end timestamps, result receipt changes and approval state.

Event identity is deterministic over:

```text
execution-service + execution_id + audit_revision + event_type
```

This keeps the ID stable when a transaction is rolled back and retried, while allowing a
later real transition to receive a distinct identity even if the same state value appears
again.

The event payload contains a SHA-256 digest of `result_payload`, not the full provider
output. This prevents model output, secrets or PII from being copied wholesale into the
Audit chain.

The execution org is checked against the authoritative invocation org. A mismatch or
missing invocation fails the transaction closed.

Durable event types:

- `execution.persisted.created`
- `execution.persisted.status_changed`
- `execution.persisted.attempt_changed`
- `execution.persisted.lease_changed`
- `execution.persisted.result_changed`
- `execution.persisted.approval_changed`
- `execution.persisted.changed`

## Identity

`api_keys.audit_revision` is also database-owned. Meaningful changes include org/user
provenance, key material/prefix, label, status, expiry and revocation metadata.

`last_used_at`-only updates are intentionally excluded. Authentication use telemetry can
be high volume and does not represent an administrative security mutation. It requires a
separate metrics/retention policy, not one immutable security event per resolve.

The event never includes `key_hash` or the raw API key. It records only non-secret
provenance such as key prefix, label, status, expiry and revocation metadata.

Durable event types:

- `identity.api_key.persisted.issued`
- `identity.api_key.persisted.revoked`
- `identity.api_key.persisted.expiry_changed`
- `identity.api_key.persisted.material_changed`
- `identity.api_key.persisted.changed`

Identity currently uses `api_key_id` as the stable trace id. Admin actor context can be
provided inside the same transaction through:

```sql
select set_config('cex.audit.actor_id', 'principal-id', true);
select set_config('cex.audit.actor_label', 'Principal Label', true);
```

When absent, actor id falls back to the key user id. Application code should set these
transaction-local values when issue/revoke/rotate operations are converted to explicit SQL
transactions.

## Existing-row baseline

The migrations deliberately do not bulk-enqueue every pre-existing Execution or API-key
row inside the schema migration. Existing rows retain `audit_revision = 0` until their next
meaningful mutation. A separate bounded, resumable baseline/backfill job must:

- snapshot the source row under a stable cursor;
- set revision 1 and enqueue a `persisted.baseline` event atomically;
- rate-limit against Audit outbox SLOs;
- produce cardinality and checksum evidence;
- be exactly replayable and safe across restart.

Production promotion is blocked until that baseline job is implemented and its residual
revision-0 count is zero or explicitly waived.

## Trigger trade-offs

The first production slice uses database triggers because current repositories write the
authoritative rows through multiple code paths. Triggers guarantee coverage without
relying on each caller to remember a second insert.

Trade-offs:

- trigger failure aborts the source transaction by design;
- schema changes must update trigger comparisons and payloads;
- actor context is limited unless the application sets transaction-local metadata;
- direct privileged SQL can bypass policy unless roles are narrowed;
- triggers must not grow into orchestration engines.

A later repository API may replace trigger internals, but must preserve the same atomicity,
event identity and replay tests before the triggers are removed.

## Transaction and replay tests

The migration gate must prove:

### Execution

- insert creates revision 1 and one outbox intent;
- timestamp-only update creates no new intent;
- lifecycle update increments revision and creates the expected event;
- org mismatch fails closed;
- forced rollback removes both source row and intent.

### Identity

- issue creates revision 1 and one intent;
- payload contains no `key_hash`;
- `last_used_at`-only changes neither revision nor intent count;
- revoke increments revision and records transaction-local admin actor;
- forced rollback removes both key row and intent.

### Generic enqueue

- exact replay returns the same row;
- immutable collision returns SQLSTATE 23505;
- replay never resets delivery state;
- source marker/org/trace/schema/time mismatches fail closed.

## Dual-observe rollout

Existing best-effort events use different event type names and remain temporarily enabled.
They are a compatibility stream, not a second authority.

Removal requires staging coverage evidence, dispatcher SLO compliance, a rollback drill,
consumer migration and an ADR with a deprecation window.

## Next source conversions

The next locked slice after this batch is:

1. existing-row Audit baseline/backfill;
2. ledger immutable trace/operation IDs and genesis-as-entry;
3. Money v2 backfill and dual-read verification;
4. ledger projection rebuild/reconciliation;
5. provider and ledger network calls moved outside business SQL transactions;
6. saga crash/replay/concurrency qualification.

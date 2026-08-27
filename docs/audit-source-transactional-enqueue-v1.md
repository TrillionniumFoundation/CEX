# Audit Source Transactional Enqueue v1

- Status: implementation candidate
- Audit lifecycle migrations:
  - `0060_close_audit_outbox_delivery_lifecycle.sql`
  - `0061_close_audit_outbox_delivery_transitions.sql`
- Execution migration: `0062_add_execution_transactional_audit_outbox.sql`
- Identity migration: `0063_add_identity_transactional_audit_outbox.sql`

## Invariant

For a critical authoritative mutation, the source row and canonical Audit v2 intent must
commit or roll back in the same PostgreSQL transaction.

```text
authoritative source mutation
        +
canonical Audit v2 intent
        =
one database commit
```

A synchronous HTTP Audit call may remain as compatibility telemetry, but it is not the
durability authority.

## Execution

`executions.audit_revision` is database-owned:

- insert sets revision 1;
- meaningful mutation increments it;
- timestamp-only/no-op updates retain the revision.

Covered facts include lifecycle status, invocation/tenant binding, provider target, attempt
budget, worker lease, start/end times, result-receipt changes and approval state.

Event identity is deterministic over:

```text
execution-service + execution_id + audit_revision + event_type
```

The payload stores a SHA-256 digest of `result_payload`, not provider output. Execution
tenancy must match the authoritative invocation org or the source transaction fails closed.

Durable event types:

- `execution.persisted.created`
- `execution.persisted.status_changed`
- `execution.persisted.attempt_changed`
- `execution.persisted.lease_changed`
- `execution.persisted.result_changed`
- `execution.persisted.approval_changed`
- `execution.persisted.changed`

## Identity

`api_keys.audit_revision` is database-owned. Meaningful changes include org/user
provenance, key material/prefix, label, status, expiry and revocation metadata.

`last_used_at-only` is deliberately excluded because it is high-volume authentication
telemetry rather than an administrative security mutation.

Audit payloads never contain the raw key or `key_hash`. They contain non-secret provenance:
key id/prefix, label, status, expiry, revocation facts and revision.

Durable event types:

- `identity.api_key.persisted.issued`
- `identity.api_key.persisted.revoked`
- `identity.api_key.persisted.expiry_changed`
- `identity.api_key.persisted.material_changed`
- `identity.api_key.persisted.changed`

Identity uses `api_key_id` as the stable trace id for this first slice. Transaction-local
admin actor context may be supplied with:

```sql
select set_config('cex.audit.actor_id', 'principal-id', true);
select set_config('cex.audit.actor_label', 'Principal Label', true);
```

## Existing-row baseline

Schema migrations deliberately do not flood the outbox with every pre-existing Execution
or API-key row. Rows predating the source triggers retain `audit_revision = 0`.

The bounded baseline job must:

- select revision-0 rows in deterministic batches;
- serialize a single worker per source;
- atomically set revision 1 and enqueue `persisted.baseline`;
- persist progress and last processed source id;
- pause at a configured nonterminal outbox backlog;
- remain exact-replay safe after restart;
- report residual counts and completion/blocked state.

Production promotion remains blocked until residual revision-0 rows reach zero or have a
reviewed waiver.

## Trigger trade-offs

The first slice uses database triggers because authoritative rows are written through
multiple application paths. Trigger failure aborts the source transaction by design.

Trade-offs:

- schema changes must update trigger comparisons and payloads;
- actor context is limited unless applications set transaction-local metadata;
- privileged SQL must be constrained with least-privilege roles;
- triggers must remain coverage mechanisms, not orchestration engines.

## Required probes

Execution:

- insert creates revision 1 and one intent;
- timestamp-only update creates no intent;
- meaningful update increments revision and creates the expected intent;
- org mismatch fails closed;
- rollback removes source and intent.

Identity:

- issue creates revision 1 and one intent;
- payload does not contain `key_hash`;
- `last_used_at-only` changes neither revision nor intent count;
- revoke records transaction-local actor context;
- rollback removes source and intent.

Generic enqueue/delivery:

- exact enqueue replay returns the same row;
- immutable collision returns SQLSTATE 23505;
- replay does not reset lifecycle state;
- source/org/trace/schema/time mismatches fail closed;
- only an active owning lease may ACK/fail;
- verified remote receipt is required before `delivered`;
- retry/dead-letter transitions preserve evidence.

## Dual-observe rollout

Legacy best-effort events use distinct event names and may remain temporarily enabled. They
are a compatibility stream, not a second authority. Removal requires staging coverage,
dispatcher SLO evidence, rollback drill, consumer migration and an ADR.

## Next locked slice

1. existing-row Audit baseline/backfill;
2. immutable Ledger trace/operation identity;
3. genesis-as-entry;
4. Money v2 backfill/read cutover;
5. projection rebuild/reconciliation;
6. provider and Ledger network calls outside business SQL transactions;
7. Saga crash/replay/concurrency qualification.

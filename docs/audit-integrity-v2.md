# Audit Integrity v2

## Goals

Audit v2 adds the minimum integrity primitives that the legacy best-effort event table lacks:

- stable caller-generated `event_id` for replay;
- authenticated `writer_service_id` stored independently of domain actor fields;
- versioned event contract;
- per-tenant monotonic sequence;
- previous-hash/current-hash chain;
- append-only mutation trigger;
- durable outbox schema with bounded claim lease and attempt budget.

## API

### Append

`POST /v1/audit/events/v2`

Requires workload authentication. Compatibility/anonymous writers are rejected even when local auth mode is off.

The request supplies domain facts only:

```json
{
  "event_id": "uuid",
  "trace_id": "uuid",
  "org_id": "uuid-or-null",
  "actor_type": "policy-engine",
  "actor_id": "operator-1",
  "event_type": "execution.approved",
  "schema_version": "cex.audit.event.v2",
  "occurred_at": "RFC3339",
  "payload": {}
}
```

Writer identity and authentication scheme come from middleware.

Exact replay of the same `event_id` returns the original record with `replayed=true`. Reuse of the id with different immutable content returns `409`.

### Read

`GET /v1/audit/events/v2/trace/:trace_id`

Requires `audit:read` and applies the existing optional org restriction. A trace containing global/multiple-org v2 events is not returned to an org-scoped reader.

### Metrics

`GET /metrics/audit-v2`

Exports event count, chain count, outbox backlog and oldest nonterminal outbox age.

## Chain model

- org event chain key: `org:<org_uuid>`;
- event without org: `global`;
- one chain-head row is locked per append;
- sequence increments under the same transaction;
- event hash is SHA-256 over a canonical JSONB envelope including previous hash, sequence, ids, authenticated writer, domain actor, schema, time and payload;
- the genesis previous-hash input is the literal `GENESIS`, while the stored previous hash is null.

The chain provides tamper evidence, not external notarization. A privileged database superuser could still rewrite tables/functions. Production hardening must add restricted roles, immutable export and signed checkpoints.

## Append-only behavior

`cex_audit_events_v2` rejects UPDATE and DELETE through a trigger. Corrections must be new events referencing the prior event.

## Durable outbox

`cex_audit_outbox_v1` is source-side delivery state:

- unique event id;
- pending/claimed/retry-wait/delivered/dead-letter states;
- `FOR UPDATE SKIP LOCKED` claim function;
- short lease;
- bounded attempts;
- JSON object envelope;
- queue summary view.

This migration creates the durable primitive. Existing services have not yet switched their business transactions to insert outbox rows; that is the next rollout phase.

## Required next steps

1. Add source-service helpers that insert outbox rows in the same transaction as business state.
2. Implement an authenticated outbox dispatcher that calls audit v2.
3. Mark delivered only after the returned event id/hash is verified.
4. Add retry/dead-letter transition functions and acknowledgement.
5. Add append-only database role and deny direct legacy writes in production.
6. Export signed chain-head checkpoints to immutable storage.
7. Add PII/secret redaction and schema registry enforcement.
8. Run concurrent append, exact replay, collision, mutation denial and restore verification tests.

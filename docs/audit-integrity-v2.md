# Audit Integrity v2

## Status

- Append/hash-chain substrate: implementation candidate
- Delivery lifecycle: migration 0060
- Same-transaction source coverage: Execution and Identity in migrations 0061–0062
- Production evidence: pending executable CI, upgrade/restore and fault-injection results

## Goals

Audit v2 adds the integrity primitives that the legacy best-effort event table lacks:

- stable caller-generated `event_id` for replay;
- authenticated `writer_service_id` stored independently of domain actor fields;
- versioned event contract;
- per-tenant monotonic sequence;
- previous-hash/current-hash chain;
- append-only mutation trigger;
- durable source outbox;
- bounded claim lease and attempt budget;
- verified delivery receipts and dead-letter evidence.

## API

Audit v2 uses `/v2/audit/...` as its canonical version boundary. The older
`/v1/audit/events/v2...` paths remain temporary authenticated compatibility aliases.

### Append

Canonical endpoint: `POST /v2/audit/events`

Temporary compatibility alias: `POST /v1/audit/events/v2`

Workload authentication is required. The request supplies domain facts only:

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

Writer identity and authentication scheme come from middleware. Exact replay of the same
`event_id` returns the original record with `replayed=true`; different immutable content
returns `409`.

### Read

Canonical endpoint: `GET /v2/audit/events/trace/:trace_id`

Temporary compatibility alias: `GET /v1/audit/events/v2/trace/:trace_id`

Both require `audit:read` and apply the configured org restriction.

### Metrics

Canonical endpoint: `GET /v2/audit/metrics`

Compatibility/operator alias: `GET /metrics/audit-v2`

The service exports event count, chain count, outbox backlog and oldest nonterminal age.
Migration 0060 additionally provides `cex_audit_outbox_delivery_summary_v1`.

## Route compatibility policy

- only canonical routes are allowed in new SDKs/OpenAPI/examples;
- aliases share the same authentication and persistence behavior;
- removal requires caller search, zero-use telemetry, an ADR and a deprecation window;
- an alias may never weaken authentication, tenancy, replay or persistence semantics.

## Chain model

- org event chain key: `org:<org_uuid>`;
- event without org: `global`;
- one chain-head row is locked per append;
- sequence increments in the append transaction;
- event hash is SHA-256 over a canonical JSONB envelope;
- genesis hash input is literal `GENESIS`; stored previous hash is null.

The chain provides tamper evidence, not external notarization. A privileged database
superuser could still rewrite tables/functions. Production hardening requires restricted
roles, immutable export and externally signed checkpoints.

## Append-only behavior

`cex_audit_events_v2` rejects UPDATE and DELETE through a trigger. Corrections are new
events that reference the prior event.

## Source outbox and delivery

`cex_audit_outbox_v1` is the source-side delivery authority.

```text
pending/retry_wait -> claimed -> delivered
                         |
                         +-> retry_wait
                         |
                         +-> dead_letter
```

Migration 0060 adds validated exact-replay enqueue, ACK receipt/hash/sequence evidence,
retry scheduling, dead letter, worker/lease enforcement and a delivery summary view.

The dedicated `audit-outbox-dispatcher` authenticates to the canonical endpoint. A 2xx
response is acknowledged only after immutable fields, writer identity, chain key, sequence
and hash format are verified. Remote append success followed by local ACK failure is
recovered by event-id replay after lease expiry.

See `docs/audit-outbox-delivery-v1.md`.

## Source transaction coverage

### Execution

Migration 0061 adds database-owned `executions.audit_revision` and same-transaction outbox
intent generation for insert and meaningful lifecycle changes. Timestamp-only updates are
suppressed. Provider result payloads are represented by digest rather than copied into the
Audit payload.

### Identity

Migration 0062 adds database-owned `api_keys.audit_revision` and same-transaction intents
for issue, revoke, expiry, material and administrative metadata changes. `last_used_at-only`
auth telemetry is suppressed, and raw keys/key hashes are never copied.

See `docs/audit-source-transactional-enqueue-v1.md`.

Coverage remains incomplete until pre-existing rows are baselined and ledger financial
operations, repairs and other authoritative tables have equivalent source-transaction
binding.

## Writer and source identity

For direct writes, `writer_service_id` is the authenticated source workload.

For durable outbox delivery, Audit writer identity is the authenticated
`audit-outbox-dispatcher`; the original source service is bound in the immutable outbox row
and hashed payload marker `_cex_audit_source_service`.

This is an explicit workload-token-v1 delegation compromise. A future JWT/mTLS contract
should carry signed issuer/audience/delegation claims.

## Sensitive data policy

Audit payloads must not contain API keys/key hashes, workload/admin tokens, database URLs,
provider prompts/results by default, raw credentials/session secrets or unrestricted PII.
Execution result content is represented by digest in the source trigger. Additional schemas
require registry validation and redaction tests before production promotion.

## Required next steps

1. Execute dispatcher compile/unit/integration tests in GitHub CI.
2. Run fresh and supported-version upgrade migrations.
3. Fault-inject Audit outage and remote-success/local-ACK crash.
4. Add bounded baseline/backfill for revision-0 Execution and API-key rows.
5. Add operator-authorized dead-letter inspection/requeue with immutable evidence.
6. Add least-privilege source/dispatcher/Audit database roles.
7. Add source binding for ledger operation identity and repair.
8. Propagate transaction-local admin/worker actor context from service repositories.
9. Export externally signed chain-head checkpoints.
10. Remove synchronous compatibility writes only after staging evidence and rollback drill.

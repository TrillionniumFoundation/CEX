# Matrix transport durability v1

Status: active Matrix transport durability contract  
Implementation owner: `matrix-integration`  
Migration: `services/matrix-entry-adapter/migrations/0001_transport_durability.sql`  
Production authorization: `not_granted`

## Scope

This contract defines durable transport state shared by the Matrix entry adapter, poller and relay. It does not make Matrix user, room or event claims authoritative for CEX domains. Identity, research, Ledger, World/Game and finality decisions remain with their owning services. The transport layer owns only opaque sync position, stable source-event identity, delivery intent, lease history and poison-event observations.

## Cursor compare-and-set and lease fencing

Each polling partition has one opaque cursor, monotonically increasing revision and monotonically increasing lease fence. A worker must acquire the partition lease before reading or advancing its cursor. A different owner cannot take an unexpired lease; takeover after expiry receives a higher fence. Cursor advancement requires the exact owner, fence, expected revision and an unexpired lease.

The cursor value is opaque Matrix transport data. CEX never parses it to infer room, user or domain authority. A stale owner, stale fence, stale revision or expired lease cannot advance the cursor. Multi-instance operation is permitted only through these compare-and-set and lease fencing rules.

## Inbox identity and atomic admission

Every source event is bound to a stable Matrix event ID and SHA-256 of the exact bounded normalized bytes. First admission is `accepted`; an exact retry is `replay`; reuse of the same event ID with different bytes is an identity collision and aborts the transaction. Inbox records are immutable.

`cex_matrix_register_delivery_and_advance_v1` verifies the cursor lease, accepts the event, creates or replays one stable delivery and advances the cursor in one PostgreSQL transaction. If any identity, lease, payload or revision check fails, none of the three state changes commits. A poller must not advance a cursor merely because a remote HTTP request was attempted.

For Matrix sync batches containing multiple supported events, callers must register all deliveries in one explicit database transaction and advance the batch cursor only after every supported event has durable acceptance. The single-event helper is the canonical primitive and does not authorize skipping unregistered events.

## Outbox, retry and response loss

A delivery has one UUID, source event ID, destination, immutable payload bytes represented by JSON plus SHA-256, maximum attempt count and append-only transition history. Delivery identity and payload fields cannot be updated after insertion. Claims use `FOR UPDATE SKIP LOCKED`, increment the fence and commit before network I/O.

Only the current owner with the exact fence and an unexpired lease can finish a claim. Retryable failure returns the same stable delivery to `pending` while budget remains; budget exhaustion or permanent failure enters `dead_letter`; verified downstream acceptance enters `sent`. An expired claim may be reclaimed with a higher fence and the history records whether the previous state was `pending` or `claimed`.

Downstream requests must carry the stable delivery ID as their idempotency identity. If the remote effect may have occurred and the local acknowledgement response is lost, the worker recovers through `cex_matrix_lookup_delivery_v1` and downstream lookup/replay using the same delivery ID and payload hash. It must not create a second delivery identity or advance the cursor from transport success alone.

## Poison-event isolation

Unsupported, malformed or repeatedly failing source events are recorded by stable ID, exact hash, partition and bounded failure code. Exact repeated observation increments a counter; different bytes or classification under the same event identity collide. Operators can acknowledge the record without deleting it. Poison-event acknowledgement is not permission to invent a downstream success or silently skip a cursor range.

A production poller needs an explicit policy mapping each failure class to retry, durable poison isolation or operator stop. The default must stop or isolate with evidence; it may not discard an unknown event and continue advancing the same cursor without a reviewed rule.

## Database security

All functions pin `search_path` to `pg_catalog, public` and are revoked from `public`. Runtime deployments grant only the required functions to a least-privilege transport role; migration ownership remains separate. Inbox and delivery-history rows reject update/delete. The outbox trigger freezes delivery ID, source identity, destination, payload hash, payload, retry budget and creation time.

Payloads are bounded to one MiB at the database boundary. Tokens, unrestricted message bodies, Agent private keys, CEX credentials and provider secrets must not enter payload, errors, metrics or poison notes. Application-level request and response bounds remain mandatory before database calls.

## Verification

Static and regression checks:

```text
python3 scripts/test-matrix-transport-durability.py
python3 scripts/check-matrix-transport-durability.py
```

PostgreSQL behavior check:

```text
MATRIX_TEST_DATABASE_URL=postgres://... \
  bash scripts/check-matrix-transport-postgres.sh \
  --evidence run/matrix-transport-postgres.json
```

The PostgreSQL check must cover active lease exclusion, expired-lease takeover, stale fence/revision rejection, atomic event/delivery/cursor commit, exact replay and collision, concurrent-safe claim, wrong-owner completion rejection, retry exhaustion, dead letter, sent lookup, immutable history/payload, poison-event replay/collision and acknowledgement.

## Deployment and rollback

Apply the migration with the approved schema owner before enabling durable workers. Start with active polling and sending disabled, import or explicitly initialize each partition cursor, then qualify one partition in shadow. Readiness requires database connectivity, schema/function presence, credential separation and a valid partition lease; process liveness alone is insufficient.

Rollback stops pollers and delivery workers before reverting application code. The migration is additive and durable records remain available for recovery; normal rollback must not drop inbox, outbox, cursor, history or poison evidence. A destructive retirement requires a separate migration, zero-use evidence, retained export and independent review.

## Current implementation boundary

The migration and executable database contract close the reusable persistence primitives. Existing `matrix-bot-poller`, `matrix-bot-relay` and `matrix-entry-adapter` processes remain supporting Alpha until their runtime paths use the durable functions, carry stable delivery identity to downstream services and pass restart/multi-instance tests against PostgreSQL. Source presence or static validation alone is not runtime promotion.

## Change protocol

Changes to cursor semantics, delivery identity, retry classification, payload bounds, poison policy, migration functions, application wiring or downstream idempotency require this contract, affected module contracts, module catalog, executable negative tests and exact-head hosted evidence to change together. No checker or migration can grant production authorization.

# Audit Outbox Delivery v1

- Status: implementation candidate
- Migration: `0060_close_audit_outbox_delivery_lifecycle.sql`
- Worker: `audit-outbox-dispatcher`
- Canonical sink: `POST /v2/audit/events`

## Purpose

Audit v2 already provides authenticated append, deterministic replay, tenant sequencing,
hash chaining and append-only event storage. This slice closes the missing delivery loop
between a source transaction and the Audit service.

The durable order is:

```text
source mutation + outbox intent commit
             |
             v
pending/retry_wait --claim--> claimed
             |
             +-- verified Audit v2 receipt --> delivered
             |
             +-- transient failure ----------> retry_wait
             |
             +-- permanent/budget exhausted -> dead_letter
```

A business transaction is not allowed to report durable success while its required Audit
intent is absent. The dispatcher may be temporarily unavailable without losing the intent.

## Database state machine

`cex_audit_outbox_v1` stores:

- immutable `event_id`, source service, org, trace and Audit v2 envelope;
- attempt count and maximum attempt budget;
- claim owner and lease expiry;
- last HTTP/error evidence;
- verified event hash and tenant sequence;
- complete Audit append receipt;
- delivered/dead-letter timestamps.

Transitions are exposed only through:

- `cex_claim_audit_outbox_v1`;
- `cex_mark_audit_outbox_delivered_v1`;
- `cex_fail_audit_outbox_delivery_v1`.

ACK and failure transitions reject a missing row, the wrong worker, an expired lease, a
different event id, or invalid hash/sequence/receipt evidence. An exact repeated ACK
returns the already-delivered row. A different receipt for the same row is a collision.

## Generic source enqueue

`cex_enqueue_audit_outbox_v1` validates the canonical Audit v2 envelope inside the source
transaction:

- non-nil event and trace IDs;
- relational/envelope org parity;
- `cex.audit.event.v2` schema;
- valid actor/event/time/payload shape;
- no `_cex_audit_writer` spoofing;
- `_cex_audit_source_service` parity;
- one immutable event identity.

An advisory transaction lock serializes concurrent attempts for the same event. Exact
replay returns the existing lifecycle row and never resets attempts, leases, delivery
evidence or dead-letter state.

## Dispatcher trust model

The dispatcher authenticates as the dedicated workload `audit-outbox-dispatcher`. The
Audit event writer is therefore the dispatcher. The originating service remains bound in
the immutable source outbox row and the hashed payload marker
`_cex_audit_source_service`.

This is an explicit delegation model for workload-token v1. A future JWT/mTLS revision
should carry signed delegation claims rather than relying on a shared database-backed
worker. The internal HTTP client refuses redirects so workload credentials are not
forwarded to another endpoint.

## Delivery verification

A 2xx response is not sufficient by itself. Before ACK, the worker verifies:

- event, trace and org IDs;
- actor, event type and schema;
- payload equality;
- occurred-at equality at PostgreSQL microsecond precision;
- authenticated dispatcher writer identity and auth scheme;
- expected chain key;
- positive tenant sequence;
- current and previous hash formats.

Only then is the receipt persisted and the row marked delivered.

If the remote append succeeds but local ACK fails, the row stays claimed. After lease
expiry it is replayed with the same event id. Audit v2 returns the original record, which
can then be acknowledged exactly. The worker must not classify that crash window as a new
business failure.

## Failure classification

Retryable:

- transport/read failure;
- HTTP 408, 425, 429;
- HTTP 5xx;
- malformed or unverifiable success receipt, because exact replay can recover it;
- success response whose body exceeds the configured protocol limit.

Permanent:

- authentication/authorization rejection;
- event collision;
- redirects;
- non-transient 4xx;
- invalid source envelope or source marker.

Retry delay uses bounded exponential backoff, deterministic event-derived jitter and a
numeric `Retry-After` value when present. The database remains the final authority for
attempt-budget exhaustion and dead-letter transition.

## Resource bounds

`Content-Length` is rejected before body collection when it exceeds the configured limit.
The collected body is checked again after read. Chunked responses are additionally bounded
by the request timeout; this version does not claim a streaming byte-hard-cap for a
malicious chunked peer.

Production defaults:

```text
batch size:       25
lease:            60s
request timeout:  20s
poll:              2s
response limit: 256 KiB
```

Request timeout must be shorter than the claim lease.

## Operation

One batch:

```bash
cargo run --locked -p audit-service --bin audit-outbox-dispatcher -- --once
```

Continuous:

```bash
cargo run --locked -p audit-service --bin audit-outbox-dispatcher
```

Production uses `deploy/systemd/cex-audit-outbox-dispatcher.service`.

## Metrics and alerting

The existing Audit v2 metrics expose outbox backlog and oldest age. Migration 0060 adds
`cex_audit_outbox_delivery_summary_v1`, including retry-budget and unverified-delivery
counts.

Minimum alerts:

- nonzero dead-letter count;
- oldest pending/retry age above the SLO;
- expired claimed lease backlog;
- unverified legacy delivered rows;
- repeated auth rejection;
- dispatcher process unavailable.

## Rollout gates

Before promotion:

1. Fresh and upgrade migrations execute.
2. Claim, retry, re-claim, ACK replay, ACK collision and dead-letter probes pass.
3. Workload token rotation is rehearsed.
4. Audit outage/recovery demonstrates eventual delivery.
5. Remote-success/local-ACK-crash is fault-injected.
6. Operator dead-letter inspection/requeue is added with immutable evidence.
7. Least-privilege database roles replace broad application ownership.

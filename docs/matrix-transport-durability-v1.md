# Matrix Transport Durability Contract v1

Status: repository implementation candidate  
Owner: `matrix-integration`  
Production authorization: `not_granted`  
Schema owner: `services/matrix-entry-adapter/migrations/0001_transport_durability.sql`

This document is normative for the CEX-owned Matrix transport path. It binds the poller, relay, adapter boundary, PostgreSQL schema, recovery rules and executable gates. It does not prove that a particular deployment, Matrix homeserver or candidate SHA has passed qualification.

## 1. Scope and authority

The contract covers transport from Matrix `/sync` observation through durable event admission, downstream adapter delivery, optional reply enqueue and Matrix homeserver send.

Matrix owns source events, room membership, sender identity and opaque sync cursors. CEX Matrix transport owns only:

- fenced cursor-lease observations;
- immutable source-event identity and hash;
- immutable delivery identity, destination, payload and payload hash;
- claim attempts, lease fences, retry/dead-letter state and transition history;
- bounded poison-event observations and explicit acknowledgement.

The Matrix transport path grants no CEX identity, entitlement, task, research, financial, World/Game or finality authority. An HTTP success is transport evidence only.

## 2. Runtime topology

The v1 runtime has three boundaries:

1. `matrix-bot-poller` acquires a cursor lease, reads Matrix `/sync`, validates supported events, registers the entire returned batch and advances the cursor through CAS.
2. `matrix-bot-relay` is the only outbox claimant. It dispatches `matrix-relay-adapter-v1` and `matrix-homeserver-v1` deliveries.
3. `matrix-entry-adapter` authenticates and normalizes the CEX-facing Matrix event. Its result may contain a presentation-only `projected_reply` that the relay enqueues durably.

Poller and relay share PostgreSQL procedures; they do not share process-local queues, cursor files or in-memory acceptance authority.

## 3. Durable schema

### 3.1 `matrix_transport_cursors`

One row exists per explicit partition. The row stores an opaque cursor, monotonically increasing `cursor_revision`, lease owner, monotonically increasing `lease_fence`, expiry and update time. The cursor is never parsed, synthesized or reset to make a deployment appear healthy.

### 3.2 `matrix_transport_inbox`

The inbox stores one immutable row per Matrix source event ID with a SHA-256 source fingerprint, partition and observed cursor. Update and delete are rejected by trigger. Exact replay returns `replay`; a reused event ID with different hash or partition raises an identity collision.

### 3.3 `matrix_transport_outbox`

The outbox stores one immutable delivery identity per `(source_event_id, destination)`, including delivery UUID, payload SHA-256, JSON payload and max-attempt policy. Identity-bearing fields are protected by trigger. Mutable claim/outcome fields are restricted to the stored procedures.

Allowed states are `pending`, `claimed`, `sent` and `dead_letter`. `claimed` requires owner and expiry. `sent` requires `sent_at`.

### 3.4 `matrix_transport_delivery_history`

Every creation, claim and completion transition is appended with delivery ID, lease fence, prior state, new state, owner and bounded error code. Update and delete are rejected by trigger.

### 3.5 `matrix_transport_poison_events`

A poison row binds source event ID, source hash, partition and failure code. Exact recurrence increments the observation count. Different bytes or classification under the same event ID raise a collision. Operator acknowledgement is one-time, actor-bound and note-bound; it never edits the original identity or advances a cursor.

## 4. Stored procedure contract

The migration exposes twelve procedures/functions and revokes public execution:

| Function | Contract |
|---|---|
| `cex_matrix_reject_immutable_mutation_v1` | rejects mutation of immutable evidence tables |
| `cex_matrix_guard_outbox_identity_v1` | rejects changes to delivery identity, payload or creation policy |
| `cex_matrix_acquire_cursor_lease_v1` | creates/acquires a live partition lease and increments fence |
| `cex_matrix_advance_cursor_v1` | owner/fence/revision/expiry-bound cursor CAS |
| `cex_matrix_accept_source_event_v1` | exact replay or fail-closed event collision |
| `cex_matrix_enqueue_delivery_v1` | exact replay or fail-closed delivery collision |
| `cex_matrix_register_delivery_and_advance_v1` | single-event atomic registration plus cursor CAS helper |
| `cex_matrix_claim_delivery_v1` | `FOR UPDATE SKIP LOCKED` claim with attempt and fence increment |
| `cex_matrix_finish_delivery_v1` | matching live claim to sent, pending or dead letter |
| `cex_matrix_lookup_delivery_v1` | response-loss lookup bound to delivery ID and payload hash |
| `cex_matrix_record_poison_event_v1` | exact poison recurrence or collision |
| `cex_matrix_acknowledge_poison_event_v1` | one-time operator acknowledgement |

Runtime roles receive only explicitly reviewed execute grants; table-owner or broad public access is not an activation requirement.

## 5. Poller transaction and cursor rules

For one successful `/sync` response, the poller:

1. holds a live lease for the configured partition;
2. validates all supported events before authority changes;
3. begins one PostgreSQL transaction;
4. calls source-event admission for each supported event;
5. enqueues the deterministic `matrix-relay-adapter-v1` delivery for each event;
6. calls cursor CAS once with the same owner, fence and expected revision;
7. commits only after CAS succeeds.

Any error rolls back the entire batch. Unsupported event kinds may be ignored. A malformed event in a supported class is recorded as poison outside the batch transaction and the cursor is held, preventing silent loss.

An empty but valid sync batch still advances the opaque cursor through CAS. Poller restart reuses the persisted cursor and deterministic identities.

## 6. Relay claim and delivery rules

The relay is the sole generic outbox claimant. Claims are ordered by creation and delivery ID, use `SKIP LOCKED`, increment attempt count and fence, and expire after a bounded lease.

### 6.1 Adapter destination

For `matrix-relay-adapter-v1`, the relay sends:

- stable delivery UUID;
- stable payload SHA-256;
- stable idempotency key;
- authenticated adapter credential;
- bounded event payload.

If the adapter returns `projected_reply`, the relay creates a deterministic `matrix-homeserver-v1` delivery and completes the adapter delivery in the same PostgreSQL transaction. Failure of either action rolls back both.

### 6.2 Matrix destination

For `matrix-homeserver-v1`, the delivery UUID is the Matrix transaction ID. A timeout, response loss, process crash or expired claim therefore retries the same Matrix send identity. The runtime must never generate a new transaction ID for a retry.

### 6.3 Outcome classification

- HTTP 2xx with a valid bounded response permits transport completion.
- 408, 425, 429 and 5xx are retryable.
- other 4xx responses are permanent unless a later protocol version explicitly classifies them otherwise.
- transport errors and bounded-response failures are retryable until the attempt budget is exhausted.
- unknown destination, invalid stored payload or identity mismatch is poison/permanent failure.

A stale owner or stale fence cannot finish a claim. When a process dies after a possible remote effect, lease expiry makes the same immutable delivery eligible for recovery.

## 7. Authentication and secret separation

The direct relay compatibility route requires `x-relay-token`. Poller-to-database, relay ingress, relay-to-adapter and relay-to-Matrix credentials are separate authorities. In production-like profiles the relay ingress, adapter and Matrix tokens must be pairwise distinct.

Production-like HTTP endpoints require HTTPS. Redirects are disabled. Tokens, database URLs, message bodies, raw cursors and unrestricted payloads are forbidden in logs, metrics and errors.

## 8. Bounded configuration

The implementation recognizes the following operational classes:

- identity: partition ID, poller worker ID, relay worker ID;
- timing: Matrix sync timeout, cursor lease, relay HTTP timeout, relay claim lease, poll interval;
- capacity: sync maximum bytes, relay maximum attempts and fixed claim batch size;
- trust: Matrix homeserver URL/token, adapter URL/token, relay ingress token and PostgreSQL URL;
- posture: `CEX_RUNTIME_PROFILE` or reviewed compatibility alias.

Cursor lease must exceed sync timeout by at least five seconds. Relay lease must exceed HTTP timeout by at least five seconds. Production-like startup fails before polling, listening or claiming when required durable state, explicit identities, HTTPS endpoints or credential separation are absent.

## 9. Recovery matrix

| Failure point | Required result |
|---|---|
| before inbox insert | no cursor advancement; event is observed again |
| after inbox insert but before outbox enqueue | transaction rollback; event is observed again |
| after outbox enqueue but before cursor CAS | transaction rollback; event is observed again |
| after commit but before poller log/ack | persisted cursor prevents old-batch re-fetch; exact identities remain |
| adapter accepted, response lost | same delivery ID and payload hash are retried/reconciled |
| reply enqueued, adapter completion fails | both roll back |
| Matrix accepted, response lost | same Matrix transaction ID is retried |
| worker dies while claimed | expiry exposes same delivery with a higher fence |
| stale worker finishes | rejected by owner/fence/expiry check |
| malformed supported event | poison row; cursor held |
| attempt budget exhausted | delivery enters immutable-history-backed dead letter |

## 10. Verification and evidence

Repository checks:

```text
cargo fmt --all -- --check
cargo check --locked -p matrix-bot-poller -p matrix-bot-relay --all-targets
cargo test --locked -p matrix-bot-poller -p matrix-bot-relay --all-targets
cargo clippy --locked -p matrix-bot-poller -p matrix-bot-relay --all-targets -- -D warnings
python3 scripts/check-matrix-runtime-wiring.py
python3 scripts/check-matrix-transport-durability.py
python3 scripts/test-matrix-transport-durability.py
bash scripts/check-matrix-transport-postgres.sh
```

Qualification additionally requires exact-candidate hosted evidence for:

- PostgreSQL procedure execution and trigger enforcement;
- concurrent claim and stale-fence rejection;
- process-kill recovery at every matrix in section 9;
- real Matrix transaction-ID response-loss behavior;
- adapter exact replay/collision behavior;
- sustained backlog/load and poison/dead-letter operations;
- credential custody, alert routing and rollback rehearsal.

Static source markers, successful formatting, an unexecuted workflow, a locally fabricated report or a different SHA do not qualify the candidate.

## 11. Deployment and rollback

Apply the migration through an approved schema owner, revoke owner access from resident processes, then start adapter, relay and poller under separate least-privilege credentials. Readiness must expose database/schema validity separately from external Matrix or adapter reachability.

Rollback stops new poller lease acquisition and relay claims, waits for or fences active claims, preserves cursor/inbox/outbox/history/poison evidence, and deploys only a schema/protocol-compatible binary. Operators must never delete evidence, reset cursors, lower fences or mark unknown outcomes successful to clear an alert.

## 12. Compatibility and change protocol

Opaque cursors, source hashes, destination names, payload hashes, delivery UUIDs and Matrix transaction IDs are protocol identities. Any change requires an explicit version, golden replay and collision fixtures, expand/backfill/verify/cutover/contract migration steps, consumer inventory, rollback evidence and a new exact-tree candidate.

No document, script, repository commit or module owner may grant production authorization. Final activation remains an independent human and operational authority.

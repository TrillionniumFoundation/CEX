# Matrix Durable Result Lookup and Reconciliation v1

Status: source implemented; runtime qualification pending  
Candidate branch: `fix/cex-v12-audit-remediation-20260905`  
Production authorization: **not granted**

## 1. Purpose

This contract closes the repository-side design gap where a Matrix relay can lose
an adapter response after Consumer Entry has already accepted a Matrix operation.
A timeout, interrupted body, invalid response, or duplicate marker must never be
interpreted as proof of success and must never trigger an unbounded blind replay.

The recovery path is a read-only, principal-bound lookup of the exact durable
Consumer Entry replay record. It does not call the Gateway, create an invocation,
advance a Matrix transport cursor, reopen a dead letter, or send a Matrix event.
Those later state transitions require a separately authorized operator action.

## 2. Components

### 2.1 Consumer Entry lookup

Production binary wiring:

- source: `services/consumer-entry-api/src/matrix_result_lookup.rs`
- binary integration: `services/consumer-entry-api/src/main.rs`
- endpoint: `POST /v1/matrix/messages/result`
- durable source: the configured `CONSUMER_ENTRY_REPLAY_STORE_PATH`
- exact cache key: `matrix-event:<event_id>`

Request body:

```json
{
  "event_id": "$matrix-event-id",
  "matrix_user_id": "@principal:homeserver",
  "room_id": "!room:homeserver"
}
```

The request rejects unknown JSON fields and enforces bounded Matrix identifiers.
It requires both the Consumer Entry ingress token and a signed user-session
assertion. The assertion is bound to all of:

- `source_kind = matrix_result_lookup`;
- the exact Matrix user as `subject`;
- the exact room;
- the configured audience and allowed issuer;
- the selected issuer/key identity;
- issued-at, expiry, skew, and maximum TTL;
- a SHA-256 fingerprint of schema, event ID, Matrix user, and room.

The HMAC is verified over the base64url assertion bytes. A missing key, unknown
issuer, stale assertion, mismatched fingerprint, or invalid signature fails
closed without revealing key-selection details.

### 2.2 Adapter reconciliation surface

Production facade wiring:

- source: `services/matrix-entry-adapter/src/result_reconciliation.rs`
- facade integration: `services/matrix-entry-adapter/src/lib.rs`
- endpoint: `POST /v1/matrix/results/lookup`

Request body:

```json
{
  "event_id": "$matrix-event-id",
  "room_id": "!room:homeserver",
  "sender": "@principal:homeserver"
}
```

The adapter endpoint requires its own `x-entry-token`. It uses the same resolved
Consumer Entry session-auth issuer, key ID, secret, audience, and TTL selected by
the validated adapter configuration. It then calls only the Consumer Entry
lookup endpoint. It does **not** call `/v1/matrix/messages` or any invocation
creation endpoint.

A successful response uses the existing adapter envelope with:

- `accepted = true`;
- `action = task_result_reconciled`;
- exact event, room, and sender identity;
- the original cached `ConsumerTaskResponse` as `forwarded`;
- `projected_reply = null`;
- `reconciliation.read_only = true`.

The adapter independently verifies the Consumer Entry lookup schema, resolved
disposition, outer identities, cached source kind, cached event/user/room, and a
non-empty task ID. Any mismatch is held as an error envelope.

## 3. Durable replay evidence rules

Consumer Entry returns a result only when all of the following are true:

1. the configured replay store is a bounded regular non-symlink file;
2. the file parses as the replay-store schema;
3. the exact `matrix-event:<event_id>` entry exists;
4. the entry remains inside `replay_window_secs` and is not implausibly future;
5. the entry contains a recorded response, not only a seen marker;
6. the cached response source is `matrix_message`;
7. cached Matrix user, room, event, and identity scope match the caller;
8. the cached task ID is non-empty and agrees with any raw invocation ID.

A concurrent non-atomic replay-store write can produce a transient parse failure.
The lookup performs one bounded retry and then returns service unavailable. It
never falls back to reconstructing or resubmitting the business request.

## 4. Failure semantics

| Condition | Consumer Entry | Adapter |
|---|---|---|
| unknown/missing result | `404` | unresolved/held |
| expired replay evidence | `410` | unresolved/held |
| marker without response | `409` | unresolved/held |
| identity conflict | `409` | identity-mismatch hold |
| auth failure | `401` | auth/consumer-auth hold |
| replay store unavailable | `503` | consumer-unavailable hold |
| oversized/interrupted/non-JSON response | n/a | response-unverified hold |

No failure path returns `accepted=true`. No error includes secrets, replay bytes,
Gateway prompts, or internal key-selection diagnostics.

## 5. Relationship to relay delivery state

This v1 lookup resolves the evidence question: whether Consumer Entry durably
recorded the exact operation result for the original Matrix principal and event.
It deliberately does not mutate `matrix_transport_outbox` or emit a reply.

A relay delivery held as `adapter_response_unknown_*` remains held until a
separate database-authorized reconciliation transition records the lookup
evidence and explicitly decides whether to mark the adapter leg complete and
whether a bounded Matrix reply should be enqueued. Direct table updates, blanket
runtime DML, and automatic dead-letter replay are prohibited.

Therefore:

- lookup success is not by itself a transport-state transition;
- lookup failure is not proof that no effect occurred outside the replay window;
- a generated response or local fixture is not production evidence;
- production release remains blocked until the database transition, role grants,
  PostgreSQL regression, and real response-loss rehearsal are independently
  qualified.

## 6. Source and regression gates

Repository source gate:

```bash
python3 scripts/check-matrix-result-reconciliation.py
```

Hosted candidate workflow additionally runs:

```bash
cargo fmt -p matrix-entry-adapter -p consumer-entry-api -- --check
cargo test --locked -p matrix-entry-adapter --all-targets
cargo test --locked -p consumer-entry-api --all-targets
cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings
cargo clippy --locked -p consumer-entry-api --all-targets -- -D warnings
```

The base-owned trusted gate runs the complete locked workspace and Matrix
PostgreSQL chain inside its isolated container when a `rog` runner is actually
allocated. A queued job, empty job, skipped step, generated JSON, or source-only
checker is not execution evidence.

## 7. Required qualification before release

The implementation remains non-authoritative until all of these are satisfied
on one unchanged candidate SHA/tree:

1. Consumer Entry and adapter format, unit tests, and strict Clippy pass.
2. A disposable PostgreSQL 16 run qualifies the complete Matrix migration chain.
3. A response-loss rehearsal proves lookup success after a lost adapter response.
4. Negative rehearsals prove wrong user, room, event, key, issuer, audience,
   fingerprint, stale assertion, corrupt store, and missing response all fail.
5. An explicit least-privilege database reconciliation function and role grant is
   installed and tested; no direct table DML is used.
6. Relay/operator recovery records immutable evidence before changing delivery
   state and cannot enqueue an unbound cross-room reply.
7. Required hosted workflows, protected-branch policy, independent review, and
   V12 external authorities bind the same final candidate.

Until those conditions hold, repository and production status remain:

```text
all_plan_gaps_closed=false
production_authorization=not_granted
```

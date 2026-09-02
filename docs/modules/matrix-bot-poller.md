# matrix-bot-poller module contract

Status: active module contract  
Workspace member: `apps/matrix-bot-poller`  
Package: `matrix-bot-poller`  
Kind: `application`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Polls an authenticated Matrix `/sync` stream, preserves opaque cursor/event identity and forwards only supported messages through the Matrix adapter/consumer-entry boundary.

**Non-goals.** It is not an event store, Matrix identity authority, scheduler for CEX jobs, user/room governance service or substitute for homeserver ordering, edit and redaction semantics.

## Authority and owned state

The poller owns only transport-local cursor and delivery observations when they are explicitly persisted. It owns no consumer, research, Agent, financial, World/Game or finality state. Matrix remains authoritative for source events and sync tokens; downstream services remain authoritative for accepted actions.

Without durable cursor state, the process is supporting Alpha and downstream idempotency is the final duplicate barrier. A process-local cache, successful HTTP response or advanced in-memory token cannot become durable acceptance evidence.

## Source layout and entry points

- `src/main.rs`: polling loop, configuration, Matrix client, event filtering, downstream client and current test surface.

Catalog-bound entry points:

- `apps/matrix-bot-poller/src/main.rs`

Any new worker, partitioning strategy, storage owner or public command grammar must update the module catalog and this contract. The single-file executable should be split into configuration, Matrix client, cursor repository, delivery worker and telemetry modules before production promotion.

## Interfaces and contracts

The executable consumes Matrix sync responses and invokes the versioned downstream adapter/consumer-entry HTTP contract. It preserves the opaque `next_batch` token, source event ID, room/sender relationship, edit/redaction linkage and scoped idempotency identity.

Only an explicit command allowlist is forwarded. Unknown event versions, malformed content, ambiguous edits/redactions or unsupported media fail closed or enter bounded dead-letter handling; they never map to a privileged default action.

## Persistence, concurrency, and recovery

Cursor advancement occurs only after durable downstream acceptance of every event covered by the prior cursor. On timeout or response loss after possible acceptance, the event is retried with the same identity and the cursor remains fenced until exact replay or reconciliation resolves the outcome.

Production promotion requires durable cursor/deduplication storage with singleton lease or partition fencing, compare-and-set cursor advancement, poison-event isolation, restart recovery and retained transition evidence. Multiple instances cannot advance one cursor concurrently without a monotonically fenced lease.

## Configuration and secrets

Configuration includes Matrix homeserver/base URL, access token, user/device/room scope, poll interval and long-poll timeout, downstream URL/principal, body/media limits, cursor partition/store, worker identity, lease/retry/dead-letter limits and runtime profile.

Production-like startup must fail before polling when required durable storage, credentials, explicit partition ownership, trusted endpoints or bounded limits are absent. Matrix access/sync tokens and downstream credentials must come from approved custody and never be logged or embedded in committed examples.

## Security and trust boundaries

Scope the Matrix token to the minimum account/device/rooms; reject redirects and untrusted homeserver identity; bound decompressed event/media sizes; validate event type and sender/room linkage; redact message bodies, access tokens, sync tokens and personal identifiers from telemetry.

A Matrix sender or room field is untrusted until verified by the homeserver contract and mapped by the downstream identity authority. Metrics use aggregate partition/outcome labels rather than raw event, room, sender or tenant identifiers.

## Verification

Required commands:

```text
cargo test -p matrix-bot-poller
cargo clippy -p matrix-bot-poller --all-targets -- -D warnings
```

Required behavioral focus:

- Cursor persistence and advancement ordering, exact duplicate replay and restart.
- Matrix rate limits, server errors, response loss, downstream timeout and poison-event isolation.
- Edit/redaction handling and command allowlist fail-closed behavior.
- Singleton lease or multi-instance fencing before any horizontally scaled deployment.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Run as a singleton per cursor partition until a durable fenced cursor repository is qualified. Readiness must be false unless Matrix authentication, cursor ownership/store and downstream trust are valid; liveness reports only process health. Monitor sync lag, oldest unresolved event, repeated failure count, rate-limit duration, cursor lease expiry and dead-letter age.

Rollback stops polling and releases/fences cursor ownership before switching binaries. Preserve the exact last committed cursor, unresolved-event identity and delivery evidence; never advance or reset a cursor to make a deployment appear healthy. Operators record artifact/config identities, readiness dimensions, rollback boundary and escalation owner.

## Compatibility and change protocol

Changes to command filtering, event translation, edit/redaction handling, cursor representation or partition strategy require an explicit protocol version, replay fixtures and migration/rollback evidence. A new Matrix API version must run dual-read/shadow comparison before cutover.

Changes to authority, interfaces, persistence, configuration, retry semantics or deployment topology require this contract, the module catalog, Matrix architecture/threat model, executable tests, hosted gate wiring and a new shared candidate trigger. No module document may declare repository closure or production authorization.

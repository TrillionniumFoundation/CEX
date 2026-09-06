# matrix-bot-poller module contract

Status: active module documentation  
Module path: `apps/matrix-bot-poller`  
Lifecycle: `supporting_alpha`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Poll Matrix /sync, persist the next_batch cursor, filter supported text events and forward them to matrix-bot-relay.

## Non-goals

- Full Matrix appservice operation.
- Owning business commands or reply projection.
- Treating a local cursor file as a highly available production queue.

## Authority and state ownership

The poller owns its local sync cursor and recent-event dedupe window only. Matrix is authoritative for room events and the relay/adapter chain owns delivery processing.

## Interfaces

- Matrix Client-Server /sync.
- matrix-bot-relay /v1/inbound/matrix-event.
- Local startup and health/logging behavior.

## Data and persistence

- Persisted next_batch cursor and bounded recent-event IDs.
- Production promotion requires encrypted durable state, exclusive consumer ownership and tested failover.

## Security and configuration

- Protect Matrix access tokens and state files.
- Validate homeserver TLS and reject unbounded event bodies.
- Ignore self-sent events using configured bot identity.

## Failure and recovery

- Cursor is advanced only after accepted forwarding policy.
- Restart resumes from persisted next_batch and dedupe window.
- Repeated malformed or poison events require bounded handling and operator evidence.

## Observability

- Sync latency/failures, cursor age, events inspected/forwarded/ignored/duplicated and relay response class.

## Verification

- `cargo test --locked -p matrix-bot-poller`
- `cargo clippy --locked -p matrix-bot-poller --all-targets -- -D warnings`
- `scripts/check-matrix-bot-entry-v1.sh`
- `docs/matrix-bot-poller-v1.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Bot polling remains supporting Alpha. Migration to appservice mode must preserve event identity, replay semantics and a rollback path.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

# matrix-bot-relay module contract

Status: active module documentation  
Module path: `apps/matrix-bot-relay`  
Lifecycle: `supporting_alpha`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Receive Matrix events, call the Matrix entry adapter and deliver projected replies with bounded retry/queue behavior.

## Non-goals

- Owning task or domain state.
- Serving as production-grade distributed queue without qualification.
- Interpreting business commands beyond transport metadata.

## Authority and state ownership

The relay owns transport-attempt and queue state only. Matrix homeserver owns room events; the adapter/consumer services own projected application semantics.

## Interfaces

- GET /health.
- POST /v1/inbound/matrix-event.
- Matrix send API.
- matrix-entry-adapter /v1/matrix/events.

## Data and persistence

- Recent-event dedupe and persistent send queue files in the current first slice.
- Production promotion requires durable shared queue/lease semantics and multi-instance fencing.

## Security and configuration

- Do not log Matrix access tokens.
- Authenticate calls to the adapter.
- Filter self events and bind transaction IDs to stable event identity.

## Failure and recovery

- Inline retry is bounded; retryable send failures enter the persistent queue.
- Queue corruption, overflow and poison messages require explicit dead-letter evidence rather than silent drop.

## Observability

- Inbound/self/duplicate counts, adapter failures, send attempts/outcomes, queue depth/age/requeue/drop and congestion signals.

## Verification

- `cargo test --locked -p matrix-bot-relay`
- `cargo clippy --locked -p matrix-bot-relay --all-targets -- -D warnings`
- `scripts/check-matrix-bot-entry-v1.sh`
- `docs/matrix-bot-relay-v1.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

The bot-mode relay is a first slice. Appservice migration requires explicit namespace, transaction and replay compatibility; queue format changes require migration or drain.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

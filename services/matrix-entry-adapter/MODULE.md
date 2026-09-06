# matrix-entry-adapter module contract

Status: active module documentation  
Module path: `services/matrix-entry-adapter`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Translate authenticated Matrix-shaped events and bounded mobile commands into consumer-entry requests and Matrix-safe response projections.

## Non-goals

- Owning World, League, Ledger or research state.
- Acting as a full Matrix homeserver client.
- Authorizing users from raw Matrix sender text.
- Accumulating new domain logic.

## Authority and state ownership

The adapter owns channel parsing, deduplication, quotas and signed downstream session assertions. Consumer/Hepta/domain services own task and business state.

## Interfaces

- GET /health and /metrics.
- POST /v1/matrix/events and projection/admin routes.
- Signed calls to consumer-entry-api.
- Matrix-safe m.text/formatted_body/cex_card projection.

## Data and persistence

- Durable recent-event and rate-limit stores are required for production-like profiles.
- No authoritative World or financial state is stored locally.

## Security and configuration

- Ingress token, signed downstream assertion, approved issuer-registry revision, audience/method/path/body binding, payload-size limits and self-event filtering.

## Failure and recovery

- Duplicate Matrix events are replay-safe across restart.
- Registry reload validates candidate revision and approval coverage before activation.
- Upstream failure returns a bounded error or retryable channel response without local domain mutation.

## Observability

- Profile validation, ingress/session auth, registry governance, dedupe, quotas, upstream requests/failures and projection outcomes.

## Verification

- `cargo test --locked -p matrix-entry-adapter`
- `cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings`
- `scripts/check-matrix-bot-entry-v1.sh`
- `docs/matrix-entry-adapter-v1.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

The existing broad World command set is frozen compatibility surface. New commands must be channel-neutral and owned by a versioned downstream domain API.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

# consumer-entry-api module contract

Status: active module documentation  
Module path: `services/consumer-entry-api`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Consumer-facing authentication, task submission and read-model projection into Hepta/CEX backend capabilities.

## Non-goals

- Owning World gameplay, map, commerce or authoritative match state.
- Becoming the source of Ledger, Identity, Hepta research or TRNM finality facts.
- Embedding a permanent frontend monolith inside a control-plane service.

## Authority and state ownership

The service owns edge sessions, replay/rate-limit state and consumer-specific projections only. Hepta, Identity, Ledger, Nakama/World and TRNM remain authoritative for their domains.

## Interfaces

- Consumer and Matrix-facing HTTP routes.
- Signed session assertions and ingress-token protected administration.
- Read/write adapters to Gateway and bounded external domain APIs.

## Data and persistence

- Production-like replay, rate-limit and projection stores must be durable and explicitly configured.
- Domain projections record source version/revision and cannot become an alternate writer.

## Security and configuration

- Validate signed session audience, method/path/body hash, expiry, nonce and approved issuer revision.
- Rate limit by bounded subject/tenant dimensions.
- Never trust forwarded identity fields without verified assertion.

## Failure and recovery

- Replay stores and idempotency survive restart.
- Unavailable authoritative domains return degraded/held projections rather than local fabricated state.
- World compatibility routes are frozen pending extraction.

## Observability

- Ingress auth failures, replay/collision, rate limits, upstream class, projection staleness, source revision and queue age.

## Verification

- `cargo test --locked -p consumer-entry-api`
- `cargo clippy --locked -p consumer-entry-api --all-targets -- -D warnings`
- `docs/consumer-entry-matrix-architecture-v1.md`
- `PROJECT_BOUNDARY.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

World and legacy product surfaces are compatibility-only and may not expand. New domain behavior must live behind a versioned external adapter owned by the correct repository.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

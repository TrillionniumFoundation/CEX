# paper-raid-bff module contract

Status: active module documentation  
Module path: `services/paper-raid-bff`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Narrow browser/mobile BFF for the Paper Raid alpha, including session mapping, signed receipts, consumer projections and first-playable browser flows.

## Non-goals

- Owning research truth.
- Mutating Nakama or finality state outside versioned commands.
- Treating UI completion as paper or economic finality.

## Authority and state ownership

The BFF owns edge/session and presentation state only. Hepta owns research facts, Nakama owns match state and verified TRNM receipts own finality.

## Interfaces

- Browser/mobile HTTP endpoints.
- Hepta Paper Raid v2 contract boundary.
- Browser E2E and signed receipt retrieval.
- Container readiness probes.

## Data and persistence

- BFF-specific PostgreSQL/session state and immutable receipt/cache records where documented.
- No unrestricted research body or provider secret in logs.

## Security and configuration

- Audience-separated signed sessions and receipts.
- Fixed-alpha allowlist or database-backed access control with explicit profile validation.
- Pinned downstream authorities and body-hash binding.

## Failure and recovery

- PostgreSQL restart and exact replay preserve session/receipt identity.
- Downstream unavailability yields bounded unavailable/pending state, not fabricated completion.

## Observability

- Session/auth failures, downstream latency, receipt verification, browser journey outcomes, database readiness and queue/cache state.

## Verification

- `cargo test --locked -p paper-raid-bff`
- `cargo clippy --locked -p paper-raid-bff --all-targets -- -D warnings`
- `services/paper-raid-bff/scripts/check-postgres.sh`
- `services/paper-raid-bff/scripts/check-browser-e2e.sh`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Consumer projections may add optional fields. Authoritative command, receipt or signature changes require a new explicit protocol version and E2E fixtures.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

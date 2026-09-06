# hepta-research-league module contract

Status: active module documentation  
Module path: `services/hepta-research-league`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Research challenge, team/Agent registration, paper workflow, evaluation/reproduction, Nakama control integration and finality projection.

## Non-goals

- Running external Agents.
- Owning Nakama live match state.
- Creating chain finality from an HTTP success.
- Replacing legal/scientific human judgment.

## Authority and state ownership

Hepta is authoritative for research rules, assets, submissions, review and evaluation facts. Nakama owns live match facts; independently verified TRNM receipts own finality.

## Interfaces

- Versioned Hepta and Paper Raid HTTP/OpenAPI contracts.
- Signed Nakama control/completion boundaries.
- Typed finality verifier boundary.
- Migration and readiness commands.

## Data and persistence

- PostgreSQL research facts, outbox/inbox, leases, immutable key snapshots and finality projections.
- Content-addressed research assets remain in governed object storage.

## Security and configuration

- Proof of possession and key-epoch rotation.
- Pinned Nakama/finality authorities, canonical frames and tamper rejection.
- Separate migrator, runtime and finality roles.

## Failure and recovery

- Strict PostgreSQL recovery, concurrent claim fencing, wrong-owner acknowledgement rejection and expired-lease reclaim.
- Chain outage remains pending_finality; content loss places records on hold.

## Observability

- Outbox/control age, claim attempts, backend identity, finality age/result, review state and readiness security checks.

## Verification

- `cargo test --locked -p hepta-research-league`
- `cargo clippy --locked -p hepta-research-league --all-targets -- -D warnings`
- `scripts/check-hepta-postgres-integration.sh --mode full`
- `docs/openapi/hepta-research-league-v1.yaml`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Research command versions are explicit. Compatibility projections name their source version and never collapse consent, authorship, review or finality states.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

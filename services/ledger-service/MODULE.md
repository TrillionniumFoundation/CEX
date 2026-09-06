# ledger-service module contract

Status: active module documentation  
Module path: `services/ledger-service`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Exact account opening, append-only monetary effects, reservation, consume/refund settlement, operation identity, receipt lookup and projection parity.

## Non-goals

- Inferring historical precision.
- Treating float/major-unit compatibility fields as authoritative writes.
- Calling remote systems inside a database transaction.
- Granting TRNM chain finality.

## Authority and state ownership

Ledger v2 functions and append-only effects are authoritative for CEX exact local monetary state. TRNM/finality remains authoritative for independently finalized chain receipts.

## Interfaces

- Exact /v2 account and effect HTTP APIs.
- Durable reserve/settlement commands and authenticated receipt lookup.
- Retired v1 value-writing routes return 410 Gone.

## Data and persistence

- PostgreSQL is required in production-like profiles.
- Every effect carries currency unit, scale, signed minor units, operation identity, trace, reference and scoped idempotency.
- Legacy in-memory/f64 structures are compatibility/test-only and must not enter production authority.

## Security and configuration

- Scoped admin/service principals, explicit trace requirements and append-only database controls.
- Reject dual exact/legacy monetary intent and receipt/content mismatch.

## Failure and recovery

- Identical replay returns the original effect; collisions fail.
- Response loss is resolved by exact receipt lookup, never by transport inference.
- Database recovery must compare balances, identities and content hashes.

## Observability

- Projection parity, replay/collision, insufficient funds, operation latency, lock contention and receipt lookup outcomes.

## Verification

- `cargo test --locked -p ledger-service`
- `cargo clippy --locked -p ledger-service --all-targets -- -D warnings`
- `scripts/check-p0-migrations-postgres.sh`
- `scripts/check-p0-wiring.py`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Read compatibility does not grant write authority. New value writes use exact minor units only; removal of legacy code requires zero caller telemetry and an explicit retirement decision.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

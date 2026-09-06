# audit-service module contract

Status: active module documentation  
Module path: `services/audit-service`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Authenticated append-only Audit v2 events, source-baseline backfill, durable outbox delivery and operator/audit query surfaces.

## Non-goals

- Replacing domain databases as truth.
- Logging secrets or unrestricted research content.
- Deleting or mutating committed evidence to repair delivery.

## Authority and state ownership

Audit service is authoritative for its append-only audit event and delivery intent records. Source services remain authoritative for the domain facts being referenced.

## Interfaces

- Authenticated audit append/read HTTP routes.
- Outbox dispatcher.
- Bounded source-baseline and operational recovery procedures.

## Data and persistence

- PostgreSQL audit events, source revisions, checkpoints and outbox claims.
- Source revision advancement and audit-intent creation are atomic.

## Security and configuration

- Registered writer identities, scoped readers and append-only role controls.
- Classify fields for PII, confidential research and credential exclusion.
- Export access must be separately authorized and audited.

## Failure and recovery

- Bounded retry/dead-letter with ownership fencing.
- Baseline restart is a no-op after completion and never duplicates intents.
- Backlog pressure blocks baseline expansion rather than dropping evidence.

## Observability

- Outbox depth, oldest age, attempts, delivery outcomes, source-baseline progress, pressure blocks and append authentication failures.

## Verification

- `cargo test --locked -p audit-service`
- `cargo clippy --locked -p audit-service --all-targets -- -D warnings`
- `scripts/check-audit-source-baseline-postgres.sh`
- `docs/audit-source-baseline-backfill-v1.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Historical source rows may be bounded-backfilled. New writes use authenticated Audit v2; cursor and schema changes require executable upgrade fixtures.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

# identity-service module contract

Status: active module documentation  
Module path: `services/identity-service`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Organization, actor and API-key identity resolution, scoped administration and durable identity provenance for downstream services.

## Non-goals

- Owning Ledger balances.
- Executing Agents.
- Accepting client-supplied org_id or actor_id as trusted provenance.
- Using static development identities in production-like profiles.

## Authority and state ownership

Identity service is authoritative for CEX organization/actor/API-key bindings. Agent research key epochs are governed by the Hepta Agent registry contract when that boundary applies.

## Interfaces

- Health and metrics endpoints.
- Authenticated identity-resolution and management HTTP routes.
- Internal service-authenticated calls from Gateway and approved operators.

## Data and persistence

- PostgreSQL-backed identity and API-key records in production-like profiles.
- Static identity data is local/test compatibility only and is disabled by runtime guard in production-like profiles.

## Security and configuration

- Fail closed on missing runtime profile, database preflight or admin/service credentials.
- Store only hashed API-key material where supported; audit issuance, rotation, revocation and scope changes.

## Failure and recovery

- Database loss or unavailable identity authority makes protected resolution unavailable; do not fall back to static identities.
- Rotation preserves immutable actor/org provenance and explicit revocation evidence.

## Observability

- Resolution latency/outcome, authentication failures, key revision, database readiness and audit delivery must be measurable with bounded labels.

## Verification

- `cargo test --locked -p identity-service`
- `cargo clippy --locked -p identity-service --all-targets -- -D warnings`
- `services/identity-service/tests/runtime_blackbox.rs`
- `docs/internal-service-auth-v1.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Legacy identity profiles may remain readable only under an explicit compatibility mode. New protected writes and resolution use the current authenticated contract.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

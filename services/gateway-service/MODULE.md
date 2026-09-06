# gateway-service module contract

Status: active module documentation  
Module path: `services/gateway-service`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Authenticated request normalization, identity provenance, capability lookup, invocation creation and durable exact Ledger reserve orchestration.

## Non-goals

- Owning Ledger balances.
- Executing provider calls inside request transactions.
- Embedding complex World/gameplay rules.
- Trusting client-supplied tenant provenance.

## Authority and state ownership

Gateway owns invocation ingress and reserve-command initiation. Identity owns actor/tenant bindings; Ledger owns monetary effects; Execution owns execution lifecycle.

## Interfaces

- Public invocation HTTP routes.
- Internal authenticated calls to Identity, Capability, Ledger, Execution and Audit.
- Durable reserve command/receipt lifecycle.

## Data and persistence

- Invocation and reserve command state is PostgreSQL-backed for production-like profiles.
- One immutable invocation Ledger contract binds reserve and terminal settlement identities.

## Security and configuration

- Runtime profile and database fail-fast.
- Authenticated service calls, scoped operator actions, rate limiting and strict receipt validation.
- Reject dual exact/legacy monetary intent.

## Failure and recovery

- Claim commits before Ledger I/O; outcome commits separately.
- Lease expiry, retry exhaustion, acknowledgement and requeue preserve operation identity.
- Ambiguous side effects never become invented success.

## Observability

- Reserve command age, attempt/lease state, upstream class, receipt validation, collisions, tenant-scoped rate limits and trace propagation.

## Verification

- `cargo test --locked -p gateway-service`
- `cargo clippy --locked -p gateway-service --all-targets -- -D warnings`
- `scripts/check-gateway-exact-reserve-postgres.sh`
- `.github/workflows/p0-gateway-exact-reserve-gate.yml`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

New value-bearing ingress uses exact v2 contracts. Legacy routes are read-only/retired and cannot authorize new monetary writes.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

# trnm-economy-service module contract

Status: active module documentation  
Module path: `services/trnm-economy-service`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Adapter and receipt-recovery boundary between CEX/Hepta settlement intent and the versioned TRNM economy protocol.

## Non-goals

- Owning chain consensus or validator state.
- Replacing Ledger local exact operation identity.
- Retrying an ambiguous external side effect without reconciliation.

## Authority and state ownership

The service owns adapter-local settlement intent and receipt records. Ledger owns exact local monetary effects; TRNM/finality owns independently verified chain finality.

## Interfaces

- Settlement HTTP API and readiness.
- Pinned trnm-economy-protocol v2 boundary.
- Receipt lookup/recovery operations and service-owned migration bootstrap.

## Data and persistence

- PostgreSQL settlement receipt store plus exact operation identifiers.
- Protocol whole-credit inputs convert only through checked scale multiplication; fractional or overflowing conversions fail closed.

## Security and configuration

- Authenticate callers and verify returned receipts against operation/content identity.
- Do not accept response-supplied trust anchors or development credentials in production.

## Failure and recovery

- Transport loss triggers exact receipt lookup/reconciliation.
- Replay is terminal-safe and idempotent; indeterminate outcomes cannot authorize a second side effect.

## Observability

- Pending intent age, receipt lookup latency, reconciliation inventory, retry/dead-letter counts and protocol-version failures.

## Verification

- `cargo test --locked -p trnm-economy-service`
- `cargo clippy --locked -p trnm-economy-service --all-targets -- -D warnings`
- `scripts/check-trnm-economy-settlement-contract.py`
- `docs/protocol/trnm-settlement-receipt-recovery-v1.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

The vendored TRNM protocol is semver pinned. Service-local and repository migration ownership must be explicit; protocol upgrades require golden tests and rollback/forward-fix guidance.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

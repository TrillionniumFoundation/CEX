# execution-service module contract

Status: active module documentation  
Module path: `services/execution-service`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Invocation execution state, provider dispatch commands, terminal consume/refund settlement, retry budgets, reconciliation and operator recovery.

## Non-goals

- Running platform-owned competitor Agents.
- Holding a SQL transaction open during provider or Ledger I/O.
- Automatically retrying after a possibly executed provider call.

## Authority and state ownership

Execution owns invocation execution transitions and durable dispatch/settlement command state. Providers own their external side effects; Ledger owns exact monetary effects.

## Interfaces

- Authenticated execution creation/read/admin HTTP routes.
- Durable provider dispatch and Ledger settlement workers.
- Operator acknowledgement/requeue surfaces.

## Data and persistence

- PostgreSQL command, claim, transition and reconciliation evidence.
- Stable operation identity links Invocation, provider, Ledger and Audit evidence.

## Security and configuration

- Service authentication and scoped admin principals.
- Immutable provider evidence URI/SHA-256 and strict claim ownership.
- No mutable or response-supplied evidence may authorize requeue.

## Failure and recovery

- Claim transaction commits before network I/O.
- Timeout after possible dispatch enters reconcile_required.
- Confirmed-not-executed evidence plus acknowledgement is required before requeue.
- Consume/refund commands support exact replay and dead-letter recovery.

## Observability

- Command age, attempts, lease expiry, unknown outcomes, reconciliation age, settlement outcome and worker ownership.

## Verification

- `cargo test --locked -p execution-service`
- `cargo clippy --locked -p execution-service --all-targets -- -D warnings`
- `scripts/check-execution-settlement-commands-postgres.sh`
- `.github/workflows/p0-execution-settlement-gate.yml`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Legacy diagnostics may be read, but new dispatch and terminal settlement writes use durable exact-authority commands only.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

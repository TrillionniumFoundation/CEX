# ledger-service module contract

Status: active module contract  
Workspace member: `services/ledger-service`  
Package: `ledger-service`  
Kind: `service`  
Logical module: `trnm`  
Deployable: yes  
Owner role: `ledger-authority`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Owns exact CEX Ledger state and the canonical receipt surface consumed by Gateway, Execution, and the TRNM economy boundary.

**Non-goals.** It does not infer historical precision, own Chain finality, execute providers, or treat transport success as a financial receipt.

## Authority and owned state

Exact minor-unit account/effect authority, immutable operation identity, reserve/consume/refund receipts, and intent-hash-bound receipt recovery.

Owned state: Accounts, append-only effects, reservations, terminal consume/refund evidence, operation/idempotency identity, projection parity, and immutable receipt-event history.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `api.rs`: HTTP surface and receipt validation.
- `account_control.rs`: opening/control rules.
- `ledger_effects.rs`: exact effect lifecycle.
- `receipt_lookup.rs`: intent-hash-bound response-loss recovery.
- `repository/postgres.rs`: durable Ledger persistence.
- `repository/native_economy.rs`: TRNM native economy adapter.
- `exact_memory.rs`: bounded development/test implementation, never production authority.

Catalog-bound entry points:

- `services/ledger-service/src/main.rs`
- `services/ledger-service/src/api.rs`
- `services/ledger-service/src/account_control.rs`
- `services/ledger-service/src/ledger_effects.rs`
- `services/ledger-service/src/receipt_lookup.rs`
- `services/ledger-service/src/repository/mod.rs`
- `services/ledger-service/src/repository/postgres.rs`
- `services/ledger-service/src/repository/native_economy.rs`
- `services/ledger-service/tests/http_flow.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Canonical `/v2/accounts` and `/v2/ledger/effects` value routes plus authenticated intent-bound receipt lookup. Retired v1 value-writing routes must remain fail-closed.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

PostgreSQL is the production-like authority. Exact operations use stable IDs and content fingerprints; identical replay returns the original result and different content under the same identity collides.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Database URL/role, runtime profile, administrator/service credential registries, request/body bounds, and exact-mode switches are validated before serving.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

All value uses checked signed minor units with explicit unit/scale. Receipts must bind tenant, account, trace, operation, idempotency key, amount, entry, contract hash, and replay evidence.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p ledger-service
cargo clippy -p ledger-service --all-targets -- -D warnings
bash scripts/check-p0-migrations-postgres.sh
```

Required behavioral focus:

- Exact opening/effect lifecycle, insufficient funds, replay and collision.
- Reserve/consume/refund mutual exclusion and response-loss lookup.
- Append-only guards, partial upgrade, projection parity, and cross-tenant negatives.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Apply the contiguous migration chain with a migration owner. Runtime roles cannot mutate append-only history or schema. Observe parity, effect backlog, receipt recovery, and failed authorization.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Historical records may be read, but new value writes use exact v2 only. No float, decimal rounding, major-unit inference, or compatibility fallback may create authority.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.

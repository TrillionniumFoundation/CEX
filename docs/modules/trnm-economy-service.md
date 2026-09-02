# trnm-economy-service module contract

Status: active module contract  
Workspace member: `services/trnm-economy-service`  
Package: `trnm-economy-service`  
Kind: `service`  
Logical module: `trnm`  
Deployable: yes  
Owner role: `trnm-economy`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Admits signed/versioned TRNM economy requests, freezes exact intent bytes, and coordinates durable receipt/recovery semantics with the Ledger authority.

**Non-goals.** It is not the Trillionnium Chain consensus engine, a wallet custody service, a public market, or a replacement for independently verified finality.

## Authority and owned state

Authenticated TRNM economic-intent admission, canonical contract validation, durable settlement request state, and receipt projection at the Chain adapter boundary.

Owned state: Adapter-side economic intent bytes, hashes, settlement status, and receipt projection defined by its migration. Ledger/Chain remain authoritative for their own facts.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `api.rs`: authenticated request and receipt routes.
- `config.rs`: fail-closed production configuration.
- `contract.rs`: canonical signed economic contract.
- `repository.rs`: durable intent and receipt state.
- `migrations/settlement_v1.sql`: adapter-owned schema.

Catalog-bound entry points:

- `services/trnm-economy-service/src/main.rs`
- `services/trnm-economy-service/src/api.rs`
- `services/trnm-economy-service/src/config.rs`
- `services/trnm-economy-service/src/contract.rs`
- `services/trnm-economy-service/src/repository.rs`
- `services/trnm-economy-service/migrations/settlement_v1.sql`
- `services/trnm-economy-service/tests/settlement_contract.rs`
- `services/trnm-economy-service/tests/durable_bytes_immutability.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Authenticated Axum routes defined in `api.rs`; `contract.rs` validates canonical intent shape and signing semantics; callers must use stable intent IDs and hashes.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

PostgreSQL stores immutable intent bytes before downstream effects. Response loss is recovered by intent/hash lookup rather than repeating a different monetary operation.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Production-like startup requires pairwise distinct credentials, approved issuer registries, exact Ledger endpoint/authority settings, bounded bodies, and durable database connectivity.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Verify signatures, issuer/key IDs, audience/scope, expiry, canonical bytes, intent hash, and backend identity. Credentials for entitlement, game authority, session signing, and Ledger administration must not be reused.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p trnm-economy-service
cargo clippy -p trnm-economy-service --all-targets -- -D warnings
```

Required behavioral focus:

- Canonical signature/intent validation and wrong-authority negatives.
- Durable byte immutability, response loss, duplicate/collision, and receipt recovery.
- Production credential separation and missing-backend startup failure.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Apply `migrations/settlement_v1.sql` under the approved owner, start with least-privilege runtime credentials, and expose readiness only after contract/database validation.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Whole-credit compatibility may convert only through checked scale multiplication. Fractional or ambiguous legacy values fail closed. Chain-finality claims require separately verified receipts.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.

# shared-types module contract

Status: active module contract  
Workspace member: `crates/shared-types`  
Package: `shared-types`  
Kind: `library`  
Logical module: `shared`  
Deployable: no  
Owner role: `platform-foundations`  
Production authorization: `not_granted`

This document is the module-level development contract referenced by
`docs/module-catalog-v1.json`. It describes source ownership and required
verification, but only exact-tree hosted evidence can qualify a candidate.

## Purpose and non-goals

**Purpose.** Defines the cross-service data vocabulary for exact money, Ledger v2 effects, Audit v2 envelopes, saga state, identifiers, and timestamps.

**Non-goals.** It must not contain service orchestration, database access, network clients, tenant policy, or mutable global state.

## Authority and owned state

Versioned serializable types and validation helpers shared across CEX services; it owns no durable business state.

No database tables or runtime authority. The authority is the byte/field contract of exported Rust types and their validation rules.

The module must not claim authority owned by another service merely because it
holds a projection, cache, compatibility row, HTTP response, or successful
transport status. Every cross-module write uses stable identity, explicit
versioning, and fail-closed replay/collision behavior.

## Source layout and entry points

- `money.rs`: exact minor-unit types and canonical amount rules.
- `ledger_v2.rs`: Ledger account/effect request and receipt contracts.
- `audit_v2.rs`: authenticated audit event and receipt shapes.
- `saga.rs`: shared invocation/execution saga vocabulary.
- `lib.rs`: public re-exports and module boundary.

Tracked source entry points bound by the module catalog:

- `crates/shared-types/src/lib.rs`
- `crates/shared-types/src/money.rs`
- `crates/shared-types/src/ledger_v2.rs`
- `crates/shared-types/src/audit_v2.rs`
- `crates/shared-types/src/saga.rs`

New source directories or deployable binaries must be added to the catalog and
this document in the same change. Large files should be decomposed by bounded
context without changing the public contract or moving authority implicitly.

## Interfaces and contracts

Rust public types re-exported from `src/lib.rs`. JSON compatibility is controlled through Serde field names and explicit validation. Any field or enum change is a protocol change.

Interfaces must document authentication, tenant/subject binding, request and
response bounds, idempotency scope, version negotiation, error semantics, and
retirement conditions. A successful HTTP status is never sufficient evidence
for a value, provider, research, or finality transition unless the owning
contract validates the complete receipt.

## Persistence, concurrency, and recovery

Consumers may persist these types, but this crate does not perform persistence. Readers must never infer missing historical precision or authority from a compatibility projection.

When this module performs remote I/O, durable intent/claim state must commit
before the call and the outcome must be recorded in a separate transaction.
Timeout after a possible side effect enters a pending or reconciliation state;
it must not be converted into automatic success, failure, or a second operation
identity.

## Configuration and secrets

No runtime configuration or secrets. All behavior must be deterministic from input values.

Configuration precedence and defaults are part of the runtime contract.
Production-like profiles must fail before opening a listener or starting a
worker when required durable state, credentials, trust anchors, or explicit
mode selection are absent. Example files contain placeholders only and are
never activation evidence.

## Security and trust boundaries

Validation must reject noncanonical money, malformed UUIDs, unsupported versions, ambiguous replay identity, and unbounded free-form data where the type contract specifies limits.

All untrusted inputs are bounded and validated before authority changes. Logs,
metrics, traces, and errors must avoid secrets, raw private keys, unrestricted
personal data, high-cardinality identifiers, and confidential research/provider
payloads. Break-glass behavior requires a separate audited operator contract.

## Verification

Required module commands:

```text
  cargo test -p shared-types
  cargo clippy -p shared-types --all-targets -- -D warnings
```

Required behavioral coverage:

- Unit tests must cover canonical serialization, boundary values, overflow, replay equality, and collision cases.
- Downstream service tests remain responsible for authorization, persistence, and network behavior.

These commands are necessary but not sufficient for repository qualification.
The exact candidate SHA must pass the authoritative hosted workflow and be
included in the generated immutable candidate manifest.

## Deployment and operations

Not independently deployable. It is linked into dependent workspace packages and is covered by workspace exact-tree evidence.

Operators must record artifact/image identity, configuration profile, database
and external dependency identities, health/readiness results, rollback
boundary, retained evidence, and on-call ownership. Repository CI does not
replace representative-volume recovery, sustained load, real credential
custody, or independent production approval.

## Compatibility and change protocol

Additive fields require explicit default/option semantics; enum variants and serialization changes require versioned protocol review and golden-vector updates.

Every change that modifies authority, persistence, public types, routes,
configuration, migration ownership, external dependencies, retry semantics, or
deployment topology must update:

1. this module contract;
2. `docs/module-catalog-v1.json` when metadata or entry points change;
3. the relevant protocol/ADR and machine-readable traceability entry;
4. executable tests and hosted gate wiring;
5. the shared candidate trigger, creating a new exact-tree candidate.

No module document may declare repository closure or production authorization.

# capability-service module contract

Status: active module contract  
Workspace member: `services/capability-service`  
Package: `capability-service`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `capability-registry`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Publishes the bounded capability catalog used by callers to select supported models, Agents, tools, or workflows.

**Non-goals.** It does not host models, execute tools, own provider credentials, determine entitlement, mutate Ledger state, or authorize a Paper Raid participant.

## Authority and owned state

Capability metadata and version registry for supported control-plane flows; it grants no execution, Agent, financial, or finality authority by itself.

Owned state: Capability metadata, versions, status, and descriptive pricing/configuration fields supported by the current implementation. It is not an authority for actual provider availability.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `lib.rs`: catalog model, routes, and state.
- `main.rs`: process startup.
- `tests/http_flow.rs`: HTTP contract.

Catalog-bound entry points:

- `services/capability-service/src/main.rs`
- `services/capability-service/src/lib.rs`
- `services/capability-service/tests/http_flow.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Axum routes and Rust handlers in `lib.rs`; `main.rs` starts the service. Consumers must treat capability IDs and versions as explicit contract identifiers.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

The current package has no SQL dependency; any in-memory catalog is supporting Alpha only. Production durability requires an explicit versioned repository contract before promotion.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Bind/listen settings and any catalog seed source. Unknown fields, duplicate IDs, unsupported versions, and production-like implicit defaults should fail closed.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Capability metadata is untrusted descriptive input unless separately approved. It may not contain credentials or grant authority based on self-declared provider/Agent capabilities.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p capability-service
cargo clippy -p capability-service --all-targets -- -D warnings
```

Required behavioral focus:

- Duplicate/unknown capability and version handling.
- Stable serialization and HTTP error mapping.
- Fail-closed production posture when durable authority is required but unavailable.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Deploy only as a supporting read/control surface. Readiness must state whether the catalog is durable, seeded, and internally consistent.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Capability IDs are stable; version changes are additive and explicit. Retirement requires caller inventory and a deprecation/translation plan.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.

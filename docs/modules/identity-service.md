# identity-service module contract

Status: active module contract  
Workspace member: `services/identity-service`  
Package: `identity-service`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `identity-control-plane`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Provides the trusted internal identity boundary for organizations, actors, and API-key administration used by other CEX services.

**Non-goals.** Matrix identities, browser cookies, Agent scientific authority, Ledger balances, provider execution, and Chain finality are not owned here.

## Authority and owned state

Organization/tenant identity, API-key lifecycle, authenticated actor resolution, and durable identity governance within the CEX control plane.

Owned state: Identity/API-key records and their audit revision. Authentication material is never returned after issuance and usage telemetry must not rewrite identity authority.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `main.rs`: process startup and listener boundary.
- `lib_entry.rs`: library entry used by tests/other packages.
- `lib.rs`: routes, state, validation, and persistence orchestration.
- `tests/http_flow.rs`: API behavior.
- `tests/runtime_blackbox.rs`: startup and durable-backend behavior.

Catalog-bound entry points:

- `services/identity-service/src/main.rs`
- `services/identity-service/src/lib_entry.rs`
- `services/identity-service/src/lib.rs`
- `services/identity-service/tests/http_flow.rs`
- `services/identity-service/tests/runtime_blackbox.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Axum HTTP routes from `src/lib.rs`, a thin library entry in `lib_entry.rs`, and the deployable server in `main.rs`. Internal callers must send authenticated service identity and tenant scope.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

PostgreSQL is required for production-like authority. Startup must fail closed when the durable backend or required schema is unavailable; no in-memory write authority may silently replace it.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Uses shared runtime-profile guards, database configuration, bind address, and service credentials. Production-like profiles reject weak/default tokens and ambiguous backend selection.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Raw keys, key hashes, database URLs, and admin credentials must not enter responses, logs, or Audit payloads. Issue/revoke/rotate operations require explicit administrative authority and tenant checks.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p identity-service
cargo clippy -p identity-service --all-targets -- -D warnings
```

Required behavioral focus:

- Key issue/replay/collision/revoke/expiry and tenant isolation.
- Production-like startup with missing database or invalid credentials must fail before listening.
- Audit revision changes only for meaningful authority changes, not `last_used_at` telemetry.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Run migrations with the approved migration owner, then start the runtime with a least-privilege database role. Readiness must distinguish process health from durable identity authority.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Legacy identity reads may remain for migration, but new authoritative writes must use the current authenticated contract. Removal requires caller inventory and zero-use evidence.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.

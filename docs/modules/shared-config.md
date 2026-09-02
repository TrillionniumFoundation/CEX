# shared-config module contract

Status: active module contract  
Workspace member: `crates/shared-config`  
Package: `shared-config`  
Kind: `library`  
Logical module: `shared`  
Deployable: no  
Owner role: `platform-foundations`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Centralizes fail-closed configuration primitives so production-like services interpret profiles, credentials, database settings, and downstream clients consistently.

**Non-goals.** It is not a secret manager, deployment manifest, feature-flag service, or owner of service-specific business settings.

## Authority and owned state

Canonical runtime-profile normalization, service-auth parsing, guarded database mode selection, and shared client configuration.

Owned state: No durable state. It is authoritative for shared parsing and validation semantics before asynchronous runtime mutation.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `runtime_guard.rs`: runtime-profile and production-like fail-fast rules.
- `service_auth.rs`: internal workload credential parsing and validation.
- `service_client.rs`: bounded downstream client configuration.
- `lib.rs`: public exports.

Catalog-bound entry points:

- `crates/shared-config/src/lib.rs`
- `crates/shared-config/src/runtime_guard.rs`
- `crates/shared-config/src/service_auth.rs`
- `crates/shared-config/src/service_client.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Rust helpers for runtime guards, service authentication, and HTTP/database client settings. Service-specific environment names remain documented by each service.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

Configuration is loaded at startup; dynamic reload is forbidden unless a service defines an authenticated, audited, versioned reload contract.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Production-like profiles must reject development placeholders, weak credentials, silent in-memory fallbacks, and ambiguous configuration precedence.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Secrets should be supplied through approved secret files or injected environment boundaries, never committed values. Validation must run before listeners or background workers start.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p shared-config
cargo clippy -p shared-config --all-targets -- -D warnings
```

Required behavioral focus:

- Production-like placeholder rejection and pre-runtime mutation safety.
- Credential parsing, duplicate/collision handling, and client timeout bounds.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Not independently deployable. Every deployable package using shared guards must preserve fail-fast startup.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Changing normalization or default behavior can alter every service. Such changes require cross-service regression and explicit migration guidance.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.

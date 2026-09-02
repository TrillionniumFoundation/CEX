# shared-tracing module contract

Status: active module contract  
Workspace member: `crates/shared-tracing`  
Package: `shared-tracing`  
Kind: `library`  
Logical module: `shared`  
Deployable: no  
Owner role: `platform-foundations`  
Production authorization: `not_granted`

This document is the module-level development contract referenced by `docs/module-catalog-v1.json`. Only exact-tree hosted evidence can qualify a candidate.

## Purpose and non-goals

**Purpose.** Initializes structured tracing consistently across deployable Rust packages and centralizes common subscriber behavior.

**Non-goals.** It does not own Audit facts, distributed trace storage, sampling-policy approval, or payload redaction for arbitrary callers.

## Authority and owned state

The crate owns workspace tracing initialization and common field conventions. It owns no persistent business state; logs and spans are observations, never financial, scientific, provider, or finality authority.

## Source layout and entry points

- `crates/shared-tracing/src/lib.rs` contains initialization and common tracing helpers.

Any new exporter, field convention, or public helper must update the catalog and this contract.

## Interfaces and contracts

Services initialize tracing once before opening listeners or starting workers. Callers should use bounded route templates and low-cardinality outcome fields. Correlation identifiers may be included when necessary, but raw request bodies and external payloads are not tracing fields.

## Persistence, concurrency, and recovery

Exporter and collector retention are external. Process restart may discard unflushed spans and reset process-local counters; authoritative facts must already be in their owning durable stores. Tracing failure must not silently weaken authentication or value checks.

## Configuration and secrets

The crate consumes the logging/filter configuration supported by `tracing-subscriber`. Each deployable service documents any additional exporter endpoint, transport security, buffering, and backpressure behavior. Secret values are never valid filter fields or span attributes.

## Security and trust boundaries

Do not record authentication material, signing material, connection strings, unrestricted prompts/results, paper bodies, or high-cardinality personal identifiers. Any new field must identify its cardinality, sensitivity, retention, and operator purpose.

## Verification

Required module commands:

```text
cargo test -p shared-tracing
cargo clippy -p shared-tracing --all-targets -- -D warnings
```

Service tests must additionally cover initialization behavior and verify that sensitive inputs do not enter logs/metrics. These commands are necessary but not sufficient for repository qualification.

## Deployment and operations

The crate is not independently deployable. Dependent services own collector reachability, dashboards, alerts, retention, and incident response. A green health endpoint or dashboard cannot replace durable evidence.

## Compatibility and change protocol

Field-name and level changes affect dashboards, alerts, and retained queries. Add fields conservatively and coordinate removal with every consumer. Changes require this document, the module catalog, affected service tests/runbooks, traceability when normative, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.

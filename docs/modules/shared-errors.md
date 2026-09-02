# shared-errors module contract

Status: active module contract  
Workspace member: `crates/shared-errors`  
Package: `shared-errors`  
Kind: `library`  
Logical module: `shared`  
Deployable: no  
Owner role: `platform-foundations`  
Production authorization: `not_granted`

This document is the module-level development contract referenced by `docs/module-catalog-v1.json`. Only exact-tree hosted evidence can qualify a candidate.

## Purpose and non-goals

**Purpose.** Provides serializable typed errors that cross internal Rust boundaries without coupling services to implementation-specific strings.

**Non-goals.** It is not a logging layer, retry engine, transport, or substitute for service-specific error registries.

## Authority and owned state

This crate owns the shared typed error vocabulary only. Each service remains authoritative for HTTP mapping, retry policy, operator response, and durable incident evidence. It owns no persistent state.

## Source layout and entry points

- `crates/shared-errors/src/lib.rs` defines and exports the public error contract.

New source files or public error families must be added to the module catalog and this document in the same change.

## Interfaces and contracts

Public `thiserror` and Serde types are the compatibility surface. Callers must preserve stable machine-readable codes, distinguish validation/authentication/collision/unavailable/unknown-outcome classes, and attach bounded human context without exposing confidential values.

## Persistence, concurrency, and recovery

Errors may be copied into Audit or metrics only through reviewed redaction and cardinality rules. A transport timeout after a possible remote effect must remain an unknown-outcome or reconciliation condition; it may not be recast as success, definite failure, or permission to create a second operation identity.

## Configuration and secrets

This crate has no runtime configuration. It must never require or carry runtime secret material. Configuration of error-to-HTTP or retry behavior stays in the owning service.

## Security and trust boundaries

Error responses must reveal enough for deterministic handling without exposing authentication material, connection strings, confidential research data, provider output, or internal stack details. Authorization failures should not become existence or tenant-enumeration oracles.

## Verification

Required module commands:

```text
cargo test -p shared-errors
cargo clippy -p shared-errors --all-targets -- -D warnings
```

Coverage must include serialization round trips, stable error codes, redaction boundaries, and downstream mapping tests for any new semantic class. These commands remain necessary but not sufficient for repository qualification.

## Deployment and operations

The crate is not independently deployable. It is linked into dependent packages and inherits their release, rollback, observability, and incident procedures.

## Compatibility and change protocol

Renaming or removing a machine-readable code is breaking. New classes are additive only when old readers fail safely. Any change affecting retry or reconciliation semantics requires updates to this contract, the module catalog, owning service tests/runbooks, traceability, and the shared candidate trigger.

No module document may declare repository closure or production authorization.

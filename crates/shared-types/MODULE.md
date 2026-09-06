# shared-types module contract

Status: active module documentation  
Module path: `crates/shared-types`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Versioned Rust data types and serde wire shapes shared by CEX control-plane services, including identity, capability, invocation and exact Ledger v2 contracts.

## Non-goals

- Owning business state or persistence.
- Performing network I/O.
- Introducing implicit monetary conversion or compatibility authority.

## Authority and state ownership

This crate owns type and serialization definitions only. Domain services remain the authoritative writers of the state represented by those types.

## Interfaces

- Rust public types and constructors.
- Serde JSON representations used by HTTP and durable-command boundaries.

## Data and persistence

- No persistent state.
- Authoritative monetary contracts use explicit currency unit, scale and signed minor-unit representations; display-only or legacy values must not become write authority.

## Security and configuration

- Validate untrusted strings and identifiers at the service boundary.
- Never place credentials, private keys or unrestricted research bodies in shared DTOs.

## Failure and recovery

- Deserialization or validation failures are terminal for the affected request and must not be silently defaulted.
- Schema changes require compatibility fixtures and an explicit migration/retirement rule.

## Observability

- Errors should expose stable machine-readable codes at the consuming service, not ad-hoc Debug text.

## Verification

- `cargo test --locked -p shared-types`
- `cargo clippy --locked -p shared-types --all-targets -- -D warnings`
- `scripts/check-p0-wiring.py`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Additive fields require explicit defaults only when the semantic default is safe. Breaking wire changes require a new versioned type; legacy money precision must never be inferred.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

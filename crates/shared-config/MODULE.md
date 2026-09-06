# shared-config module contract

Status: active module documentation  
Module path: `crates/shared-config`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Configuration parsing, runtime-profile enforcement, scoped administration credentials, internal service authentication and hardened HTTP-client construction.

## Non-goals

- Secret storage.
- Repository administration.
- Service-specific domain defaults.
- Allowing production-like profiles to inherit local-development credentials.

## Authority and state ownership

Environment or secret-manager material is input; the crate owns validation rules, not the underlying credential authority. Production authorization remains external.

## Interfaces

- Rust configuration loaders.
- runtime_guard profile validation.
- service_auth middleware configuration.
- service_client hardened client construction.

## Data and persistence

- No business persistence.
- Configuration snapshots should be represented by non-secret revision/digest metadata when audited.

## Security and configuration

- Production-like profiles require explicit fail-fast posture, database preflight and non-placeholder credentials.
- Raw credential values must never be logged or returned by health endpoints.

## Failure and recovery

- Invalid or conflicting production configuration is a startup failure.
- Credential rotation must support overlap, revocation and observable active revision without accepting an unapproved fallback.

## Observability

- Expose profile, validation status and non-secret source/revision metadata.
- Configuration rejection uses stable exit code and bounded error code.

## Verification

- `cargo test --locked -p shared-config`
- `cargo clippy --locked -p shared-config --all-targets -- -D warnings`
- `docs/runtime-profile-and-startup-guard-v1.md`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Direct source inclusion with #[path] is compatibility debt and must not expand. Consumers should depend on the crate public API; breaking configuration changes require migration guidance.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

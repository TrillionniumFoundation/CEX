# shared-errors module contract

Status: active module documentation  
Module path: `crates/shared-errors`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Shared error envelopes and stable machine-readable error semantics used across CEX service boundaries.

## Non-goals

- Logging or alert routing.
- Encoding service-specific recovery policy inside a generic error.
- Using free-form messages as protocol contracts.

## Authority and state ownership

The originating service owns the failure decision. This crate may standardize codes and envelopes but never converts an ambiguous remote outcome into success or retryable failure.

## Interfaces

- Rust error/envelope types.
- Serialization helpers for HTTP and internal service responses.

## Data and persistence

- No persistence.
- Error codes are durable compatibility identifiers; human messages are diagnostic and may evolve.

## Security and configuration

- Do not return secrets, bearer tokens, database URLs or raw provider payloads.
- Authentication and authorization failures must not reveal whether protected resources exist.

## Failure and recovery

- Each code must document retryability, operator action and whether an idempotent replay is safe.
- Unknown provider outcomes map to reconcile-required semantics rather than blind retry.

## Observability

- Metrics and logs should group by bounded error code and upstream class; avoid unbounded message labels.

## Verification

- `cargo test --locked -p shared-errors`
- `cargo clippy --locked -p shared-errors --all-targets -- -D warnings`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Existing error codes are not repurposed. Semantically different failures receive new codes; consumers must ignore unknown optional envelope fields.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

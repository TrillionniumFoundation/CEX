# hepta-paper-raid-contracts module contract

Status: active module documentation  
Module path: `crates/hepta-paper-raid-contracts`  
Lifecycle: `active`  
Production authorization: `not_granted`

This document is the canonical module-level development contract required by `docs/module-documentation-standard-v1.md`. It describes the repository implementation boundary; it does not grant deployment or production approval.

## Scope

Versioned canonical command, evidence, signature and bundle contracts shared by Hepta Research League and Paper Raid consumer surfaces.

## Non-goals

- Owning research workflow state.
- Persisting publications.
- Trusting response-supplied signing keys.
- Treating a score as legal intellectual-property adjudication.

## Authority and state ownership

The crate owns canonical wire framing and deterministic hashing rules. Hepta owns research facts, Nakama owns live match facts and TRNM/finality owns independently verified finality.

## Interfaces

- Rust protocol types.
- Canonical JSON/hash/signature frames.
- Assets and golden fixtures consumed by OpenAPI and contract tests.

## Data and persistence

- No mutable service state.
- Content-addressed assets and immutable key snapshots are referenced by digest and version.

## Security and configuration

- Verify signatures against pinned authority and immutable key epochs.
- Reject duplicate keys, mixed roster roots, unsupported versions and non-canonical encodings.

## Failure and recovery

- Exact replay of identical immutable input returns the same result; identity reuse with different content is a collision.
- Unavailable content-addressed bytes place the workflow on hold rather than substitute mutable content.

## Observability

- Verification failures should identify protocol version, bounded reason code and trust-anchor revision without leaking secret material.

## Verification

- `cargo test --locked -p hepta-paper-raid-contracts`
- `cargo clippy --locked -p hepta-paper-raid-contracts --all-targets -- -D warnings`
- `docs/openapi/hepta-paper-raid-v2.yaml`

The exact candidate must also pass the active repository gates named by `docs/development-doc-authority-v1.json`.

## Compatibility and retirement

Breaking contract changes require a new explicit version and golden fixtures. Compatibility projections never invent consent, authorship, precision or finality.

A retirement or ownership transfer requires an accepted ADR, a catalog lifecycle change, explicit data/protocol migration, zero authoritative writers left behind, and requalification of the resulting exact tree.

# trnm-economy-protocol module contract

Status: active module contract
Workspace member: `vendor/trnm-economy-protocol`
Package: `trnm-economy-protocol`
Kind: `contract-library`
Logical module: `trnm`
Deployable: no resident CEX service
Owner role: `trnm-integration`
Production authorization: `not_granted`

This contract records the inspected source boundary and required integration checks. It is not a successful build, provenance attestation or production qualification.

## Purpose and non-goals

This vendored package defines economy-facing policy, entitlement and backend compatibility shapes. It is imported through the workspace dependency alias `term-exchange-protocol`, but its Cargo package identity is `trnm-economy-protocol` version 2.4.0. It is not a settlement engine, wallet database, signature verifier or authority to operate a public market.

## Authority and owned state

The crate owns serializable contract vocabulary and default policy values only. CEX Ledger remains responsible for exact off-chain effects, while Chain and game authorities retain their own receipts and identity. Creating a value-entitlement struct, calling its shape validator, or selecting CexConnected does not credit an account or grant an entitlement. The embedding service owns replay records and decision audit history.

## Source layout and entry points

Catalog-bound sources:

- `vendor/trnm-economy-protocol/src/lib.rs`

The root Cargo manifest names this vendored package explicitly. Internal source modules are not additional services. The source paths are build/contract inputs, not independent release evidence.

## Interfaces and contracts

`ServerSignedValueEntitlementV1` binds contract version, entitlement, issuer/key, actor/account, source/source ID, intent, positive credit amount, currency, budget day, issuance/expiry and signature. `ValueEntitlementSource` distinguishes Battle from Contract. V2 additionally binds signature algorithm, match, rules, build, result/participants hashes and nonce. Both `signing_payload` methods clone the value, clear `signature`, then serialize the remaining structure through Serde JSON. Clients must not invent a differently normalized signing representation.

`validate_shape` rejects the wrong contract, blank required identities, nonpositive credit amount, a currency other than `wallet_credits`, reversed issuance/expiry, or an empty signature. V2 checks the algorithm label `ed25519` and hash string lengths. These checks are NOT cryptographic verification: they do not establish key trust, check current expiry, consume a nonce, prove hash authenticity, or prove the bound match happened. The caller must perform those checks before financial admission.

The default `TrnmEconomyPolicy` disables soft-credit conversion and the public player market. Its declared event/daily reward caps are 100/300 and the reversible window is 86,400 seconds; merely carrying those values does not enforce a cumulative budget. `EconomyMode` defaults to OfflineLocal. Protocol, kernel and backend version identifiers remain distinct from the Cargo version.

## Persistence, concurrency, and recovery

The library has no authoritative durable ledger or cross-process uniqueness constraint. The caller must persist a canonical request commitment, actor/account binding, admission decision and exact downstream receipt before acknowledging an effect. Replays must use the same operation identity; an unknown remote outcome must be reconciled rather than given a fresh entitlement ID. Budget enforcement must be transactional across concurrent issuers, not inferred from this crate's default policy.

## Configuration and secrets

There is no independent listener or deployment configuration for this library. The signature and key ID are contract data; private signing keys and issuer trust configuration belong to approved service custody. The compile-time package version is not an accepted issuer revision. Do not log signing inputs containing account identities or expose a private key through fixtures, public artifacts or this module contract.

## Security and trust boundaries

Treat decoded entitlement data and policy fields as untrusted caller input. Shape acceptance is deliberately narrower than issuer authentication, time validation, revocation, event evidence and spending authority. Reject an unrecognized version at the service boundary and preserve exact integer accounting. In particular, a nonblank signature and two 64-character hash strings do not establish an Ed25519 result or verified result/participants commitments.

## Verification

Required Linux qualification commands:

```text
cargo test --locked -p trnm-economy-protocol --all-targets
cargo clippy --locked -p trnm-economy-protocol --all-targets -- -D warnings
```

Run against the committed Cargo.lock and complete checkout. Missing toolchain or source is failure, not a skip. In addition, `python3 scripts/check-cargo-workspace-authority.py` must bind actual Cargo membership and discovered targets to this catalog entry. The full document, semantic inventory and consumer integration gates remain required. None of these Rust commands executed in the current authoring environment.

## Deployment and operations

Not independently deployable as a CEX service. Package it with the service that consumes its types and record its version/source commitment in the service build. Service readiness must include real issuer configuration and durable effect storage rather than a successful library import. Rollback must preserve previous request/receipt identities and use a version-compatible consuming binary; changing a default policy must not retroactively rewrite accepted effects.

## Compatibility and change protocol

The workspace keeps the existing dependency alias and exact 2.4.0 package selection. This module-documentation change does not change vendor bytes or Cargo.lock. This economy package is not one of the four crates listed in `vendor/trnm-chain-vendor-manifest.json`; do not fabricate equivalent Chain provenance for it. Any source update requires its own reviewed origin, contract/golden-vector compatibility and locked integration evidence. Current runtime qualification remains open.

# hepta-paper-raid-contracts module contract

Status: active module contract  
Workspace member: `crates/hepta-paper-raid-contracts`  
Package: `hepta-paper-raid-contracts`  
Kind: `contract-library`  
Logical module: `hepta`  
Deployable: no  
Owner role: `hepta-contracts`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Freezes the canonical contract surface used by Hepta and the Paper Raid BFF for signed commands, receipts, hashes, and compatibility fixtures.

**Non-goals.** It does not decide research truth, perform database writes, contact Nakama/Chain, host Agents, or grant publication/finality authority.

## Authority and owned state

Canonical Paper Raid signed-byte frames, identifiers, cryptographic validation, and cross-service contract types.

Owned state: No runtime state. Its authority is canonical byte construction, signature verification, version tags, and validation invariants.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `src/lib.rs`: versioned contract definitions and crypto validation.
- `assets/legacy-golden-qualification-brief.md`: historical compatibility fixture context.

Catalog-bound entry points:

- `crates/hepta-paper-raid-contracts/src/lib.rs`
- `crates/hepta-paper-raid-contracts/assets/legacy-golden-qualification-brief.md`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Rust contract types plus Ed25519/Base64/SHA-256 helpers. Golden vectors are compatibility evidence and must not be silently regenerated after a breaking change.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

Services persist contract snapshots and hashes. Replayed bytes must remain identical; a reused identity with different bytes is a collision.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

No runtime secrets. Verification keys are supplied by owning services under pinned-key policies.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Domain separators, key IDs, nonces, expiry, expected versions, payload hashes, and bounded lengths must all be covered by the signed frame.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p hepta-paper-raid-contracts
cargo clippy -p hepta-paper-raid-contracts --all-targets -- -D warnings
```

Required behavioral focus:

- Canonical byte equality and deterministic fingerprints.
- Signature, wrong-key, replay, expiry, field-tamper, and unsupported-version negatives.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Not independently deployable. Included in Hepta/BFF binaries and exact-tree evidence.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Protocol changes require a new explicit version, positive and tamper-negative vectors, reader migration, and retirement criteria for the old version.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.

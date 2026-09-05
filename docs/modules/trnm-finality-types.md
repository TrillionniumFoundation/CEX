# trnm-finality-types module contract

Status: active module contract
Workspace member: `vendor/trnm-finality-types`
Package: `trnm-finality-types`
Kind: `contract-library`
Logical module: `trnm`
Deployable: no resident CEX service
Owner role: `trnm-integration`
Production authorization: `not_granted`

This contract records the inspected source boundary and required integration checks. It is not a successful build, provenance attestation or production qualification.

## Purpose and non-goals

This vendored contract library provides consensus identity, signed-command, quorum, proof and receipt wire types. It exposes the vocabulary used by node-independent verification, but it is not a running consensus node, an RPC trust service or a declaration that any specific receipt is final. Local compilation does not create an external Chain authority.

## Authority and owned state

The public library re-exports the cometbft, protocol and state_proof modules and selected crypto utilities. It owns type/encoding and validation rules only. Validator sets, chain IDs, trust anchors and accepted checkpoints must come from independently configured authority. A structurally valid user-supplied validator set must not become trusted simply because it can be deserialized.

## Source layout and entry points

Catalog-bound sources:

- `vendor/trnm-finality-types/src/lib.rs`
- `vendor/trnm-finality-types/src/cometbft.rs`
- `vendor/trnm-finality-types/src/crypto.rs`
- `vendor/trnm-finality-types/src/protocol.rs`
- `vendor/trnm-finality-types/src/state_proof.rs`

The root Cargo manifest names this vendored package explicitly. Internal source modules are not additional services. The source paths are build/contract inputs, not independent release evidence.

## Interfaces and contracts

The crate root exports `decode_hash32`, `hash_domain` and `Hash32`, alongside the proof/protocol types. Receipt consumers use types such as `FinalityReceiptV1`, `ValidatorSetV1`, `MerkleProofV1` and quorum certificates through the verifier boundary. Hash parsing and domain separation are protocol operations: callers must preserve the expected field encodings, domain labels and ordered input bytes rather than using a generic hash of presentation JSON.

CometBFT receipt and AppHash proof data are distinct from the legacy finality-receipt vocabulary. Select the intended protocol version explicitly and pass typed proof data to its matching verifier. Serializing/deserializing a wire type is not a substitute for verifying receipt/header/transaction/object relationships, validator authority or trusting-period rules. Caller-visible validation failures remain rejection results; adapters must not replace failed hashes with zero hashes or empty proofs.

## Persistence, concurrency, and recovery

The crate defines receipt/proof data, not durable storage ownership. An embedding verifier or service must retain exact input bytes, protocol version, configured trust anchor identity and verified result. Restart must restore the accepted trust context rather than learning it from the receipt under inspection. Concurrent verification can operate on immutable values, but updating trusted checkpoints or accepting financial effects requires the owning service's own concurrency protocol.

## Configuration and secrets

No resident database, access token or independent listener is supplied by this package. Explicit receipt/validator/trust inputs carry the configured boundary. Public keys are not private signing credentials, and serialized validators are not a key-custody system. Private keys must not enter routine verification artifacts or logs. Required inherited edition/license/author metadata comes from the embedding workspace and is not a runtime trust decision.

## Security and trust boundaries

The consuming service must bound untrusted receipt sizes before deserialization, select approved algorithms/versions and distinguish content commitment from issuer authority. Domain separation prevents accidental mixing of hash purposes only when callers preserve the specified bytes. Do not infer proof validity from a type name, convert parsing errors into an accepted default, or treat a verified Chain receipt as permission for an unrelated CEX operation.

## Verification

Required Linux qualification commands:

```text
cargo test --locked -p trnm-finality-types --all-targets
cargo clippy --locked -p trnm-finality-types --all-targets -- -D warnings
```

Run against the committed Cargo.lock and complete checkout. Missing toolchain or source is failure, not a skip. In addition, `python3 scripts/check-cargo-workspace-authority.py` must bind actual Cargo membership and discovered targets to this catalog entry. The full document, semantic inventory and consumer integration gates remain required. None of these Rust commands executed in the current authoring environment.

## Deployment and operations

Not independently deployable. The package is linked into the verifier or another consuming service. Operational readiness is defined by those consumers and their genuine trust configuration; this library has no standalone health endpoint. Rollback requires compatible receipt schemas and retention of previously accepted trust/receipt evidence. Observe bounded error categories in the service instead of exporting entire private proof payloads.

## Compatibility and change protocol

The existing Chain vendor manifest records the intended source subtree and per-file digests. Compare current bytes against that record and its reviewed upstream revision before declaring provenance valid; the manifest's presence is not validation. No vendor file is rewritten by this round. Updating types or hash domains requires corresponding verifier, canonical golden vectors, downstream compatibility and exact-tree tests in the same reviewed change.

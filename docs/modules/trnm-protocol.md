# trnm-protocol module contract

Status: active module contract
Workspace member: `vendor/trnm-protocol`
Package: `trnm-protocol`
Kind: `contract-library`
Logical module: `trnm`
Deployable: no resident CEX service
Owner role: `trnm-integration`
Production authorization: `not_granted`

This contract records the inspected source boundary and required integration checks. It is not a successful build, provenance attestation or production qualification.

## Purpose and non-goals

This vendored library defines canonical Chain transaction and applied-record formats. It binds outer transaction fields to already signed research/Paper Raid commands; it does not execute those commands or prove their inclusion/finality. CEX's importing this library does not move Chain consensus or research authority into the CEX runtime.

## Authority and owned state

The crate owns schema identifiers, canonical encoding/decoding, field bounds and command-binding validation. It has no authoritative account database, fee collection process or persistent replay ledger. Actual nonce consumption, gas accounting, command execution and state roots belong to the appropriate Chain or service authority, not a successful constructor.

## Source layout and entry points

Catalog-bound sources:

- `vendor/trnm-protocol/src/lib.rs`

The root Cargo manifest names this vendored package explicitly. Internal source modules are not additional services. The source paths are build/contract inputs, not independent release evidence.

## Interfaces and contracts

`CanonicalTxV1` carries schema, sender, nonce, max_gas, decimal-serialized u128 fee_limit and a typed command. Validation checks the schema, canonical sender identity, positive nonce/max_gas and command validity. `CanonicalResearchTxV1` additionally carries a payload type, command ID and the complete signed deterministic-CBOR command as hex.

`from_signed_command` constructs and validates the outer research transaction. `canonical_bytes` emits its accepted JSON representation. `from_canonical_bytes` bounds input, decodes, validates and compares byte-for-byte re-encoding, rejecting alternate field order, whitespace, escaping, duplicate/unknown fields or number spelling. `signed_research_command` decodes the bounded lower-hex payload and invokes the Research command parser. Preserve the signed bytes; do not rebuild a signature from presentation fields.

The source declares separate Paper Raid finality transaction/payload and applied-record identifiers for versions 2, 3 and 4. UnsupportedSchema, UnsupportedPayloadType, NonCanonical, NonPositive, OutOfRange and signed-command/binding failures are explicit `ProtocolError` cases. The visible bounds include 160-byte IDs, 256 KiB signed command bodies and an 8 KiB Paper Raid applied-record bound. Protocol compatibility must not be inferred from the common crate version 0.1.0.

## Persistence, concurrency, and recovery

Canonical bytes and fingerprints can be persisted as immutable input commitments by the calling service, but this crate does not commit a transaction or reserve its nonce. Durable command-id replay must compare exact original signed bytes. After response loss, retain the original transaction identity and reconcile against the authoritative result rather than rewrapping with another nonce. Restoring a JSON struct alone does not restore an accepted Chain checkpoint.

## Configuration and secrets

No separate listener, database connection or private-key configuration is part of this library. Constructors consume signed command values and explicit fee/gas inputs. Fee limits are exact integers, not floating-point display amounts. Secret custody, chain selection and allowed signers are calling-system configuration; neither an embedded public key nor a well-formed signed wrapper automatically establishes authorization.

## Security and trust boundaries

Parse through the canonical boundary before hashing or comparing requests. Do not accept alternate encodings merely because parsed objects compare equal. A wrapper must match the signed command, but valid binding does not prove the signer is authorized, the transaction executed or the receipt is final. Preserve bounded error categories at the service edge rather than treating any parser failure as an empty/default command.

## Verification

Required Linux qualification commands:

```text
cargo test --locked -p trnm-protocol --all-targets
cargo clippy --locked -p trnm-protocol --all-targets -- -D warnings
```

Run against the committed Cargo.lock and complete checkout. Missing toolchain or source is failure, not a skip. In addition, `python3 scripts/check-cargo-workspace-authority.py` must bind actual Cargo membership and discovered targets to this catalog entry. The full document, semantic inventory and consumer integration gates remain required. None of these Rust commands executed in the current authoring environment.

## Deployment and operations

Not independently deployable. Consumers link the package and own readiness for their actual RPC/storage/identity dependencies. Capture the package/source and schema identifiers in qualified builds. Rollback must retain canonical input commitments and support already accepted transaction versions; it cannot rewrite old signed commands, normalize old bytes differently or silently reinterpret versioned applied records.

## Compatibility and change protocol

The source is tracked by the Chain vendor manifest, which records intended provenance rather than proving current acceptance. No vendor or lockfile bytes change in this round. A canonical encoding or bound change requires upstream review, updated exact-source commitments, cross-language vectors and downstream migration compatibility. Workspace/catalog membership correctness and canonical protocol correctness are independent gates.

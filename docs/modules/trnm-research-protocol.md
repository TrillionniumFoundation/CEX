# trnm-research-protocol module contract

Status: active module contract
Workspace member: `vendor/trnm-research-protocol`
Package: `trnm-research-protocol`
Kind: `contract-library`
Logical module: `trnm`
Deployable: no resident CEX service
Owner role: `trnm-integration`
Production authorization: `not_granted`

This contract records the inspected source boundary and required integration checks. It is not a successful build, provenance attestation or production qualification.

## Purpose and non-goals

This contract library carries consensus-facing research commands shared by Chain, Hepta and Nakama, plus their deterministic state transition model. It is not a fourth product domain, a hosted Agent implementation or a scientific evaluator. Local state-machine execution is not evidence that the external consensus network accepted a command.

## Authority and owned state

Nakama may sign MatchEvidenceCommitmentV1, committing match facts and artifact roots. Hepta signs evaluation, workload, claim, license, challenge and resolution commands. The state machine checks signer DID, role and Ed25519 key against an explicit genesis-derived AuthoritySetV1. Match evidence alone cannot authorize accepted research workload or claims. The caller owns actual storage, admission policy and accepted Chain state.

## Source layout and entry points

Catalog-bound sources:

- `vendor/trnm-research-protocol/src/lib.rs`
- `vendor/trnm-research-protocol/src/canonical.rs`
- `vendor/trnm-research-protocol/src/command.rs`
- `vendor/trnm-research-protocol/src/state.rs`
- `vendor/trnm-research-protocol/src/types.rs`
- `vendor/trnm-research-protocol/src/paper_raid.rs`
- `vendor/trnm-research-protocol/tests/protocol_v1.rs`

The root Cargo manifest names this vendored package explicitly. Internal source modules are not additional services. The source paths are build/contract inputs, not independent release evidence.

## Interfaces and contracts

The v1 consensus encoding is `rfc8949-deterministic-cbor-array-v1`: definite arrays, shortest integer/length encodings, fixed numeric discriminants and 32-byte hash/key byte strings. It does not use maps, floating-point or indefinite items. `ResearchCommandV1::from_canonical_bytes` and `SignedResearchCommandV1::from_canonical_bytes` reject unsupported versions/discriminants, wrong or non-minimal lengths, trailing bytes and nonidentical round trips.

The command frame is `[1, command_tag, typed_payload]`. The signed envelope binds version/encoding, chain, command ID, signer DID, authority role, nonce, public key and typed command before its signature. ExternalKey is a domain-separated 32-byte identifier; canonical UUIDs use raw UUID bytes, while other IDs follow the explicit visible-ASCII namespace rules. Do not use an application-local UUID-to-integer translation.

`ResearchProtocolState::apply` returns Idempotent for identical signed command replays and AlteredReplay for a reused command ID with different bytes. Claim/challenge transitions preserve original signed claim content while updating separate current allocation. Changed object references, canonical object bytes/leaf hashes and sorted state snapshot bytes support Chain storage. The paper_raid module also carries versioned finality commands; its newer variants require their own matching consumer/verifier contract, not implicit v1 compatibility.

## Persistence, concurrency, and recovery

`export_snapshot/from_snapshot` supports serialized restoration with graph validation. The owning consensus/store layer must atomically persist applied-command identity and every changed object; serializing only a projection is insufficient. Restore the explicit authority set and compare canonical snapshot commitments. Concurrency ordering belongs to the state owner: do not merge divergent in-memory states or mark a failed/altered replay as a successful fresh application.

## Configuration and secrets

The crate receives authority sets, chain identities and signed commands as explicit inputs rather than providing a resident service configuration. Real signer keys remain under Hepta/Nakama custody. Golden fixtures contain deterministic testing material, not production credentials or independent signatures. Research artifacts, raw streams and datasets stay off-chain; protocol fields carry commitments and bounded accounting/licensing metadata.

## Security and trust boundaries

Verify canonical encoding, signature, role and DID/key binding before using state transitions. Node ingress should also apply its capability policy; library validation does not replace that first boundary. A valid signature from a Nakama key cannot elevate a match statement to a Hepta evaluation. Bind domain-separated external keys and hashes to their namespaces and retain immutable original signed claims through challenges and contributor-allocation changes.

## Verification

Required Linux qualification commands:

```text
cargo test --locked -p trnm-research-protocol --all-targets
cargo clippy --locked -p trnm-research-protocol --all-targets -- -D warnings
```

Run against the committed Cargo.lock and complete checkout. Missing toolchain or source is failure, not a skip. In addition, `python3 scripts/check-cargo-workspace-authority.py` must bind actual Cargo membership and discovered targets to this catalog entry. The full document, semantic inventory and consumer integration gates remain required. None of these Rust commands executed in the current authoring environment.

## Deployment and operations

Not independently deployable. It is linked into an authorized consumer/state owner, which owns persistence readiness, input budgets, consensus ordering and observability. Run both library tests and `tests/protocol_v1.rs` with the checked-in golden vectors. Rollback requires compatible snapshot/command schemas and retention of command replay history; accepting the same ID as a new command after rollback is prohibited by the integration contract.

## Compatibility and change protocol

Keep package identity, deterministic encodings, numeric tags and hash namespaces pinned to a reviewed Chain source. The existing vendor manifest includes this package and its golden/test files; validate those bytes rather than merely citing the manifest. A source or snapshot transition requires coordinated Chain/Hepta/Nakama consumers, cross-language vectors and exact-tree qualification. This documentation change does not update vendor bytes or certify a real deployment.

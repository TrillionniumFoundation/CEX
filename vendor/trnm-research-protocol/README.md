# TRNM Research Protocol v1

`trnm-research-protocol` is the consensus-facing contract between Trillionnium
Chain, Hepta Research League, and Nakama. It is an internal Chain crate, not a
fourth top-level product module.

## Responsibility boundary

- Nakama may sign only `MatchEvidenceCommitmentV1`. It commits the completed
  match facts, roots, ruleset, dataset, and off-chain archive hash.
- Hepta may sign evaluation, workload, claim, license, challenge, and
  resolution commands.
- The state machine checks the signer DID, role, and Ed25519 public key against
  a genesis-derived `AuthoritySetV1`. Node ingress should independently enforce
  its capability policy as a first gate.
- Raw event streams, submissions, evaluations, reproductions, datasets, and
  research artifacts are never protocol fields. Only 32-byte commitments and
  bounded accounting/licensing metadata are on-chain.

Nakama establishes match facts. It cannot mint accepted research workload or a
research claim. Workload is valid only after an accepted Hepta evaluation.

## Consensus encoding

All consensus values use `rfc8949-deterministic-cbor-array-v1`:

- definite-length arrays only;
- shortest-form unsigned integers and lengths;
- no maps, floating-point values, or indefinite items;
- the first array item is always protocol version `1`;
- enum values are fixed unsigned integer discriminants;
- hashes and external keys are 32-byte CBOR byte strings.

`ResearchCommandV1::from_canonical_bytes` and
`SignedResearchCommandV1::from_canonical_bytes` reject unknown versions,
unknown discriminants, non-minimal forms, wrong lengths, trailing bytes, and
values that do not re-encode byte-for-byte identically.

The command wrapper is:

```text
[1, command_tag, typed_payload]
```

The signed envelope is:

```text
[
  1,
  "rfc8949-deterministic-cbor-array-v1",
  chain_id,
  command_id,
  signer_did,
  authority_role,
  nonce,
  ed25519_public_key,
  typed_command,
  ed25519_signature
]
```

The signing message is the same array without the signature and with array
length 9.

Golden CBOR, hashes, fingerprints, test keys, and deterministic signatures for
all seven command variants are in
`fixtures/protocol-v1-golden.json`.

## External keys

`ExternalKey` is a domain-separated 32-byte SHA-256 key. Canonical UUIDs are
hashed from their 16 raw bytes; other external IDs use strict visible ASCII.
The namespace and identifier kind are length-framed into the hash. Hepta and
Nakama therefore do not need a private UUID-to-integer mapping table.

## Object and state semantics

The protocol exposes immutable v1 match, evaluation, workload, license, and
resolution objects plus versioned claim/challenge objects. Claim resolution
keeps the originally signed claim payload immutable and updates a separate
current claimant allocation.

State transitions are:

```text
claim: Active/Amended/LicenseAmendmentRequired -> Challenged
challenge: Open -> Resolved
resolution:
  Uphold                  -> claim Active
  Reject                  -> claim Rejected
  AmendContributorShares  -> claim Amended
  RequireLicenseAmendment -> claim LicenseAmendmentRequired
```

`ResearchProtocolState::apply` is command-id idempotent. Repeating identical
signed bytes returns `Idempotent`; reusing a command ID with altered signed
bytes returns `AlteredReplay`.

For Chain storage and Merkle integration:

- `ApplyOutcome::changed_object_refs` lists every leaf changed by a command;
- `object_canonical_bytes` exports the current exact-version object leaf;
- `object_leaf_hash` applies the protocol leaf hash domain;
- `current_object_refs` provides stable `(kind, key)` enumeration;
- `canonical_snapshot_bytes/hash` cover the sorted full state;
- `export_snapshot/from_snapshot` provide serde persistence with graph
  validation on restore.

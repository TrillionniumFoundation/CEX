# Hepta Chain Contract Source v1

## Status

Hepta consumes four Chain-owned Rust crates from the canonical private Chain
repository at one immutable Git revision:

- repository: `https://github.com/TrillionniumFoundation/Trillionnium-Chain.git`
- source branch: `feature/chain-paper-raid-receipt-v2`
- revision: `4adfbadaa8c35cd3515f20381eb6b80d6885f457`
- root tree: `396ae6037b24037aff6983fd30d6baf906fda687`
- packages: `trnm-research-protocol`, `trnm-protocol`,
  `trnm-finality-types`, and `trnm-finality-verifier`, each exactly version
  `0.1.0`

The root workspace manifest is the single source of the local vendored paths,
and `vendor/trnm-chain-vendor-manifest.json` freezes the source branch, commit,
root tree, crate trees, file set, and byte hashes. Builds and release gates use
`--locked`.

## Boundary

Hepta does not read a sibling Chain worktree. The pinned crates provide the
Chain-native signed-command rules, canonical CBOR and typed transaction/object
references, AppHash receipt V2 wire types, trust-anchor fixtures, and canonical
finality verifier. The verifier returns the exact authenticated Research V1,
Paper Raid finality V2, or Paper Raid finality V3 domain command; callers must
dispatch on that type before mutating local state. These crates must not be
replaced by copied structs or Hepta's legacy local receipt verifier.

The local `/trnm/finality` and `/verify` wire format remains a separate legacy
adapter. `/trnm/finality/live` uses native Chain types and proof/QC semantics.
The legacy Paper finality V1 projection accepts only an exact Research V1
command already queued locally; Paper Raid V2/V3 commands belong to their
dedicated versioned lane and fail closed there. Consolidating the adapters is a
separate protocol migration.

## Release isolation

A clean Hepta release build uses only the checked-in vendored crates and does not
need Chain repository credentials or a sibling Chain checkout. Updating the
boundary is a separate reviewed operation that uses a repository-external fresh
clone, detaches at the reviewed commit, copies only the four immutable Chain
subtrees byte-for-byte, and refreshes their provenance manifest. Credentials
must never appear in Cargo manifests, lockfiles, logs, images, or repository
URLs.

## Verification

From the canonical Hepta root:

```bash
bash scripts/project-preflight.sh --dev
cargo deny check sources
cargo test --locked -p hepta-research-league
cargo check --locked --workspace
```

The PostgreSQL-backed release gate additionally requires
`HEPTA_TEST_DATABASE_URL` and runs the locked test/check commands.

Typed-domain evidence is deliberately combined. Chain verifier tests assemble
and cryptographically verify Research V1, Paper Raid V2, and Paper Raid V3
receipts. Hepta then sends valid signed V2/V3 domain commands through the same
post-verification memory and PostgreSQL pipeline called by the HTTP handler,
twice each, and compares the reachable Paper-finality state surfaces before and
after the exact lane-mismatch response. This is not presented as a V2/V3 HTTP
end-to-end fixture test. The repository Research V1 receipt fixture does run
end to end through HTTP and proves create, exact replay, and a queued-command
tamper rejection that cannot be bypassed by replay.

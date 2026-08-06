# Hepta Chain Contract Source v1

## Status

Hepta consumes four Chain-owned Rust crates from the canonical private Chain
repository at one immutable Git revision:

- repository: `https://github.com/TrillionniumFoundation/Trillionnium-Chain.git`
- revision: `f2e3da051effabc97c2d0e7c47acd1df3d0dd4aa`
- packages: `trnm-research-protocol`, `trnm-protocol`,
  `trnm-finality-types`, and `trnm-finality-verifier`, each exactly version
  `0.1.0`

The root workspace manifest is the single source of the local vendored paths,
and `vendor/trnm-chain-vendor-manifest.json` freezes the source commit, crate
trees, file set, and byte hashes. Builds and release gates use `--locked`.

## Boundary

Hepta does not read a sibling Chain worktree. The pinned crates provide the
Chain-native signed-command rules, canonical CBOR and typed transaction/object
references, AppHash receipt V2 wire types, trust-anchor fixtures, and canonical
finality verifier. They must not be replaced by copied structs or Hepta's legacy
local receipt verifier.

The local `/trnm/finality` and `/verify` wire format remains a separate legacy
adapter. `/trnm/finality/live` uses native Chain types and proof/QC semantics.
Consolidating those adapters is a separate protocol migration.

## Release isolation

A clean Hepta release build uses only the checked-in vendored crates and does not
need Chain repository credentials or a sibling Chain checkout. Updating the
boundary is a separate reviewed operation that copies the four immutable Chain
subtrees byte-for-byte and refreshes their provenance manifest. Credentials must
never appear in Cargo manifests, lockfiles, logs, images, or repository URLs.

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

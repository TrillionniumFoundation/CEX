# Hepta Chain Contract Source v1

## Status

Hepta consumes three Chain-owned Rust crates from the canonical private Chain
repository at one immutable Git revision:

- repository: `https://github.com/TrillionniumFoundation/Trillionnium-Chain.git`
- revision: `e73d1a930991f0e308bf72854b334b6191c7fcc3`
- packages: `trnm-research-protocol`, `trnm-finality-types`, and
  `trnm-finality-verifier`, each exactly version `0.1.0`

The root workspace manifest is the single source of this pin. Individual Hepta
services use workspace dependencies, and the committed `Cargo.lock` records the
resolved Git source and commit. Builds and release gates use `--locked`.

## Boundary

Hepta does not read a sibling Chain worktree. The pinned crates provide the
Chain-native signed-command rules, canonical CBOR and object references, finality
receipt wire types, and canonical finality verifier. They must not be replaced by
copied structs or Hepta's legacy local receipt verifier.

The local `/trnm/finality` and `/verify` wire format remains a separate legacy
adapter. `/trnm/finality/live` uses native Chain types and proof/QC semantics.
Consolidating those adapters is a separate protocol migration.

## Private-repository access

A clean build needs read-only access to the private Chain repository. Developer
machines may use their normal Git credential helper. CI must use a read-only
deploy credential that can fetch only the pinned repository; a token scoped only
to the Hepta repository is insufficient. Credentials must never appear in Cargo
manifests, lockfiles, logs, images, or repository URLs.

Cached builds may work offline after the exact revision has been fetched. A clean
offline machine cannot reconstruct a private Git dependency, so offline success
is not a substitute for the immutable source pin.

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

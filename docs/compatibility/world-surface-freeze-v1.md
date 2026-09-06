# World compatibility surface freeze v1

Status: active repository control  
Production authorization: `not_granted`

CEX is the Hepta control plane. It does not own World gameplay, map simulation, commerce, tactics, progression or authoritative game-server state. Those domains belong to `TrillionniumFoundation/Trillionnium-World`.

The existing World implementation embedded in `consumer-entry-api` and the broad Matrix command carrier are quarantined compatibility surfaces. `docs/compatibility/world-surface-freeze-v1.json` binds both complete crate trees, both complete recursive `src` trees and every one of their 30 tracked Rust source blobs. The explicit 13-file World/Matrix inventory remains as a classified subset, not as the discovery mechanism.

A change anywhere below either quarantined source root must update the exact Git-object manifest and be reviewed as either:

1. a security/defect correction that does not add domain scope; or
2. an extraction step that moves authority behind a versioned World-owned API.

The checker derives the complete recursive tracked inventory from Git, requires every Rust file to be reachable from a standard `lib.rs`, `main.rs` or `src/bin` entrypoint, and rejects inventory or blob drift. It does not rely on filenames containing `world`.

The following indirect source-entry mechanisms are forbidden in the quarantined crates:

- `#[path = ...]` module redirection;
- Rust `include!(...)` source inclusion;
- `OUT_DIR` generated Rust source;
- Cargo build scripts;
- untracked or unmanifested source files;
- direct sibling-repository Cargo path dependencies.

Hosted source-head and prospective-merge checks exercise hostile fixtures for nested World modules, neutral-stem modules, extra Matrix carriers, indirect inclusion, generated source, build scripts and sibling-repository paths.

## CEX target boundary

CEX may retain:

- authenticated channel ingress;
- request normalization and replay/rate-limit controls;
- versioned calls to World;
- read-only, source-versioned client projections;
- evidence links to Hepta, Ledger or TRNM.

CEX may not define or persist authoritative World map, combat, economy simulation, company, shop, work-order or progression state.

## Exit criteria

The freeze remains until a World-owned versioned adapter exists, cross-repository E2E evidence is recorded and the embedded source is deleted or permanently quarantined. Repository checks can close source-containment gaps; they cannot self-certify production migration, a durable single-writer cutover or deployment authorization.

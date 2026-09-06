# World compatibility surface freeze v1

Status: active repository control  
Production authorization: `not_granted`

CEX is the Hepta control plane. It does not own World gameplay, map simulation, commerce, tactics, progression or authoritative game-server state. Those domains belong to `TrillionniumFoundation/Trillionnium-World`.

The existing World implementation embedded in `consumer-entry-api` and the broad Matrix command carrier are quarantined compatibility surfaces. `docs/compatibility/world-surface-freeze-v1.json` binds both complete crate trees, both complete recursive `src` trees and every one of their 30 tracked Rust source blobs. The explicit 13-file World/Matrix inventory remains a classified subset, not the discovery mechanism.

A change anywhere below either quarantined source root must update the exact Git-object manifest and be reviewed as either:

1. a security or defect correction that does not add domain scope; or
2. an extraction step that moves authority behind a versioned World-owned API.

The source checker derives the complete recursive tracked inventory from Git, requires every Rust file to be reachable from a standard `lib.rs`, `main.rs` or `src/bin` entrypoint, and rejects inventory or blob drift. It does not rely on filenames containing `world`.

## Transitive local Cargo closure

Freezing only the two root source trees is insufficient because a normal, renamed, target-specific, development or build dependency could carry new authority from elsewhere in the repository. The boundary therefore closes the complete local Cargo graph reachable from the two quarantined roots.

The only permitted local packages are:

- `crates/shared-tracing`, classified as observability support only;
- `vendor/trnm-economy-protocol`, classified as the versioned economy protocol only;
- `services/ledger-service`, reachable only as a development/test fixture from `consumer-entry-api`;
- `crates/shared-config` and `crates/shared-types`, reachable only through the Ledger test fixture.

The manifest records every allowed edge by source package, dependency alias, resolved package identity, target package path, dependency kind, target selector and declaration source. Every reachable package is bound by exact package tree, Cargo manifest blob and source tree; Ledger tests are bound separately as well.

`scripts/check-project-boundary-cargo-closure.py` recursively resolves:

- ordinary dependencies;
- development dependencies;
- build dependencies;
- target-specific dependency tables;
- `workspace = true` declarations;
- renamed dependencies through their `package` identity.

Any extra or missing package or edge fails. Local proc-macro packages, implicit or explicit build scripts, workspace patch/replace tables, Cargo source replacement, `#[path]`, Rust `include!` and `OUT_DIR` generated Rust inputs are forbidden throughout the reachable local closure.

Hostile fixtures cover a new workspace carrier, an existing unquarantined carrier, renamed aliases, target/dev/build edges, a proc-macro helper, generated Rust, an out-of-root carrier and Cargo source replacement. The exact source head and prospective merge tree must both pass.

## Changed-path enforcement

`PROJECT_BOUNDARY.json::deny_changed_paths_regex` covers the workspace manifest, Cargo configuration, both quarantined package trees and every allowed local dependency package tree. `scripts/check-project-boundary-changed-paths.py` applies that policy to the actual base-to-head path set and refuses acceptance unless the transitive Cargo checker is also green. A controlled package or manifest change without a same-candidate boundary manifest update fails.

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

# Hepta Control Plane Boundary

- Project ID: `hepta-control-plane`
- Canonical repository: `TrillionniumFoundation/CEX`
- Lane: Hepta/CEX control plane
- Lifecycle: active
- Production authorization: `not_granted`

A developer's local directory is not repository authority. Scripts and documentation must use the discovered repository root rather than a machine-specific absolute path.

## Owns

- Hepta APIs and research/evaluation orchestration;
- identity, capability and invocation control;
- exact local settlement coordination and Audit evidence;
- SQL migrations owned by the Hepta control plane;
- versioned adapters at the Hepta boundary.

## Does not own

- Chain consensus, runtime, state, validators or finality implementation;
- World gameplay, maps, campaigns, economy simulation or game-server state;
- Nakama matchmaking, rooms, presence or authoritative match state;
- cross-repository production cutover authority.

## Compatibility quarantine

Existing embedded World and broad Matrix command surfaces are frozen in `docs/compatibility/world-surface-freeze-v1.json`. They may receive only reviewed security or defect corrections that add no domain scope, or extraction changes that remove authority from CEX. They may not gain new authoritative writers.

The quarantine is two-dimensional:

1. **Recursive source closure.** Both quarantined package trees, recursive Rust source inventories, standard module graphs and classified World/Matrix files are bound to exact Git objects.
2. **Transitive local Cargo closure.** Every local package reachable through ordinary, development, build, target-specific or renamed dependencies is resolved recursively and compared with an exact package-and-edge allowlist. Allowed support packages are bound by package tree, Cargo manifest and source tree.

The boundary rejects new or reused local authority carriers, package aliases, target/dev/build edges, local proc macros, build scripts, generated Rust inputs, Cargo source replacement and path dependencies that escape the repository. `PROJECT_BOUNDARY.json` is applied to the actual changed-path set, and source-head plus prospective-merge gates must both pass.

Sibling source trees must not be used through Cargo `path` dependencies. Publish or pin a versioned contract or artifact instead. The remote repository name `CEX` is retained as a compatibility identifier for the active Hepta control-plane repository; it does not expand the product boundary.

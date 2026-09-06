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
- cross-repository E2E harnesses and compatibility locks.

## Compatibility quarantine

Existing embedded World and broad Matrix command surfaces are frozen in `docs/compatibility/world-surface-freeze-v1.json`. They may receive only reviewed security/defect corrections or extraction changes. They may not gain new domain scope or new authoritative writers.

Sibling source trees must not be used through Cargo `path` dependencies. Publish or pin a versioned contract/artifact instead. The remote repository name `CEX` is retained as a compatibility identifier for the active Hepta control-plane repository; it does not expand the product boundary.

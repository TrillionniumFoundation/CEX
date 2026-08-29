# Hepta Control Plane Boundary

- Project ID: `hepta-control-plane`
- Canonical root: `/home/alex/projects/hepta-control-plane`
- Lane: Hepta/CEX control plane
- Canonical remote: `TrillionniumFoundation/CEX`

## Owns

Hepta APIs and services, research/evaluation orchestration, identity and
capability policy, settlement control, SQL migrations, and adapters at the
Hepta boundary.

## Does not own

- Chain consensus, runtime, state, validators, or finality implementation
- World gameplay, campaigns, economy simulation, or game-server state
- Nakama matchmaking, rooms, presence, or authoritative match state
- Cross-repository E2E harnesses and compatibility locks

Sibling source trees must not be used through Cargo `path` dependencies.
Publish or pin a versioned contract/artifact instead. The old `CEX` path is a
compatibility link, not a development root.

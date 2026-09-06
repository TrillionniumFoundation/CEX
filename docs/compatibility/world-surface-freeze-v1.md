# World compatibility surface freeze v1

Status: active repository control  
Production authorization: `not_granted`

CEX is the Hepta control plane. It does not own World gameplay, map simulation, commerce, tactics, progression or authoritative game-server state. Those domains belong to `TrillionniumFoundation/Trillionnium-World`.

The existing World implementation embedded in `consumer-entry-api` and the broad Matrix command carrier are frozen compatibility surfaces. Their exact Git blobs are recorded in `docs/compatibility/world-surface-freeze-v1.json`. A change to a frozen blob must update the manifest and be reviewed as either:

1. a security/defect correction that does not add domain scope; or
2. an extraction step that moves authority behind a versioned World-owned API.

New World source files, new authoritative writers and new direct sibling-repository Cargo path dependencies are forbidden.

## CEX target boundary

CEX may retain:

- authenticated channel ingress;
- request normalization and replay/rate-limit controls;
- versioned calls to World;
- read-only, source-versioned client projections;
- evidence links to Hepta, Ledger or TRNM.

CEX may not define or persist authoritative World map, combat, economy simulation, company, shop, work-order or progression state.

## Exit criteria

The freeze remains until a World-owned versioned adapter exists, cross-repository E2E evidence is recorded and the embedded source is deleted or quarantined. Repository checks can freeze expansion; they cannot self-certify completion of the cross-repository transfer.

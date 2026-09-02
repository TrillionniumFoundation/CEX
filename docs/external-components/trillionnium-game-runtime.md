# Trillionnium Game/Nakama runtime authority candidate

Status: active external-component contract  
Catalog ID: `trillionnium-game-runtime`  
Repository: `TrillionniumFoundation/TrillionniumGame`  
Workspace member: no  
Production evidence: external  
Production authorization: `not_granted`

## Purpose and authority

The pinned TrillionniumGame revision is the external authority candidate for Game/Nakama command processing, authoritative game storage and World-transition integration. It remains outside the CEX Cargo workspace and may become authoritative only within the exact deployed scope proven by its own repository controls and independent production evidence.

CEX remains authoritative only for its documented Hepta, exact Ledger, settlement, identity, Audit and edge boundaries. The Game runtime does not become CEX source code merely because a CEX workflow checks it out. CEX transport success does not prove that a Game command committed, that Nakama state advanced or that a downstream receipt reached Chain finality.

## Exact source and contract binding

The CEX-side source pin is declared in `evidence/world-authority-external-lock.json`. `.github/workflows/world-authority-external-evidence.yml` checks out the exact commit/tree, validates Game authority/storage scripts, runs Go tests, race checks, vet and builds bounded artifacts. `.github/workflows/world-settlement-external-evidence.yml` separately validates the pinned settlement integration and exact receipt relationships.

A moving branch/tag, local sibling path, stale workflow run or artifact from another commit cannot establish compatibility. Any source, protocol, database or command change creates a new integration candidate and requires updated lock, consumer fixtures, evidence and shared CEX trigger.

## Owned and non-owned state

Within an independently qualified deployment, the Game/Nakama runtime may own:

- authoritative Game command admission and ordering;
- game-session state, roster/presence and match-local storage assigned by its accepted architecture;
- durable World-transition commands and exact runtime receipts;
- runtime-local deduplication, fencing, recovery and audit evidence.

It does not own CEX Ledger balances, Hepta research truth, external Agent private keys, CEX identity/API-key governance or Trillionnium Chain consensus/finality. Projection rows and copied receipts never transfer those authorities.

## Interfaces and compatibility

All CEX/Game interactions require versioned contracts, stable command and idempotency identities, tenant/player binding, explicit body limits, authenticated service principals, canonical receipt bytes, timeout/unknown-outcome semantics and retirement conditions.

The World fixture is deterministic test input only. Promotion from fixture validation to authoritative online execution requires separate deployed-runtime evidence, database/storage recovery, network policy, credential custody, match drain/cutover and rollback proof.

## Failure, reconciliation and rollback

A timeout after a possible Game/Nakama side effect enters an unknown or reconciliation state. CEX may not repeat a command under a new identity, infer success from HTTP status or settle value without the complete owning receipt.

Rollback must preserve command/receipt identity and drain or fence active work before switching revisions. Restoring a prior binary without its compatible schema, protocol keys and storage state is not a valid rollback. Divergent CEX/Game/World receipt sets are a release blocker until independently explained and reconciled.

## Security and operational boundary

The Game runtime uses independent least-privilege credentials and must not share CEX Ledger administrator, Hepta issuer, Agent signing or Chain validator secrets. Build workflows receive read-only source access; deployment credentials belong to approved external custody. Public online, public market or commercial activation is denied until explicit independent evidence exists.

Required external evidence includes real runtime topology, persistent storage, multi-instance fencing, backup/restore, sustained load, credential rotation/revocation, incident/rollback drills and independent security/operations review. Repository-owned CEX Actions artifacts cannot self-certify those facts.

## Verification and evidence limits

CEX wiring is checked through:

```text
python3 scripts/check-module-documentation.py
python3 scripts/check-development-docs.py
```

The exact-source workflows prove only the checks recorded for the pinned source identities. They do not prove owner-repository branch protection, real Nakama deployment, representative-volume recovery, legal/commercial approval, public activation or final human go/no-go.

## Change protocol

Changes to Game authority, source pin, consumed protocol, storage responsibility, World role, settlement receipt or evidence workflow require this document, the module catalog/index, compatibility matrix, negative tests and a new shared candidate trigger. Production authorization remains `not_granted` until the immutable external evidence bundle and final human decision bind the exact qualified CEX and Game revisions.

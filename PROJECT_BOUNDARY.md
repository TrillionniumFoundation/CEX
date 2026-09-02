# Hepta Control Plane / CEX repository boundary

- Project ID: `hepta-control-plane`
- Canonical repository root: repository root containing this file
- Lane: Hepta/CEX control plane
- Canonical remote: `TrillionniumFoundation/CEX`
- Runtime policy: `external_only`
- Production authorization: `not_granted`

## Owns

This repository owns the bounded Hepta/CEX control-plane source assigned by the active module catalog: research and evaluation orchestration, tenant/identity and external-Agent capability declarations, exact off-chain Ledger and settlement control, Audit, SQL migrations, consumer/Matrix adapters and the contracts/evidence needed to integrate external authorities.

Execution Service owns durable execution-request, external-Agent work/evidence correlation, historical provider reconciliation and terminal settlement state. It does not execute participating Agents or host model inference. Capability Service owns only the validated read snapshot of external Agent declarations.

## Does not own

- participating Agent processes, models, prompts, inference execution, provider credentials or Agent private keys;
- Chain consensus, validator runtime, state transition implementation or finality authority;
- World gameplay, campaigns, economy simulation or authoritative World state;
- Trillionnium Game/Nakama matchmaking, rooms, presence, authoritative match state or external Game storage;
- Matrix homeserver accounts, rooms, event ordering, sync tokens or redaction authority;
- content-addressed object-store bytes outside the bounded digest/receipt contract;
- cross-repository production deployments, legal/commercial approvals or final human release authority.

## Integration rule

Sibling source trees must not be consumed through Cargo `path` dependencies, machine-specific absolute paths or unpinned moving branches. Publish or pin a versioned contract/artifact and declare every external authority or fixture in `docs/module-catalog-v1.json` and `docs/modules/index.md`.

The pinned Trillionnium-World source is a deterministic compatibility fixture only. The pinned TrillionniumGame/Nakama source is an external authority candidate and remains outside this Cargo workspace. Checking either repository from CEX CI does not transfer its authority into CEX or grant production activation.

## Change and qualification rule

A boundary change requires an accepted ADR, affected module/external-component contracts, machine-readable traceability, executable negative tests and a new shared candidate trigger. `scripts/check-external-agent-runtime-boundary.py` rejects reintroduction of local model execution or local capability discovery.

Source, documentation and CI can qualify one exact repository candidate only after real non-empty hosted execution. Protected-main governance, independent review, downstream immutable revision binding, external V12-X1 through V12-X8 evidence and final human go/no-go remain separate authorities.

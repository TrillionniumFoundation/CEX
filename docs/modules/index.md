# Workspace module documentation index

Status: active canonical module index  
Production authorization: `not_granted`

This index is the human-readable view of `../module-catalog-v1.json`. Cargo metadata is authoritative for actual workspace membership; the machine-readable catalog records the required documentation coverage; each module document is authoritative only within its stated source and ownership boundary.

## Completion rule

A repository candidate fails the development-document gate when any Cargo workspace member is absent from the catalog, a documented source entry point is missing, a module document lacks a required section, the catalog and `Cargo.toml` disagree, an external dependency used by source/workflows lacks a stable catalog identity, or a module document contradicts an accepted architecture decision.

The exact-tree integrity record binds the successful module-documentation result to the candidate commit/tree. Presence and word count alone do not establish technical truth; `scripts/check-external-agent-runtime-boundary.py` additionally checks the accepted external-Agent-only implementation boundary.

## Logical architecture

The product architecture has three top-level domains:

- **Hepta** owns research-control, signed external-Agent admission and bounded product-edge facts.
- **Nakama** is an external authoritative realtime/session runtime; it is not a Rust workspace member in this repository.
- **TRNM** owns Chain-facing economy/finality contracts. The CEX Ledger remains authoritative only for its exact off-chain Ledger state and must not claim Chain consensus finality.

Participating Agents, model providers, Trillionnium World fixtures and the Trillionnium Game runtime remain external. CEX may validate declarations, commands, receipts and evidence, but does not host or execute participating Agents. Shared crates provide types, configuration, tracing and error vocabulary but own no durable business state.

## Workspace modules

| Workspace member | Package | Kind | Logical module | Deployable | Owner role | Module contract |
|---|---|---|---|---:|---|---|
| `crates/shared-types` | `shared-types` | library | shared | no | `platform-foundations` | [shared-types.md](shared-types.md) |
| `crates/shared-errors` | `shared-errors` | library | shared | no | `platform-foundations` | [shared-errors.md](shared-errors.md) |
| `crates/shared-tracing` | `shared-tracing` | library | shared | no | `platform-foundations` | [shared-tracing.md](shared-tracing.md) |
| `crates/shared-config` | `shared-config` | library | shared | no | `platform-foundations` | [shared-config.md](shared-config.md) |
| `crates/hepta-paper-raid-contracts` | `hepta-paper-raid-contracts` | contract-library | hepta | no | `hepta-contracts` | [hepta-paper-raid-contracts.md](hepta-paper-raid-contracts.md) |
| `services/identity-service` | `identity-service` | service | hepta | yes | `identity-control-plane` | [identity-service.md](identity-service.md) |
| `services/ledger-service` | `ledger-service` | service | trnm | yes | `ledger-authority` | [ledger-service.md](ledger-service.md) |
| `services/trnm-economy-service` | `trnm-economy-service` | service | trnm | yes | `trnm-economy` | [trnm-economy-service.md](trnm-economy-service.md) |
| `services/gateway-service` | `gateway-service` | service | hepta | yes | `gateway-runtime` | [gateway-service.md](gateway-service.md) |
| `services/execution-service` | `execution-service` | service | hepta | yes | `execution-runtime` | [execution-service.md](execution-service.md) |
| `services/audit-service` | `audit-service` | service | hepta | yes | `audit-integrity` | [audit-service.md](audit-service.md) |
| `services/capability-service` | `capability-service` | service | hepta | yes | `capability-registry` | [capability-service.md](capability-service.md) |
| `services/consumer-entry-api` | `consumer-entry-api` | service | hepta | yes | `consumer-edge` | [consumer-entry-api.md](consumer-entry-api.md) |
| `services/hepta-research-league` | `hepta-research-league` | service | hepta | yes | `hepta-research` | [hepta-research-league.md](hepta-research-league.md) |
| `services/paper-raid-bff` | `paper-raid-bff` | service | hepta | yes | `paper-raid-edge` | [paper-raid-bff.md](paper-raid-bff.md) |
| `services/matrix-entry-adapter` | `matrix-entry-adapter` | adapter-service | hepta | yes | `matrix-integration` | [matrix-entry-adapter.md](matrix-entry-adapter.md) |
| `apps/matrix-bot-relay` | `matrix-bot-relay` | application | hepta | yes | `matrix-integration` | [matrix-bot-relay.md](matrix-bot-relay.md) |
| `apps/matrix-bot-poller` | `matrix-bot-poller` | application | hepta | yes | `matrix-integration` | [matrix-bot-poller.md](matrix-bot-poller.md) |
| `vendor/trnm-economy-protocol` | `trnm-economy-protocol` | contract-library | trnm | no | `trnm-integration` | [trnm-economy-protocol.md](trnm-economy-protocol.md) |
| `vendor/trnm-finality-types` | `trnm-finality-types` | contract-library | trnm | no | `trnm-integration` | [trnm-finality-types.md](trnm-finality-types.md) |
| `vendor/trnm-finality-verifier` | `trnm-finality-verifier` | library | trnm | no | `trnm-integration` | [trnm-finality-verifier.md](trnm-finality-verifier.md) |
| `vendor/trnm-protocol` | `trnm-protocol` | contract-library | trnm | no | `trnm-integration` | [trnm-protocol.md](trnm-protocol.md) |
| `vendor/trnm-research-protocol` | `trnm-research-protocol` | contract-library | trnm | no | `trnm-integration` | [trnm-research-protocol.md](trnm-research-protocol.md) |

All links in this table are relative to `docs/modules/`; the machine catalog retains the full repository path for each document.

## External authoritative and integration components

External authorities and pinned integration dependencies have stable catalog IDs so automated checks do not depend on capitalization, repository aliases or display wording. None is a Cargo workspace member.

| Catalog ID | Human authority or role | Owned boundary | Supporting contract |
|---|---|---|---|
| `nakama` | Nakama | ordered realtime session events, roster epochs, checkpoints and signed completion receipts | `docs/hepta-paper-raid-state-machines-v1.md` |
| `trillionnium-chain` | Trillionnium Chain | consensus/finality, challenges, resolutions and immutable Chain receipts | `docs/protocol/version-compatibility-matrix-v1.md` |
| `matrix-homeserver` | Matrix homeserver | Matrix accounts, rooms, opaque sync tokens, redactions and source event transport | `docs/consumer-entry-matrix-architecture-v1.md` |
| `content-addressed-object-store` | Content-addressed object store | immutable research/provider artifact bytes addressed by verified digests | `docs/security-threat-model-v1.md` |
| `external-providers-and-agents` | Independently operated providers and Agents | Agent execution, model/provider credentials and Agent signing keys; CEX retains only bounded declarations, commands, receipts, commitments and evidence | `docs/hepta-agent-protocol-v1.md` |
| `trillionnium-world-fixture` | Pinned Trillionnium-World fixture | deterministic transition and compatibility input only; no CEX/Game/Nakama/Ledger/Chain authority | `docs/external-components/trillionnium-world-fixture.md` |
| `trillionnium-game-runtime` | Pinned TrillionniumGame/Nakama authority candidate | external Game command, storage and World-transition runtime subject to independent qualification | `docs/external-components/trillionnium-game-runtime.md` |

## Change protocol

New workspace members require a catalog entry and module document in the same commit. Removing or renaming a member requires an explicit compatibility and data-retirement decision. Every new binary or integration test used for qualification must be represented by a catalog source entry point.

External IDs are stable protocol names. Renaming, adding or changing one requires the catalog, index, external-component contract, threat-model/compatibility material, checker and consumer migration in the same candidate. A source, workflow, documentation-only or test-only change creates a new exact tree, advances the shared candidate trigger and must be requalified. No catalog or module document may grant production authorization.

## Actual Cargo membership and vendored packages

The explicit list now names 23 packages: the existing 18 first-party members and
five vendored path dependencies. This is not a new Chain service or fourth product
domain. The Chain runtime remains external; only the copied libraries and the
Unix verifier utility are local build targets. Vendored source and its provenance
manifest are not rewritten by this documentation correction. The economy crate
is imported as `term-exchange-protocol` but its package name is
`trnm-economy-protocol`; aliases cannot create or erase catalog identities.

`python3 scripts/check-cargo-workspace-authority.py` invokes real Cargo metadata
with the committed lock, compares actual members to the explicit list and catalog,
and rejects an undocumented automatic binary, test, example, benchmark or build
script target. It is required after toolchain setup in both existing Linux and
Windows Rust authority lanes. The existing full document, semantic inventory,
compilation and release gates remain separate requirements. No saved metadata
file can be supplied as a substitute for that invocation. See
`../cargo-workspace-authority-v1.md` for source-only versus executed evidence.

## Repository-adjacent tool boundary

`tools/paper-raid-agent-bridge` is a Node.js operational bridge and is intentionally outside the
Cargo workspace and the 23-member module catalog. It is not part of the default CEX release
candidate. If a future release makes it a shipped component, the change must add a machine
catalog entry, a dedicated contract covering its daemon/CLI protocol, key custody, upgrade and
rollback, and corresponding verification gates before it is treated as a module.

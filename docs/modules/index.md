# Workspace module documentation index

Status: active canonical module index  
Production authorization: `not_granted`

This index is the human-readable view of `../module-catalog-v1.json`. The
machine-readable catalog is authoritative for workspace membership and
documentation coverage; each module document is authoritative only within its
stated source and ownership boundary.

## Completion rule

A repository candidate fails the development-document gate when any Cargo
workspace member is absent from the catalog, a documented source entry point is
missing, a module document lacks a required section, or the catalog and
`Cargo.toml` disagree. The exact-tree integrity record binds the successful
module-documentation result to the candidate commit/tree.

## Logical architecture

The product architecture has three top-level domains:

- **Hepta** owns research-control and product-edge facts.
- **Nakama** is an external authoritative realtime/session runtime; it is not a
  Rust workspace member in this repository.
- **TRNM** owns Chain-facing economy/finality contracts. The CEX Ledger remains
  authoritative only for its exact off-chain Ledger state and must not claim
  Chain consensus finality.

Shared crates provide types, configuration, tracing, and error vocabulary but
own no durable business state.

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

All links in this table are relative to `docs/modules/`; the machine catalog
retains the full repository path for each document.

## External authoritative components

External authorities have stable catalog IDs so automated checks do not depend
on capitalization or display wording. None is a Cargo workspace member.

| Catalog ID | Human authority | Owned boundary | Supporting contract |
|---|---|---|---|
| `nakama` | Nakama | ordered realtime session events, roster epochs, checkpoints, and signed completion receipts | `docs/hepta-paper-raid-state-machines-v1.md` |
| `trillionnium-chain` | Trillionnium Chain | consensus/finality, challenges, resolutions, and immutable Chain receipts | `docs/protocol/version-compatibility-matrix-v1.md` |
| `matrix-homeserver` | Matrix homeserver | Matrix accounts, rooms, opaque sync tokens, and source event transport | `docs/consumer-entry-matrix-architecture-v1.md` |
| `content-addressed-object-store` | Content-addressed object store | immutable research/provider artifact bytes addressed by verified digests | `docs/security-threat-model-v1.md` |
| `external-providers-and-agents` | External providers and Agents | independently operated execution endpoints and signing keys; CEX retains only bounded contracts and evidence | `docs/security-threat-model-v1.md` |

## Change protocol

New workspace members require a catalog entry and module document in the same
commit. Removing or renaming a member requires an explicit compatibility and
data-retirement decision. External authority IDs are stable protocol names;
renaming one requires a catalog, index, threat-model/compatibility, checker, and
consumer migration in the same candidate. A documentation-only change still
creates a new candidate tree and must be requalified.

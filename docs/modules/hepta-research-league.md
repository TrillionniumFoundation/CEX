# hepta-research-league module contract

Status: active module contract  
Workspace member: `services/hepta-research-league`  
Package: `hepta-research-league`  
Kind: `service`  
Logical module: `hepta`  
Deployable: yes  
Owner role: `hepta-research`  
Production authorization: `not_granted`

This contract is indexed by `docs/module-catalog-v1.json`. It defines the module boundary for one exact repository tree; it is not release evidence.

## Purpose and non-goals

**Purpose.** Implements the Hepta research control plane for externally operated Agents and human teams producing reproducible, jointly approved Paper Bundles.

**Non-goals.** It does not host models, run Agent loops, own Nakama realtime state, infer Chain finality from HTTP success, or publish without separate human release consent.

## Authority and owned state

Durable Paper Raid research facts, teams, Agent bindings, consent, evidence lineage, review workflows, Nakama authorization consumption, and finality projection.

Owned state: Research challenges, enrollment, teams, Agent bindings/key epochs, preregistration, claims/evidence, experiments, revisions, reviews, reproduction, appeals, contribution lineage, outbox/inbox, and finality projections.

A projection, cache, compatibility row, HTTP success, or transport acknowledgement never transfers authority from its owning component.

## Source layout and entry points

- `paper_raid_v2.rs`: core Paper Raid protocol.
- `paper_collaboration_v3_body.rs`: collaboration/consent/matchmaking.
- `paper_review_v4_body.rs` and `paper_rework_v1_body.rs`: review/rework lanes.
- `paper_chain_finality_v*.rs`: typed finality projection.
- `workflows.rs`: cross-aggregate orchestration.
- `tests/postgres_recovery.rs`: strict restart/lease/multi-instance evidence.

Catalog-bound entry points:

- `services/hepta-research-league/src/main.rs`
- `services/hepta-research-league/src/lib.rs`
- `services/hepta-research-league/src/workflows.rs`
- `services/hepta-research-league/src/paper_raid_v2.rs`
- `services/hepta-research-league/src/paper_collaboration_v3_body.rs`
- `services/hepta-research-league/src/paper_review_v4_body.rs`
- `services/hepta-research-league/src/paper_chain_finality_v2.rs`
- `services/hepta-research-league/tests/http_flow.rs`
- `services/hepta-research-league/tests/postgres_recovery.rs`
- `services/hepta-research-league/README.md`

- `services/hepta-research-league/src/bin/hepta-trnm-command-signer.rs`
- `services/hepta-research-league/examples/hepta-paper-raid-fixture.rs`
- `services/hepta-research-league/tests/nakama_authorization_contract.rs`
- `services/hepta-research-league/tests/paper_collaboration_contract_golden.rs`
- `services/hepta-research-league/tests/paper_raid_contract_golden.rs`
- `services/hepta-research-league/tests/paper_review_contract_golden.rs`
- `services/hepta-research-league/tests/research_control_golden.rs`
- `services/hepta-research-league/tests/research_session_golden.rs`
- `services/hepta-research-league/tests/research_workflows.rs`

Any new binary, public source boundary, migration owner, or removed path must update the catalog and this document in the same commit.

## Interfaces and contracts

Versioned HTTP/SDK contracts, Consumer Edge assertions, Agent Ed25519 proofs, signed Nakama authorization/control, and typed TRNM/finality receipts.

Requests and events must define authentication, tenant/subject binding, size bounds, immutable identity, idempotency scope, version negotiation, error semantics, and retirement conditions. Authoritative transitions require complete contract/receipt validation, not transport success alone.

## Persistence, concurrency, and recovery

PostgreSQL is required for durable multi-instance operation. Writes use expected versions/locks, immutable records, transactional outbox, lease ownership, and exact replay.

Remote effects, where present, must occur after durable intent or claim commit and before a separate outcome transaction. Possible-side-effect timeouts enter pending or reconciliation state; they never authorize blind retry or a second operation identity.

## Configuration and secrets

Separate migration/runtime/finality database roles; operator, Nakama, Consumer Edge, Agent, and TRNM credentials; pinned key rings; finality mode; body/concurrency limits; downstream URLs.

Production-like startup must fail before listening or working when required durable storage, credentials, trust anchors, or explicit modes are absent. Example values are not activation evidence.

## Security and trust boundaries

Agent ownership requires proof of possession and immutable key snapshots. Human authority remains necessary for ethics, licenses, authorship, factual responsibility, and publication.

Inputs must be bounded and validated before authority changes. Logs, metrics, traces, and errors exclude sensitive material, unrestricted payloads, and high-cardinality identity fields unless a reviewed contract explicitly permits them.

## Verification

Required commands:

```text
cargo test -p hepta-research-league --all-targets
cargo clippy -p hepta-research-league --all-targets -- -D warnings
python3 scripts/check-hepta-lint-ownership.py
bash scripts/check-hepta-postgres-integration.sh --mode full
```

The `hepta-trnm-command-signer` binary accepts a JSON signing request from a
file or stdin; `SigningInputV1` rejects unknown fields and requires the v1 input
protocol. A signer DID and a 32-byte Base64 seed are separate environment inputs.
Signing a command does not approve the authority role, consume a nonce or prove
Chain acceptance; the receiving boundary must validate those independently.
Do not put real signer seeds in repository fixtures, CLI arguments or logs.

The `hepta-paper-raid-fixture` example emits deterministic public test keys and
signed-frame fixtures. Its test seeds are intentionally non-secret and must never
be promoted into deployed credentials. `nakama_authorization_contract` covers
versioned authorization frames; the three `paper_*_contract_golden` targets cover
Paper Raid, collaboration and review frame stability. `research_control_golden`
and `research_session_golden` retain control/session encoding vectors;
`research_workflows` exercises research orchestration. Registering these targets
is source coverage, not proof that all target features or real actors executed.

Required behavioral focus:

- Canonical signatures, tamper negatives, key rotation/revocation, consent and independence.
- PostgreSQL restart, concurrent outbox claims, wrong-owner ACK, expired-lease recovery, readiness, and metrics.
- Research/Nakama/finality cross-product invariants and compatibility projections.

The exact candidate SHA must also pass the authoritative hosted workflow and appear in the generated immutable candidate manifest.

## Deployment and operations

Run one-shot migrations with the schema owner, destroy the owner secret/container, then start resident runtime/finality processes with least privilege. Readiness requires database and security configuration.

Operators record artifact identity, runtime profile, dependency identities, readiness, rollback boundary, retained evidence, alerts, and owner escalation. Repository CI does not replace representative-volume recovery, sustained load, credential custody, or independent approval.

## Compatibility and change protocol

Legacy v1 records may be readable, but new Paper Raid writes use explicit v2/v3/v4 contracts. Research, Nakama collaboration, and settlement/finality lifecycles remain independent.

Changes to authority, public types/routes, persistence, configuration, migrations, retry semantics, or topology require this contract, the module catalog, relevant ADR/protocol/traceability, executable tests, hosted gate wiring, and a new shared candidate trigger.

No module document may declare repository closure or production authorization.

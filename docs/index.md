# CEX development-document authority index

Status: active

This file is the canonical entry point for development, qualification, and operator documentation. The repository root `readme.md` is a non-normative navigation page and cannot establish current status, qualification, or production authorization.

## Authority order

When two sources appear to disagree, use the following precedence:

1. immutable exact-commit release evidence and the generated candidate manifest within the scope they actually prove;
2. accepted ADRs and versioned protocol specifications, including ADR-004's external-Agent-only product boundary;
3. `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`, its active implementation addendum, and the Sequence 52 architecture closure contract, which may implement but may not silently supersede an accepted ADR;
4. component status, state-machine, threat-model, acceptance, SLO, compatibility, module-contract, evidence-intake, and runbook documents;
5. historical plans, archived evidence, research notes, and examples.

Source presence, prose, a template, a local command result, or a green run on another SHA never overrides exact-tree evidence. An implementation plan cannot reintroduce a capability explicitly rejected by an accepted ADR without a new accepted superseding decision.

## Active authority set

- Architecture decision: `../decisions/adr-004-three-module-external-agent-battle-platform.md`
- Sequence 52 architecture closure: `architecture/external-agent-runtime-boundary-sequence-52.md`
- Sequence 52 architecture traceability: `traceability/sequence-52-architecture-v1.json`
- Plan: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`
- Implementation addendum: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md`
- Machine-readable authority: `development-doc-authority-v1.json`
- Machine-readable requirement traceability: `traceability/v12-requirements-v1.json`
- Component status: `status/component-status-v1.md`
- Hepta external Agent protocol: `hepta-agent-protocol-v1.md`
- Hepta state machines: `hepta-paper-raid-state-machines-v1.md`
- Security threat model: `security-threat-model-v1.md`
- Clean-deployment acceptance: `clean-deployment-acceptance-v1.md`
- SLO and recovery contract: `slo-recovery-contract-v1.md`
- Protocol compatibility matrix: `protocol/version-compatibility-matrix-v1.md`
- TRNM production credential contract: `trnm-production-credential-contract-v1.md`

The executable architecture boundary is `scripts/check-external-agent-runtime-boundary.py`. It is called by `scripts/check-development-docs.py` and rejects a default in-process provider route, local inference execution, local model discovery, production activation of the legacy compatibility feature, or documentation that transfers external Agent authority into CEX.

## Parent-plan architecture correction

The parent v12 plan remains normative for exact Ledger, Gateway, Execution lifecycle, Audit, migration, recovery, and evidence controls. Its provider-specific Ollama/OpenClaw wording is historical compatibility scope under accepted ADR-004. It may be used to verify migration-0088 historical rows and negative evidence handling, but it does not authorize a current CEX-hosted inference path. The active replacement is the signed external-Agent protocol and the Sequence 52 closure contract. The retired `/v1/executions/:id/process` route is absent from the default router.

## Workspace module contracts

The Cargo workspace is documented through two active supporting contracts:

- Machine-readable module catalog: `docs/module-catalog-v1.json`
- Human module index: `docs/modules/index.md`

Every Cargo member must appear exactly once in the catalog and have one dedicated document under `docs/modules/`. Nakama, Trillionnium Chain, Matrix homeserver, content-addressed object storage, external providers/Agents, Trillionnium World fixtures, and the Trillionnium Game runtime remain external authorities or integrations and must not be invented as local Cargo members.

The module catalog is verified by `scripts/check-module-documentation.py`; that checker is executed by `scripts/check-development-docs.py`. The exact repository identity and successful documentation result are recorded by `scripts/check-repository-integrity.py`.

## Semantic tooling implementation guide

`semantic-route-extraction-v2.md` documents bounded explicit Rust route extraction, unresolved declarations, consistent input snapshots, Consumer projection checks and generated-inventory review. It supports Blocks H and K without changing their authority. Passing its synthetic/source tests does not prove complete module documentation or repository qualification; the complete current generated inventory and exact-tree gates remain required.

## Qualification source and packet integrity

`qualification-source-integrity-v1.md` defines read-only qualification inputs and the TRNM closed build-packet profile. The obsolete Sequence 53 source-writing workflows are retired; committed locks and generated inventories must be prepared before qualification. LICENSE, SECURITY.md, CONTRIBUTING.md and CHANGELOG.md provide repository policy/navigation, not release authority or proof of branch enforcement. None of these files grants production authorization.

## Runner probe and qualification evidence

The bounded desktop and fleet workflows are manual, no-checkout connectivity probes. A runner probe definition is not execution evidence. A queued job, `runner_id=0`, an empty runner identity, `steps=[]`, missing logs, or an artifact from a different SHA provides no repository-qualification credit. Qualification requires non-empty execution of every required context on one unchanged candidate and a generated immutable candidate manifest.

## External production evidence intake

The repository defines a shape and anti-self-certification boundary for the eight external production gates:

- Contract: `docs/external-production-evidence-contract-v1.md`
- Shape-only template: `docs/templates/cex-external-production-evidence-bundle-v1.json`
- Structural checker: `scripts/check-external-production-evidence-contract.py`

The source tree may validate only the contract and empty template. Real evidence remains in approved external custody. Structural bundle validation cannot verify that an issuer is genuinely independent and cannot grant production authorization. V12-X1 through V12-X8 remain `blocked_upstream` until the responsible external actors issue and accept immutable evidence for one exact qualified candidate.

## Lifecycle labels

- `active`: normative for current repository work.
- `accepted`: architectural or protocol decision that remains binding within its scope.
- `operational`: executable runbook or acceptance procedure.
- `historical`: retained for audit only; not a source of current status.
- `template`: shape-only input; never release evidence.
- `module contract`: the technical boundary for one Cargo member; not release evidence by itself.
- `external evidence contract`: structural intake policy; never proof that the external activity or approval occurred.
- `legacy compatibility`: retained source/data needed for upgrade or audit, excluded from default production authority.

## Production authorization boundary

Repository qualification may establish that source, migrations, tests, workflows, documentation, architecture boundaries, and generated evidence are internally consistent on one commit/tree. It cannot authorize production. Representative-volume disaster recovery, real deployment and rollback, real external Agent/provider evidence, secret custody, sustained soak, independent reviews, legal/commercial approvals, protected-branch administration, and final human go/no-go remain externally evidenced gates. The production-authorization value remains `not_granted` until those independent records and the final human decision exist.

## Change protocol

A change to any active authority or workspace member must:

1. preserve the explicit `not_granted` production-authorization posture;
2. update the machine-readable authority or traceability document when normative scope changes;
3. update the affected module-catalog entry and dedicated module contract when membership, ownership, interfaces, persistence, configuration, verification, or deployment changes;
4. preserve ADR-004 unless a new accepted ADR explicitly supersedes it;
5. update the external evidence contract, template, and checker together when evidence shape or gate ordering changes;
6. pass `scripts/check-external-agent-runtime-boundary.py`, `scripts/check-module-documentation.py`, `scripts/check-external-production-evidence-contract.py --contract-only`, and `scripts/check-development-docs.py`;
7. pass repository-integrity attestation on the exact tree;
8. update the sole shared candidate trigger and rerun all authoritative v12 hosted gates before repository qualification.

## Matrix scope upgrade

`matrix-stream-scope-v1.md` specifies migration-0004 immutable stream identity,
token-owner checks, legacy-cursor upgrade holds and the narrowed plain-reply
contract. It is supporting implementation documentation, not release evidence.


## Matrix SQL execution safety

`matrix-sql-runner-v4.md` defines the unified legacy/current test entrypoints,
single-session identity and reset controls, retained original assertions, v4
observation report and its explicit non-qualification boundary.

## Matrix ID filter execution

`matrix-filter-definition-v1.md` defines the migration-0005 immutable definition
pin and reviewed upgrade. It supersedes ordinary unresolved-ID sync and keeps
definition acquisition, content validation and exact-tree qualification separate.

## Actual Cargo workspace authority

`cargo-workspace-authority-v1.md` closes the explicit-member-only inventory blind
spot: five existing vendored path packages are named in the root workspace,
catalog and dedicated module contracts. Real `cargo metadata --locked --no-deps`
must agree with that source inventory and document every discovered target. This
is not a claim that the complete repository has passed or that a Chain runtime
has moved into CEX. Vendor provenance, canonical protocol compatibility, generated
semantic inventory, runtime checks and independent release gates are preserved.

## Vendor identity diagnosis

`status/vendor-provenance-reconciliation-round14.md` records the exact one-file
verifier divergence and introducing commit. It does not waive the original
vendor policy or claim source/runtime qualification.

## Shared Matrix profile implementation

`matrix-profile-sharing-v1.md` records consolidation into the existing
shared-config crate, unchanged profile semantics, retained startup boundaries and
remaining embedded-library/runtime acceptance. It is not execution evidence.

`status/matrix-profile-consolidation-round15.json` records the current parser
delta; other gaps remain in the parent audit-remediation snapshot.

# CEX development-document authority index

Status: active

This file is the canonical entry point for development, qualification, and operator documentation. The repository root `readme.md` is a non-normative navigation page and cannot establish current status, qualification, or production authorization.

## Authority order

When two sources appear to disagree, use the following precedence:

1. immutable exact-commit release evidence and the generated candidate manifest;
2. `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md` together with its active implementation addendum;
3. accepted ADRs and versioned protocol specifications;
4. component status, state-machine, threat-model, acceptance, SLO, compatibility, module-contract, evidence-intake, and runbook documents;
5. historical plans, archived evidence, research notes, and examples.

Source presence, prose, a template, or a green run on another SHA never overrides exact-tree evidence.

## Active authority set

- Plan: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`
- Implementation addendum: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md`
- Machine-readable authority: `development-doc-authority-v1.json`
- Machine-readable requirement traceability: `traceability/v12-requirements-v1.json`
- Component status: `status/component-status-v1.md`
- Hepta state machines: `hepta-paper-raid-state-machines-v1.md`
- Security threat model: `security-threat-model-v1.md`
- Clean-deployment acceptance: `clean-deployment-acceptance-v1.md`
- SLO and recovery contract: `slo-recovery-contract-v1.md`
- Protocol compatibility matrix: `protocol/version-compatibility-matrix-v1.md`
- TRNM production credential contract: `trnm-production-credential-contract-v1.md`

## Workspace module contracts

The Cargo workspace is documented through two active supporting contracts:

- Machine-readable module catalog: `docs/module-catalog-v1.json`
- Human module index: `docs/modules/index.md`

Every Cargo member must appear exactly once in the catalog and have one dedicated document under `docs/modules/`. Nakama, Trillionnium Chain, Matrix homeserver, content-addressed object storage, external providers, and external Agents remain external authorities and must not be invented as local Cargo members.

The module catalog is verified by `scripts/check-module-documentation.py`; that checker is executed by `scripts/check-development-docs.py`. The exact repository identity and successful documentation result are recorded by `scripts/check-repository-integrity.py`.

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

## Production authorization boundary

Repository qualification may establish that source, migrations, tests, workflows, documentation, and generated evidence are internally consistent on one commit/tree. It cannot authorize production. Representative-volume disaster recovery, real deployment and rollback, provider evidence, secret custody, sustained soak, independent reviews, legal/commercial approvals, and final human go/no-go remain externally evidenced gates. The production-authorization value remains `not_granted` until those independent records and the final human decision exist.

## Change protocol

A change to any active authority or workspace member must:

1. preserve the explicit `not_granted` production-authorization posture;
2. update the machine-readable authority or traceability document when normative scope changes;
3. update the affected module-catalog entry and dedicated module contract when membership, ownership, interfaces, persistence, configuration, verification, or deployment changes;
4. update the external evidence contract, template, and checker together when evidence shape or gate ordering changes;
5. pass `scripts/check-module-documentation.py`, `scripts/check-external-production-evidence-contract.py --contract-only`, and `scripts/check-development-docs.py`;
6. pass repository-integrity attestation on the exact tree;
7. trigger all authoritative v12 hosted gates through the shared candidate trigger before repository qualification.

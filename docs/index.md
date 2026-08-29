# CEX development-document authority index

Status: active

This file is the canonical entry point for development, qualification, and operator documentation. The repository root `readme.md` is deliberately not part of the normative development-document chain.

## Authority order

When two sources appear to disagree, use the following precedence:

1. immutable exact-commit release evidence and the generated candidate manifest;
2. `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md` together with its active implementation addendum;
3. accepted ADRs and versioned protocol specifications;
4. component status, state-machine, threat-model, acceptance, SLO, compatibility, and runbook documents;
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

The active set is validated by `scripts/check-development-docs.py`. The exact repository identity and authority-set digests are emitted by `scripts/check-repository-integrity.py`.

## Lifecycle labels

- `active`: normative for current repository work.
- `accepted`: architectural or protocol decision that remains binding within its scope.
- `operational`: executable runbook or acceptance procedure.
- `historical`: retained for audit only; not a source of current status.
- `template`: shape-only input; never release evidence.

## Production authorization boundary

Repository qualification may establish that source, migrations, tests, workflows, and generated evidence are internally consistent on one commit/tree. It cannot authorize production. Representative-volume disaster recovery, real deployment and rollback, provider evidence, secret custody, sustained soak, independent reviews, legal/commercial approvals, and final human go/no-go remain externally evidenced gates. The production-authorization value remains `not_granted` until those independent records and the final human decision exist.

## Change protocol

A change to any active authority document must:

1. preserve the explicit `not_granted` production-authorization posture;
2. update the machine-readable authority or traceability document when scope changes;
3. pass the documentation contract checker;
4. pass repository-integrity attestation on the exact tree;
5. trigger all authoritative v12 hosted gates through the shared candidate trigger before repository qualification.

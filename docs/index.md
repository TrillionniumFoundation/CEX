# CEX development-document authority index

Status: active

This file is the canonical entry point for development, qualification, and operator documentation. The repository root `readme.md` is deliberately not part of the normative development-document chain.

## Authority order

When two sources appear to disagree, use the following precedence:

1. immutable exact-commit release evidence and the generated candidate manifest;
2. `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md` together with its active implementation addendum;
3. accepted ADRs and versioned protocol specifications;
4. component status, workspace module contracts, blocker posture, state-machine, threat-model, acceptance, SLO, compatibility, boundary, governance and runbook documents;
5. historical plans, archived evidence, research notes, and examples.

Source presence, prose, a template, or a green run on another SHA never overrides exact-tree evidence.

## Active authority set

- Plan: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`
- Implementation addendum: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md`
- Machine-readable authority: `development-doc-authority-v1.json`
- Base requirement traceability: `traceability/v12-requirements-v1.json`
- P0 blocker traceability: `traceability/v12-p0-blockers-v1.json`
- Module catalog: `module-catalog-v1.json`
- Module documentation standard: `module-documentation-standard-v1.md`
- Component status: `status/component-status-v1.md`
- Hepta state machines: `hepta-paper-raid-state-machines-v1.md`
- Security threat model: `security-threat-model-v1.md`
- Clean-deployment acceptance: `clean-deployment-acceptance-v1.md`
- SLO and recovery contract: `slo-recovery-contract-v1.md`
- Protocol compatibility matrix: `protocol/version-compatibility-matrix-v1.md`
- Capability production posture: `capability-production-posture-v1.md`
- Authoritative money isolation: `authoritative-money-isolation-v1.md`
- Project boundary: `../PROJECT_BOUNDARY.md`
- World compatibility freeze: `compatibility/world-surface-freeze-v1.md`
- Repository governance: `repository-governance-policy-v1.md`

Each active Cargo workspace member is represented in the module catalog and has one colocated `<member>/MODULE.md`. `scripts/check-module-documentation.py` validates exact workspace/catalog/document equality. Capability, exact-money, World boundary and source governance are executable contracts, not prose-only claims. The complete active set is validated by `scripts/check-development-docs.py`. The exact repository identity and authority-set digests are emitted by `scripts/check-repository-integrity.py`.

## Lifecycle labels

- `active`: normative for current repository work.
- `accepted`: architectural or protocol decision that remains binding within its scope.
- `operational`: executable runbook or acceptance procedure.
- `supporting_alpha`: executable supporting surface whose production qualification remains incomplete.
- `experimental`: non-authoritative implementation that cannot be promoted without an explicit decision.
- `deprecated`: compatibility-only surface with a documented retirement path.
- `historical`: retained for audit only; not a source of current status.
- `template`: shape-only input; never release evidence.

## Production authorization boundary

Repository qualification may establish that source, migrations, tests, workflows, generated evidence, module documentation, Capability authority, exact-money isolation and compatibility boundaries are internally consistent on one commit/tree. It cannot authorize production or fabricate repository-administration state. Representative-volume disaster recovery, real deployment and rollback, provider evidence, secret custody, sustained soak, independent reviews, legal/commercial approvals, actual `main` Ruleset enforcement, cross-repository World authority transfer and final human go/no-go remain independently evidenced gates. The production-authorization value remains `not_granted` until those records and the final human decision exist.

## Change protocol

A change to any active authority document, Cargo workspace member, exact-money file, frozen World surface or protected governance path must:

1. preserve the explicit `not_granted` production-authorization posture;
2. update the machine-readable authority, traceability, module catalog or compatibility inventory when scope changes;
3. update the owning module contract for interface, state, security, recovery or compatibility changes;
4. pass the module, Capability, money-isolation, project-boundary, source-governance and development-document contract checkers;
5. pass repository-integrity attestation on the exact tree;
6. trigger all authoritative v12 hosted gates through the shared candidate trigger before repository qualification;
7. report actual Ruleset and upstream-transfer state without inference.

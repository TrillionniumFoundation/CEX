# CEX development-document authority index

Status: active

This file is the canonical entry point for development, qualification, and
operator documentation. The repository root `readme.md` is a non-normative
navigation page and cannot establish current readiness or release authority.

## Authority order

When two sources appear to disagree, use the following precedence:

1. immutable exact-commit release evidence and the generated candidate manifest;
2. `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md` together with its active implementation addendum;
3. accepted ADRs and versioned protocol specifications;
4. component status, module catalog/contracts, state-machine, threat-model,
   acceptance, SLO, compatibility, and runbook documents;
5. historical plans, archived evidence, research notes, and examples.

Source presence, prose, a template, or a green run on another SHA never
overrides exact-tree evidence.

## Active authority set

- Plan: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`
- Implementation addendum: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md`
- Machine-readable authority: `development-doc-authority-v1.json`
- Machine-readable requirement traceability: `traceability/v12-requirements-v1.json`
- Machine-readable workspace module catalog: `module-catalog-v1.json`
- Human workspace module index: `modules/index.md`
- Component status: `status/component-status-v1.md`
- Hepta state machines: `hepta-paper-raid-state-machines-v1.md`
- Security threat model: `security-threat-model-v1.md`
- Clean-deployment acceptance: `clean-deployment-acceptance-v1.md`
- SLO and recovery contract: `slo-recovery-contract-v1.md`
- Protocol compatibility matrix: `protocol/version-compatibility-matrix-v1.md`
- TRNM production credential contract: `trnm-production-credential-contract-v1.md`

The active set is validated by `scripts/check-development-docs.py`. That
checker reads `Cargo.toml`, requires an exact one-to-one catalog entry for every
workspace member, validates all module contracts and source entry points, and
rejects temporary exact-SHA convergence workflows. The exact repository
identity, canonical-document digests, and complete module-document digest are
emitted by `scripts/check-repository-integrity.py`.

## Workspace module documentation

`module-catalog-v1.json` is the machine authority for module membership,
ownership, source entry points, verification commands, and module-document
paths. `modules/index.md` is the human index. Every workspace package has a
dedicated module contract covering:

- purpose and non-goals;
- authority and owned state;
- source layout and entry points;
- interfaces and contracts;
- persistence, concurrency, and recovery;
- configuration and secrets;
- security and trust boundaries;
- verification;
- deployment and operations;
- compatibility and change protocol.

A new, removed, or renamed Cargo workspace member must update the catalog and
module documentation in the same commit. File presence or word count alone is
not completion: the checker validates membership, package names, source paths,
required sections, and fail-closed authority markers.

## Lifecycle labels

- `active`: normative for current repository work.
- `accepted`: architectural or protocol decision that remains binding within its scope.
- `operational`: executable runbook or acceptance procedure.
- `historical`: retained for audit only; not a source of current status.
- `template`: shape-only input; never release evidence.

## Production authorization boundary

Repository qualification may establish that source, migrations, tests,
workflows, generated evidence, and complete module documentation are internally
consistent on one commit/tree. It cannot authorize production.
Representative-volume disaster recovery, real deployment and rollback,
provider evidence, secret custody, sustained soak, independent reviews,
legal/commercial approvals, and final human go/no-go remain externally
evidenced gates. The production-authorization value remains `not_granted`
until those independent records and the final human decision exist.

## Change protocol

A change to any active authority document or workspace member must:

1. preserve the explicit `not_granted` production-authorization posture;
2. update the machine-readable authority, module catalog, or traceability
   document when scope changes;
3. update the affected module contract and executable verification;
4. pass the documentation contract checker;
5. pass repository-integrity attestation on the exact tree;
6. trigger all authoritative v12 hosted gates through the shared candidate
   trigger before repository qualification.

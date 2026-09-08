# CEX development-document authority index

Status: active  
Candidate sequence: `54`  
Production authorization: `not_granted`

This is the canonical entry point for CEX development, qualification and operator documentation. The root `readme.md` is navigation only and cannot establish current status, qualification or production authorization.

## Authority order

When sources disagree, apply this precedence:

1. immutable exact-commit release evidence and the generated candidate manifest, limited to what the retained artifacts actually prove;
2. accepted ADRs and versioned protocol specifications, especially ADR-004's external-Agent-only boundary;
3. the v12 parent plan, active implementation addendum and the Sequence 54 non-regression integration plan;
4. the frozen Sequence 52 architecture closure and its machine traceability for the external-Agent runtime boundary;
5. component status, module contracts, state machines, threat model, acceptance, SLO, compatibility, evidence-intake and runbook documents;
6. historical plans, archived evidence, research notes and examples.

Source presence, prose, a template, a local command, a queued job, a zero-step run or a green result on another SHA never overrides exact-tree evidence.

## Active authority set

- Parent plan: `docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`
- Implementation addendum: `docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md`
- Current non-regression integration: `docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-SEQUENCE54-INTEGRATION.md`
- Machine authority: `docs/development-doc-authority-v1.json`
- Shared candidate trigger: `docs/release-evidence/p0-candidate-trigger.json`
- Base requirement traceability: `docs/traceability/v12-requirements-v1.json`
- Sequence 54 integration traceability: `docs/traceability/v12-sequence54-integration-v1.json`
- Component status: `docs/status/component-status-v1.md`
- Ruleset policy: `docs/repository-ruleset-required-contexts-v1.json`
- Rust active-surface policy: `docs/security/rust-toolchain-surfaces-v1.json`

The Sequence 54 layer does not rewrite history. It preserves the later Sequence 52/53 functional tree—23 Cargo members, migration head `0088_enforce_provider_terminal_evidence_binding.sql`, Matrix durability, provider terminal-evidence binding and Paper Raid product controls—while incorporating the later Rust 1.98.1, container, Ruleset and runner-diagnostics security controls.

## External-Agent architecture boundary

- Accepted decision: `decisions/adr-004-three-module-external-agent-battle-platform.md`
- Sequence 52 architecture closure: `docs/architecture/external-agent-runtime-boundary-sequence-52.md`
- Sequence 52 architecture traceability: `docs/traceability/sequence-52-architecture-v1.json`
- Executable checker: `scripts/check-external-agent-runtime-boundary.py`
- Protocol: `docs/hepta-agent-protocol-v1.md`

Sequence 52 remains the frozen architecture-baseline number. Sequence 54 is the repository candidate number. The two values have different meanings and must not be collapsed.

The parent plan remains normative for exact Ledger, Gateway, Execution lifecycle, Audit, migration, recovery and evidence controls. Its provider-specific Ollama/OpenClaw wording is historical compatibility scope under ADR-004. It does not authorize CEX-hosted inference, local model discovery, provider credentials or Agent private keys. The retired `/v1/executions/:id/process` route is absent from the default router, and the compatibility worker remains non-default and forbidden in production-like profiles.

## Workspace module contracts

The actual Cargo workspace is documented through:

- machine catalog: `docs/module-catalog-v1.json`
- human index: `docs/modules/index.md`
- one dedicated file under `docs/modules/` for every active workspace member.

`scripts/check-module-documentation.py` requires exact equality among Cargo members, catalog entries, package identities, source entry points and module documents. External authorities such as Nakama, Trillionnium Chain, Matrix homeserver, object storage, participating Agents/providers and World/Game runtimes are not invented as local Cargo members.

## Sequence 54 executable controls

- `scripts/check-sequence54-integration.py` prevents regression of the 23-member workspace, migration 0088, module catalog, external-Agent boundary, Matrix durability, Paper Raid controls, Ruleset policy and source-governance constraints.
- `scripts/check-rust-toolchain-convergence.py` derives active carriers recursively from the Git tree and rejects active Rust 1.95.0/1.98.0 selectors, floating channels, unregistered selectors, symlinks and gitlinks.
- `.github/workflows/p0-sequence54-integration.yml` executes the integration checker, toolchain checker, document/module contracts, Paper Raid boundary checks, locked metadata, formatting, all-target tests and strict Clippy on one exact SHA.
- `.github/workflows/p0-rust-toolchain-convergence.yml` binds the source, base and GitHub prospective merge identities and records the executed Rust release commit.
- `scripts/apply-main-ruleset.py`, `scripts/verify-main-ruleset.py` and `scripts/probe-main-ruleset-v2.sh` apply, read back and negatively exercise the complete main-branch admission policy.

Repository workflows are evidence producers. They must not push source, update `main`, self-approve or manufacture external evidence.

## Runner probe and qualification evidence

The desktop, ROG, Pocket4 and MacBook self-hosted probes are manual, main-only, no-checkout workflows with `permissions: {}`. A runner probe definition is not execution evidence. A queued job, `runner_id=0`, empty runner identity, `steps=[]`, missing logs or an artifact from a different SHA earns no qualification credit.

The GitHub-hosted allocation diagnostic is also not an admission check. Repository qualification requires non-empty successful execution of every required context on the same unchanged source SHA and prospective merge tree.

## Core technical authority

- Hepta state machines: `docs/hepta-paper-raid-state-machines-v1.md`
- Security threat model: `docs/security-threat-model-v1.md`
- Clean-deployment acceptance: `docs/clean-deployment-acceptance-v1.md`
- SLO and recovery: `docs/slo-recovery-contract-v1.md`
- Protocol compatibility: `docs/protocol/version-compatibility-matrix-v1.md`
- TRNM production credential contract: `docs/trnm-production-credential-contract-v1.md`
- Semantic route extraction: `docs/semantic-route-extraction-v2.md`
- Qualification source integrity: `docs/qualification-source-integrity-v1.md`
- Cargo workspace authority: `docs/cargo-workspace-authority-v1.md`
- Matrix stream scope: `docs/matrix-stream-scope-v1.md`
- Matrix SQL runner: `docs/matrix-sql-runner-v4.md`
- Matrix filter definition: `docs/matrix-filter-definition-v1.md`
- Matrix profile sharing: `docs/matrix-profile-sharing-v1.md`
- Matrix wire response/quarantine: `docs/matrix-wire-response-v1.md`

## External production evidence intake

- Contract: `docs/external-production-evidence-contract-v1.md`
- Shape-only template: `docs/templates/cex-external-production-evidence-bundle-v1.json`
- Structural checker: `scripts/check-external-production-evidence-contract.py`

The source tree can validate only the contract and template shape. It cannot certify issuer independence, real deployment, real recovery, legal approval or final go/no-go.

## Production authorization boundary

Repository qualification may prove source, migrations, tests, workflows, documentation, architecture boundaries and generated evidence are internally consistent on one exact candidate. It cannot authorize production.

The following remain independently evidenced: non-empty exact-SHA and prospective-merge execution; live main Ruleset activation and negative probes; two fresh eligible reviews including independent security approval; representative-volume restore; real deployment/cutover/rollback; external Agent/provider outcomes; credential custody; World authority transfer and no-dual-writer proof; sustained SLO qualification; security, operations, financial, legal and commercial approvals; and final human go/no-go.

Until all such records exist, `production_authorization` remains `not_granted`.

## Change protocol

Every active authority or workspace change must:

1. preserve `production_authorization=not_granted`;
2. update machine authority and traceability in the same candidate;
3. update the module catalog and affected `docs/modules/` contracts;
4. preserve ADR-004 unless an accepted superseding ADR exists;
5. preserve exact migration and protocol compatibility;
6. pass `scripts/check-external-agent-runtime-boundary.py`, `scripts/check-sequence54-integration.py`, `scripts/check-rust-toolchain-convergence.py`, `scripts/check-module-documentation.py`, the external evidence contract checker and `scripts/check-development-docs.py`;
7. pass repository-integrity attestation and all required hosted gates on one exact SHA and prospective merge tree;
8. obtain live Ruleset verification, fresh eligible reviews and a generated immutable candidate manifest before merge.

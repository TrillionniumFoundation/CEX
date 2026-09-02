# Component status and evidence posture

Status: active status model

This document describes maturity and required evidence. It is not release evidence and does not pin a moving branch name to a readiness claim. Exact status must be read from the candidate manifest generated for one commit/tree.

| Component | Repository maturity | Authoritative write surface | Required hosted evidence | Production posture |
|---|---|---|---|---|
| Ledger exact authority | repository-candidate | `cex_open_account_v2`, append-only exact effects, operation identity | migration gate, Rust gate, release candidate gate | not authorized |
| Gateway exact reserve | repository-candidate | durable reserve command and verified receipt | Gateway gate and release candidate gate | shadow/fail-closed |
| Execution settlement | repository-candidate | durable consume/refund command and transition history | Execution gate and release candidate gate | shadow/fail-closed |
| Provider reconciliation | repository-candidate | immutable outcome artifacts and terminal-safe replay | provider gate and release candidate gate | real provider evidence still external |
| Audit baseline/delivery | repository-candidate | authenticated append, outbox, bounded source baseline | migration and release candidate gates | independent operations review pending |
| TRNM economy adapter | repository-candidate | exact Ledger-backed native economy and receipt recovery | Rust and release candidate gates | no production cutover implied |
| Hepta Research League | functional Alpha / repository-candidate slice | durable research facts, signed control, PostgreSQL outbox/inbox, finality projection | strict Hepta PostgreSQL job inside Rust gate plus aggregate recovery rerun | no production activation |
| Nakama integration | contract-qualified boundary | authoritative ordered session events and signed receipts | contract/golden tests and Hepta recovery | live topology rehearsal external |
| Consumer entry and BFF | supporting Alpha | identity mapping/read models only; not scientific fact authority | Rust workspace tests | user-journey and operational qualification pending |
| Workspace module documentation | repository-candidate contract | Cargo workspace membership, owner and authority mapping, dedicated technical documents | module checker, documentation checker, repository-integrity job | not release evidence by itself |
| External evidence intake | repository-candidate contract | shape, candidate binding, immutable URI/digest, issuer and gate-order validation only | contract-only documentation and integrity checks | cannot self-certify X1-X8 or grant production authorization |
| Release evidence | repository-candidate | generated immutable evidence payload and candidate manifest | all five exact-SHA gates plus aggregate gate | explicitly not production authorization |

## Status vocabulary

- `repository-candidate`: implementation and repository-hosted evidence can qualify one exact tree.
- `functional Alpha`: the vertical slice is executable but has not completed every external production gate.
- `supporting Alpha`: usable supporting surface whose end-to-end production qualification remains incomplete.
- `contract-qualified boundary`: protocol and repository checks exist, while live topology evidence remains external.
- `blocked_upstream`: no repository edit can supply the required independent evidence.

## Module documentation posture

`Cargo.toml` is the workspace-membership source. `docs/module-catalog-v1.json` must contain exactly the same members, and every member must have one dedicated technical contract indexed by `docs/modules/index.md`. `scripts/check-module-documentation.py` verifies the bijection, package names, owners, source entry points, required sections, and external-authority boundaries. The result is still only repository evidence after it executes on the exact candidate tree and is retained by the aggregate gate.

## External evidence posture

`docs/external-production-evidence-contract-v1.md` and its shape-only template define how V12-X1 through V12-X8 records are structurally admitted. The checker cannot establish genuine issuer independence, perform the underlying real-world activity, issue an approval, or grant production authorization. Real records remain under external custody and must bind one exact repository-qualified candidate.

## Non-equivalences

- A completed Nakama session is not a completed paper.
- A completed paper is not economic finality.
- Complete module documentation is not executable qualification.
- A structurally valid external bundle is not accepted external evidence by itself.
- Repository qualification is not deployment approval.
- A template manifest is not a candidate manifest.
- A run on a parent, branch tip, merge tree, or later commit is not evidence for another tree.

## Current blockers by class

Repository-actionable blockers are defined in the v12 plan and active addendum and are machine-tracked in `traceability/v12-requirements-v1.json`. External production blockers remain the eight independently evidenced gates in the parent plan. They may be linked from issues or approval systems, but cannot be marked closed by this status document, a template, or structural validation.

# Component status and evidence posture

Status: active status model

This document describes maturity and required evidence. It is not release
evidence and does not pin a moving branch name to a readiness claim. Exact
status must be read from the candidate manifest generated for one commit/tree.

| Component | Repository maturity | Authoritative write surface | Required hosted evidence | Production posture |
|---|---|---|---|---|
| Ledger exact authority | repository-candidate | `cex_open_account_v2`, append-only exact effects, operation identity | migration gate, Rust gate, release candidate gate | not authorized |
| Gateway exact reserve | repository-candidate | durable reserve command and verified receipt | Gateway gate and release candidate gate | shadow/fail-closed |
| Execution settlement | repository-candidate | durable consume/refund command and transition history | Execution gate and release candidate gate | shadow/fail-closed |
| Provider reconciliation | repository-candidate | immutable outcome artifacts and terminal-safe replay | provider gate and release candidate gate | real provider evidence still external |
| Audit baseline/delivery | repository-candidate | authenticated append, outbox, bounded source baseline | migration and release candidate gates | independent operations review pending |
| TRNM economy adapter | repository-candidate | exact Ledger-backed native economy and receipt recovery | Rust and release candidate gates | no production cutover implied |
| Hepta Research League | functional Alpha / repository-candidate slice | durable research facts, signed control, PostgreSQL outbox/inbox, finality projection | strict Hepta PostgreSQL job inside Rust gate plus aggregate recovery rerun | no production activation |
| Nakama integration | contract-qualified external boundary | authoritative ordered session events and signed receipts | contract/golden tests and Hepta recovery | live topology rehearsal external |
| Consumer entry and BFF | supporting Alpha | identity/session and bounded read-model state only; not scientific fact authority | Rust workspace tests | user-journey and operational qualification pending |
| Workspace module documentation | repository-candidate | exact `Cargo.toml` member set, `module-catalog-v1.json`, and one module contract per member | development-document and repository-integrity jobs on the exact SHA | documentation is not production authorization |
| Release evidence | repository-candidate | generated immutable evidence payload and candidate manifest | all five exact-SHA gates plus aggregate gate | explicitly not production authorization |

## Status vocabulary

- `repository-candidate`: implementation and repository-hosted evidence can qualify one exact tree.
- `functional Alpha`: the vertical slice is executable but has not completed every external production gate.
- `supporting Alpha`: usable supporting surface whose end-to-end production qualification remains incomplete.
- `contract-qualified`: the repository defines and tests a boundary whose live external topology is not yet qualified.
- `blocked_upstream`: no repository edit can supply the required independent evidence.

## Module-document status

The current workspace membership and module-document coverage are defined by
`docs/module-catalog-v1.json`. `scripts/check-development-docs.py` rejects:

- a workspace member without a catalog entry or dedicated module contract;
- a catalog package name that differs from the member `Cargo.toml`;
- missing source entry points, owner roles, verification commands, or required
  module-document sections;
- a catalog/module document that grants production authority;
- temporary exact-SHA convergence workflows that can bypass the candidate
  manifest authority.

The exact-tree integrity record hashes every module contract, so a later
documentation change creates a new candidate.

## Non-equivalences

- A completed Nakama session is not a completed paper.
- A completed paper is not economic finality.
- Complete module documentation is not executable hosted evidence.
- Repository qualification is not deployment approval.
- A template manifest is not a candidate manifest.
- A run on a parent, branch tip, merge tree, or later commit is not evidence for another tree.

## Current blockers by class

Repository-actionable blockers are defined in the v12 plan and active addendum
and are machine-tracked in `traceability/v12-requirements-v1.json`. External
production blockers remain the eight independently evidenced gates in the
parent plan. They may be linked from issues or approval systems, but cannot be
marked closed by this status document.

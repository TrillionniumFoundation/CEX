# CEX Development Plan v12 — implementation and evidence addendum

Status: active normative addendum

Parent plan: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`

This addendum deepens the parent plan without changing its exact-money invariants, migration head, rollout posture, or external approval boundary. If this addendum conflicts with the parent plan, the parent plan controls. The two documents form the active repository implementation plan.

## 1. Additional repository-actionable closure blocks

### Block H — development-document authority and exact-tree integrity

The repository must provide one canonical documentation entry point, a machine-readable authority declaration, and a requirement traceability ledger. Every repository-actionable requirement must identify its normative source, implementation locations, executable verification, hosted gate, and evidence posture.

Required behavior:

- historical plans cannot silently become current authority;
- templates, status prose, and branch-local runs cannot be presented as release evidence;
- every branch push must execute the authoritative Rust gate without path-filter gaps;
- the exact commit, tree, Cargo lock, migration chain, workflow set, and canonical document set must be hashed into a machine-readable integrity record;
- root `readme.md` is non-normative navigation and cannot establish current status or release evidence;
- production authorization remains explicitly `not_granted`.

Closure evidence:

- `scripts/check-development-docs.py` succeeds;
- `scripts/check-repository-integrity.py` succeeds on the exact candidate tree;
- the `repository-integrity` job inside `rust-service-gate` succeeds;
- the aggregate release-candidate workflow stores the integrity record in its immutable evidence payload.

### Block I — Hepta durable PostgreSQL recovery

A hosted gate must prove that Hepta's PostgreSQL-backed state is not silently skipped and that restart, concurrent outbox claiming, wrong-owner acknowledgement rejection, expired-lease recovery, readiness, and operator metrics execute against PostgreSQL 16.

Required behavior:

- `HEPTA_REQUIRE_POSTGRES_TESTS=1` converts a missing or empty `HEPTA_TEST_DATABASE_URL` into a hard test failure;
- the hosted lane uses a fresh PostgreSQL 16 service and a dedicated database URL;
- the recovery test serializes destructive fixtures with an advisory lock;
- multiple service instances cannot claim the same outbox event;
- an expired lease can be recovered by a new worker without losing identity or event evidence;
- package tests and Clippy remain green with warnings denied.

Closure evidence:

- `scripts/check-hepta-postgres-integration.sh` succeeds in full mode;
- the `hepta-postgres-integration` job inside `rust-service-gate` succeeds on the exact candidate SHA;
- the aggregate release-candidate workflow reruns the strict recovery test and stores a Hepta evidence record.

### Block J — complete-suite lint ownership and fail-closed source identity

The complete Hepta package suite is authoritative. A recovery-only test cannot substitute for package tests plus all-target Clippy. The Paper Raid child modules inherit the Base64 engine trait from `paper_raid_v2`; their extracted bodies must therefore remove redundant local `Engine as _` imports and their wrappers must remain thin, attribute-free `include!` modules. Each cleaned body is bound to an immutable source-body Git blob. A crate-wide warning allowance, `-A warnings`, module-level lint suppression, or unguarded source split is forbidden.

Required behavior:

- all unit and integration tests execute before strict Clippy in the full hosted lane;
- Clippy runs with `-D warnings` over every Hepta target;
- the four cleaned source bodies and four thin wrappers are validated by `scripts/check-hepta-lint-ownership.py`;
- each body has an exact Git blob SHA, exactly one canonical Base64 value import, and no redundant local `Engine as _` import;
- each wrapper is an attribute-free `include!` with no lint suppression;
- any body or wrapper drift fails before Cargo execution and requires direct source cleanup or an explicitly reviewed new contract;
- the exact lint contract is included in retained Hepta evidence.

Closure evidence:

- `scripts/check-hepta-lint-ownership.py` succeeds;
- `scripts/check-hepta-postgres-integration.sh --mode full` succeeds;
- the exact-SHA Hepta job reports every test successful and Clippy clean under warnings denied.

### Block K — complete workspace module documentation and ownership

The documentation authority must cover the actual Cargo workspace rather than only central P0 workstreams. `Cargo.toml` is the source of truth for membership. Every workspace member must have one machine-readable catalog entry and one dedicated technical contract.

Required behavior:

- `docs/module-catalog-v1.json` contains exactly the Cargo workspace members, with no missing, duplicate, stale, or invented member;
- each catalog package name matches the member's `[package].name`;
- every catalog entry identifies kind, logical module, deployability, maturity, owner role, authority boundary, documentation path, source entry points, and executable verification commands;
- every dedicated document defines purpose and non-goals, authority and owned state, source layout, interfaces, persistence/concurrency/recovery, configuration/secrets, security/trust, verification, deployment/operations, and compatibility/change protocol;
- `docs/modules/index.md` maps all workspace members and the external Nakama, Chain, Matrix, object-store, provider, and external-Agent authorities;
- `scripts/check-module-documentation.py` verifies workspace/catalog equality, package identity, source-path existence, document depth, ownership, navigation, and external-component boundaries;
- `scripts/check-development-docs.py` executes that module checker as part of Block H evidence;
- root `readme.md` provides non-normative discovery only;
- one-time convergence, self-patch, force-update, or candidate-construction scripts are absent from the frozen tree;
- module-document validation is bound to the exact commit/tree through the repository-integrity record and generated candidate manifest;
- production authorization remains `not_granted`.

Closure evidence:

- `scripts/check-module-documentation.py` succeeds and reports the complete workspace;
- `scripts/check-development-docs.py` succeeds with the module contract enabled;
- `scripts/check-repository-integrity.py` records the exact candidate commit/tree and successful documentation check;
- the `repository-integrity` job and aggregate candidate workflow succeed on that same SHA.

### Block L — external production evidence intake and anti-self-certification

The repository must define one machine-checkable shape for V12-X1 through V12-X8 while preserving their external, non-self-certifiable status. Structural validation may reject malformed or cross-candidate evidence; it may never create, infer, waive, or approve the underlying real-world facts.

Required behavior:

- `docs/external-production-evidence-contract-v1.md` defines candidate binding, issuer identity, immutable URI/digest, UTC time, scope, gate ordering, revocation, retention, and final-human-decision requirements;
- `docs/templates/cex-external-production-evidence-bundle-v1.json` is shape-only, contains exactly V12-X1 through V12-X8, carries no evidence, and explicitly denies production authorization;
- real external evidence is stored under approved external custody and is not committed into the source tree;
- every evidence record binds the exact candidate commit/tree and repository-candidate manifest, identifies its issuer/organization/role, and explicitly states independence from repository automation;
- `scripts/check-external-production-evidence-contract.py --contract-only` validates the source contract and anti-self-certification boundary;
- optional bundle validation checks structure and cross-gate consistency but still reports `production_authorization=not_granted` and `checker_may_grant_production_authorization=false`;
- V12-X8 cannot structurally pass before V12-X1 through V12-X7 pass on the same candidate, and only a real final human `go` decision can grant authorization outside repository automation;
- a waiver, source-tree evidence file, mutable URI, credential-bearing URI, cross-candidate record, revocation, or inferred approval fails closed.

Closure evidence:

- the contract-only checker succeeds on the exact tree;
- `scripts/check-development-docs.py` executes the contract-only checker without changing the established candidate-manifest schema;
- the exact-tree repository-integrity job retains the successful documentation result;
- real X1-X8 closure remains external and cannot be claimed by this block.

## 2. Documentation completion contract

The active documentation set must define all of the following:

1. authority and precedence;
2. component maturity and evidence posture;
3. complete workspace module documentation, ownership, authority boundaries, source entry points, verification, deployment and change protocol;
4. Research, Nakama, and Settlement state machines and their cross-product invariants;
5. trust boundaries, threats, controls, residual risks, and independent-review boundaries;
6. clean-deployment acceptance from an empty database through exact-tree evidence generation;
7. SLI/SLO targets, recovery semantics, and the distinction between CI regression evidence and production qualification;
8. protocol read/write authority, compatibility, migration, and retirement conditions;
9. machine-readable requirement-to-code-to-test-to-gate traceability;
10. complete-suite lint ownership without broad warning suppression;
11. an executable external-evidence intake shape that cannot self-certify production gates.

Documentation is complete only when the checker validates the authority set, the Cargo workspace/catalog bijection, every dedicated module contract, the external-evidence source contract, and every referenced source path, and the exact-tree integrity record binds the successful result to the candidate tree. Word count, file presence, a root README, a template, or a central plan alone is not completion.

## 3. Candidate sequencing

The final candidate sequence is:

1. freeze one candidate commit/tree;
2. run module documentation, external-evidence contract, authority, hygiene, static wiring, lint-ownership, and repository-integrity checks;
3. run the five authoritative v12 workflows, with `rust-service-gate` containing Blocks H, I, J, K, and L;
4. run the aggregate candidate workflow on the same SHA;
5. generate and validate the immutable evidence payload and candidate manifest;
6. report actual branch/ruleset enforcement without inference;
7. stop at repository qualification unless every external production gate has independent evidence.

The shared qualification trigger is the sole committed freeze authority; a stale or missing trigger, or any secondary freeze marker, is a fail-closed repository wiring error, not a reason to infer qualification.

A later commit, including a documentation-only commit, creates a new tree and must be requalified.

Transient files written below `run/p0-release-support` are operator diagnostics only. They are outside the canonical payload allow-list; lifecycle qualification is bound to the retained `database-lifecycle.json` record and the exact hosted run/job/step attestations.

## 4. External production gates remain upstream blockers

The following cannot be closed by this addendum or by repository automation: representative-volume recovery on real topology; real deployment/cutover/rollback; real provider outcome artifacts; credential custody review; sustained production-like soak and SLO qualification; independent security, operations, and financial-control review; legal/commercial/provider approvals; and final human go/no-go.

Repository closure therefore yields `REPOSITORY_CLOSED_CANDIDATE`, never production activation.

## 5. Definition of addendum closure

This addendum is closed on one exact commit only when:

- all active documents and machine-readable ledgers validate;
- the Cargo workspace, module catalog, module index, and all dedicated module contracts validate;
- the external-evidence template and anti-self-certification checker validate without claiming real X1-X8 closure;
- all repository-actionable traceability entries resolve to existing files and executable gates;
- strict Hepta PostgreSQL recovery cannot skip;
- the exact Hepta lint ownership contract validates and the complete package suite passes;
- the exact-tree integrity record is generated and included in release evidence;
- the five authoritative workflows and aggregate candidate workflow succeed on that SHA;
- no placeholder, temporary remediation/convergence workflow, broad warning allowance, fabricated external evidence, or fabricated approval is used;
- the generated candidate manifest continues to deny production authorization.

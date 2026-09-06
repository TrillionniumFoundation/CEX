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
- root `readme.md` is outside this authority chain and is not required for closure;
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

The complete Hepta package suite is authoritative. A recovery-only test cannot substitute for package tests plus all-target Clippy. Where a child module inherits the Base64 engine trait from `paper_raid_v2` and also carries the same local trait import, the repository may use only a module-local lint expectation bound to an immutable source-body Git blob. A crate-wide warning allowance, `-A warnings`, or unguarded source split is forbidden.

Required behavior:

- all unit and integration tests execute before strict Clippy in the full hosted lane;
- Clippy runs with `-D warnings` over every Hepta target;
- the four source bodies and four thin wrappers are validated by `scripts/check-hepta-lint-ownership.py`;
- each body has an exact Git blob SHA and exactly one known inherited `Engine` import;
- each wrapper uses `expect(unused_imports)`, never `allow(unused_imports)`;
- any body or wrapper drift fails before Cargo execution and requires direct source cleanup or an explicitly reviewed new contract;
- the exact lint contract is included in retained Hepta evidence.

Closure evidence:

- `scripts/check-hepta-lint-ownership.py` succeeds;
- `scripts/check-hepta-postgres-integration.sh --mode full` succeeds;
- the exact-SHA Hepta job reports every test successful and Clippy clean under warnings denied.

### Block K — workspace module documentation completeness

Documentation completeness is evaluated against the actual Cargo workspace, not a hand-maintained subset of services. Every active workspace member must have one machine-readable catalog entry and one colocated module contract.

Required behavior:

- `[workspace].members` in `Cargo.toml` and `docs/module-catalog-v1.json` are exactly equal and use the same order;
- every catalog entry identifies stable module ID, repository path, kind, lifecycle, bounded-context owner and canonical `<member>/MODULE.md`;
- every module contract defines scope, non-goals, authoritative state, interfaces, data/persistence, security/configuration, failure/recovery, observability, verification and compatibility/retirement;
- module documents deny production authorization and contain no machine-specific absolute paths;
- a workspace addition, removal, move or ownership transfer updates Cargo, the catalog and the module contract in the same commit;
- quarantined source is not represented as an active workspace member or current protocol authority.

Closure evidence:

- `scripts/check-module-documentation.py` succeeds;
- `scripts/check-development-docs.py` invokes and validates the module checker;
- `repository-integrity`, Linux, Windows and strict Hepta lanes execute the module contract on the exact candidate tree.

### Block L — Capability production startup must fail closed

`capability-service` may use demo records and local OpenClaw discovery only in explicit development/test profiles. A production-like process must obtain its complete authority from one explicit validated registry snapshot and must reject ambiguity before it binds a listener.

Required behavior:

- `CEX_RUNTIME_PROFILE` or `APP_ENV` is explicit and non-conflicting;
- beta, staging and production require a non-empty valid `CAPABILITY_STATIC_REGISTRY_JSON`;
- IDs are unique, required identity fields are non-empty and at least one capability is enabled;
- demo, placeholder, change-me, local-development and `openclaw-local` authority markers are rejected;
- local OpenClaw discovery variables are forbidden in production-like profiles;
- bind address and record limits are validated;
- malformed or absent authority exits with configuration error code 78 instead of loading defaults.

Closure evidence:

- `scripts/check-capability-production-posture.py` succeeds;
- `cargo test --locked -p capability-service` succeeds;
- all-target Capability Clippy succeeds with warnings denied inside the Rust gate.

### Block M — authoritative exact-money isolation

The repository may retain explicitly enumerated binary-floating compatibility, display and non-production test surfaces, but no such surface may become a new authoritative value write or a source for exact minor units.

Required behavior:

- all listed authoritative exact-money files contain no `f32` or `f64`;
- exact amount/balance/reserve paths contain no rounding, truncation or lossy integer cast;
- Gateway reserve, Execution terminal settlement and TRNM receipt handling consume versioned exact contracts;
- production-like Ledger startup requires PostgreSQL and fail-fast posture;
- every remaining value-related float file is classified in the machine-readable compatibility allowlist;
- adding a new legacy value-float path fails the exact-tree contract until explicitly reviewed;
- compatibility reads never grant write authority and v1 value writes remain retired.

Closure evidence:

- `scripts/check-authoritative-money-isolation.py` succeeds;
- `scripts/check-ledger-caller-cutover.py` and `scripts/check-p0-wiring.py` remain green;
- exact Gateway and Execution hosted gates remain green on the candidate SHA.

### Block N — World/Consumer boundary containment

CEX is the Hepta control plane and does not own World gameplay, map simulation, commerce, tactics, progression or authoritative game-server state. Existing embedded World code is compatibility debt that must be frozen while authority is transferred through a versioned World-owned interface.

Required behavior:

- the complete known embedded World source inventory and broad Matrix command carrier are bound to exact Git blobs;
- new World source files, new authoritative writers and new external sibling Cargo path dependencies are forbidden;
- a frozen blob may change only for a reviewed security/defect correction that adds no domain scope, or for an extraction step;
- Consumer Entry and Matrix module contracts state that their World surfaces are compatibility-only;
- project-boundary policy covers `world_`, `real_world_`, `openstreetmap_` and `trillionnium_world_` paths;
- full cross-repository transfer is not self-certified by the freeze and remains upstream until a World-owned contract and cross-repository evidence exist.

Closure evidence:

- `scripts/check-project-boundary.py` succeeds;
- `docs/compatibility/world-surface-freeze-v1.json` exactly matches the source inventory;
- the Rust gate executes the boundary check on every candidate tree.

### Block O — source-governance and reviewed-change enforcement

Repository workflows are evidence producers, not source-control writers. Source-side governance must prevent a repeat of temporary convergence automation while actual GitHub Ruleset state remains independently observable.

Required behavior:

- `.github/CODEOWNERS` owns workflows, authority/traceability, migrations, project boundaries, exact-money contracts and critical services;
- no workflow requests `contents: write`;
- no workflow shell step performs `git push`;
- recursive hygiene rejects temporary closure triggers anywhere in the repository;
- the former sequence-44 convergence workflow and execution trigger file are absent;
- the source policy explicitly requires PR review, CODEOWNERS, required checks, resolved conversations, force-push prevention and signed/equivalently verified merge identity;
- source checks report actual Ruleset enforcement as an independent GitHub setting and never fabricate it.

Closure evidence:

- `scripts/check-source-governance.py` succeeds;
- `scripts/check-p0-release-candidate-hygiene.py` succeeds;
- Rust and aggregate release-candidate gates retain these results on the exact SHA.

## 2. Documentation completion contract

The active documentation set must define all of the following:

1. authority and precedence;
2. component maturity and evidence posture;
3. Research, Nakama, and Settlement state machines and their cross-product invariants;
4. trust boundaries, threats, controls, residual risks, and independent-review boundaries;
5. clean-deployment acceptance from an empty database through exact-tree evidence generation;
6. SLI/SLO targets, recovery semantics, and the distinction between CI regression evidence and production qualification;
7. protocol read/write authority, compatibility, migration, and retirement conditions;
8. machine-readable requirement-to-code-to-test-to-gate traceability;
9. complete-suite lint ownership without broad warning suppression;
10. exact Cargo-workspace-to-module-catalog-to-module-document coverage;
11. Capability production fail-closed configuration;
12. exact-money authority isolation and legacy-float containment;
13. World/Consumer ownership containment and transfer boundary;
14. source governance without write-capable convergence automation.

Documentation is complete only when the checker validates the files and the exact-tree integrity record binds their digests. Word count or file presence alone is not completion.

## 3. Candidate sequencing

The final candidate sequence is:

1. freeze one candidate commit/tree;
2. run module-documentation, Capability posture, exact-money isolation, project-boundary, source-governance, development-document, hygiene, static wiring, lint-ownership and repository-integrity checks;
3. run the five authoritative v12 workflows, with `rust-service-gate` containing Blocks H through O;
4. run the aggregate candidate workflow on the same SHA;
5. generate and validate the immutable evidence payload and candidate manifest;
6. report actual branch/ruleset enforcement without inference;
7. stop at repository qualification unless every external production and administration gate has independent evidence.

A later commit, including a documentation-only commit, creates a new tree and must be requalified.

## 4. External production and administration gates remain upstream blockers

The following cannot be closed by this addendum or by repository automation: representative-volume recovery on real topology; real deployment/cutover/rollback; real provider outcome artifacts; credential custody review; sustained production-like soak and SLO qualification; independent security, operations, and financial-control review; legal/commercial/provider approvals; final human go/no-go; actual GitHub `main` Ruleset activation and verification; and the cross-repository transfer of authoritative World state to its owning repository.

Repository closure therefore yields `REPOSITORY_CLOSED_CANDIDATE`, never production activation or a fabricated administration pass.

## 5. Definition of addendum closure

This addendum is closed on one exact commit only when:

- all active documents, machine-readable ledgers, module catalog and active module contracts validate;
- all repository-actionable traceability entries resolve to existing files and executable gates;
- strict Hepta PostgreSQL recovery cannot skip;
- the exact Hepta lint ownership contract validates and the complete package suite passes;
- Capability production-like startup cannot select demo, placeholder or local discovery authority;
- exact-money authority contains no binary floating-point path or lossy conversion;
- embedded World surfaces cannot expand without explicit manifest review;
- no write-capable or source-pushing workflow and no temporary closure trigger remains;
- the exact-tree integrity record is generated and included in release evidence;
- the five authoritative workflows and aggregate candidate workflow succeed on that SHA;
- no placeholder, broad warning allowance, fabricated external approval or undocumented workspace module is used;
- the generated candidate manifest continues to deny production authorization;
- actual Ruleset and cross-repository transfer status are reported as upstream blockers until independent evidence exists.

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

## 2. Documentation completion contract

The active documentation set must define all of the following:

1. authority and precedence;
2. component maturity and evidence posture;
3. Research, Nakama, and Settlement state machines and their cross-product invariants;
4. trust boundaries, threats, controls, residual risks, and independent-review boundaries;
5. clean-deployment acceptance from an empty database through exact-tree evidence generation;
6. SLI/SLO targets, recovery semantics, and the distinction between CI regression evidence and production qualification;
7. protocol read/write authority, compatibility, migration, and retirement conditions;
8. machine-readable requirement-to-code-to-test-to-gate traceability.

Documentation is complete only when the checker validates the files and the exact-tree integrity record binds their digests. Word count or file presence alone is not completion.

## 3. Candidate sequencing

The final candidate sequence is:

1. freeze one candidate commit/tree;
2. run documentation, hygiene, static wiring, and repository-integrity checks;
3. run the five authoritative v12 workflows, with `rust-service-gate` containing Blocks H and I;
4. run the aggregate candidate workflow on the same SHA;
5. generate and validate the immutable evidence payload and candidate manifest;
6. report actual branch/ruleset enforcement without inference;
7. stop at repository qualification unless every external production gate has independent evidence.

A later commit, including a documentation-only commit, creates a new tree and must be requalified.

## 4. External production gates remain upstream blockers

The following cannot be closed by this addendum or by repository automation: representative-volume recovery on real topology; real deployment/cutover/rollback; real provider outcome artifacts; credential custody review; sustained production-like soak and SLO qualification; independent security, operations, and financial-control review; legal/commercial/provider approvals; and final human go/no-go.

Repository closure therefore yields `REPOSITORY_CLOSED_CANDIDATE`, never production activation.

## 5. Definition of addendum closure

This addendum is closed on one exact commit only when:

- all active documents and machine-readable ledgers validate;
- all repository-actionable traceability entries resolve to existing files and executable gates;
- strict Hepta PostgreSQL recovery cannot skip;
- the exact-tree integrity record is generated and included in release evidence;
- the five authoritative workflows and aggregate candidate workflow succeed on that SHA;
- no placeholder, temporary remediation workflow, or fabricated external approval is used;
- the generated candidate manifest continues to deny production authorization.

# CEX Development Plan v12 — implementation and evidence addendum

Status: active normative addendum

Parent plan: `CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md`  
Accepted architecture: `decisions/adr-004-three-module-external-agent-battle-platform.md`  
Sequence-52 closure contract: `docs/architecture/external-agent-runtime-boundary-sequence-52.md`  
Sequence-52 traceability: `docs/traceability/sequence-52-architecture-v1.json`

**Sequence-52 correction.** This addendum deepens the parent plan without changing its exact-money invariants, migration head, rollout posture, or external approval boundary. The accepted ADR controls the product/runtime boundary: parent-plan provider-specific Ollama/OpenClaw dispatch requirements are retained historical command/evidence and reconciliation requirements, not authority for CEX to execute participating Agents. The retired `/process` route is absent from the default Execution router. The parent plan and addendum otherwise form the active repository implementation plan.

## 1. Repository-actionable closure blocks

### Block H — development-document authority and exact-tree integrity

The repository must provide one canonical documentation entry point, a machine-readable authority declaration, requirement traceability, and exact-tree integrity evidence.

Required behavior:

- historical plans, templates, status prose, local output, and runs from another SHA cannot become current release evidence;
- every repository-actionable requirement identifies source, implementation, verification, hosted gate, and evidence posture;
- the exact commit, tree, Cargo lock, migration chain, workflow set, and active documents are bound into the repository-integrity record;
- `scripts/check-development-docs.py` and `scripts/check-repository-integrity.py` pass on the exact candidate tree;
- production authorization remains `not_granted`.

### Block I — Hepta durable PostgreSQL recovery

A non-skippable PostgreSQL 16 gate must prove restart durability, concurrent outbox claiming, wrong-owner acknowledgement rejection, expired-lease recovery, readiness, and operator metrics.

Required behavior:

- `HEPTA_REQUIRE_POSTGRES_TESTS=1` makes a missing database URL a hard failure;
- multiple service instances cannot claim the same outbox event;
- an expired lease can be recovered without losing event identity or evidence;
- the complete package suite and all-target Clippy pass with warnings denied;
- the exact-SHA `hepta-postgres-integration` job and aggregate recovery rerun retain non-empty hosted evidence.

### Block J — complete-suite lint ownership and fail-closed source identity

The complete Hepta package suite is authoritative. Recovery-only testing cannot replace package tests plus all-target Clippy. Paper Raid body/wrapper ownership must remain explicit and immutable.

Required behavior:

- `scripts/check-hepta-lint-ownership.py` validates the exact source-body Git blobs and thin, attribute-free wrappers;
- redundant imports, broad warning allowances, `-A warnings`, module-level lint suppression, and unguarded source splits fail closed;
- every unit and integration target executes before strict Clippy;
- the exact-SHA hosted lane retains the lint contract and complete-suite result.

### Block K — complete workspace module documentation and ownership

The documentation authority covers the actual Cargo workspace rather than only central P0 workstreams. `Cargo.toml` is the source of truth for membership.

Required behavior:

- `docs/module-catalog-v1.json` contains exactly the Cargo workspace members;
- package names match each member's `[package].name`;
- every entry identifies kind, logical module, deployability, maturity, owner, authority, documentation, source entry points, and verification commands;
- every member has one dedicated contract covering purpose, authority, source layout, interfaces, persistence/recovery, configuration/secrets, security, verification, deployment/operations, and compatibility/change protocol;
- every conventional or explicitly declared Cargo target used by the package appears in the catalog and its module document;
- catalog verification commands appear verbatim in the corresponding module document;
- deployable modules define readiness and rollback, while non-deployable crates state deployment non-applicability;
- `docs/modules/index.md` maps all members and all declared external authorities/integration dependencies, including external Agents, Trillionnium World fixture, and Trillionnium Game runtime;
- `scripts/check-module-documentation.py` proves workspace/catalog equality, source existence, target facts, document depth, semantic ownership, navigation, and external boundaries;
- complete workspace module documentation is included in `scripts/check-development-docs.py` and exact-tree integrity evidence.

### Block L — external production evidence intake and anti-self-certification

The repository defines one machine-checkable shape for V12-X1 through V12-X8 while preserving their external, non-self-certifiable status. Structural validation may reject malformed evidence but may never create, infer, waive, or approve the underlying real-world facts.

Required behavior:

- `docs/external-production-evidence-contract-v1.md` defines candidate binding, issuer identity, immutable URI/digest, UTC time, scope, ordering, revocation, retention, and final-human-decision requirements;
- `docs/templates/cex-external-production-evidence-bundle-v1.json` remains an empty shape-only template and denies production authorization;
- real external evidence and the downloaded candidate manifest remain outside the source tree under approved external custody;
- bundle validation requires both `--bundle` and `--candidate-manifest`;
- each supplied file is acquired once through a bounded, no-final-symlink, regular-file read and copied to a private single-read immutable snapshot;
- every parser and child validator consumes those snapshots and the snapshots are rechecked after validation;
- the candidate-manifest snapshot matches its declared SHA-256, passes `scripts/check-release-baseline-manifest.py`, and matches repository, commit/tree, migration `0088_enforce_provider_terminal_evidence_binding.sql`, and artifact scope;
- because the authoritative manifest validator emits diagnostics before its result, the intake checker accepts only its final complete trailing JSON object together with zero exit code, canonical schema, `status=ok`, and the exact snapshot path;
- accepted evidence cannot predate the generated candidate manifest or postdate bundle generation;
- one immutable evidence object cannot be reused across gates;
- no external gate record may reuse a candidate-manifest or repository evidence URI/SHA-256, including candidate payload, hosted evidence, SBOM, provenance, Cargo-lock, or migration evidence;
- a repository-owned evidence identity, including CEX Actions and `artifact://cex-p0-evidence-*`, cannot prove an external production gate;
- V12-X8 `pass` requires exactly one record and `final_human_decision` must be the same immutable record as V12-X8, including URI, digest, issuer, role, scope, candidate identity, and timestamp;
- X8 cannot precede accepted X1-X7 evidence;
- `scripts/check-external-production-evidence-contract.py --self-test` exercises the inherited binding cases plus mixed-output, exact-snapshot, candidate-artifact-isolation, and repository-ownership cases;
- structural validation always reports `checker_may_grant_production_authorization=false` and `production_authorization=not_granted`.

Closure evidence:

- `scripts/check-external-production-evidence-contract.py --contract-only` succeeds;
- `scripts/check-external-production-evidence-contract.py --self-test` succeeds;
- `scripts/check-development-docs.py` executes both without changing the established candidate-manifest schema or 18-requirement count;
- exact-tree hosted evidence retains the successful result;
- real X1-X8 closure remains external and cannot be claimed by Block L.

### Block M — accepted external-Agent-only runtime boundary

The default CEX runtime implements ADR-004 and cannot silently revert to an AI-native execution platform.

Required behavior:

- `services/execution-service` default features are empty;
- the default workspace build does not compile a local provider worker target;
- the retired `/process` route is absent from the default library/router, and the integration test requires `404 Not Found` for that path;
- `providers.rs` contains only provider identity/input/output compatibility types and a fail-closed `external_agent_runtime_required` result; it does not perform HTTP inference, spawn a model CLI, read a prompt, or copy target/prompt/body details into errors;
- historical provider commands, reconciliation artifacts, migration `0088` terminal-evidence guards, and PostgreSQL negative tests remain available for upgrade/audit evidence;
- `execution-provider-dispatch-worker` is excluded from the default build by `legacy-local-provider-dispatch`, rejects every production-like profile, and requires an explicit local-only opt-in;
- no authoritative workflow, deployment manifest, production configuration, or example configuration activates that feature or opt-in;
- the default Execution integration target proves no network/process provider execution, no prompt disclosure, and no retired process route;
- Capability Service consumes only `CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON`, validates a bounded strict field set and stable external Agent identities, reports structured readiness, and performs no local model discovery or CLI execution;
- `scripts/check-external-agent-runtime-boundary.py` validates the decision, source, feature, routing, capability, documentation, activation-surface, runner-probe, and candidate-sequence facts;
- `scripts/check-development-docs.py` invokes the architecture checker and preserves `production_authorization=not_granted`.

Closure evidence:

```text
python3 scripts/check-external-agent-runtime-boundary.py
cargo test -p capability-service --all-targets
cargo clippy -p capability-service --all-targets -- -D warnings
cargo test -p execution-service --test external_agent_boundary
cargo check -p execution-service --all-targets --no-default-features
cargo clippy -p execution-service --all-targets --no-default-features -- -D warnings
```

Block M can close the repository contradiction only. It cannot prove an external Agent deployment, provider legal approval, production credential custody, real reconciliation, downstream World/Game binding, runner allocation, protected-main governance, or final release authorization.

## 2. Documentation completion contract

The active documentation set defines:

1. authority and precedence, with accepted ADRs above implementation plans;
2. component maturity and evidence posture;
3. complete workspace module documentation, source facts, and ownership;
4. the external-Agent-only runtime, absent retired process route, and strict capability registry boundary;
5. Research, Nakama, and Settlement state machines;
6. trust boundaries, threats, controls, and residual risks;
7. clean-deployment acceptance;
8. SLI/SLO and recovery semantics;
9. protocol compatibility, migration, and retirement;
10. requirement-to-code-to-test-to-gate traceability;
11. complete-suite lint ownership;
12. external-evidence intake with exact snapshot, manifest, time, issuer, candidate-artifact isolation, and X8 same-record binding;
13. a strict separation between runner-probe definitions and real non-empty exact-SHA execution evidence.

Documentation is complete only when executable checkers validate the authority set, workspace/catalog bijection, Cargo target facts, dedicated module contracts, external components, ADR-004 runtime boundary, external-evidence source contract, embedded regression self-tests, and referenced paths, and the exact-tree integrity record binds that success to the candidate. Word count, file presence, a README, template, probe definition, or central plan alone is not completion.

## 3. Candidate sequencing

The candidate sequence is:

1. implement source, migration, architecture, documentation, and test changes;
2. freeze one candidate commit/tree through the sole shared trigger;
3. run module documentation, external-Agent runtime boundary, external-evidence contract/self-tests, authority, hygiene, static wiring, lint ownership, and repository integrity;
4. run the five authoritative v12 workflows, including Blocks H through M;
5. run the aggregate candidate workflow on the same SHA;
6. generate and validate immutable evidence, SBOM, provenance, and candidate manifest;
7. report actual runner identity, executed steps, retained logs/artifacts, and branch/ruleset enforcement without inference;
8. stop at repository qualification unless every external production gate has independent evidence.

A later change, including documentation-only or test-only changes, creates a new tree and invalidates earlier exact-SHA evidence.

## 4. External production gates remain upstream blockers

Repository automation cannot close runner allocation; protected-main administration; independent review; representative-volume disaster recovery; real deployment/cutover/rollback; real external Agent/provider outcomes; credential custody; sustained production-like soak; independent security, operations, and financial-control review; legal/commercial/provider approval; downstream immutable World/Game revision binding; or final human go/no-go.

Repository closure therefore yields `REPOSITORY_CLOSED_CANDIDATE`, never production activation.

## 5. Definition of addendum closure

This addendum closes on one exact commit only when:

- all active documents, machine-readable ledgers, module contracts, external component contracts, and external-intake self-tests validate;
- all repository-actionable traceability resolves to executable gates;
- ADR-004 and Block M are enforced by source and checker, with no local inference or retired process route in the default build;
- strict Hepta PostgreSQL recovery and complete-suite lint ownership pass;
- the exact-tree integrity record includes the successful documentation and architecture results;
- all authoritative workflows and the aggregate workflow execute non-empty and succeed on the same SHA;
- no placeholder, temporary convergence workflow, broad warning allowance, fabricated external evidence, waiver, runner-probe substitution, or fabricated approval is used;
- the generated candidate manifest continues to deny production authorization.

External production gates, protected-branch administration, independent review, downstream immutable revision binding, and final human authorization remain separate authorities and must be evidenced truthfully.

# Clean-deployment and first-playable acceptance

Status: active executable acceptance contract

This document defines repository-level acceptance from empty infrastructure. It does not claim that CI is a representative production deployment.

## 1. Immutable inputs

Record before execution:

- repository, branch, commit SHA and tree SHA;
- active plan, addendum, base traceability and P0 blocker traceability digests;
- `Cargo.lock` digest;
- migration head and full migration-chain digest;
- workflow, module catalog, compatibility inventory and canonical-document digests;
- container image or runner identity;
- PostgreSQL major version;
- actual GitHub Ruleset state, reported without inference.

Any source change creates a new candidate and invalidates prior exact-tree qualification.

## 2. Static preflight

Run, in order:

```bash
python3 scripts/check-module-documentation.py
python3 scripts/check-capability-production-posture.py
python3 scripts/check-authoritative-money-isolation.py
python3 scripts/check-project-boundary.py
python3 scripts/check-source-governance.py
python3 scripts/check-development-docs.py
python3 scripts/check-hepta-lint-ownership.py
python3 scripts/check-p0-release-candidate-hygiene.py
python3 scripts/check-p0-wiring.py
python3 scripts/check-repository-integrity.py --output run/repository-integrity.json
cargo fmt --all --check
```

Failure is terminal for that candidate; no later successful test may override it.

## 3. Capability authority acceptance

For beta, staging and production, start `capability-service` only with an explicit runtime profile and non-empty validated static registry. Acceptance requires unique IDs, non-empty authority fields, at least one enabled record, configurable bind address and rejection of demo/placeholder/local OpenClaw authority. Invalid configuration must exit before serving with code 78.

Development discovery remains non-authoritative and cannot be used as production evidence.

## 4. Core exact-money database acceptance

Against fresh PostgreSQL 16:

1. apply the complete numbered migration chain through `0084_make_provider_reconciliation_replay_terminal_safe.sql`;
2. execute representative existing-row upgrade paths;
3. verify exact account opening, append-only effects, replay/collision and insufficient-funds recovery;
4. verify Invocation contract lifecycle and terminal mutual exclusion;
5. verify Gateway reserve, Execution settlement and provider reconciliation fault matrices;
6. verify authenticated Audit baseline/delivery and TRNM response-loss receipt recovery;
7. run the bounded exact Ledger soak and CI dump/restore comparison;
8. prove all authoritative money files contain no binary floating-point type or lossy conversion;
9. prove every remaining value-related float file is explicitly classified as compatibility, display or non-production test state.

The five authoritative workflows are the hosted implementation of these steps. Legacy compatibility presence never grants write authority.

## 5. Project-boundary and source-governance acceptance

Acceptance requires:

- the embedded World source inventory and Matrix command carrier match their exact frozen Git blobs;
- no new World source file, authoritative writer or external sibling Cargo path dependency exists;
- Consumer/Matrix module contracts identify World behavior as compatibility-only;
- `.github/CODEOWNERS` owns workflows, migrations, authority, exact money and project boundaries;
- no workflow has `contents: write` or executes `git push`;
- no temporary closure trigger or convergence workflow remains.

The freeze prevents repository-local scope growth. It does not prove the external World authority transfer. Likewise, source policy does not prove that GitHub has activated the required `main` Ruleset; actual settings remain an independently observed blocker.

## 6. Hepta durable acceptance

Provide a dedicated PostgreSQL URL and force strict execution:

```bash
export HEPTA_TEST_DATABASE_URL='postgres://user:password@127.0.0.1:5432/hepta_test'
export HEPTA_REQUIRE_POSTGRES_TESTS=1
python3 scripts/check-hepta-lint-ownership.py
bash scripts/check-hepta-postgres-integration.sh --mode full
```

Acceptance requires:

- exact source-body lint ownership validates before Cargo execution;
- fresh schema initialization succeeds;
- `/ready` reports database reachability and valid security configuration;
- two service instances persist disjoint Agent records;
- pending outbox metrics are exposed;
- restart preserves durable profiles;
- concurrent workers claim different events;
- wrong-owner acknowledgement is rejected;
- an expired worker lease is reclaimed and acknowledged by a new worker;
- every unit and integration target passes;
- all-target Clippy passes with `-D warnings` and no broad warning allowance.

A missing database URL in strict mode is a failure, not a skip. A recovery-only result is supporting evidence and cannot substitute for the full package lane.

## 7. Paper Raid first-playable evidence

A production-like product rehearsal must eventually demonstrate three humans and three independently keyed Agents progressing through team consent, preregistration, research, experiment, draft, integrity review, reproduction, author approval and immutable `PaperBundleV2`; one complete signed Nakama session; and pending or independently verified finality. CI contract/golden tests support this path, but the live multi-service rehearsal remains external until its topology and artifacts are independently recorded.

## 8. Release-candidate acceptance

The aggregate gate must:

- check exact branch-tree event identity;
- rerun repository integrity and strict Hepta recovery;
- wait for all five authoritative exact-SHA gates;
- upload the evidence payload;
- generate a non-template candidate manifest with real run IDs and artifact digests;
- validate SBOM and provenance;
- record actual branch/ruleset and World-transfer state;
- deny production authorization.

## 9. Fail-closed conditions

Reject the candidate on any placeholder hash/run ID, missing gate, different head SHA, migration-head mismatch, skipped strict database test, incomplete Hepta package lane, unguarded lint suppression, unpinned third-party Action, temporary remediation workflow, write-capable workflow, source-pushing workflow, undocumented workspace member, invalid Capability registry authority, unclassified value float, floating authoritative money, expanded frozen World surface, external path dependency, legacy monetary write, ambiguous provider retry, mutable evidence or inferred external approval.

## 10. External deployment and administration acceptance

Real service identities, network policy, secret custody, representative data volume, object storage, provider endpoints, rollback, sustained load, cross-repository World transfer, actual GitHub `main` Ruleset enforcement and human approvals are outside repository self-certification. Their absence yields `BLOCKED_UPSTREAM`, not an invented pass.

# Clean-deployment and first-playable acceptance

Status: active executable acceptance contract

This document defines repository-level acceptance from empty infrastructure. It does not claim that CI is a representative production deployment.

## 1. Immutable inputs

Record before execution:

- repository, branch, commit SHA and tree SHA;
- active plan and addendum digests;
- `Cargo.lock` digest;
- numbered migration head and full numbered migration-chain digest;
- workflow and canonical-document digests;
- module catalog and complete workspace-member count;
- external-production evidence contract/template digests;
- container image or runner identity;
- PostgreSQL major version.

Any source change creates a new candidate and invalidates prior exact-tree qualification.

## 2. Static preflight

Run, in order:

```bash
python3 scripts/check-module-documentation.py
python3 scripts/check-external-production-evidence-contract.py --contract-only
python3 scripts/check-development-docs.py
python3 scripts/check-hepta-lint-ownership.py
python3 scripts/check-p0-release-candidate-hygiene.py
python3 scripts/check-p0-wiring.py
python3 scripts/check-repository-integrity.py --output run/repository-integrity.json
cargo fmt --all --check
```

The module check must prove an exact bijection between `Cargo.toml` workspace members and `docs/module-catalog-v1.json`, a dedicated technical contract for every member, existing catalog-bound source entry points, complete ownership and external-authority boundaries, and a non-normative root navigation page.

The external-evidence contract check validates only the source-tree shape, empty template, gate ordering, and anti-self-certification boundary. It cannot prove or close V12-X1 through V12-X8 and must continue to report `production_authorization=not_granted`.

Failure is terminal for that candidate; no later successful test may override it.

## 3. Core exact-money database acceptance

Against fresh PostgreSQL 16:

1. apply the complete numbered migration chain through `0088_enforce_provider_terminal_evidence_binding.sql`;
2. when the TRNM economy settlement lane is in scope, apply the service-owned bootstrap at `services/trnm-economy-service/migrations/settlement_v1.sql`; this migration is intentionally outside the numbered chain;
3. execute representative existing-row upgrade paths;
4. verify exact account opening, append-only effects, replay/collision and insufficient-funds recovery;
5. verify Invocation contract lifecycle and terminal mutual exclusion;
6. verify Gateway reserve, Execution settlement and provider reconciliation fault matrices;
7. verify authenticated Audit baseline/delivery and TRNM response-loss receipt recovery;
8. run the bounded exact Ledger soak and CI dump/restore comparison.

The five authoritative workflows are the hosted implementation of these steps.

## 4. Hepta durable acceptance

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

## 5. Paper Raid first-playable evidence

A production-like product rehearsal must eventually demonstrate three humans and three independently keyed Agents progressing through team consent, preregistration, research, experiment, draft, integrity review, reproduction, author approval and immutable `PaperBundleV2`; one complete signed Nakama session; and pending or independently verified finality. CI contract/golden tests support this path, but the live multi-service rehearsal remains external until its topology and artifacts are independently recorded.

## 6. Release-candidate acceptance

The aggregate gate must:

- check exact branch-tree event identity;
- rerun module documentation, external-evidence contract, repository integrity, and strict Hepta recovery;
- wait for all five authoritative exact-SHA gates;
- upload the evidence payload;
- generate a non-template candidate manifest with real run IDs and artifact digests;
- validate SBOM and provenance;
- record actual branch/ruleset state;
- deny production authorization.

## 7. Fail-closed conditions

Reject the candidate on any placeholder hash/run ID, missing gate, different head SHA, migration-head mismatch, skipped strict database test, incomplete workspace-module contract, malformed external-evidence contract, source-committed live external evidence, incomplete Hepta package lane, unguarded lint suppression, unpinned third-party Action, temporary remediation/convergence workflow, legacy monetary write, ambiguous provider retry, mutable evidence, waiver, or inferred external approval.

## 8. External deployment acceptance

Real service identities, network policy, secret custody, representative data volume, object storage, provider endpoints, rollback, sustained load and human approvals are outside repository self-certification. Their absence yields `BLOCKED_UPSTREAM`, not an invented pass. A structurally valid external bundle is accepted only after its responsible human control owners independently verify issuer identity, signatures, scope, underlying activity, retention, and revocation state.

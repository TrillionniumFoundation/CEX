# Repository contract inventory v1

Status: active repository-fact contract  
Owner: platform foundations  
Production authorization: `not_granted`

## Purpose

`python3 scripts/check-repository-contract-inventory.py` derives one deterministic inventory directly from the checked-out candidate tree. It closes the visibility gap caused by maintaining API, configuration, Cargo-target and database-object lists only in prose. The inventory is evidence about source shape; it is not an API compatibility approval, production deployment record or human authorization.

## Derived facts

For every Cargo workspace member the inventory records the package name, conventional, explicit and auto-discovered Cargo targets, literal Axum route registrations, environment-variable reads, CEX metric names and unresolved non-literal route calls. It also scans repository and member migrations for created tables, views, materialized views, functions, types and triggers.

Library, binary, build-script and explicitly declared targets must appear in `docs/module-catalog-v1.json`. Auto-discovered tests, examples and benches are retained in the generated facts even when they are not selected as module entry points. A missing required target, malformed workspace member, package mismatch or unreadable source fails the check.

## Extraction limits

The scanner is source based and deliberately conservative. Macro-generated routes, dynamically assembled paths and SQL generated outside tracked migration files are reported only through unresolved counts or downstream tests. It does not infer authentication, tenancy, authorization, data ownership or backward compatibility from a route name. Those semantics remain in module contracts, protocol specifications and executable integration tests.

Comments are excluded from route, configuration and metric extraction. Rust string literals are decoded only for the bounded identifiers used by the inventory. SQL comments are removed before object matching. The output has no wall-clock timestamp, host path, network result or mutable branch identity, so equal source bytes produce equal JSON.

## Commands

```text
python3 scripts/test-repository-contract-inventory.py
python3 scripts/check-repository-contract-inventory.py --output run/repository-contract-inventory.json
```

The first command exercises chained methods, comments, unresolved routes, environment and metric extraction, Cargo target coverage, SQL objects, deterministic output and path-escape rejection. The second command writes the complete inventory and emits a small machine-readable check result.

## CI and evidence

The `repository-integrity` job runs both commands on the exact candidate checkout and uploads `run/repository-contract-inventory.json` beside the repository-integrity record. A run with no allocated runner, no executed steps, no retained artifact or a different SHA has no qualification value.

The inventory feeds subsequent Consumer Entry and Matrix decomposition work by providing a code-derived baseline. It does not itself prove durable cursor fencing, outbox recovery, World/Game authority separation, API compatibility, representative-volume recovery or external review.

## Change protocol

Changes to Cargo membership, target layout, routes, configuration reads, metric names, migrations or inventory extraction require regression updates and a new candidate freeze. The checker always emits `checker_may_grant_production_authorization=false` and `production_authorization=not_granted`.

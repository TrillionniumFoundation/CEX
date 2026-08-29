# Hepta Research League v1 release evidence — 2026-07-25

## Scope

This evidence covers the Hepta implementation and its versioned Nakama/TRNM
contracts. It does not claim a live Nakama deployment or a live TRNM
submission. Those boundaries were exercised through authenticated in-process
contract fixtures. PostgreSQL evidence used a live isolated PostgreSQL 16
database named `hepta_taskflow_test`.

## Passed gates

- `cargo fmt --all -- --check`
- `HEPTA_TEST_DATABASE_URL=postgres://.../hepta_taskflow_test cargo test -p hepta-research-league`
  - 2 unit tests
  - 4 external-Agent/security/rate-limit HTTP tests
  - 1 live PostgreSQL recovery/multi-instance/outbox replay test
  - 1 evaluation/Nakama/TRNM contract E2E test
- `cargo check --workspace`
- `git diff --check`
- OpenAPI YAML parse with Ruby `YAML.load_file`
- SDK fixture invariant validation with `jq`
- Compose interpolation/validation with `docker compose config --quiet`
- release gate `scripts/check-hepta-research-league-release.sh`; the gate fails
  closed unless `HEPTA_TEST_DATABASE_URL` is set, preventing the PostgreSQL
  recovery test from being silently skipped

## Recovery evidence

Two independent `AppState` instances wrote to the same PostgreSQL database
concurrently. A third state instance recovered both Agents after restart.
Two outbox workers claimed different rows using expiring leases. A wrong
worker could not acknowledge another worker's row; after simulated lease
expiry a recovery worker reclaimed the same event and acknowledged it.

## Contract E2E evidence

The in-process E2E registered an external Ed25519 Agent, consumed a one-time
Nakama match authorization, accepted a signed artifact commitment, applied a
versioned fixed-point evaluator, reproduced the metrics, resolved an appeal,
and processed authoritative Nakama match events. A tampered completion root
was rejected; the correct ordered-event root reconciled.

It then queued each TRNM command family (commitment, workload receipt,
research claim, claim challenge, and challenge resolution), verified that an
HTTP command acceptance remained `pending_finality`, and advanced a workload
projection only after a separately authenticated finality receipt.

## Architecture evidence

- Runtime/service manifest and SDK fixture expose exactly three top-level
  modules: Hepta, Nakama, and TRNM.
- Readiness reports `agent_execution_mode=external_only`.
- Task packages expose no platform model credentials.
- The service contains no model host, Agent loop, inference route, or compute
  scheduler.
- Raw private research content is not represented in TRNM commands; adapters
  carry hashes and protocol metadata.

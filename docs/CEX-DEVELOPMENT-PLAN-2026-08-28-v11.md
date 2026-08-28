# CEX Development Plan v11 — Durable Execution Ledger Settlement

- Status: Active implementation candidate
- Date: 2026-08-28
- Integration branch: `feature/hepta-production-baseline-p0`
- Source baseline: `ff13a4f27b05dfaaf9c7cce4905f670a5f61f85f`
- Canonical migration head: `0067_add_execution_ledger_settlement_commands.sql`
- Release posture: Draft integration candidate; not production-ready

## 1. Facts corrected before this slice

The canonical remote baseline remained plan v10, migration 0066 and the isolated exact Execution
adapter. Earlier claims that v11/0067 had already landed were not present in the repository and are
not treated as evidence. This plan supersedes v10 only when the accompanying 0067 patch is applied
to the exact source baseline and passes required checks.

## 2. P0-N5 delivered by this candidate

This slice adds:

- immutable consume/refund settlement commands bound to the 0066 contract;
- exact request snapshot and request fingerprint;
- shadow/active rollout;
- short claim transaction, worker lease and bounded attempt budget;
- Ledger network call after claim commit;
- separate verified outcome transaction;
- verified receipt and receipt hash;
- explicit `retry_wait`, `reconcile_required` and `dead_letter` states;
- final-attempt unknown outcome protection;
- operator acknowledgement and bounded requeue evidence;
- append-only transition history, Audit intents and status projection;
- static architecture gate, PostgreSQL lifecycle gate, CI workflow and systemd deployment unit.

## 3. Activation boundary

The worker is deployable in shadow posture, but `api.rs` must not directly call
`settle_invocation`. Existing provider and legacy Ledger calls remain inside business SQL
transactions. Activating the exact adapter there would reintroduce the very lock/crash defect P0-N5
closes and may create double settlement.

Caller activation requires all of the following in one reviewed slice:

1. the source state transition and command enqueue commit together;
2. the corresponding legacy Ledger call is removed for that exact path;
3. only the independent worker performs the side effect;
4. Execution advances only from verified command evidence;
5. replay, cancellation and terminal mutual exclusion remain deterministic.

## 4. Next locked slice — Gateway exact registration and reserve

Deliver:

1. exact Invocation ingress using currency, scale and string minor units;
2. exact/legacy dual-input rejection before any upstream call;
3. 0066 contract registration in the Invocation source transaction;
4. durable reserve command/outcome or an equivalent short-transaction orchestration boundary;
5. canonical `/v2/ledger/effects` reserve with stable operation identity;
6. explicit unknown-result reconciliation;
7. exact contract propagation into Execution;
8. same-transaction enqueue of 0067 consume/refund command at the terminal decision point;
9. removal of the matching legacy `/v1/ledger/*` side effect from exact paths;
10. `legacy_v1` / `dual` / `require_v2` telemetry and production-like compatibility rejection.

## 5. Following P0 sequence

### P0-N2 Genesis-as-entry

- nonzero account creation becomes one same-transaction genesis grant effect;
- exact replay returns the original account/effect evidence;
- zero-value creation records explicit no-value semantics;
- historical nonzero balances receive inventory/reconciliation evidence, never fabricated history.

### P0-N3 Money v2 read cutover

- backfill and dual-read comparison;
- mismatch metrics and operational stop conditions;
- authoritative minor-unit read switch;
- remove value-bearing binary floating point;
- rollback and reconstruction evidence.

### P0-N4 Projection rebuild and reconciliation

- rebuild account balance/reserved projections from append-only authoritative entries;
- detect drift, quarantine unsafe repair and emit immutable repair evidence;
- property/concurrency tests for reserve/consume/refund/grant.

### P0-N6 Provider dispatch separation

- durable provider command and receipt;
- provider network call after claim commit;
- unknown-result/replay semantics;
- no prompt in argv and minimal child environment;
- terminal workflow coordination with 0067 settlement.

## 6. Security and operations following the transaction work

- least-privilege PostgreSQL roles for source enqueue, worker transition and operator recovery;
- authenticated operator APIs for acknowledgement/requeue;
- signed external Audit chain-head checkpoints;
- backup/restore, reconciliation and dead-letter drills;
- SLOs for queue age, lease expiry, unknown outcomes and projection drift.

## 7. Definition of done for P0-N5

P0-N5 is complete only when:

- 0067 applies from a fresh database and an upgrade baseline;
- Rust formatting, unit tests and all-target workspace compile pass;
- concurrent claim and exact replay behavior pass against PostgreSQL;
- crash after remote success/before local receipt is injected and recovered;
- final-attempt lease expiry produces `reconcile_required`, not assumed failure;
- operator acknowledgement/requeue requires fresh evidence per incident;
- required GitHub jobs allocate runners and execute every step;
- a non-template release manifest binds reviewed source, migration, binaries and test artifacts.

Source presence, static inspection or a historical local PASS is not sufficient.

## 8. Merge and release rule

PR #1 remains Draft and unmerged while required checks have no trusted execution evidence. No P0
candidate may be called production-ready until the exact commit/tree, migration head, SBOM,
provenance, restore evidence and approvals are recorded in a release manifest.

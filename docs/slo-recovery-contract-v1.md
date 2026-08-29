# SLO, observability and recovery contract

Status: active target contract; production targets are not yet qualified

## 1. Evidence classes

- `repository regression`: bounded CI execution proving deterministic behavior on one tree.
- `production-like qualification`: sustained execution on representative topology and volume.
- `production observation`: live SLI measurements under approved activation.

Only the first class is repository-self-certifiable.

## 2. Required SLIs

| Domain | SLI | Required dimensions | Failure signal |
|---|---|---|---|
| Gateway | exact reserve success/error latency | tenant, operation kind, upstream class | reserve backlog, collision or receipt-validation failure |
| Execution | settlement command age and terminal outcome | consume/refund, attempt, worker | lease expiry, retry exhaustion, reconciliation required |
| Provider | unknown-outcome inventory | provider, model/capability, age | indeterminate artifact or unacknowledged requeue |
| Audit | outbox depth and oldest event age | source service, attempt | delivery backlog or baseline pressure block |
| Ledger | projection parity and exact replay | account/currency scale/operation kind | parity mismatch, overflow or duplicate content collision |
| Hepta | outbox/control age, claim attempts and storage backend | command/event kind, worker | expired lease, repeated failure or non-PostgreSQL backend |
| Finality | pending age and verifier result | protocol version, trust anchor | stale pending, challenge or verification failure |
| Release | exact-SHA gate completeness | workflow path, run ID, conclusion | missing/different-SHA/failed hosted evidence |

## 3. Initial targets

The following are design targets, not achieved-SLO claims:

- no loss of committed exact Ledger, Audit, or Hepta facts (`RPO target: 0` for committed database state);
- deterministic replay after process restart;
- command/outbox recovery before lease plus bounded operator response;
- no automatic retry after a potentially executed provider side effect;
- release qualification only after all exact-SHA hosted gates complete successfully;
- recovery procedures preserve operation identity and append evidence rather than mutate history.

Numeric latency, availability, queue-age, RTO and endurance thresholds must be frozen from representative-volume qualification. They must not be copied from bounded CI timing.

## 4. Recovery matrix

| Incident | Automatic action | Operator action | Forbidden action |
|---|---|---|---|
| worker crash before remote I/O | lease expiry and reclaim | inspect attempts if repeated | create a second operation identity |
| timeout after possible provider I/O | enter `reconcile_required` | attach immutable outcome artifact | blind automatic retry |
| Ledger response loss | exact receipt lookup/replay | verify operation/content identity | infer success from transport status |
| Audit delivery failure | bounded retry/dead-letter | acknowledge and requeue under policy | delete append-only intent |
| Hepta outbox worker loss | reclaim expired lease | inspect oldest age and attempts | wrong-owner acknowledgement |
| Chain unavailable | remain `pending_finality` | restore verifier/connectivity | label paper/economics finalized |
| evidence artifact unavailable | fail candidate or hold workflow | restore by verified digest | substitute mutable URI/content |
| database loss | restore and compare exact state | execute independent DR plan | activate from unverified restore |

## 5. Backup and restore

CI custom-format dump/restore must compare row counts, exact balances, operation identity and content hashes. Production qualification additionally requires representative volume, real storage, encryption/key custody, retention, restore timing and application-level reconciliation. The latter remains an external gate.

## 6. Alert and runbook requirements

Every paging signal must identify owner, severity, threshold source, diagnostic query, safe mitigation, rollback boundary and evidence to retain. Break-glass actions must be separately authorized and auditable. A dashboard or green health endpoint alone is not an SLO.

# Execution Ledger Settlement Commands v1

- Status: P0 implementation candidate
- Source baseline: `feature/hepta-production-baseline-p0@ff13a4f27b05dfaaf9c7cce4905f670a5f61f85f`
- Migration series:
  - `0067_add_execution_ledger_settlement_schema.sql`
  - `0068_add_execution_ledger_settlement_guards.sql`
  - `0069_add_execution_ledger_settlement_enqueue.sql`
  - `0070_add_execution_ledger_settlement_claim.sql`
  - `0071_add_execution_ledger_settlement_finish.sql`
  - `0072_add_execution_ledger_settlement_operator.sql`
- Worker: `execution-settlement-worker`
- Canonical Ledger target: `POST /v2/ledger/effects`
- Exact request authority: `cex_invocation_ledger_effect_request_v1`

## 1. Objective

Execution currently performs provider and Ledger network calls while a business SQL transaction
and locked Execution/Invocation rows remain open. The isolated exact settlement adapter cannot be
safely activated inside that transaction. The 0067–0072 migration series introduces a durable
consume/refund command and a worker that performs Ledger I/O only after the claim transaction
commits.

The target sequence is:

```text
short source transaction
  └─ enqueue immutable settlement command
commit
short worker claim transaction
commit
Ledger HTTP outside every business transaction
short verified outcome transaction
  └─ receipt / retry / reconcile-required / dead-letter
commit
```

This slice deliberately does **not** change `api.rs` to call the adapter. Caller activation is a
separate cutover and remains fail-closed until it can remove the corresponding legacy side effect.

## 2. Immutable command contract

`cex_execution_ledger_settlement_commands_v1` binds one terminal intent to one Invocation:

- deterministic command ID derived from Invocation ID;
- Invocation, Execution and organization IDs;
- `consume` or `refund` action;
- deterministic 0066 operation ID;
- immutable 0066 contract hash;
- exact Ledger v2 request snapshot;
- canonical request SHA-256 fingerprint;
- source service/principal and schema version;
- `shadow` or `active` execution mode.

A database `BEFORE INSERT` trigger re-resolves the Execution row and 0066 contract. Direct inserts
that disagree on tenant, trace, action, operation identity, request projection or fingerprint fail
closed. Immutable fields cannot be updated after insertion.

Only one settlement command may exist per Invocation. An exact enqueue replay returns the existing
command; a consume/refund conflict or any other immutable drift raises a unique collision.

## 3. State machine

```text
pending ──────┐
              ├─ claim ─> claimed ─> succeeded
retry_wait ───┘                  ├─ retry_wait
                                 ├─ reconcile_required
                                 └─ dead_letter

reconcile_required/dead_letter
  └─ explicit acknowledgement + bounded requeue ─> pending
```

Additional rules:

- workers can only claim `active` commands;
- an untouched `shadow` command may be promoted once;
- claim uses `FOR UPDATE SKIP LOCKED`;
- attempt count increments in the short claim transaction;
- expired claims below the attempt budget return to `retry_wait` for exact replay;
- an expired claim after the final attempt becomes `reconcile_required`, never an assumed failure;
- terminal receipt evidence and transition evidence are append-only;
- commands cannot be deleted.

## 4. Worker transaction boundary

The worker claims commands through an autocommit SQL function and receives a finite lease. It then
calls the isolated exact adapter with no explicit `sqlx::Transaction` alive. The outcome is written
through a separate autocommit SQL function.

Batch size is constrained by a serial lease-budget calculation:

```text
batch_size × (HTTP timeout + DB margin) + safety margin < lease
```

This prevents later commands in a claimed batch from starting after their lease has effectively
expired. Defaults are batch 2, request timeout 20 seconds and lease 90 seconds.

If Ledger succeeds but local receipt persistence fails, the worker does not rewrite the command or
claim success. It leaves the claim intact. After lease expiry the database either schedules an exact
replay or, on the final attempt, records `reconcile_required` for operator inspection.

## 5. Verified receipt

A successful HTTP status alone is insufficient. Before `succeeded`, PostgreSQL verifies:

- account ID;
- trace ID;
- operation ID and action;
- scoped idempotency key;
- exact amount minor units;
- currency unit and scale;
- non-nil Ledger entry ID;
- 0066 contract hash;
- 0066 terminal status (`consumed` or `refunded`);
- 0066 last operation and entry evidence.

The canonical receipt receives its own SHA-256 digest. Exact completion replay must supply the same
receipt and replay flag or it fails as a collision.

## 6. Outcome classification

| Adapter outcome | Durable command state | Meaning |
|---|---|---|
| Verified first apply / exact replay | `succeeded` | Receipt and 0066 evidence match |
| Explicit retryable response | `retry_wait` | Exact operation can be replayed |
| Timeout, ambiguous transport, invalid receipt | `reconcile_required` | Remote outcome cannot be asserted |
| Permanent validation/auth/tenancy/collision rejection | `dead_letter` | Operator review required |
| Legacy fallback returned unexpectedly | `reconcile_required` | Exact worker configuration/contract drift |

When a retryable response consumes the final automatic attempt, the worker records
`reconcile_required` rather than dead-lettering an outcome it cannot prove.

## 7. Operator recovery

`cex_acknowledge_execution_ledger_settlement_v1` records actor, reason and time for
`reconcile_required` or `dead_letter` commands. Exact acknowledgement replay is allowed; different
acknowledgement content collides.

`cex_requeue_execution_ledger_settlement_v1` requires that acknowledgement, adds a bounded number of
attempts, records requeue evidence, increments the requeue counter and clears the live
acknowledgement. A later incident therefore requires a fresh explicit acknowledgement rather than
silently inheriting an old one. Replaying the identical requeue request after a lost database
response returns the existing requeue result without adding attempts twice; different content
fails as a collision.

## 8. Audit and observability

Command creation, promotion, lease recovery, durable outcomes, acknowledgement and requeue all
write authenticated Audit v2 intents in the same database transaction as their state change.

`cex_execution_ledger_settlement_status_v1` reports counts by mode/action/status, exhausted budgets,
unacknowledged operator items, oldest ready command, oldest claim lease and oldest outstanding
operator item.

## 9. Configuration

Required:

```text
DATABASE_URL
CEX_RUNTIME_PROFILE or APP_ENV
CEX_EXECUTION_LEDGER_MODE=dual|require_v2
LEDGER_ADMIN_TOKENS_JSON or another ledger:manage credential source
```

Worker settings:

```text
CEX_EXECUTION_SETTLEMENT_WORKER_ID
CEX_EXECUTION_SETTLEMENT_BATCH_SIZE
CEX_EXECUTION_SETTLEMENT_LEASE_SECONDS
CEX_EXECUTION_SETTLEMENT_POLL_SECONDS
CEX_EXECUTION_SETTLEMENT_REQUEST_TIMEOUT_SECONDS
CEX_EXECUTION_SETTLEMENT_DATABASE_MAX_CONNECTIONS
CEX_EXECUTION_SETTLEMENT_RUN_ONCE
```

Production-like profiles reject weak/default Ledger credentials. HTTP redirects are disabled and
request time is bounded.

## 10. Activation and rollback

The expand migration series is safe to deploy while all commands remain `shadow`. The worker
ignores shadow commands. Rollout is:

1. deploy migrations 0067–0072, the worker binary and metrics;
2. enqueue shadow commands from a staging caller and compare intent/projection;
3. promote selected commands to active;
4. validate first-apply, exact-replay and unknown-outcome recovery;
5. only then activate caller-side enqueue and remove the matching legacy side effect.

Rollback stops the worker and keeps commands/evidence intact. The migrations are append-only;
tables, commands and receipts must not be dropped to simulate rollback.

## 11. Evidence required before merge

- Rust formatting, compile and unit tests on the exact commit/tree;
- fresh and upgrade PostgreSQL execution of all migrations through 0072;
- command lifecycle and concurrency probe;
- remote-success/local-receipt-failure crash injection;
- worker restart and lease-expiry replay;
- token rotation and cross-tenant negative tests;
- required GitHub checks with actual allocated runners;
- release manifest bound to source, migration head and artifacts.

# Existing-Row Audit Baseline Backfill v1

- Status: implementation candidate
- Migration: `0064_add_audit_source_baseline_backfill.sql`
- Operator entry point: `scripts/backfill-audit-source-baselines.sh`
- Sources:
  - `execution-service`
  - `identity-service`

## Purpose

Migrations 0062 and 0063 protect new and meaningfully changed rows, but rows created before
those triggers retain `audit_revision = 0`. This job creates a tamper-evident snapshot intent
without flooding the outbox during schema migration.

## Transaction invariant

For every selected source row:

```text
revision 0 -> revision 1
        +
persisted.baseline Audit v2 intent
        +
durable progress update
        =
one PostgreSQL transaction
```

A trigger or enqueue failure rolls back all three effects.

## Baseline event types

- `execution.persisted.baseline`
- `identity.api_key.persisted.baseline`

Execution snapshots include lifecycle/lease/attempt/approval facts and a digest of provider
result payload. They do not copy provider output.

Identity snapshots include non-secret key provenance and status. They never include the raw
API key or `key_hash`.

## Bounded execution

The database function accepts:

- source service;
- worker id;
- batch size, 1–1000;
- maximum nonterminal Audit outbox backlog.

One transaction processes at most one batch. An advisory transaction lock serializes workers
per source. `FOR UPDATE SKIP LOCKED` prevents waiting on a row already owned by another
transaction.

The shell runner is intentionally bounded. Its default is one batch per source:

```bash
DATABASE_URL=postgres://... \
  scripts/backfill-audit-source-baselines.sh \
  --source all \
  --batch-size 100 \
  --max-outbox-backlog 5000 \
  --max-batches 1
```

Operators must observe dispatcher throughput and oldest outbox age before raising the batch
or transaction count.

## Restart and replay

Selection is based on authoritative `audit_revision = 0`, not only the stored cursor.
Therefore:

- a committed row is not selected again;
- a rolled-back row remains eligible;
- deterministic event IDs make an already-present exact intent safe;
- `last_source_id` is evidence/progress metadata, not the sole correctness boundary.

A completed source may be invoked again; it returns `processed=0`.

## Backlog protection

Before mutating a source row, the function counts outbox rows in:

- `pending`
- `claimed`
- `retry_wait`

When the configured ceiling is reached, the source is marked `blocked`; no source row or
intent is changed. Reinvocation resumes after the backlog drops.

## Durable progress

`cex_audit_source_baseline_progress_v1` records:

- status;
- worker;
- last source id;
- cumulative and last-batch counts;
- actual remaining count at the last transaction;
- outbox backlog/limit;
- start, batch, block and completion times;
- last error metadata.

`cex_audit_source_baseline_status_v1` joins durable progress with live revision-0 counts and
baseline-intent counts.

Inspect status:

```bash
DATABASE_URL=postgres://... \
  scripts/backfill-audit-source-baselines.sh --status
```

## Exit criteria

Production promotion requires for both sources:

- status `complete`;
- live `actual_remaining_count = 0`;
- no unresolved baseline collision;
- dispatcher backlog/age within SLO;
- fresh and supported-upgrade tests;
- restart and rollback atomicity evidence;
- reviewed waiver for any intentionally excluded row.

## Rollback posture

The migration is expand-only. Do not delete baseline intents or decrement revisions to roll
back an application deployment. Stop the runner and leave durable evidence intact. Any
future contract change must append a new event/schema version.

## Security boundary

The first slice relies on a transaction-local custom setting to authorize the special
revision `0 -> 1` transition. Least-privilege database roles must later restrict execution of
the backfill function and direct table writes. The function itself enforces source allow-list,
worker length, batch bounds, backlog bounds and per-source serialization.

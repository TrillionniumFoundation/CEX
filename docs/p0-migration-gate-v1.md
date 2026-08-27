# P0 Migration Gate v1

## Static contract

Run:

```bash
python3 scripts/check-p0-migrations.py
```

It verifies:

- numbered filenames and a contiguous `0001..HEAD` sequence;
- non-empty lowercase files;
- release manifest template points to the actual migration head;
- P0 migrations (`0055+`) are transaction-wrapped;
- destructive operations require an explicit reviewed marker;
- every created/replaced function declares `SET search_path`;
- `SECURITY DEFINER` cannot appear without explicit search-path control.

The static checker is intentionally conservative. A destructive migration marker does not approve the operation; it only makes the required review visible.

## PostgreSQL contract

Run only against a disposable database:

```bash
DATABASE_URL=postgres://.../cex_test \
  bash scripts/check-p0-migrations-postgres.sh
```

The script refuses databases whose name does not contain `test`, `ci`, `scratch` or `tmp`, unless the explicit emergency override is set.

It applies every numbered migration in order and then runs transactionally rolled-back assertions for:

- exact Money v2 numeric/minor synchronization;
- ledger entry minor units;
- shadow saga commands cannot be claimed;
- active saga command claim and lease;
- audit v2 exact replay;
- event-id collision rejection;
- tenant sequence and previous-hash linkage;
- append-only mutation denial;
- durable audit outbox claim and attempt state.

## Hosted workflow

`.github/workflows/p0-migration-gate.yml` provisions PostgreSQL 16 and runs both checks.

This is a fresh-schema gate. The next required workflow must also restore representative supported old schemas and apply only the upgrade suffix, because a fresh apply cannot reveal all upgrade/backfill issues.

## Evidence

A passing run should be attached to the release baseline manifest as `migration-fresh`. Upgrade fixtures and rollback/roll-forward drills require separate evidence records.

# TRNM Economy Disaster-Recovery Runbook v1

Updated: 2026-07-14

This runbook covers the persistent single-node TRNM economy profile. It does
not claim multi-region high availability or public-market operational
readiness.

## Automated gate

Run:

```bash
scripts/check-trnm-economy-disaster-recovery.sh
```

The gate creates a custom-format logical backup of the organizations,
accounts, ledger and `trnm_*` tables, restores it into an isolated database,
and compares authoritative intent, receipt, escrow and identity row counts. It
then starts a second ledger process against the primary PostgreSQL database and
races the same intent through both processes. The PostgreSQL advisory lock and
unique constraints must produce one intent, one receipt, one ledger entry and
one balance change.

The temporary restore database and second ledger are removed on exit. The
primary services and database remain authoritative.

For physical recovery run:

```bash
scripts/check-trnm-postgres-pitr-failover.sh
scripts/check-trnm-postgres-pitr-failover.sh --chaos
scripts/check-trnm-postgres-streaming-standby-chaos.sh
```

Create and verify a persistent same-host base-backup cache with:

```bash
scripts/create-trnm-postgres-base-backup.sh
scripts/check-trnm-postgres-base-backups.sh
TRNM_PITR_PERSISTENT_BACKUP_ID=base-YYYYMMDDTHHMMSSZ-N \
  scripts/check-trnm-postgres-pitr-failover.sh
```

The creator writes into the dedicated `postgres_base_backups` volume under a
unique `.incomplete` directory, runs `pg_verifybackup`, writes a checksummed
verification record, and only then atomically publishes the directory. Failed
or interrupted backups remain visibly incomplete. The checker never deletes
WAL or backups. A persistent backup only receives
`restore_drill_verified=true` after that exact backup has been copied into an
isolated data volume, verified again, replayed to a later named restore point,
promoted and proven writable.

PostgreSQL is configured with `wal_level=replica`, `archive_mode=on`, a
separate WAL archive volume and a 60-second archive timeout. The physical gate
takes a full `pg_basebackup`, writes markers around a named restore point,
replays archived WAL into an isolated instance, proves the later marker is
absent, promotes the restored instance and writes to it. `--chaos` also stops
the primary container, verifies ledger fail-closed and the promoted recovery
instance alive, then restores the primary service.
The streaming-standby drill takes a second physical copy with `standby.signal`,
waits for a primary marker to replay, stops the primary, verifies ledger
readiness fails closed, promotes the standby writable and restores the original
single-node service.

## Recovery sequence

1. Stop `cex-trnm-consumer.service` and `cex-trnm-ledger.service` before a
   destructive restore. Keep `LEDGER_FAIL_FAST=true`; never switch production
   to the in-memory repository.
2. Preserve the failed database volume and service logs before changing state.
3. Restore the latest verified logical backup into a new database, apply every
   migration through `0030_add_trnm_online_product_identity.sql`, and
   run the automated gate against that database.
4. Verify account non-negative constraints, one receipt per intent, escrow
   state constraints and reconciliation cursors before changing the service
   `DATABASE_URL`.
5. Start ledger first, verify `/v1/trnm/economy/readiness`, then start consumer
   and reconcile every bound client. Pending priority compensation must drain
   before the normal outbox.
6. Keep the previous database read-only until wallet, receipt and cursor parity
   has been signed off.

## PITR and HA boundary

The checked-in gates prove logical backup/restore, same-database cross-instance
exactly-once, one-shot physical backup/restore drills, continuous local WAL
archival, point-in-time restore and same-host promotion. A drill-created base
backup is temporary. A persistent verified base backup plus archived WAL is a
same-host recovery cache only; it is not an independent disaster-recovery copy.

No automatic expiry is permitted while the repository lacks an encrypted
off-host backup chain managed by a dependency-aware tool such as pgBackRest,
WAL-G or Barman. In particular, operators must not delete WAL by age, filename
or disk-watermark scripts. Until an off-host repository exists, a low-space
condition must fail closed and trigger expansion or migration, not pruning.

The target policy is at least seven days of off-host PITR, two verified full
backups, weekly full plus daily differential backups, and WAL continuity from
the oldest retained full backup. Expiry may only be enabled after both retained
full backups pass `pg_verifybackup` and real restore/promotion drills. The
reported recovery window must be calculated from the oldest verified backup
start through the newest gap-free archived WAL, not inferred from file age.

Production high availability still requires a genuinely separate standby host,
replication slot monitoring, quorum/fencing, failover orchestration, encrypted
off-host retention and cross-node network-partition tests. Same-host promotion
must not be described as regional or multi-node HA.

Suggested operational objectives for the trusted system market are RPO <= 15
minutes and RTO <= 60 minutes. Public player trading must define stricter
objectives, custody and dispute ownership before it can be enabled.

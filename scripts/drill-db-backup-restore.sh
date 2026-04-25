#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"

OUT_DIR="run/drills"
KEEP_RESTORE_DB="false"
RESTORE_DB=""
SUMMARY_FILE=""
DUMP_FILE=""

usage() {
  cat <<'EOF'
Usage: scripts/drill-db-backup-restore.sh [--out-dir <dir>] [--restore-db <name>] [--summary-file <path>] [--dump-file <path>] [--keep-restore-db]

Runs a production-readiness database backup/restore drill against the configured
local Postgres container. The drill:
  1. writes a pg_dump custom-format backup of the configured CEX database,
  2. creates a temporary restore database in the same Postgres instance,
  3. restores the dump into that temporary database,
  4. compares row counts for core tables between source and restored DB,
  5. writes a JSON summary under run/drills/ by default,
  6. drops the temporary restore DB unless --keep-restore-db is set.

The helper uses scripts/_dev-helpers.sh Docker discovery, including the
passwordless `sudo -n docker` fallback when direct Docker socket access is not
available. Set CEX_DOCKER_USE_SUDO=1 to force the sudo Docker path.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      [[ $# -ge 2 ]] || { echo "Error: --out-dir requires a value" >&2; exit 2; }
      OUT_DIR="$2"
      shift 2
      ;;
    --restore-db)
      [[ $# -ge 2 ]] || { echo "Error: --restore-db requires a value" >&2; exit 2; }
      RESTORE_DB="$2"
      shift 2
      ;;
    --summary-file)
      [[ $# -ge 2 ]] || { echo "Error: --summary-file requires a value" >&2; exit 2; }
      SUMMARY_FILE="$2"
      shift 2
      ;;
    --dump-file)
      [[ $# -ge 2 ]] || { echo "Error: --dump-file requires a value" >&2; exit 2; }
      DUMP_FILE="$2"
      shift 2
      ;;
    --keep-restore-db)
      KEEP_RESTORE_DB="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

cex_load_env

if ! cex_can_use_docker_postgres; then
  echo "no usable Docker Postgres container found; cannot run backup/restore drill" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
if [[ -z "$RESTORE_DB" ]]; then
  RESTORE_DB="${CEX_POSTGRES_DB}_restore_${RUN_ID//[^A-Za-z0-9_]/_}"
fi
if [[ ! "$RESTORE_DB" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "restore database name must match ^[A-Za-z_][A-Za-z0-9_]*$: $RESTORE_DB" >&2
  exit 2
fi
if [[ -z "$DUMP_FILE" ]]; then
  DUMP_FILE="$OUT_DIR/db-backup-$RUN_ID.dump"
fi
if [[ -z "$SUMMARY_FILE" ]]; then
  SUMMARY_FILE="$OUT_DIR/db-backup-restore-$RUN_ID.summary.json"
fi

RESTORE_CREATED="false"
current_counts_file=""
restore_counts_file=""
cleanup() {
  local code=$?
  [[ -z "$current_counts_file" ]] || rm -f "$current_counts_file"
  [[ -z "$restore_counts_file" ]] || rm -f "$restore_counts_file"
  if [[ "$RESTORE_CREATED" == "true" && "$KEEP_RESTORE_DB" != "true" ]]; then
    cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" dropdb -U "$CEX_POSTGRES_USER" --if-exists "$RESTORE_DB" >/dev/null 2>&1 || true
  fi
  exit "$code"
}
trap cleanup EXIT

psql_scalar() {
  local db="$1"
  local sql="$2"
  cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" \
    psql -U "$CEX_POSTGRES_USER" -d "$db" -v ON_ERROR_STOP=1 -Atc "$sql"
}

core_tables=(organizations users api_keys accounts ledger_entries invocations executions audit_events capabilities)

table_exists_sql() {
  local table="$1"
  printf "select case when to_regclass('public.%s') is null then 'missing' else 'present' end" "$table"
}

row_count_sql() {
  local table="$1"
  printf "select count(*) from public.%s" "$table"
}

current_counts_file="$(mktemp)"
restore_counts_file="$(mktemp)"

for table in "${core_tables[@]}"; do
  if [[ "$(psql_scalar "$CEX_POSTGRES_DB" "$(table_exists_sql "$table")")" == "present" ]]; then
    printf '%s=%s\n' "$table" "$(psql_scalar "$CEX_POSTGRES_DB" "$(row_count_sql "$table")")" >> "$current_counts_file"
  else
    printf '%s=missing\n' "$table" >> "$current_counts_file"
  fi
done

started_at_epoch="$(date +%s)"
echo "==> writing backup $DUMP_FILE"
cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" \
  pg_dump -U "$CEX_POSTGRES_USER" -d "$CEX_POSTGRES_DB" -Fc > "$DUMP_FILE"

if [[ ! -s "$DUMP_FILE" ]]; then
  echo "backup dump is empty: $DUMP_FILE" >&2
  exit 1
fi

echo "==> creating restore database $RESTORE_DB"
cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" dropdb -U "$CEX_POSTGRES_USER" --if-exists "$RESTORE_DB" >/dev/null
cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" createdb -U "$CEX_POSTGRES_USER" "$RESTORE_DB"
RESTORE_CREATED="true"

echo "==> restoring backup into $RESTORE_DB"
cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" \
  pg_restore -U "$CEX_POSTGRES_USER" -d "$RESTORE_DB" --no-owner --no-privileges < "$DUMP_FILE"

for table in "${core_tables[@]}"; do
  if [[ "$(psql_scalar "$RESTORE_DB" "$(table_exists_sql "$table")")" == "present" ]]; then
    printf '%s=%s\n' "$table" "$(psql_scalar "$RESTORE_DB" "$(row_count_sql "$table")")" >> "$restore_counts_file"
  else
    printf '%s=missing\n' "$table" >> "$restore_counts_file"
  fi
done

if ! diff -u "$current_counts_file" "$restore_counts_file" >/tmp/cex-db-drill-counts.diff; then
  cat /tmp/cex-db-drill-counts.diff >&2
  echo "restored table counts do not match source" >&2
  exit 1
fi

ended_at_epoch="$(date +%s)"
source_table_count="$(psql_scalar "$CEX_POSTGRES_DB" "select count(*) from information_schema.tables where table_schema='public'")"
restore_table_count="$(psql_scalar "$RESTORE_DB" "select count(*) from information_schema.tables where table_schema='public'")"
dump_bytes="$(wc -c < "$DUMP_FILE" | tr -d ' ')"

python3 - "$SUMMARY_FILE" "$RUN_ID" "$started_at_epoch" "$ended_at_epoch" "$CEX_POSTGRES_CONTAINER_NAME" "$CEX_POSTGRES_DB" "$RESTORE_DB" "$DUMP_FILE" "$dump_bytes" "$source_table_count" "$restore_table_count" "$KEEP_RESTORE_DB" "$current_counts_file" <<'PY'
import json
from pathlib import Path
import sys
summary_path = Path(sys.argv[1])
counts = {}
for line in Path(sys.argv[13]).read_text().splitlines():
    key, value = line.split('=', 1)
    counts[key] = None if value == 'missing' else int(value)
summary = {
    'ok': True,
    'kind': 'db_backup_restore_drill',
    'run_id': sys.argv[2],
    'started_at_epoch': int(sys.argv[3]),
    'ended_at_epoch': int(sys.argv[4]),
    'duration_seconds': int(sys.argv[4]) - int(sys.argv[3]),
    'postgres_container': sys.argv[5],
    'source_database': sys.argv[6],
    'restore_database': sys.argv[7],
    'restore_database_kept': sys.argv[12] == 'true',
    'dump_file': sys.argv[8],
    'dump_bytes': int(sys.argv[9]),
    'source_public_table_count': int(sys.argv[10]),
    'restore_public_table_count': int(sys.argv[11]),
    'core_table_counts': counts,
}
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n')
PY

echo "DRILL_OK summary=$SUMMARY_FILE dump=$DUMP_FILE restore_db=$RESTORE_DB kept=$KEEP_RESTORE_DB"

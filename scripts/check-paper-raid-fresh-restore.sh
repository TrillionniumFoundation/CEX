#!/usr/bin/env bash
set -euo pipefail

# Verify a Paper Raid PostgreSQL database can be restored into a genuinely new
# database. The caller owns the disposable PostgreSQL container; this helper
# never connects to a production host and never mutates the source database.

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"

container=""
database_user=""
source_database=""
restore_database=""
label="paper-raid"
summary_file=""
keep_restore_database="false"

usage() {
  cat <<'EOF'
Usage: scripts/check-paper-raid-fresh-restore.sh \
  --container <name> --user <database-user> --database <database> \
  [--label <label>] [--restore-db <database>] [--summary-file <path>] \
  [--keep-restore-db]

Creates a custom-format pg_dump from the named disposable PostgreSQL
container, restores it into a newly-created database, and requires schema and
exact per-table row-count parity. The source database is never dropped or
written. By default the fresh restore database is removed on exit.

The helper uses `sudo -n docker` by default, matching the Paper Raid gates.
Set PAPER_RAID_DOCKER_USE_SUDO=0 when the caller has direct Docker access.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --container)
      [[ $# -ge 2 ]] || { echo "--container requires a value" >&2; exit 2; }
      container="$2"
      shift 2
      ;;
    --user)
      [[ $# -ge 2 ]] || { echo "--user requires a value" >&2; exit 2; }
      database_user="$2"
      shift 2
      ;;
    --database)
      [[ $# -ge 2 ]] || { echo "--database requires a value" >&2; exit 2; }
      source_database="$2"
      shift 2
      ;;
    --restore-db)
      [[ $# -ge 2 ]] || { echo "--restore-db requires a value" >&2; exit 2; }
      restore_database="$2"
      shift 2
      ;;
    --label)
      [[ $# -ge 2 ]] || { echo "--label requires a value" >&2; exit 2; }
      label="$2"
      shift 2
      ;;
    --summary-file)
      [[ $# -ge 2 ]] || { echo "--summary-file requires a value" >&2; exit 2; }
      summary_file="$2"
      shift 2
      ;;
    --keep-restore-db)
      keep_restore_database="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n "$container" ]] || { echo "--container is required" >&2; exit 2; }
[[ -n "$database_user" ]] || { echo "--user is required" >&2; exit 2; }
[[ -n "$source_database" ]] || { echo "--database is required" >&2; exit 2; }
[[ "$database_user" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || {
  echo "database user must be a simple PostgreSQL identifier" >&2
  exit 2
}
[[ "$source_database" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || {
  echo "source database must be a simple PostgreSQL identifier" >&2
  exit 2
}
[[ "$label" =~ ^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$ ]] || {
  echo "label must contain only bounded ASCII letters, digits, '_' or '-'" >&2
  exit 2
}

if [[ -z "$restore_database" ]]; then
  restore_database="${source_database}_restore_$(date -u +%Y%m%d%H%M%S)_$$"
fi
[[ "$restore_database" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || {
  echo "restore database must be a simple PostgreSQL identifier" >&2
  exit 2
}
[[ "$restore_database" != "$source_database" ]] || {
  echo "refusing to use the source database as the restore target" >&2
  exit 2
}

if [[ -z "$summary_file" ]]; then
  summary_file="$REPO_ROOT/run/paper-raid-restore/${label}-$(date -u +%Y%m%dT%H%M%SZ)-$$.summary.json"
fi

if [[ "${PAPER_RAID_DOCKER_USE_SUDO:-1}" == "1" ]]; then
  docker_command=(sudo -n docker)
else
  docker_command=(docker)
fi

docker_exec() {
  "${docker_command[@]}" exec "$container" "$@"
}

psql_scalar() {
  local database="$1"
  local sql="$2"
  docker_exec psql \
    --username "$database_user" \
    --dbname "$database" \
    --no-psqlrc \
    --set ON_ERROR_STOP=1 \
    --tuples-only --no-align --command "$sql"
}

table_list() {
  psql_scalar "$1" \
    "SELECT quote_ident(n.nspname) || '.' || quote_ident(c.relname)
       FROM pg_class c
       JOIN pg_namespace n ON n.oid = c.relnamespace
      WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p')
      ORDER BY 1"
}

row_count_manifest() {
  local database="$1"
  local output="$2"
  : >"$output"
  while IFS= read -r table; do
    [[ -n "$table" ]] || continue
    printf '%s=%s\n' "$table" "$(psql_scalar "$database" "SELECT count(*) FROM $table")" >>"$output"
  done < <(table_list "$database")
}

schema_dump() {
  local database="$1"
  local output="$2"
  # PostgreSQL 17+ emits a random \\restrict token in plain dumps. Those two
  # guard lines carry no schema meaning and are removed before parity compare.
  docker_exec pg_dump \
    --username "$database_user" \
    --dbname "$database" \
    --schema-only \
    --no-owner \
    --no-privileges \
    --no-comments |
    sed -E '/^\\(un)?restrict[[:space:]]/d' >"$output"
}

umask 077
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/paper-raid-restore.XXXXXXXX")"
dump_file="$work_dir/$label.dump"
source_counts="$work_dir/source.counts"
restore_counts="$work_dir/restore.counts"
source_schema="$work_dir/source.schema.sql"
restore_schema="$work_dir/restore.schema.sql"
restore_created="false"
started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

cleanup() {
  local status=$?
  if [[ "$restore_created" == "true" && "$keep_restore_database" != "true" ]]; then
    docker_exec dropdb --username "$database_user" --if-exists "$restore_database" >/dev/null 2>&1 || true
  fi
  rm -rf "$work_dir"
  exit "$status"
}
trap cleanup EXIT

# Fail before producing a misleading summary if the caller points at a dead
# or non-PostgreSQL container.
docker_exec pg_isready --username "$database_user" --dbname "$source_database" >/dev/null

echo "==> Paper Raid fresh restore: dumping $source_database ($label)"
docker_exec pg_dump \
  --username "$database_user" \
  --dbname "$source_database" \
  --format=custom \
  --no-owner \
  --no-privileges >"$dump_file"
[[ -s "$dump_file" ]] || { echo "pg_dump produced an empty archive" >&2; exit 1; }

docker_exec pg_restore --list <"$dump_file" >/dev/null
row_count_manifest "$source_database" "$source_counts"
schema_dump "$source_database" "$source_schema"

echo "==> Paper Raid fresh restore: creating $restore_database"
docker_exec dropdb --username "$database_user" --if-exists "$restore_database" >/dev/null
docker_exec createdb --username "$database_user" "$restore_database"
restore_created="true"

echo "==> Paper Raid fresh restore: restoring into $restore_database"
"${docker_command[@]}" exec -i "$container" pg_restore \
  --username "$database_user" \
  --dbname "$restore_database" \
  --no-owner \
  --no-privileges \
  --exit-on-error \
  --single-transaction <"$dump_file"

row_count_manifest "$restore_database" "$restore_counts"
if ! diff -u "$source_counts" "$restore_counts"; then
  echo "fresh restore row-count parity failed" >&2
  exit 1
fi

schema_dump "$restore_database" "$restore_schema"
if ! cmp -s "$source_schema" "$restore_schema"; then
  echo "fresh restore schema parity failed" >&2
  diff -u "$source_schema" "$restore_schema" >&2 || true
  exit 1
fi

dump_sha256="$(sha256sum "$dump_file" | awk '{print $1}')"
source_schema_sha256="$(sha256sum "$source_schema" | awk '{print $1}')"
restore_schema_sha256="$(sha256sum "$restore_schema" | awk '{print $1}')"
dump_bytes="$(wc -c <"$dump_file" | tr -d '[:space:]')"
ended_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

python3 - "$summary_file" "$label" "$container" "$database_user" \
  "$source_database" "$restore_database" "$started_at" "$ended_at" \
  "$dump_file" "$dump_bytes" "$dump_sha256" "$source_schema_sha256" \
  "$restore_schema_sha256" "$keep_restore_database" "$source_counts" <<'PY'
import json
import sys
from pathlib import Path

(
    summary_path,
    label,
    container,
    database_user,
    source_database,
    restore_database,
    started_at,
    ended_at,
    dump_file,
    dump_bytes,
    dump_sha256,
    source_schema_sha256,
    restore_schema_sha256,
    keep_restore_database,
    counts_path,
) = sys.argv[1:]

counts = {}
for line in Path(counts_path).read_text(encoding="utf-8").splitlines():
    key, value = line.split("=", 1)
    counts[key] = int(value)

summary = {
    "schema": "trnm.paper-raid.fresh-restore.v1",
    "status": "passed",
    "kind": "paper_raid_postgres_fresh_restore",
    "label": label,
    "container": container,
    "database_user": database_user,
    "source_database": source_database,
    "restore_database": restore_database,
    "restore_database_kept": keep_restore_database == "true",
    "started_at": started_at,
    "ended_at": ended_at,
    "dump_file": dump_file,
    "dump_bytes": int(dump_bytes),
    "dump_sha256": f"sha256:{dump_sha256}",
    "source_schema_sha256": f"sha256:{source_schema_sha256}",
    "restore_schema_sha256": f"sha256:{restore_schema_sha256}",
    "schema_parity": source_schema_sha256 == restore_schema_sha256,
    "row_count_parity": True,
    "public_table_count": len(counts),
    "source_row_counts": counts,
    "boundary": "disposable PostgreSQL logical dump/restore; off-host backup durability is a separate release concern",
}

path = Path(summary_path)
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "PAPER_RAID_FRESH_RESTORE_OK summary=$summary_file restore_db=$restore_database kept=$keep_restore_database"

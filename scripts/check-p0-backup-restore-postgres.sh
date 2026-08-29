#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
evidence_dir="${CEX_P0_EVIDENCE_DIR:-$root/run/p0-release-evidence}"
work_dir=$(mktemp -d)
run_component="${GITHUB_RUN_ID:-local}_$$_${GITHUB_RUN_ATTEMPT:-1}"
restore_db="cex_p0_restore_${run_component//[^A-Za-z0-9_]/_}"

if ! [[ "$restore_db" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "unsafe restore database name: $restore_db" >&2
  exit 64
fi

mkdir -p "$evidence_dir"
mapfile -t urls < <(python3 - "$DATABASE_URL" "$restore_db" <<'PY'
from urllib.parse import urlsplit, urlunsplit
import sys

source = urlsplit(sys.argv[1])
if source.scheme not in {"postgres", "postgresql"} or not source.netloc:
    raise SystemExit("DATABASE_URL must be a postgres URL")

def with_database(name: str) -> str:
    return urlunsplit((source.scheme, source.netloc, "/" + name, source.query, source.fragment))

print(with_database("postgres"))
print(with_database(sys.argv[2]))
PY
)
admin_url=${urls[0]}
restore_url=${urls[1]}
dump_path="$work_dir/p0-release.dump"
list_path="$work_dir/p0-release.dump.list"
restore_created=false

cleanup() {
  status=$?
  if [[ "$restore_created" == true ]]; then
    psql "$admin_url" -X -v ON_ERROR_STOP=1 -v db="$restore_db" >/dev/null <<'SQL' || true
select pg_terminate_backend(pid)
  from pg_stat_activity
 where datname=:'db'
   and pid <> pg_backend_pid();
drop database if exists :"db";
SQL
  fi
  rm -rf "$work_dir"
  exit "$status"
}
trap cleanup EXIT

fingerprint_sql=$(cat <<'SQL'
select json_build_object(
  'public_table_count',(
    select count(*) from information_schema.tables where table_schema='public'
  ),
  'organization_count',(select count(*) from public.organizations),
  'account_count',(select count(*) from public.accounts),
  'ledger_entry_count',(select count(*) from public.ledger_entries),
  'audit_outbox_count',(select count(*) from public.cex_audit_outbox_v1),
  'soak_account',(
    select json_build_object(
      'account_id',account_id,
      'balance_minor',balance_minor,
      'reserved_minor',reserved_minor,
      'currency_unit',currency_unit,
      'currency_scale',currency_scale
    )
      from public.accounts
     where account_id='90000000-0000-4000-8000-000000000101'
  ),
  'soak_operation_count',(
    select count(distinct operation_id)
      from public.ledger_entries
     where account_id='90000000-0000-4000-8000-000000000101'
  ),
  'soak_ledger_sha256',(
    select 'sha256:' || encode(digest(coalesce(string_agg(
      concat_ws('|',entry_id::text,operation_id::text,operation_kind,
        amount_minor::text,currency_scale::text,request_fingerprint),
      E'\n' order by entry_id
    ),''),'sha256'),'hex')
      from public.ledger_entries
     where account_id='90000000-0000-4000-8000-000000000101'
  ),
  'soak_audit_sha256',(
    select 'sha256:' || encode(digest(coalesce(string_agg(
      concat_ws('|',outbox_id::text,event_id::text,status,envelope::text),
      E'\n' order by outbox_id
    ),''),'sha256'),'hex')
      from public.cex_audit_outbox_v1
     where envelope #>> '{payload,account_id}'='90000000-0000-4000-8000-000000000101'
  )
);
SQL
)

source_fingerprint=$(psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -Atc "$fingerprint_sql")
if [[ -z "$source_fingerprint" ]] || [[ "$source_fingerprint" == *'"soak_account" : null'* ]]; then
  echo "source database lacks committed exact-soak evidence" >&2
  exit 1
fi

started_at_epoch=$(date +%s)
pg_dump "$DATABASE_URL" --format=custom --no-owner --no-privileges --file="$dump_path"
test -s "$dump_path"
pg_restore --list "$dump_path" >"$list_path"
test -s "$list_path"
dump_sha256="sha256:$(sha256sum "$dump_path" | awk '{print $1}')"
dump_bytes=$(wc -c <"$dump_path" | tr -d ' ')
archive_items=$(grep -vc '^;' "$list_path")

psql "$admin_url" -X -v ON_ERROR_STOP=1 -v db="$restore_db" >/dev/null <<'SQL'
select pg_terminate_backend(pid)
  from pg_stat_activity
 where datname=:'db'
   and pid <> pg_backend_pid();
drop database if exists :"db";
create database :"db";
SQL
restore_created=true

pg_restore --dbname="$restore_url" --no-owner --no-privileges --exit-on-error "$dump_path"
restored_fingerprint=$(psql "$restore_url" -X -v ON_ERROR_STOP=1 -Atc "$fingerprint_sql")

python3 - "$source_fingerprint" "$restored_fingerprint" <<'PY'
import json
import sys
source = json.loads(sys.argv[1])
restored = json.loads(sys.argv[2])
if source != restored:
    print(json.dumps({"source": source, "restored": restored}, indent=2, sort_keys=True))
    raise SystemExit("restored database fingerprint differs from source")
PY

ended_at_epoch=$(date +%s)
python3 - \
  "$evidence_dir/backup-restore.json" \
  "$source_fingerprint" \
  "$restored_fingerprint" \
  "$dump_sha256" \
  "$dump_bytes" \
  "$archive_items" \
  "$started_at_epoch" \
  "$ended_at_epoch" \
  "${GITHUB_SHA:-unknown}" <<'PY'
import json
from pathlib import Path
import sys

out = {
    "schema": "cex.p0-backup-restore.v1",
    "ok": True,
    "source": json.loads(sys.argv[2]),
    "restored": json.loads(sys.argv[3]),
    "dump_sha256": sys.argv[4],
    "dump_bytes": int(sys.argv[5]),
    "archive_items": int(sys.argv[6]),
    "started_at_epoch": int(sys.argv[7]),
    "ended_at_epoch": int(sys.argv[8]),
    "duration_seconds": int(sys.argv[8]) - int(sys.argv[7]),
    "commit_sha": sys.argv[9],
    "restore_database_retained": False,
}
Path(sys.argv[1]).write_text(json.dumps(out, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "P0 exact-state backup/restore passed: $evidence_dir/backup-restore.json"

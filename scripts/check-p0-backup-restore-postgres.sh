#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
root="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
: "${DATABASE_URL:?DATABASE_URL is required}"
BASE_URL="$(cex_effective_database_url)"
cex_sync_postgres_env_from_database_url "$BASE_URL"
BASE_URL_SAFE="$(cex_database_url_without_password "$BASE_URL")"

# Keep the drill on one coherent PostgreSQL client path.  A host installation
# is preferred when all three clients are available; falling back piecemeal
# (for example, host psql plus a container pg_dump) could compare different
# servers or connection policies.  The Docker path is intentionally limited to
# a local URL whose published port/socket has been proved to be the selected
# PostgreSQL container.
postgres_client_mode="host"
if ! command -v psql >/dev/null 2>&1 \
  || ! command -v pg_dump >/dev/null 2>&1 \
  || ! command -v pg_restore >/dev/null 2>&1; then
  postgres_client_mode="docker"
  if ! cex_postgres_host_is_local; then
    echo "host PostgreSQL clients are incomplete and Docker fallback requires a local DATABASE_URL" >&2
    exit 2
  fi
  if ! cex_can_use_docker_postgres; then
    echo "host PostgreSQL clients are incomplete and no usable Docker Postgres container was found" >&2
    exit 127
  fi
  if ! cex_postgres_docker_socket_is_target; then
    echo "Docker PostgreSQL container port does not match DATABASE_URL; refusing socket fallback" >&2
    exit 2
  fi
fi

# The target container's PostgreSQL listener is on its internal 5432 socket,
# while DATABASE_URL may name an arbitrary host-published port (for example
# 55432:5432).  Rewrite only the host/port for the Docker namespace and retain
# the already credential-free user/path/query bytes.  Query options such as
# sslmode, connect_timeout, and application_name therefore have identical
# semantics in host and Docker modes; routing/identity query keys were already
# rejected by cex_database_url_without_password().
docker_base_url_safe=""
if [[ "$postgres_client_mode" == "docker" ]]; then
  if ! docker_base_url_safe="$(python3 - "$BASE_URL_SAFE" <<'PY'
from urllib.parse import urlsplit, urlunsplit
import sys

source = urlsplit(sys.argv[1])
if source.scheme not in {"postgres", "postgresql"} or not source.netloc:
    raise SystemExit("DATABASE_URL must be a PostgreSQL URI")
if not source.hostname:
    raise SystemExit("DATABASE_URL must include a host")

# BASE_URL_SAFE has already removed the password.  Preserve the exact raw
# userinfo (including percent-encoding) while moving the authority to the
# target container's internal loopback listener.
if "@" in source.netloc:
    userinfo, _hostport = source.netloc.rsplit("@", 1)
    netloc = f"{userinfo}@127.0.0.1:5432"
else:
    netloc = "127.0.0.1:5432"
print(urlunsplit((source.scheme, netloc, source.path, source.query, "")))
PY
)"; then
    echo "cannot derive a Docker-local credential-free PostgreSQL URL" >&2
    exit 2
  fi
fi

docker_url_for_database() {
  local database="$1"
  cex_database_url_for_database "$database" "$docker_base_url_safe"
}

run_docker_exec() {
  if cex_postgres_password_is_set; then
    cex_docker_exec_with_password "$(cex_postgres_password_value)" \
      "$CEX_POSTGRES_CONTAINER_NAME" "$@"
  else
    cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" "$@"
  fi
}

run_docker_exec_stdin() {
  if cex_postgres_password_is_set; then
    cex_docker_exec_with_password_stdin "$(cex_postgres_password_value)" \
      "$CEX_POSTGRES_CONTAINER_NAME" "$@"
  else
    cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" "$@"
  fi
}

# This helper is deliberately host-only.  Docker invocations must go through
# run_docker_exec[_stdin], which carry decoded credentials via an anonymous
# env-file rather than exposing PGPASSWORD in argv.
run_with_postgres_password() {
  if [[ ${PGPASSWORD+x} ]]; then
    PGPASSWORD="$PGPASSWORD" "$@"
  elif [[ "${CEX_DATABASE_URL_PASSWORD_PRESENT:-0}" == "1" \
          || "${CEX_POSTGRES_PASSWORD_EXPLICIT:-0}" == "1" ]]; then
    PGPASSWORD="${CEX_POSTGRES_PASSWORD:-}" "$@"
  else
    "$@"
  fi
}

run_psql() {
  local database="$1"
  local url="$2"
  shift 2
  if [[ "$postgres_client_mode" == "host" ]]; then
    run_with_postgres_password psql "$url" "$@"
  else
    local docker_url
    docker_url="$(docker_url_for_database "$database")"
    # Always retain -i in Docker mode: callers use both -c and SQL heredocs.
    run_docker_exec_stdin psql "$docker_url" "$@"
  fi
}

run_pg_dump() {
  local database="$1"
  local url="$2"
  local output="$3"
  if [[ "$postgres_client_mode" == "host" ]]; then
    run_with_postgres_password pg_dump "$url" \
      --format=custom --no-owner --no-privileges --file="$output"
  else
    local docker_url
    docker_url="$(docker_url_for_database "$database")"
    # A container cannot see the host work directory.  Stream the custom
    # archive over stdout and materialize it only on the host.
    run_docker_exec pg_dump --dbname="$docker_url" \
      --format=custom --no-owner --no-privileges >"$output"
  fi
}

run_pg_restore_list() {
  local dump_file="$1"
  local list_file="$2"
  if [[ "$postgres_client_mode" == "host" ]]; then
    pg_restore --list "$dump_file" >"$list_file"
  else
    # pg_restore reads a custom archive from stdin when no filename is given.
    cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" \
      pg_restore --list <"$dump_file" >"$list_file"
  fi
}

run_pg_restore() {
  local database="$1"
  local url="$2"
  local dump_file="$3"
  if [[ "$postgres_client_mode" == "host" ]]; then
    run_with_postgres_password pg_restore --dbname="$url" \
      --no-owner --no-privileges --exit-on-error "$dump_file"
  else
    local docker_url
    docker_url="$(docker_url_for_database "$database")"
    run_docker_exec_stdin pg_restore --dbname="$docker_url" \
      --no-owner --no-privileges --exit-on-error <"$dump_file"
  fi
}

evidence_dir="${CEX_P0_EVIDENCE_DIR:-$root/run/p0-release-evidence}"
work_dir=$(mktemp -d)
admin_url=""
restore_url=""
restore_created=false

cleanup() {
  local status=$?
  set +e
  if [[ "$restore_created" == true && -n "$admin_url" ]]; then
    run_psql postgres "$admin_url" -X -v ON_ERROR_STOP=1 -v db="$restore_db" >/dev/null <<'SQL' || true
select pg_terminate_backend(pid)
  from pg_stat_activity
 where datname=:'db'
   and pid <> pg_backend_pid();
drop database if exists :"db";
SQL
  fi
  rm -rf -- "$work_dir"
  exit "$status"
}
trap cleanup EXIT
run_component="${GITHUB_RUN_ID:-local}_$$_${GITHUB_RUN_ATTEMPT:-1}"
restore_db="cex_p0_restore_${run_component//[^A-Za-z0-9_]/_}"

if ! [[ "$restore_db" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "unsafe restore database name: $restore_db" >&2
  exit 64
fi

admin_url="$(cex_database_url_for_database postgres "$BASE_URL_SAFE")"
restore_url="$(cex_database_url_for_database "$restore_db" "$BASE_URL_SAFE")"
dump_path="$work_dir/p0-release.dump"
list_path="$work_dir/p0-release.dump.list"

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

source_fingerprint=$(run_psql "$CEX_POSTGRES_DB" "$BASE_URL_SAFE" \
  -X -v ON_ERROR_STOP=1 -Atc "$fingerprint_sql")
if [[ -z "$source_fingerprint" ]] || [[ "$source_fingerprint" == *'"soak_account" : null'* ]]; then
  echo "source database lacks committed exact-soak evidence" >&2
  exit 1
fi

started_at_epoch=$(date +%s)
run_pg_dump "$CEX_POSTGRES_DB" "$BASE_URL_SAFE" "$dump_path"
test -s "$dump_path"
run_pg_restore_list "$dump_path" "$list_path"
test -s "$list_path"
dump_sha256="sha256:$(sha256sum "$dump_path" | awk '{print $1}')"
dump_bytes=$(wc -c <"$dump_path" | tr -d ' ')
archive_items=$(grep -vc '^;' "$list_path")

run_psql postgres "$admin_url" -X -v ON_ERROR_STOP=1 -v db="$restore_db" >/dev/null <<'SQL'
select pg_terminate_backend(pid)
  from pg_stat_activity
 where datname=:'db'
   and pid <> pg_backend_pid();
drop database if exists :"db";
create database :"db";
SQL
restore_created=true

run_pg_restore "$restore_db" "$restore_url" "$dump_path"
restored_fingerprint=$(run_psql "$restore_db" "$restore_url" \
  -X -v ON_ERROR_STOP=1 -Atc "$fingerprint_sql")

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
tree_sha=$(git -C "$root" rev-parse 'HEAD^{tree}')
python3 - \
  "$evidence_dir/backup-restore.json" \
  "$source_fingerprint" \
  "$restored_fingerprint" \
  "$dump_sha256" \
  "$dump_bytes" \
  "$archive_items" \
  "$started_at_epoch" \
  "$ended_at_epoch" \
  "${GITHUB_SHA:-unknown}" \
  "$tree_sha" \
  "$root" <<'PY'
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(sys.argv[11]) / "scripts"))
from evidence_safe_io import write_json_nofollow

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
    "tree_sha": sys.argv[10],
    "restore_database_retained": False,
}
write_json_nofollow(Path(sys.argv[1]), out)
PY

echo "P0 exact-state backup/restore passed: $evidence_dir/backup-restore.json"

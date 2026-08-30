#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
if [[ ! ${DATABASE_URL+x} || -z "${DATABASE_URL}" ]]; then
  echo "streaming drill requires an explicit DATABASE_URL matching the primary container" >&2
  exit 2
fi
if [[ ${DATABASE_URL+x} ]]; then
  cex_sync_postgres_env_from_database_url "$(cex_effective_database_url)"
fi
if [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" ]] && ! cex_postgres_host_is_local; then
  echo "refusing non-local DATABASE_URL: streaming drill uses Docker-local PostgreSQL operations" >&2
  exit 2
fi
if [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" && "${CEX_POSTGRES_HOST:-}" != "127.0.0.1" ]]; then
  # The standby client runs in host-network mode and the HBA mutation below
  # installs IPv4 loopback/gateway rules only.  Refuse localhost/IPv6 aliases
  # rather than claiming a failover drill against an unverified endpoint.
  echo "streaming drill requires DATABASE_URL host 127.0.0.1" >&2
  exit 2
fi
if [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" ]] \
   && ! cex_postgres_docker_socket_is_target; then
  echo "refusing DATABASE_URL whose local port is not the Docker PostgreSQL target" >&2
  exit 2
fi
if ! [[ "$CEX_POSTGRES_USER" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "streaming drill requires a simple PostgreSQL role name for pg_hba.conf" >&2
  exit 2
fi
STREAMING_POSTGRES_PASSWORD=""
STREAMING_POSTGRES_PASSWORD_SET=0
if cex_postgres_password_is_set; then
  STREAMING_POSTGRES_PASSWORD="$(cex_postgres_password_value)"
  STREAMING_POSTGRES_PASSWORD_SET=1
fi
# The drill installs SCRAM replication rules before taking the physical copy.
# A passwordless branch cannot authenticate against those rules (and would
# otherwise fail only after mutating pg_hba.conf), so reject it up front.
if [[ "$STREAMING_POSTGRES_PASSWORD_SET" != "1" || -z "$STREAMING_POSTGRES_PASSWORD" ]]; then
  echo "streaming drill requires a non-empty PostgreSQL password" >&2
  exit 2
fi
if [[ "$STREAMING_POSTGRES_PASSWORD" == *$'\n'* ||
      "$STREAMING_POSTGRES_PASSWORD" == *$'\r'* ||
      "$STREAMING_POSTGRES_PASSWORD" == *$'\t'* ||
      "$STREAMING_POSTGRES_PASSWORD" == *$'\v'* ||
      "$STREAMING_POSTGRES_PASSWORD" == *$'\f'* ]]; then
  echo "streaming drill refuses a PostgreSQL password containing control characters" >&2
  exit 2
fi
STREAMING_PRIMARY_PORT="${CEX_POSTGRES_PORT:-5432}"
if [[ ! "$STREAMING_PRIMARY_PORT" =~ ^[0-9]+$ ]] || (( STREAMING_PRIMARY_PORT < 1 || STREAMING_PRIMARY_PORT > 65535 )); then
  echo "streaming drill requires a valid PostgreSQL primary port" >&2
  exit 2
fi
if ! STREAMING_PRIMARY_URL_SAFE="$(cex_database_url_without_password "$(cex_effective_database_url)")"; then
  echo "cannot derive a credential-free primary URL for the streaming drill" >&2
  exit 2
fi

RUN_ID="standby-$(date +%s)-${RANDOM}"
STANDBY_VOLUME="cex_trnm_standby_${RANDOM}_$$"
STANDBY_CONTAINER="cex-trnm-standby-${RANDOM}-$$"
STANDBY_PORT="${TRNM_STANDBY_PORT:-55434}"
if [[ ! "$STANDBY_PORT" =~ ^[0-9]+$ ]] || (( STANDBY_PORT < 1 || STANDBY_PORT > 65535 )); then
  echo "streaming drill requires a valid standby port" >&2
  exit 2
fi
if [[ "$STANDBY_PORT" == "$STREAMING_PRIMARY_PORT" ]]; then
  echo "standby port must differ from the primary PostgreSQL port" >&2
  exit 2
fi
if command -v ss >/dev/null 2>&1 &&
   ss -H -ltn 2>/dev/null | awk -v port=":${STANDBY_PORT}" '$4 ~ port"$" { found=1 } END { exit !found }'; then
  echo "standby port is already listening on the host" >&2
  exit 2
fi
# Serialize HBA edits and primary stop/start operations.  A second drill must
# fail before touching the shared PostgreSQL container rather than interleave
# backups and restore the wrong pg_hba.conf.
cex_require_cmd flock
STREAMING_LOCK_FILE="${CEX_STREAMING_LOCK_FILE:-${TMPDIR:-/tmp}/cex-trnm-postgres-streaming-standby.lock}"
exec {STREAMING_LOCK_FD}>"$STREAMING_LOCK_FILE"
if ! flock -n "$STREAMING_LOCK_FD"; then
  echo "another streaming standby drill is already running" >&2
  exit 2
fi
PRIMARY_STOPPED=false
HBA_BACKUP_NAME="pg_hba.conf.cex-streaming-${RUN_ID}.bak"
HBA_BACKED_UP=false

# Docker's host-network mode can present the gateway of the *primary's
# attached network* as the source address.  Do not assume the default bridge:
# compose projects commonly use a network such as 172.21.0.0/16.
STREAMING_REPLICATION_CIDRS=(127.0.0.1/32)
STREAMING_REPLICATION_ADDRESSES=(127.0.0.1)
declare -A STREAMING_SEEN_CIDRS=([127.0.0.1/32]=1)
if ! STREAMING_NETWORK_GATEWAYS_RAW="$(cex_docker inspect \
  --format '{{range .NetworkSettings.Networks}}{{.Gateway}}{{"\n"}}{{end}}' \
  "$CEX_POSTGRES_CONTAINER_NAME" 2>/dev/null)"; then
  echo "cannot inspect the primary PostgreSQL container networks" >&2
  exit 2
fi
streaming_valid_ipv4() {
  local address="$1" octet
  [[ "$address" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || return 1
  IFS=. read -r -a octets <<<"$address"
  for octet in "${octets[@]}"; do
    (( 10#$octet <= 255 )) || return 1
  done
}
while IFS= read -r STREAMING_GATEWAY; do
  [[ -z "$STREAMING_GATEWAY" ]] && continue
  if ! streaming_valid_ipv4 "$STREAMING_GATEWAY"; then
    echo "primary container exposed an invalid Docker network gateway" >&2
    exit 2
  fi
  [[ "$STREAMING_GATEWAY" == "127.0.0.1" ]] && continue
  STREAMING_CIDR="${STREAMING_GATEWAY}/32"
  if [[ -z "${STREAMING_SEEN_CIDRS[$STREAMING_CIDR]+x}" ]]; then
    STREAMING_REPLICATION_CIDRS+=("$STREAMING_CIDR")
    STREAMING_REPLICATION_ADDRESSES+=("$STREAMING_GATEWAY")
    STREAMING_SEEN_CIDRS["$STREAMING_CIDR"]=1
  fi
done <<<"$STREAMING_NETWORK_GATEWAYS_RAW"

standby_psql() {
  cex_docker_run_with_password "$STREAMING_POSTGRES_PASSWORD" \
    --rm --network host postgres:16 \
    psql -h 127.0.0.1 -p "$STANDBY_PORT" -U "$CEX_POSTGRES_USER" \
      -d "$CEX_POSTGRES_DB" -v ON_ERROR_STOP=1 "$@"
}

streaming_reload_primary_hba() {
  cex_docker exec -u postgres "$CEX_POSTGRES_CONTAINER_NAME" \
    bash -ceu 'pg_ctl -D "${PGDATA:-/var/lib/postgresql/data}" reload' >/dev/null
}

streaming_assert_primary_hba() {
  local expected_count="${1:-${#STREAMING_REPLICATION_CIDRS[@]}}"
  local require_policy="${2:-1}"
  local addresses_sql="" address result errors exact_replication total_replication
  for address in "${STREAMING_REPLICATION_ADDRESSES[@]}"; do
    [[ -z "$addresses_sql" ]] || addresses_sql+=","
    addresses_sql+="'${address}'"
  done
  if ! result="$(cex_psql_stdin -Atc \
    "select count(*) filter (where error is not null),
            count(*) filter (where type like 'host%'
              and database = array['replication']::text[]
              and user_name = array['${CEX_POSTGRES_USER}']::text[]
              and auth_method = 'scram-sha-256'
              and address = any(array[${addresses_sql}]::text[])),
            count(*) filter (where type like 'host%'
              and 'replication' = any(database))
       from pg_hba_file_rules" 2>/dev/null)"; then
    echo "cannot inspect effective pg_hba.conf rules" >&2
    return 1
  fi
  IFS='|' read -r errors exact_replication total_replication <<<"$result"
  if [[ "$require_policy" == "0" ]]; then
    [[ "$errors" == "0" ]] || {
      echo "effective pg_hba.conf contains $errors parse errors" >&2
      return 1
    }
    return 0
  fi
  if [[ "$errors" != "0" || "$exact_replication" != "$expected_count" \
        || "$total_replication" != "$expected_count" ]]; then
    echo "effective pg_hba.conf is not the expected exact replication policy" >&2
    return 1
  fi
}

cleanup() {
  local original_status=$?
  local cleanup_status=0

  # A stopped container cannot be `docker exec`'d.  Bring the primary back
  # first, then restore/reload its HBA from the durable backup.
  if [[ "$PRIMARY_STOPPED" == true ]]; then
    if cex_docker start "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null &&
       cex_wait_postgres 90 1; then
      PRIMARY_STOPPED=false
    else
      echo "failed to restart the primary PostgreSQL container" >&2
      cleanup_status=1
    fi
  fi

  if [[ "$HBA_BACKED_UP" == true ]]; then
    if ! cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" bash -ceu '
      backup="${PGDATA}/$1"
      [[ -f "$backup" ]] || { echo "streaming HBA backup is missing" >&2; exit 1; }
      mv -- "$backup" "$PGDATA/pg_hba.conf"
      [[ ! -e "$backup" && -f "$PGDATA/pg_hba.conf" ]]
    ' -- "$HBA_BACKUP_NAME" >/dev/null 2>&1; then
      echo "failed to restore the primary pg_hba.conf" >&2
      cleanup_status=1
    else
      HBA_BACKED_UP=false
      if [[ "$PRIMARY_STOPPED" != true ]] && ! streaming_reload_primary_hba; then
        echo "failed to reload the restored primary pg_hba.conf" >&2
        cleanup_status=1
      elif [[ "$PRIMARY_STOPPED" != true ]] && ! streaming_assert_primary_hba 0 0; then
        echo "restored primary pg_hba.conf is not effective" >&2
        cleanup_status=1
      fi
    fi
  fi

  # Remove the standby before deleting its named volume.  Wipe credential
  # files first when possible; volume deletion is still required and is
  # treated as a cleanup failure if Docker cannot complete it.
  if cex_docker container inspect "$STANDBY_CONTAINER" >/dev/null 2>&1; then
    cex_docker exec "$STANDBY_CONTAINER" bash -ceu \
      'rm -f -- /var/lib/postgresql/data/.pgpass /var/lib/postgresql/data/postgresql.auto.conf' \
      >/dev/null 2>&1 || true
    if ! cex_docker rm -f "$STANDBY_CONTAINER" >/dev/null 2>&1; then
      echo "failed to remove the streaming standby container" >&2
      cleanup_status=1
    fi
  fi
  if cex_docker volume inspect "$STANDBY_VOLUME" >/dev/null 2>&1; then
    if ! cex_docker run --rm -v "$STANDBY_VOLUME:/standby" postgres:16 \
      bash -ceu 'rm -f -- /standby/.pgpass /standby/postgresql.auto.conf' \
      >/dev/null 2>&1; then
      echo "failed to wipe streaming standby credential files" >&2
      cleanup_status=1
    fi
    if ! cex_docker volume rm "$STANDBY_VOLUME" >/dev/null 2>&1; then
      echo "failed to remove the streaming standby volume" >&2
      cleanup_status=1
    fi
  fi
  exec {STREAMING_LOCK_FD}>&- || true
  if (( original_status != 0 )); then
    return "$original_status"
  fi
  return "$cleanup_status"
}
trap cleanup EXIT

if ! cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" bash -ceu '
  backup="${PGDATA}/$1"
  [[ ! -e "$backup" ]] || { echo "streaming HBA backup already exists" >&2; exit 1; }
  cp -p "$PGDATA/pg_hba.conf" "$backup"
' -- "$HBA_BACKUP_NAME"; then
  echo "failed to back up the primary pg_hba.conf" >&2
  exit 1
fi
HBA_BACKED_UP=true
if ! cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" bash -ceu '
  backup="${PGDATA}/$1"
  # Remove every pre-existing host replication grant before installing the
  # exact loopback/bridge /32 rules.  Leaving a broad rule earlier in
  # pg_hba.conf would bypass this drill local-target contract.
  if grep -Eq "^[[:space:]]*include(_if_exists|_dir)?[[:space:]]" "$PGDATA/pg_hba.conf"; then
    echo "streaming drill refuses pg_hba.conf include directives" >&2
    exit 1
  fi
  user="$2"
  shift 2
  hba_tmp="${PGDATA}/pg_hba.conf.cex-tmp"
  awk '\''$1 !~ /^host/ { print; next }
    $2 ~ /replication/ {
      if ($2 != "replication") {
        print "streaming drill refuses a compound replication database field" > "/dev/stderr"
        exit 42
      }
      next
    }
    { print }'\'' \
    "$PGDATA/pg_hba.conf" >"$hba_tmp"
  mv -- "$hba_tmp" "$PGDATA/pg_hba.conf"
  for cidr in "$@"; do
    rule="host replication ${user} ${cidr} scram-sha-256"
    grep -Fqx "$rule" "$PGDATA/pg_hba.conf" || printf "%s\n" "$rule" >>"$PGDATA/pg_hba.conf"
  done
' -- "$HBA_BACKUP_NAME" "$CEX_POSTGRES_USER" "${STREAMING_REPLICATION_CIDRS[@]}"; then
  echo "failed to install the streaming replication HBA rules" >&2
  exit 1
fi
if ! streaming_reload_primary_hba; then
  echo "failed to reload the streaming replication HBA rules" >&2
  exit 1
fi
if ! streaming_assert_primary_hba; then
  echo "streaming replication HBA rules are not effective" >&2
  exit 1
fi

cex_docker volume create "$STANDBY_VOLUME" >/dev/null
cex_docker_run_with_password "$STREAMING_POSTGRES_PASSWORD" \
  --rm --network host --mount type=tmpfs,destination=/run/cex \
  -v "$STANDBY_VOLUME:/standby" postgres:16 \
  bash -ceu '
    safe_url="$1"
    user="$2"
    port="$3"
    escaped_password="$(printf "%s" "$PGPASSWORD" | sed '\''s/[:\\]/\\&/g'\'')"
    # Keep the temporary credential file outside the backup target: pg_basebackup
    # requires its destination to start empty.  Unset PGPASSWORD before -R so
    # PostgreSQL cannot persist the secret in primary_conninfo.
    printf "*:%s:*:%s:%s\\n" "$port" "$user" "$escaped_password" > /run/cex/pgpass
    chmod 600 /run/cex/pgpass
    export PGPASSFILE=/run/cex/pgpass
    unset PGPASSWORD
    # The URI has no password, so it is safe in argv; -R preserves connection
    # options such as sslmode/connect_timeout in primary_conninfo.
    env -u PGPASSWORD PGPASSFILE=/run/cex/pgpass \
      pg_basebackup --dbname="$safe_url" -D /standby -Fp -Xs -R -P >/dev/null
    [[ -f /standby/postgresql.auto.conf && -f /standby/standby.signal ]]
    # A physical base backup can include a pre-existing primary_conninfo from
    # the source.  pg_basebackup appends its own -R line, so retain only the
    # last (newly generated) assignment before changing its passfile.  This
    # prevents a stale source password or duplicate connection string from
    # surviving in the standby configuration.
    awk '\''
      /^[[:space:]]*primary_conninfo[[:space:]]*=/ { last=$0; found=1; next }
      { print }
      END { if (!found) exit 1; print last }
    '\'' /standby/postgresql.auto.conf > /standby/postgresql.auto.conf.cex-tmp
    mv -- /standby/postgresql.auto.conf.cex-tmp /standby/postgresql.auto.conf
    # Store the final passfile in the physical copy and point the generated
    # connection string at it without replacing its URI-derived options.  The
    # -R output may quote the temporary PGPASSFILE path with doubled single
    # quotes; replace that option rather than prepending a duplicate.
    printf "*:%s:*:%s:%s\\n" "$port" "$user" "$escaped_password" > /standby/.pgpass
    chmod 600 /standby/.pgpass
    grep -Eq "passfile=" /standby/postgresql.auto.conf || {
      echo "pg_basebackup did not emit a passfile in primary_conninfo" >&2
      exit 1
    }
    # The generated path may be bare or wrapped in doubled single quotes;
    # both forms are covered by the non-whitespace token substitution.
    sed -E -i "s#passfile=[^[:space:]]+#passfile=/var/lib/postgresql/data/.pgpass#g" \
      /standby/postgresql.auto.conf
    primary_conninfo_count="$(grep -Ec "^[[:space:]]*primary_conninfo[[:space:]]*=" \
      /standby/postgresql.auto.conf | tr -d "[:space:]")"
    [[ "$primary_conninfo_count" == 1 ]]
    passfile_count="$(grep -Eo "passfile=/var/lib/postgresql/data/.pgpass" \
      /standby/postgresql.auto.conf | wc -l | tr -d "[:space:]")"
    [[ "$passfile_count" == 1 ]]
    ! grep -Eqi "(/run/cex|PGPASSFILE|(^|[[:space:]])password[[:space:]]*=)" \
      /standby/postgresql.auto.conf
    : > /standby/standby.signal
    # The client image runs this setup as root; PostgreSQL itself runs as the
    # postgres OS user and needs ownership of the entire physical data tree,
    # not only the three recovery metadata files.
    chown -R postgres:postgres /standby
  ' -- "$STREAMING_PRIMARY_URL_SAFE" "$CEX_POSTGRES_USER" "$STREAMING_PRIMARY_PORT"

cex_docker run -d --name "$STANDBY_CONTAINER" --network host \
  -v "$STANDBY_VOLUME:/var/lib/postgresql/data" postgres:16 \
  postgres -p "$STANDBY_PORT" -c hot_standby=on >/dev/null
for _ in $(seq 1 90); do
  standby_psql -Atc 'select pg_is_in_recovery()' 2>/dev/null | grep -qx t && break
  sleep 1
done
if ! standby_psql -Atc 'select pg_is_in_recovery()' 2>/dev/null | grep -qx t; then
  echo "streaming standby did not reach recovery mode" >&2
  exit 1
fi

# Establish that the service is healthy before inducing the database outage;
# an already-down service must not be counted as a successful fail-closed
# observation.
TRNM_LEDGER_READINESS_URL="${LEDGER_BASE_URL:-http://127.0.0.1:7002}/v1/trnm/economy/readiness"
TRNM_CONSUMER_HEALTH_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}/health"
if ! curl --max-time 5 -fsS "$TRNM_LEDGER_READINESS_URL" >/dev/null; then
  echo "ledger readiness was not healthy before the streaming outage" >&2
  exit 1
fi

cex_psql_stdin -c "insert into trnm_dr_markers(marker_id, marker_name)
  values (gen_random_uuid(), '$RUN_ID-replicated')" >/dev/null
for _ in $(seq 1 60); do
  standby_psql -Atc "select count(*) from trnm_dr_markers
    where marker_name = '$RUN_ID-replicated'" 2>/dev/null | grep -qx 1 && break
  sleep 1
done
standby_psql -Atc "select count(*) from trnm_dr_markers
  where marker_name = '$RUN_ID-replicated'" | grep -qx 1

cex_docker stop "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null
PRIMARY_STOPPED=true
if curl --max-time 5 -fsS "$TRNM_LEDGER_READINESS_URL" >/dev/null 2>&1; then
  echo "ledger unexpectedly remained ready after primary database stop" >&2
  exit 1
fi
cex_docker exec -u postgres "$STANDBY_CONTAINER" pg_ctl \
  -D /var/lib/postgresql/data promote -w >/dev/null
[[ "$(standby_psql -Atc 'select pg_is_in_recovery()')" == f ]]
standby_psql -c "insert into trnm_dr_markers(marker_id, marker_name)
  values (gen_random_uuid(), '$RUN_ID-promoted')" >/dev/null

cex_docker start "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null
cex_wait_postgres 90 1
if ! systemctl --user restart cex-trnm-ledger.service cex-trnm-consumer.service; then
  echo "failed to restart TRNM ledger/consumer services on the original primary" >&2
  exit 1
fi
for _ in $(seq 1 90); do
  if systemctl --user is-active --quiet cex-trnm-ledger.service &&
     systemctl --user is-active --quiet cex-trnm-consumer.service &&
     curl --max-time 5 -fsS "$TRNM_LEDGER_READINESS_URL" >/dev/null 2>&1 &&
     curl --max-time 5 -fsS "$TRNM_CONSUMER_HEALTH_URL" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
if ! systemctl --user is-active --quiet cex-trnm-ledger.service ||
   ! systemctl --user is-active --quiet cex-trnm-consumer.service ||
   ! curl --max-time 5 -fsS "$TRNM_LEDGER_READINESS_URL" >/dev/null 2>&1 ||
   ! curl --max-time 5 -fsS "$TRNM_CONSUMER_HEALTH_URL" >/dev/null 2>&1; then
  echo "TRNM services did not recover on the original primary" >&2
  exit 1
fi
PRIMARY_STOPPED=false

jq -n --arg run_id "$RUN_ID" \
  '{status:"passed",run_id:$run_id,streaming_replication:true,
    replicated_marker_visible:true,primary_stop_detected:true,
    standby_promoted_writable:true,services_recovered_on_original_primary:true,
    boundary:"same-host streaming/failover chaos; multi-host fencing and quorum remain external"}'

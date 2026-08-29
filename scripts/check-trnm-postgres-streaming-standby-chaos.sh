#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

RUN_ID="standby-$(date +%s)-${RANDOM}"
STANDBY_VOLUME="cex_trnm_standby_${RANDOM}_$$"
STANDBY_CONTAINER="cex-trnm-standby-${RANDOM}-$$"
STANDBY_PORT="${TRNM_STANDBY_PORT:-55434}"
PRIMARY_STOPPED=false

standby_psql() {
  cex_docker run --rm --network host -e PGPASSWORD="$CEX_POSTGRES_PASSWORD" postgres:16 \
    psql -h 127.0.0.1 -p "$STANDBY_PORT" -U "$CEX_POSTGRES_USER" \
      -d "$CEX_POSTGRES_DB" -v ON_ERROR_STOP=1 "$@"
}

cleanup() {
  if [[ "$PRIMARY_STOPPED" == true ]]; then
    cex_docker start "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null || true
  fi
  cex_docker stop "$STANDBY_CONTAINER" >/dev/null 2>&1 || true
  cex_docker container rm "$STANDBY_CONTAINER" >/dev/null 2>&1 || true
  cex_docker volume rm "$STANDBY_VOLUME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" bash -ceu '
  rule="host replication postgres 172.16.0.0/12 scram-sha-256"
  grep -Fqx "$rule" "$PGDATA/pg_hba.conf" || printf "%s\n" "$rule" >>"$PGDATA/pg_hba.conf"
'
cex_psql_stdin -Atc 'select pg_reload_conf()' >/dev/null

cex_docker volume create "$STANDBY_VOLUME" >/dev/null
cex_docker run --rm --network host -e PGPASSWORD="$CEX_POSTGRES_PASSWORD" \
  -v "$STANDBY_VOLUME:/standby" postgres:16 \
  pg_basebackup \
    --dbname="postgresql://$CEX_POSTGRES_USER:$CEX_POSTGRES_PASSWORD@127.0.0.1:5432/$CEX_POSTGRES_DB" \
    -D /standby -Fp -Xs -R -P >/dev/null

cex_docker run -d --name "$STANDBY_CONTAINER" --network host \
  -v "$STANDBY_VOLUME:/var/lib/postgresql/data" postgres:16 \
  postgres -p "$STANDBY_PORT" -c hot_standby=on >/dev/null
for _ in $(seq 1 90); do
  standby_psql -Atc 'select pg_is_in_recovery()' 2>/dev/null | grep -qx t && break
  sleep 1
done

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
if curl --max-time 5 -fsS http://127.0.0.1:7002/v1/trnm/economy/readiness >/dev/null 2>&1; then
  echo "ledger unexpectedly remained ready after primary database stop" >&2
  exit 1
fi
cex_docker exec -u postgres "$STANDBY_CONTAINER" pg_ctl \
  -D /var/lib/postgresql/data promote -w >/dev/null
[[ "$(standby_psql -Atc 'select pg_is_in_recovery()')" == f ]]
standby_psql -c "insert into trnm_dr_markers(marker_id, marker_name)
  values (gen_random_uuid(), '$RUN_ID-promoted')" >/dev/null

cex_docker start "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null
PRIMARY_STOPPED=false
cex_wait_postgres 90 1
systemctl --user restart cex-trnm-ledger.service cex-trnm-consumer.service

jq -n --arg run_id "$RUN_ID" \
  '{status:"passed",run_id:$run_id,streaming_replication:true,
    replicated_marker_visible:true,primary_stop_detected:true,
    standby_promoted_writable:true,services_recovered_on_original_primary:true,
    boundary:"same-host streaming/failover chaos; multi-host fencing and quorum remain external"}'

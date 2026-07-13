#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

RUN_ID="pitr-$(date +%s)-${RANDOM}"
RESTORE_POINT="trnm_${RUN_ID//-/_}"
BASE_VOLUME="cex_trnm_base_${RANDOM}_$$"
RESTORE_CONTAINER="cex-trnm-pitr-${RANDOM}-$$"
RESTORE_PORT="${TRNM_PITR_RESTORE_PORT:-55433}"
CHAOS="${1:-}"
PRIMARY_STOPPED=false

restore_psql() {
  cex_docker run --rm --network host -e PGPASSWORD="$CEX_POSTGRES_PASSWORD" postgres:16 \
    psql -h 127.0.0.1 -p "$RESTORE_PORT" -U "$CEX_POSTGRES_USER" \
      -d "$CEX_POSTGRES_DB" -v ON_ERROR_STOP=1 "$@"
}

cleanup() {
  if [[ "$PRIMARY_STOPPED" == true ]]; then
    cex_docker start "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null || true
  fi
  cex_docker stop "$RESTORE_CONTAINER" >/dev/null 2>&1 || true
  cex_docker container rm "$RESTORE_CONTAINER" >/dev/null 2>&1 || true
  cex_docker volume rm "$BASE_VOLUME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

archive_volume="$(cex_docker volume ls \
  --filter label=com.docker.compose.volume=postgres_wal_archive -q | head -n 1)"
if [[ -z "$archive_volume" ]]; then
  echo "postgres WAL archive volume is not installed; recreate docker-compose postgres first" >&2
  exit 1
fi

archive_mode="$(cex_psql_stdin -Atc 'show archive_mode')"
wal_level="$(cex_psql_stdin -Atc 'show wal_level')"
[[ "$archive_mode" == on && "$wal_level" == replica ]]

cex_docker volume create "$BASE_VOLUME" >/dev/null
cex_docker run --rm --network "container:$CEX_POSTGRES_CONTAINER_NAME" \
  -e PGPASSWORD="$CEX_POSTGRES_PASSWORD" -v "$BASE_VOLUME:/backup" postgres:16 \
  pg_basebackup -h 127.0.0.1 -U "$CEX_POSTGRES_USER" -D /backup -Fp -Xs -P >/dev/null

cex_psql_stdin -c "insert into trnm_dr_markers(marker_id, marker_name)
  values (gen_random_uuid(), '$RUN_ID-before')" >/dev/null
cex_psql_stdin -Atc "select pg_create_restore_point('$RESTORE_POINT')" >/dev/null
cex_psql_stdin -c "insert into trnm_dr_markers(marker_id, marker_name)
  values (gen_random_uuid(), '$RUN_ID-after')" >/dev/null
target_wal="$(cex_psql_stdin -Atc \
  'select pg_walfile_name(pg_current_wal_insert_lsn())')"
cex_psql_stdin -Atc 'select pg_switch_wal()' >/dev/null

for _ in $(seq 1 60); do
  target_archived="$(cex_docker run --rm -v "$archive_volume:/archive:ro" postgres:16 \
    bash -ceu 'test -s "/archive/$1" && stat -c %s "/archive/$1" || true' \
      bash "$target_wal")"
  [[ "$target_archived" -eq 16777216 ]] && break
  sleep 1
done
[[ "${target_archived:-0}" -eq 16777216 ]]
archive_count="$(cex_docker run --rm -v "$archive_volume:/archive:ro" postgres:16 \
  bash -ceu 'find /archive -maxdepth 1 -type f | wc -l')"

cex_docker run --rm -v "$BASE_VOLUME:/data" postgres:16 bash -ceu \
  "printf '%s\n' \"restore_command = 'cp /archive/%f %p'\" \
    \"recovery_target_name = '$RESTORE_POINT'\" \
    \"recovery_target_action = 'promote'\" >> /data/postgresql.auto.conf;
   touch /data/recovery.signal; chown -R postgres:postgres /data"

cex_docker run -d --name "$RESTORE_CONTAINER" \
  -p "127.0.0.1:${RESTORE_PORT}:5432" \
  -v "$BASE_VOLUME:/var/lib/postgresql/data" \
  -v "$archive_volume:/archive:ro" postgres:16 >/dev/null

for _ in $(seq 1 90); do
  restore_psql -Atc 'select 1' >/dev/null 2>&1 && break
  sleep 1
done
before_count="$(restore_psql -Atc \
  "select count(*) from trnm_dr_markers where marker_name = '$RUN_ID-before'")"
after_count="$(restore_psql -Atc \
  "select count(*) from trnm_dr_markers where marker_name = '$RUN_ID-after'")"
recovery_state="$(restore_psql -Atc \
  'select pg_is_in_recovery()')"
[[ "$before_count" == 1 && "$after_count" == 0 && "$recovery_state" == f ]]

restore_psql -c \
  "insert into trnm_dr_markers(marker_id, marker_name)
   values (gen_random_uuid(), '$RUN_ID-promoted')" >/dev/null

chaos_verified=false
if [[ "$CHAOS" == "--chaos" ]]; then
  cex_docker stop "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null
  PRIMARY_STOPPED=true
  restore_psql -Atc \
    "select count(*) from trnm_dr_markers where marker_name = '$RUN_ID-promoted'" | grep -qx 1
  if curl --max-time 5 -fsS http://127.0.0.1:7002/v1/trnm/economy/readiness >/dev/null 2>&1; then
    echo "ledger unexpectedly remained ready after primary database stop" >&2
    exit 1
  fi
  cex_docker start "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null
  PRIMARY_STOPPED=false
  cex_wait_postgres 90 1
  systemctl --user restart cex-trnm-ledger.service cex-trnm-consumer.service
  chaos_verified=true
fi

jq -n --arg run_id "$RUN_ID" --arg archive_mode "$archive_mode" \
  --arg wal_level "$wal_level" --argjson archived_segments "$archive_count" \
  --arg target_wal "$target_wal" \
  --argjson chaos_verified "$chaos_verified" \
  '{status:"passed",run_id:$run_id,physical_base_backup:true,
    archive_mode:$archive_mode,wal_level:$wal_level,archived_segments:$archived_segments,
    target_wal:$target_wal,target_wal_archived_complete:true,
    pitr_before_marker_present:true,pitr_after_marker_absent:true,
    restored_instance_promoted_writable:true,primary_stop_fail_closed:$chaos_verified,
    boundary:"same-host PITR and promotion drill; multi-host quorum/fencing remains external infrastructure"}'

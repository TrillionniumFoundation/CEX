#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
if [[ ${DATABASE_URL+x} ]]; then
  cex_sync_postgres_env_from_database_url "$(cex_effective_database_url)"
fi
if [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" ]] && ! cex_postgres_host_is_local; then
  echo "refusing non-local DATABASE_URL: PITR drill uses Docker-local PostgreSQL operations" >&2
  exit 2
fi
if [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" ]] \
   && ! cex_postgres_docker_socket_is_target; then
  echo "refusing DATABASE_URL whose local port is not the Docker PostgreSQL target" >&2
  exit 2
fi
PITR_POSTGRES_PASSWORD=""
PITR_POSTGRES_PASSWORD_SET=0
if cex_postgres_password_is_set; then
  # Respect an explicitly supplied PGPASSWORD before the decoded URI/.env
  # credential; the helper has already removed only an imported shadow value.
  PITR_POSTGRES_PASSWORD="$(cex_postgres_password_value)"
  PITR_POSTGRES_PASSWORD_SET=1
fi

RUN_ID="pitr-$(date +%s)-${RANDOM}"
RESTORE_POINT="trnm_${RUN_ID//-/_}"
BASE_VOLUME="cex_trnm_base_${RANDOM}_$$"
RESTORE_CONTAINER="cex-trnm-pitr-${RANDOM}-$$"
RESTORE_PORT="${TRNM_PITR_RESTORE_PORT:-55433}"
CHAOS="${1:-}"
PRIMARY_STOPPED=false
PERSISTENT_BACKUP_ID="${TRNM_PITR_PERSISTENT_BACKUP_ID:-}"
PERSISTENT_BACKUP_VOLUME=""
PERSISTENT_BACKUP_RESTORE_VERIFIED=false
LOCK_FILE="$CEX_PROJECT_ROOT/run/trnm-postgres-backup.lock"
CURRENT_PHASE="bootstrap"

mkdir -p "$(dirname -- "$LOCK_FILE")"
exec 9>"$LOCK_FILE"
if ! flock -n 9; then
  echo "another PostgreSQL backup or restore operation holds $LOCK_FILE" >&2
  exit 1
fi

restore_psql() {
  if [[ "$PITR_POSTGRES_PASSWORD_SET" == "1" ]]; then
    cex_docker_run_with_password "$PITR_POSTGRES_PASSWORD" \
      --rm --network host postgres:16 \
      psql -h 127.0.0.1 -p "$RESTORE_PORT" -U "$CEX_POSTGRES_USER" \
        -d "$CEX_POSTGRES_DB" -v ON_ERROR_STOP=1 "$@"
  else
    cex_docker run --rm --network host postgres:16 \
      psql -h 127.0.0.1 -p "$RESTORE_PORT" -U "$CEX_POSTGRES_USER" \
        -d "$CEX_POSTGRES_DB" -v ON_ERROR_STOP=1 "$@"
  fi
}

cleanup() {
  local status=$?
  if [[ "$PRIMARY_STOPPED" == true ]]; then
    cex_docker start "$CEX_POSTGRES_CONTAINER_NAME" >/dev/null || true
  fi
  cex_docker stop "$RESTORE_CONTAINER" >/dev/null 2>&1 || true
  cex_docker container rm "$RESTORE_CONTAINER" >/dev/null 2>&1 || true
  cex_docker volume rm "$BASE_VOLUME" >/dev/null 2>&1 || true
  if [[ "$status" -ne 0 ]]; then
    echo "TRNM PostgreSQL PITR drill failed in phase $CURRENT_PHASE" >&2
  fi
  exit "$status"
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
if [[ -n "$PERSISTENT_BACKUP_ID" ]]; then
  CURRENT_PHASE="copy-persistent-base-backup"
  if [[ ! "$PERSISTENT_BACKUP_ID" =~ ^base-[0-9]{8}T[0-9]{6}Z-[0-9]+$ ]]; then
    echo "TRNM_PITR_PERSISTENT_BACKUP_ID is invalid" >&2
    exit 2
  fi
  PERSISTENT_BACKUP_VOLUME="$(cex_docker volume ls \
    --filter label=com.docker.compose.volume=postgres_base_backups -q | head -n 1)"
  if [[ -z "$PERSISTENT_BACKUP_VOLUME" ]]; then
    echo "persistent PostgreSQL base-backup volume is unavailable" >&2
    exit 1
  fi
  persistent_metadata="$(cex_docker run --rm \
    -v "$PERSISTENT_BACKUP_VOLUME:/backups:ro" postgres:16 \
    cat "/backups/$PERSISTENT_BACKUP_ID/.trnm-verified.json")"
  jq -e --arg backup_id "$PERSISTENT_BACKUP_ID" '
    .status == "verified" and .backup_id == $backup_id and
    .pg_verifybackup == true and .same_host_cache == true and
    .off_host == false and .automatic_expiration == false' \
    >/dev/null <<<"$persistent_metadata"
  cex_docker run --rm \
    -e BACKUP_ID="$PERSISTENT_BACKUP_ID" \
    -v "$PERSISTENT_BACKUP_VOLUME:/source:ro" -v "$BASE_VOLUME:/backup" \
    postgres:16 bash -ceu '
      cp -a "/source/$BACKUP_ID/." /backup/
      rm -f /backup/.trnm-verified.json
      chown -R postgres:postgres /backup
    '
else
  CURRENT_PHASE="create-temporary-base-backup"
  if [[ "$PITR_POSTGRES_PASSWORD_SET" == "1" ]]; then
    cex_docker_run_with_password "$PITR_POSTGRES_PASSWORD" \
      --rm --network "container:$CEX_POSTGRES_CONTAINER_NAME" \
      -v "$BASE_VOLUME:/backup" postgres:16 \
      pg_basebackup -h 127.0.0.1 -U "$CEX_POSTGRES_USER" -D /backup -Fp -Xs -P \
        --checkpoint=fast --manifest-checksums=SHA256 >/dev/null
  else
    cex_docker run --rm --network "container:$CEX_POSTGRES_CONTAINER_NAME" \
      -v "$BASE_VOLUME:/backup" postgres:16 \
      pg_basebackup -h 127.0.0.1 -U "$CEX_POSTGRES_USER" -D /backup -Fp -Xs -P \
        --checkpoint=fast --manifest-checksums=SHA256 >/dev/null
  fi
fi
CURRENT_PHASE="verify-base-backup-manifest"
cex_docker run --rm -v "$BASE_VOLUME:/backup:ro" postgres:16 \
  pg_verifybackup /backup >/dev/null

CURRENT_PHASE="create-restore-point"
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

CURRENT_PHASE="configure-recovery"
cex_docker run --rm -v "$BASE_VOLUME:/data" postgres:16 bash -ceu \
  "printf '%s\n' \"restore_command = 'cp /archive/%f %p'\" \
    \"recovery_target_name = '$RESTORE_POINT'\" \
    \"recovery_target_action = 'promote'\" >> /data/postgresql.auto.conf;
   touch /data/recovery.signal; chown -R postgres:postgres /data"

CURRENT_PHASE="start-restored-instance"
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

CURRENT_PHASE="prove-restored-instance-writable"
restore_psql -c \
  "insert into trnm_dr_markers(marker_id, marker_name)
   values (gen_random_uuid(), '$RUN_ID-promoted')" >/dev/null

chaos_verified=false
if [[ "$CHAOS" == "--chaos" ]]; then
  CURRENT_PHASE="primary-stop-chaos"
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

if [[ -n "$PERSISTENT_BACKUP_ID" ]]; then
  CURRENT_PHASE="record-persistent-restore-drill"
  metadata="$(cex_docker run --rm -v "$PERSISTENT_BACKUP_VOLUME:/backups:ro" \
    postgres:16 cat "/backups/$PERSISTENT_BACKUP_ID/.trnm-verified.json")"
  verified_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  updated_metadata="$(jq -c --arg run_id "$RUN_ID" --arg verified_at "$verified_at" '
    . + {restore_drill_verified:true,restore_drill_run_id:$run_id,
      restore_drill_verified_at:$verified_at}' <<<"$metadata")"
  printf '%s\n' "$updated_metadata" | cex_docker run --rm -i --user postgres \
    -e BACKUP_ID="$PERSISTENT_BACKUP_ID" \
    -v "$PERSISTENT_BACKUP_VOLUME:/backups" postgres:16 bash -ceu '
      temporary="/backups/$BACKUP_ID/.trnm-verified.json.tmp"
      cat >"$temporary"
      sync -f "$temporary"
      mv "$temporary" "/backups/$BACKUP_ID/.trnm-verified.json"
      sync -f "/backups/$BACKUP_ID"
    '
  PERSISTENT_BACKUP_RESTORE_VERIFIED=true
fi

CURRENT_PHASE="report"
jq -n --arg run_id "$RUN_ID" --arg archive_mode "$archive_mode" \
  --arg wal_level "$wal_level" --argjson archived_segments "$archive_count" \
  --arg target_wal "$target_wal" \
  --argjson chaos_verified "$chaos_verified" \
  --arg persistent_backup_id "$PERSISTENT_BACKUP_ID" \
  --argjson persistent_backup_restore_verified "$PERSISTENT_BACKUP_RESTORE_VERIFIED" \
  '{status:"passed",run_id:$run_id,physical_base_backup:true,
    base_backup_manifest_verified:true,
    archive_mode:$archive_mode,wal_level:$wal_level,archived_segments:$archived_segments,
    target_wal:$target_wal,target_wal_archived_complete:true,
    pitr_before_marker_present:true,pitr_after_marker_absent:true,
    restored_instance_promoted_writable:true,primary_stop_fail_closed:$chaos_verified,
    persistent_base_backup_id:($persistent_backup_id |
      if length > 0 then . else null end),
    persistent_base_backup_restore_verified:$persistent_backup_restore_verified,
    boundary:"same-host PITR and promotion drill; multi-host quorum/fencing remains external infrastructure"}'

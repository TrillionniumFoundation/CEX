#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

cex_require_cmd flock jq sha256sum

LOCK_FILE="$CEX_PROJECT_ROOT/run/trnm-postgres-backup.lock"
EVIDENCE_DIR="$CEX_PROJECT_ROOT/run/trnm-postgres-backups"
MIN_FREE_MIB="${CEX_TRNM_BASE_BACKUP_MIN_FREE_MIB:-12288}"
BACKUP_ID="base-$(date -u +%Y%m%dT%H%M%SZ)-${RANDOM}"
INCOMPLETE_ID="$BACKUP_ID.incomplete"
BACKUP_CONTAINER="cex-trnm-base-backup-${RANDOM}-$$"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

if ! [[ "$MIN_FREE_MIB" =~ ^[1-9][0-9]*$ ]]; then
  echo "CEX_TRNM_BASE_BACKUP_MIN_FREE_MIB must be a positive integer" >&2
  exit 2
fi

mkdir -p "$EVIDENCE_DIR" "$(dirname -- "$LOCK_FILE")"
exec 9>"$LOCK_FILE"
if ! flock -n 9; then
  echo "another PostgreSQL backup or restore operation holds $LOCK_FILE" >&2
  exit 1
fi

cleanup() {
  cex_docker rm -f "$BACKUP_CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT

archive_healthy="$(cex_psql_stdin -Atc "
  select current_setting('archive_mode') = 'on'
    and exists (select 1 from pg_stat_archiver
      where last_failed_time is null or last_archived_time >= last_failed_time)")"
if [[ "$archive_healthy" != t ]]; then
  echo "refusing base backup while WAL archiving is degraded" >&2
  exit 1
fi

cex_docker compose --project-directory "$CEX_PROJECT_ROOT" \
  -f "$CEX_PROJECT_ROOT/docker-compose.yml" run --rm postgres-backup-init >/dev/null
backup_volume="$(cex_docker volume ls \
  --filter label=com.docker.compose.volume=postgres_base_backups -q | head -n 1)"
if [[ -z "$backup_volume" ]]; then
  echo "postgres base-backup volume was not created" >&2
  exit 1
fi

available_mib="$(cex_docker run --rm -v "$backup_volume:/backups" postgres:16 \
  bash -ceu "df -Pm /backups | awk 'NR == 2 {print \$4}'")"
if (( available_mib < MIN_FREE_MIB )); then
  echo "base-backup volume requires ${MIN_FREE_MIB} MiB free; observed ${available_mib} MiB" >&2
  exit 1
fi

log_file="$EVIDENCE_DIR/$BACKUP_ID.log"
cex_docker run --name "$BACKUP_CONTAINER" --rm \
  --user postgres --network "container:$CEX_POSTGRES_CONTAINER_NAME" \
  -e PGPASSWORD="$CEX_POSTGRES_PASSWORD" \
  -e BACKUP_DIR="/backups/$INCOMPLETE_ID" \
  -e POSTGRES_USER="$CEX_POSTGRES_USER" \
  -v "$backup_volume:/backups" postgres:16 bash -ceu '
    umask 077
    test ! -e "$BACKUP_DIR"
    mkdir "$BACKUP_DIR"
    pg_basebackup -h 127.0.0.1 -U "$POSTGRES_USER" -D "$BACKUP_DIR" \
      -Fp -Xs -P --checkpoint=fast --manifest-checksums=SHA256
    pg_verifybackup "$BACKUP_DIR"
    sync -f "$BACKUP_DIR/backup_manifest"
    sync -f "$BACKUP_DIR"
  ' >"$log_file" 2>&1

manifest="$(cex_docker run --rm -v "$backup_volume:/backups:ro" postgres:16 \
  cat "/backups/$INCOMPLETE_ID/backup_manifest")"
manifest_sha256="$(cex_docker run --rm -v "$backup_volume:/backups:ro" postgres:16 \
  sha256sum "/backups/$INCOMPLETE_ID/backup_manifest" | awk '{print $1}')"
wal_ranges="$(jq -c '.["WAL-Ranges"] // []' <<<"$manifest")"
backup_bytes="$(cex_docker run --rm -v "$backup_volume:/backups:ro" postgres:16 \
  du -sb "/backups/$INCOMPLETE_ID" | awk '{print $1}')"
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
metadata="$(jq -cn \
  --arg backup_id "$BACKUP_ID" \
  --arg started_at "$STARTED_AT" \
  --arg finished_at "$FINISHED_AT" \
  --arg manifest_sha256 "$manifest_sha256" \
  --argjson wal_ranges "$wal_ranges" \
  --argjson backup_bytes "$backup_bytes" \
  '{status:"verified",backup_id:$backup_id,started_at:$started_at,
    finished_at:$finished_at,postgres_major:16,format:"plain",
    wal_method:"stream",manifest_checksum:"SHA256",
    manifest_sha256:$manifest_sha256,wal_ranges:$wal_ranges,
    backup_bytes:$backup_bytes,pg_verifybackup:true,
    restore_drill_verified:false,same_host_cache:true,off_host:false,
    automatic_expiration:false}')"

printf '%s\n' "$metadata" | cex_docker run --rm -i --user postgres \
  -e INCOMPLETE_ID="$INCOMPLETE_ID" -e BACKUP_ID="$BACKUP_ID" \
  -v "$backup_volume:/backups" postgres:16 bash -ceu '
    umask 077
    metadata_tmp="/backups/$INCOMPLETE_ID/.trnm-verified.json.tmp"
    cat >"$metadata_tmp"
    sync -f "$metadata_tmp"
    mv "$metadata_tmp" "/backups/$INCOMPLETE_ID/.trnm-verified.json"
    sync -f "/backups/$INCOMPLETE_ID"
    mv "/backups/$INCOMPLETE_ID" "/backups/$BACKUP_ID"
    sync -f /backups
  '

trap - EXIT
jq -n --argjson backup "$metadata" --arg volume "$backup_volume" \
  --arg evidence_log "$log_file" \
  '{status:"passed",backup:$backup,volume:$volume,evidence_log:$evidence_log,
    boundary:"verified persistent same-host base-backup cache; off-host disaster recovery and automatic expiry remain blocked"}'

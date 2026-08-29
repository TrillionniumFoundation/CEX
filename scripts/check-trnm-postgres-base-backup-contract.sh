#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CHECKER="$ROOT_DIR/scripts/check-trnm-postgres-base-backups.sh"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/cex-base-backup.XXXXXX")"
trap 'rm -rf -- "$TMP_DIR"' EXIT

make_backup_fixture() {
  local id="$1" started_at="$2" manifest sha
  local dir="$TMP_DIR/$id"
  mkdir -p "$dir"
  manifest='{"PostgreSQL-Backup-Manifest-Version":2,"Files":[],"WAL-Ranges":[]}'
  printf '%s' "$manifest" >"$dir/backup_manifest"
  sha="$(printf '%s' "$manifest" | sha256sum | awk '{print $1}')"
  jq -n --arg id "$id" --arg started "$started_at" --arg sha "$sha" \
    '{status:"verified",backup_id:$id,started_at:$started,
      pg_verifybackup:true,same_host_cache:true,off_host:false,
      automatic_expiration:false,manifest_sha256:$sha}' \
    >"$dir/.trnm-verified.json"
}

make_backup_fixture base-20260713T000000Z-1 2026-07-13T00:00:00Z
make_backup_fixture base-20260714T000000Z-2 2026-07-14T00:00:00Z
result="$(CEX_TRNM_BASE_BACKUP_FIXTURE_DIR="$TMP_DIR" \
  CEX_TRNM_MIN_VERIFIED_BASE_BACKUPS=2 "$CHECKER")"
jq -e '.local_cache_ready == true and .verified_backups == 2 and
  .automatic_expiration == false and .retention_policy_ready == false' \
  >/dev/null <<<"$result"

printf 'corrupt' >>"$TMP_DIR/base-20260714T000000Z-2/backup_manifest"
if CEX_TRNM_BASE_BACKUP_FIXTURE_DIR="$TMP_DIR" \
    CEX_TRNM_MIN_VERIFIED_BASE_BACKUPS=2 "$CHECKER" >/dev/null; then
  echo "corrupt backup manifest was unexpectedly accepted" >&2
  exit 1
fi

mkdir "$TMP_DIR/base-20260714T010000Z-3.incomplete"
if CEX_TRNM_BASE_BACKUP_FIXTURE_DIR="$TMP_DIR" \
    CEX_TRNM_MIN_VERIFIED_BASE_BACKUPS=1 "$CHECKER" >/dev/null; then
  echo "incomplete backup was unexpectedly accepted" >&2
  exit 1
fi

jq -n '{status:"passed",verified_catalog_required:true,
  manifest_hash_mismatch_rejected:true,incomplete_backup_rejected:true,
  automatic_expiration:false,off_host_claim:false}'

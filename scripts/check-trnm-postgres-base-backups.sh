#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"

MIN_VERIFIED="${CEX_TRNM_MIN_VERIFIED_BASE_BACKUPS:-1}"
FIXTURE_DIR="${CEX_TRNM_BASE_BACKUP_FIXTURE_DIR:-}"
if ! [[ "$MIN_VERIFIED" =~ ^[1-9][0-9]*$ ]]; then
  echo "CEX_TRNM_MIN_VERIFIED_BASE_BACKUPS must be a positive integer" >&2
  exit 2
fi

if [[ -n "$FIXTURE_DIR" ]]; then
  ROOT="$FIXTURE_DIR"
  list_dirs() { find "$ROOT" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort; }
  read_file() { cat "$ROOT/$1"; }
  hash_file() { sha256sum "$ROOT/$1" | awk '{print $1}'; }
else
  backup_volume="$(cex_docker volume ls \
    --filter label=com.docker.compose.volume=postgres_base_backups -q | head -n 1)"
  if [[ -z "$backup_volume" ]]; then
    jq -n '{status:"blocked",verified_backups:0,local_cache_ready:false,
      retention_policy_ready:false,reason:"postgres base-backup volume is absent"}'
    exit 1
  fi
  list_dirs() {
    cex_docker run --rm -v "$backup_volume:/backups:ro" postgres:16 \
      find /backups -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort
  }
  read_file() {
    cex_docker run --rm -v "$backup_volume:/backups:ro" postgres:16 \
      cat "/backups/$1"
  }
  hash_file() {
    cex_docker run --rm -v "$backup_volume:/backups:ro" postgres:16 \
      sha256sum "/backups/$1" | awk '{print $1}'
  }
fi

verified=0
invalid=0
incomplete=0
earliest=""
latest=""
while IFS= read -r entry; do
  [[ -n "$entry" ]] || continue
  if [[ "$entry" == *.incomplete ]]; then
    incomplete=$((incomplete + 1))
    continue
  fi
  if [[ ! "$entry" =~ ^base-[0-9]{8}T[0-9]{6}Z-[0-9]+$ ]]; then
    invalid=$((invalid + 1))
    continue
  fi
  metadata="$(read_file "$entry/.trnm-verified.json" 2>/dev/null || true)"
  manifest="$(read_file "$entry/backup_manifest" 2>/dev/null || true)"
  if [[ -z "$metadata" || -z "$manifest" ]]; then
    invalid=$((invalid + 1))
    continue
  fi
  manifest_sha256="$(hash_file "$entry/backup_manifest")"
  if ! jq -e --arg id "$entry" --arg sha "$manifest_sha256" '
      .status == "verified" and .backup_id == $id and
      .pg_verifybackup == true and .same_host_cache == true and
      .off_host == false and .automatic_expiration == false and
      .manifest_sha256 == $sha' >/dev/null <<<"$metadata"; then
    invalid=$((invalid + 1))
    continue
  fi
  started_at="$(jq -er .started_at <<<"$metadata")"
  [[ -z "$earliest" || "$started_at" < "$earliest" ]] && earliest="$started_at"
  [[ -z "$latest" || "$started_at" > "$latest" ]] && latest="$started_at"
  verified=$((verified + 1))
done < <(list_dirs)

local_cache_ready=false
if (( verified >= MIN_VERIFIED && invalid == 0 && incomplete == 0 )); then
  local_cache_ready=true
fi
retention_policy_ready=false
jq -n \
  --arg status "$([[ "$local_cache_ready" == true ]] && printf ready || printf blocked)" \
  --argjson verified_backups "$verified" \
  --argjson minimum_verified_backups "$MIN_VERIFIED" \
  --argjson invalid_entries "$invalid" \
  --argjson incomplete_backups "$incomplete" \
  --arg earliest_verified_backup "$earliest" \
  --arg latest_verified_backup "$latest" \
  --argjson local_cache_ready "$local_cache_ready" \
  --argjson retention_policy_ready "$retention_policy_ready" \
  '{status:$status,verified_backups:$verified_backups,
    minimum_verified_backups:$minimum_verified_backups,
    invalid_entries:$invalid_entries,incomplete_backups:$incomplete_backups,
    earliest_verified_backup:($earliest_verified_backup |
      if length > 0 then . else null end),
    latest_verified_backup:($latest_verified_backup |
      if length > 0 then . else null end),
    local_cache_ready:$local_cache_ready,
    retention_policy_ready:$retention_policy_ready,
    same_host_cache:true,off_host:false,automatic_expiration:false,
    boundary:"local cache only; seven-day off-host PITR and dependency-aware expiry are not established"}'
[[ "$local_cache_ready" == true ]]

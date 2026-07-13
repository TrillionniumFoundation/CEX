#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
ARCHIVER="$ROOT_DIR/scripts/postgres-archive-wal.sh"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/cex-wal-archive.XXXXXX")"
trap 'rm -rf -- "$TMP_DIR"' EXIT

mkdir -p "$TMP_DIR/archive"
source_file="$TMP_DIR/source-wal"
wal_name="000000010000000000000001"
dd if=/dev/zero of="$source_file" bs=16384 count=1 status=none
printf 'cex-wal-archive-contract\n' | dd of="$source_file" conv=notrunc status=none

export CEX_POSTGRES_WAL_ARCHIVE_DIR="$TMP_DIR/archive"
"$ARCHIVER" "$source_file" "$wal_name"
cmp -s "$source_file" "$TMP_DIR/archive/$wal_name"

existing_identity="$(stat -c '%i:%Y' "$TMP_DIR/archive/$wal_name")"
"$ARCHIVER" "$source_file" "$wal_name"
[[ "$(stat -c '%i:%Y' "$TMP_DIR/archive/$wal_name")" == "$existing_identity" ]]

truncate -s 123 "$TMP_DIR/archive/$wal_name"
"$ARCHIVER" "$source_file" "$wal_name"
cmp -s "$source_file" "$TMP_DIR/archive/$wal_name"
[[ "$(find "$TMP_DIR/archive" -maxdepth 1 -name '.*.tmp.*' -print -quit)" == "" ]]

if "$ARCHIVER" "$source_file" '../unsafe' >/dev/null 2>&1; then
  echo "unsafe WAL name was unexpectedly accepted" >&2
  exit 1
fi

jq -n --argjson bytes "$(stat -c %s "$TMP_DIR/archive/$wal_name")" \
  '{status:"passed",atomic_replace:true,identical_retry_idempotent:true,
    corrupt_destination_repaired:true,temporary_files_cleaned:true,
    unsafe_name_rejected:true,archived_bytes:$bytes}'

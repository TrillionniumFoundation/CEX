#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: postgres-archive-wal.sh SOURCE_PATH WAL_NAME" >&2
  exit 64
fi

source_path="$1"
wal_name="$2"
archive_dir="${CEX_POSTGRES_WAL_ARCHIVE_DIR:-/var/lib/postgresql/wal-archive}"

if [[ ! "$wal_name" =~ ^[0-9A-F]{8,24}([.][A-Za-z0-9_-]+)*$ ]]; then
  echo "refusing unsafe WAL archive name: $wal_name" >&2
  exit 64
fi
if [[ ! -f "$source_path" || ! -d "$archive_dir" ]]; then
  echo "WAL source or archive directory is unavailable" >&2
  exit 1
fi

destination="$archive_dir/$wal_name"
if [[ -f "$destination" ]] && cmp -s -- "$source_path" "$destination"; then
  exit 0
fi

temporary="$(mktemp "$archive_dir/.${wal_name}.tmp.XXXXXX")"
cleanup() {
  rm -f -- "$temporary"
}
trap cleanup EXIT

cp -- "$source_path" "$temporary"
chmod 600 "$temporary"
sync -f "$temporary"
cmp -s -- "$source_path" "$temporary"
mv -f -- "$temporary" "$destination"
sync -f "$archive_dir"
trap - EXIT

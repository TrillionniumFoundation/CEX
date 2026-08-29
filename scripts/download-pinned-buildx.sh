#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -lt 3 || "$#" -gt 4 ]]; then
  echo "usage: $0 <https-url> <sha256> <destination> [local-source]" >&2
  exit 2
fi

url=$1
expected_sha256=$2
destination=$3
local_source=${4:-}
max_attempts=8

if [[ ! "$url" =~ ^https:// ]] || [[ ! "$expected_sha256" =~ ^[0-9a-f]{64}$ ]]; then
  echo "pinned download requires an HTTPS URL and canonical SHA-256" >&2
  exit 2
fi
if [[ -L "$destination" ]]; then
  echo "pinned download destination must not be a symlink" >&2
  exit 2
fi

mkdir -p "$(dirname "$destination")"
if [[ -n "$local_source" ]]; then
  if [[ -L "$local_source" || ! -f "$local_source" || ! -r "$local_source" ]]; then
    echo "pinned Buildx source must be a readable regular non-symlink file" >&2
    exit 2
  fi
  install -m 0500 -- "$local_source" "$destination"
else
  for ((attempt = 1; attempt <= max_attempts; attempt++)); do
    partial_size=$(stat -c %s "$destination" 2>/dev/null || echo 0)
    echo "pinned Buildx download attempt $attempt/$max_attempts from byte $partial_size" >&2
    if curl --fail --location --proto '=https' --proto-redir '=https' --continue-at - \
      --connect-timeout 15 --max-time 300 --show-error --silent --tlsv1.2 \
      "$url" --output "$destination"; then
      break
    fi
    if [[ "$attempt" -eq "$max_attempts" ]]; then
      echo "pinned Buildx download exhausted bounded attempts" >&2
      exit 1
    fi
    sleep 2
  done
  chmod 0500 "$destination"
fi

if [[ -L "$destination" || ! -f "$destination" ]]; then
  echo "staged Buildx plugin is not a regular non-symlink file" >&2
  exit 1
fi
actual_sha256=$(sha256sum "$destination" | cut -d' ' -f1)
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  echo "disposable Buildx checksum mismatch" >&2
  exit 1
fi

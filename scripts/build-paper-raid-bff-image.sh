#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-paper-raid-bff-image.sh --image-ref <local-tag>

Build and verify the immutable paper-raid-bff linux/amd64 image, then retain
the verified image under a new explicit local tag. The command never pushes.
EOF
}

if [[ ${1:-} == --help ]]; then
  usage
  exit 0
fi
if [[ $# -ne 2 || $1 != --image-ref || -z $2 ]]; then
  usage >&2
  exit 64
fi

image_ref=$2
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
[[ "$(tr -d '\r\n' <"$repo_root/PROJECT_ID")" == hepta-control-plane ]] || {
  echo "paper-raid-bff image builder is outside the canonical Hepta root" >&2
  exit 1
}

bash "$repo_root/scripts/project-preflight.sh" --audit

release_lock=$(git -C "$repo_root" rev-parse \
  --path-format=absolute --git-path paper-raid-bff-release-authority.lock)
exec 9>"$release_lock"
flock -n 9 || {
  echo "another paper-raid-bff release-authority process is already running" >&2
  exit 1
}

PAPER_RAID_BFF_EXPORT_IMAGE_REF=$image_ref \
  bash "$repo_root/services/paper-raid-bff/scripts/check-image.sh"

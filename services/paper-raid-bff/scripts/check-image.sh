#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
image_name=paper-raid-bff:local-gate
container_name=paper-raid-bff-image-gate-$$
scratch_dir=$(mktemp -d)

cleanup() {
  sudo -n docker rm -f "$container_name" >/dev/null 2>&1 || true
  rm -rf "$scratch_dir"
}
trap cleanup EXIT

"$repo_root/services/paper-raid-bff/scripts/check-docker-lock.sh"

sudo -n docker build \
  --platform linux/amd64 \
  --file "$repo_root/services/paper-raid-bff/Dockerfile" \
  --tag "$image_name" \
  "$repo_root"

architecture=$(sudo -n docker image inspect "$image_name" --format '{{.Architecture}}')
configured_user=$(sudo -n docker image inspect "$image_name" --format '{{.Config.User}}')
entrypoint=$(sudo -n docker image inspect "$image_name" --format '{{json .Config.Entrypoint}}')
healthcheck=$(sudo -n docker image inspect "$image_name" --format '{{json .Config.Healthcheck.Test}}')
[[ "$architecture" == "amd64" ]]
[[ "$configured_user" == "65532:65532" ]]
[[ "$entrypoint" == '["/paper-raid-bff"]' ]]
[[ "$healthcheck" == '["CMD","/paper-raid-bff","--probe-ready"]' ]]

if sudo -n docker image inspect "$image_name" --format '{{json .Config.Env}}' \
  | rg -n 'PAPER_RAID_BFF_|replace-with'
then
  echo "runtime image contains deployment configuration" >&2
  exit 1
fi

sudo -n docker create --name "$container_name" --read-only --network none "$image_name" >/dev/null
sudo -n docker export "$container_name" --output "$scratch_dir/rootfs.tar"
if tar -tf "$scratch_dir/rootfs.tar" \
  | rg -n '(^|/)(\.git|Cargo\.toml|Cargo\.lock|src|deploy|\.env)(/|$)'
then
  echo "runtime image contains build context or configuration files" >&2
  exit 1
fi

if sudo -n docker run --rm --read-only --network none --entrypoint /bin/sh "$image_name"; then
  echo "runtime image unexpectedly contains a shell" >&2
  exit 1
fi

set +e
runtime_output=$(sudo -n docker run --rm --read-only --network none \
  "$image_name" --probe-ready 2>&1)
runtime_status=$?
set -e
[[ "$runtime_status" -eq 1 ]]
if printf '%s' "$runtime_output" | rg -n 'replace-with|login_key|secret_access_key'; then
  echo "runtime startup leaked a sensitive configuration value" >&2
  exit 1
fi

echo "paper-raid-bff immutable image gate: ok"

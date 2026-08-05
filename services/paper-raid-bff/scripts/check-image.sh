#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
image_name=paper-raid-bff:local-gate
repro_image=paper-raid-bff:reproducibility-gate-$$
container_name=paper-raid-bff-image-gate-$$
sentinel_container=paper-raid-bff-sentinel-seed-$$
sentinel_scan_container=paper-raid-bff-sentinel-scan-$$
sentinel_image=paper-raid-bff:sentinel-negative-gate-$$
scratch_dir=$(mktemp -d)
rg_bin=$(command -v rg)
revision=$(git -C "$repo_root" rev-parse HEAD)
source_date_epoch=$(git -C "$repo_root" show -s --format=%ct HEAD)
created=$(date -u -d "@$source_date_epoch" '+%Y-%m-%dT%H:%M:%SZ')
cargo_lock="$repo_root/services/paper-raid-bff/docker/Cargo.lock"
sbom="$repo_root/services/paper-raid-bff/docker/sbom.cdx.json"
cargo_lock_sha256=$(sha256sum "$cargo_lock" | cut -d' ' -f1)
sbom_sha256=$(sha256sum "$sbom" | cut -d' ' -f1)
buildx_version=v0.36.1
buildx_url=https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64
buildx_sha256=48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778
docker_config="$scratch_dir/docker-config"
buildx_plugin="$docker_config/cli-plugins/docker-buildx"

cleanup() {
  sudo -n docker rm -f "$container_name" >/dev/null 2>&1 || true
  sudo -n docker rm -f "$sentinel_container" >/dev/null 2>&1 || true
  sudo -n docker rm -f "$sentinel_scan_container" >/dev/null 2>&1 || true
  sudo -n docker image rm -f "$image_name" >/dev/null 2>&1 || true
  sudo -n docker image rm -f "$sentinel_image" >/dev/null 2>&1 || true
  sudo -n docker image rm -f "$repro_image" >/dev/null 2>&1 || true
  case "$scratch_dir" in
    /tmp/tmp.*) sudo -n rm -rf -- "$scratch_dir" ;;
    *) echo "refusing to remove unexpected scratch path: $scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT INT TERM

scan_runtime_image() {
  local candidate_image=$1
  local candidate_container=$2
  local label=$3
  local scan_root="$scratch_dir/$label"
  local content_matches="$scan_root/content.matches"
  local content_errors="$scan_root/content.errors"

  mkdir -p "$scan_root/rootfs"
  sudo -n docker create \
    --name "$candidate_container" \
    --read-only \
    --network none \
    "$candidate_image" >/dev/null
  sudo -n docker export "$candidate_container" --output "$scan_root/rootfs.tar"
  sudo -n tar -tf "$scan_root/rootfs.tar" >"$scan_root/rootfs.list"
  if [[ "$(rg -c '(^|/)paper-raid-bff$' "$scan_root/rootfs.list" || true)" != "1" ]]; then
    echo "runtime rootfs does not contain exactly one BFF binary" >&2
    return 2
  fi
  if rg -n \
    '(^|/)\.git(/|$)|(^|/)(Cargo\.toml|Cargo\.lock)$|(^|/)(src|deploy)/[^/]+\.(rs|js|sql)$|(^|/)\.env($|[./])|paper-raid[^/]*(credential|config|session|login|secret|sentinel)' \
    "$scan_root/rootfs.list"
  then
    echo "runtime image contains forbidden build/config filenames" >&2
    return 1
  fi

  sudo -n tar -xf "$scan_root/rootfs.tar" -C "$scan_root/rootfs"
  if ! cmp -s \
    "$sbom" \
    "$scan_root/rootfs/usr/share/doc/paper-raid-bff/sbom.cdx.json"
  then
    echo "runtime image SBOM differs from the deterministic checked-in document" >&2
    return 2
  fi
  if ! jq -e \
    --arg lock_sha256 "$cargo_lock_sha256" \
    '.bomFormat == "CycloneDX" and .specVersion == "1.5" and .metadata.properties == [{"name":"trnm:cargo-lock:sha256","value":$lock_sha256}] and (.components | length > 20) and (.dependencies | length > 20)' \
    "$scan_root/rootfs/usr/share/doc/paper-raid-bff/sbom.cdx.json" >/dev/null
  then
    echo "runtime image SBOM failed semantic validation" >&2
    return 2
  fi
  local scan_status=0
  sudo -n "$rg_bin" -a -n \
    'PAPER_RAID_BFF_IMAGE_SENTINEL_DO_NOT_SHIP|replace-with-|change-me@|paper-raid-test|alpha-subject-[123]' \
    "$scan_root/rootfs" >"$content_matches" 2>"$content_errors" || scan_status=$?
  case "$scan_status" in
    0)
      sed -n '1,20p' "$content_matches" >&2
      echo "runtime image contains forbidden credential/config content" >&2
      return 1
      ;;
    1) ;;
    *)
      sed -n '1,40p' "$content_errors" >&2
      echo "runtime rootfs content scan failed" >&2
      return 2
      ;;
  esac
  sudo -n docker rm -f "$candidate_container" >/dev/null
}

if [[ -n "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)" ]]; then
  echo "immutable image gate requires a clean committed worktree" >&2
  exit 1
fi
mkdir -p "$(dirname "$buildx_plugin")"
curl \
  --fail \
  --location \
  --proto '=https' \
  --retry 5 \
  --retry-all-errors \
  --retry-delay 2 \
  --connect-timeout 15 \
  --max-time 300 \
  --show-error \
  --silent \
  --tlsv1.2 \
  "$buildx_url" \
  --output "$buildx_plugin"
actual_buildx_sha256=$(sha256sum "$buildx_plugin" | cut -d' ' -f1)
if [[ "$actual_buildx_sha256" != "$buildx_sha256" ]]; then
  echo "disposable buildx checksum mismatch" >&2
  exit 1
fi
chmod 0500 "$buildx_plugin"
docker_cli=(sudo -n env "DOCKER_CONFIG=$docker_config" docker)
if ! "${docker_cli[@]}" buildx version | rg -q --fixed-strings "$buildx_version"; then
  echo "disposable buildx version gate failed" >&2
  exit 1
fi

"$repo_root/services/paper-raid-bff/scripts/check-docker-lock.sh"
"$repo_root/services/paper-raid-bff/scripts/generate-sbom.sh" "$scratch_dir/generated-sbom.cdx.json"
if ! cmp -s "$sbom" "$scratch_dir/generated-sbom.cdx.json"; then
  echo "checked-in CycloneDX SBOM is stale" >&2
  exit 1
fi

build_args=(
  --build-arg "SOURCE_DATE_EPOCH=$source_date_epoch"
  --build-arg "BUILDKIT_MULTI_PLATFORM=1"
  --build-arg "PAPER_RAID_BFF_REVISION=$revision"
  --build-arg "PAPER_RAID_BFF_CREATED=$created"
  --build-arg "PAPER_RAID_BFF_SBOM_SHA256=$sbom_sha256"
  --build-arg "PAPER_RAID_BFF_CARGO_LOCK_SHA256=$cargo_lock_sha256"
)

"${docker_cli[@]}" buildx build \
  --load \
  --no-cache \
  --provenance=false \
  --sbom=false \
  --platform linux/amd64 \
  --file "$repo_root/services/paper-raid-bff/Dockerfile" \
  "${build_args[@]}" \
  --tag "$image_name" \
  "$repo_root"

"${docker_cli[@]}" buildx build \
  --load \
  --no-cache \
  --provenance=false \
  --sbom=false \
  --platform linux/amd64 \
  --file "$repo_root/services/paper-raid-bff/Dockerfile" \
  "${build_args[@]}" \
  --tag "$repro_image" \
  "$repo_root"

image_id=$(sudo -n docker image inspect "$image_name" --format '{{.Id}}')
repro_image_id=$(sudo -n docker image inspect "$repro_image" --format '{{.Id}}')
if [[ "$image_id" != "$repro_image_id" ]]; then
  echo "independent no-cache image rebuild is not byte-deterministic" >&2
  echo "first=$image_id second=$repro_image_id" >&2
  exit 1
fi

architecture=$(sudo -n docker image inspect "$image_name" --format '{{.Architecture}}')
configured_user=$(sudo -n docker image inspect "$image_name" --format '{{.Config.User}}')
entrypoint=$(sudo -n docker image inspect "$image_name" --format '{{json .Config.Entrypoint}}')
healthcheck=$(sudo -n docker image inspect "$image_name" --format '{{json .Config.Healthcheck.Test}}')
runtime_layers=$(sudo -n docker image inspect "$image_name" --format '{{len .RootFS.Layers}}')
label_revision=$(sudo -n docker image inspect "$image_name" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}')
label_created=$(sudo -n docker image inspect "$image_name" --format '{{index .Config.Labels "org.opencontainers.image.created"}}')
label_sbom=$(sudo -n docker image inspect "$image_name" --format '{{index .Config.Labels "org.opencontainers.image.sbom"}}')
label_sbom_sha256=$(sudo -n docker image inspect "$image_name" --format '{{index .Config.Labels "org.trillionnium.sbom.sha256"}}')
label_lock_sha256=$(sudo -n docker image inspect "$image_name" --format '{{index .Config.Labels "org.trillionnium.cargo-lock.sha256"}}')
[[ "$architecture" == "amd64" ]]
[[ "$configured_user" == "65532:65532" ]]
[[ "$entrypoint" == '["/paper-raid-bff"]' ]]
[[ "$healthcheck" == '["CMD","/paper-raid-bff","--probe-ready"]' ]]
[[ "$runtime_layers" -ge 3 ]]
[[ "$label_revision" == "$revision" ]]
[[ "$label_created" == "$created" ]]
[[ "$label_sbom" == "/usr/share/doc/paper-raid-bff/sbom.cdx.json" ]]
[[ "$label_sbom_sha256" == "$sbom_sha256" ]]
[[ "$label_lock_sha256" == "$cargo_lock_sha256" ]]

if sudo -n docker image inspect "$image_name" --format '{{json .Config.Env}}' \
  | rg -n 'PAPER_RAID_BFF_(ALPHA|BIND|CAS|CONSUMER|DATABASE|EDGE|HEPTA|NAKAMA|PUBLIC|SESSION)|replace-with'
then
  echo "runtime image contains deployment configuration" >&2
  exit 1
fi

if sudo -n docker history --no-trunc --format '{{.CreatedBy}}' "$image_name" \
  | rg -n 'PAPER_RAID_BFF_(ALPHA|BIND|CAS|CONSUMER|DATABASE|EDGE|HEPTA|NAKAMA|PUBLIC|SESSION)|replace-with|login_key|secret_access_key'
then
  echo "runtime image history contains deployment configuration" >&2
  exit 1
fi

scan_runtime_image "$image_name" "$container_name" good

jq -nr '"PAPER_RAID_BFF_IMAGE_SENTINEL_DO_NOT_SHIP"' >"$scratch_dir/sentinel.conf"
sudo -n docker create --name "$sentinel_container" "$image_name" >/dev/null
sudo -n docker cp \
  "$scratch_dir/sentinel.conf" \
  "$sentinel_container:/image-proof.txt"
sudo -n docker commit "$sentinel_container" "$sentinel_image" >/dev/null
sudo -n docker rm -f "$sentinel_container" >/dev/null
set +e
scan_runtime_image "$sentinel_image" "$sentinel_scan_container" sentinel
sentinel_status=$?
set -e
if [[ "$sentinel_status" -ne 1 ]]; then
  echo "runtime rootfs gate did not reject the injected sentinel fixture" >&2
  exit 1
fi
sudo -n docker rm -f "$sentinel_scan_container" >/dev/null 2>&1 || true
sudo -n docker image rm -f "$sentinel_image" >/dev/null

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

echo "paper-raid-bff immutable image gate: ok image_id=$image_id revision=$revision sbom_sha256=$sbom_sha256"

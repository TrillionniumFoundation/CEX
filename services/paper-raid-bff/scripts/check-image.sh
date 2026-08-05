#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
image_name=paper-raid-bff:local-gate
repro_image=paper-raid-bff:reproducibility-gate-$$
container_name=paper-raid-bff-image-gate-$$
repro_container_name=paper-raid-bff-reproducibility-image-gate-$$
sentinel_container=paper-raid-bff-sentinel-seed-$$
sentinel_scan_container=paper-raid-bff-sentinel-scan-$$
sentinel_image=paper-raid-bff:sentinel-negative-gate-$$
scratch_dir=$(mktemp -d)
source_context="$scratch_dir/source"
rg_bin=$(command -v rg)
revision=$(git -C "$repo_root" rev-parse HEAD)
source_tree=$(git -C "$repo_root" rev-parse 'HEAD^{tree}')
source_date_epoch=$(git -C "$repo_root" show -s --format=%ct HEAD)
created=$(date -u -d "@$source_date_epoch" '+%Y-%m-%dT%H:%M:%SZ')
buildx_version=v0.36.1
buildx_url=https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64
buildx_sha256=48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778
docker_config="$scratch_dir/docker-config"
buildx_plugin="$docker_config/cli-plugins/docker-buildx"

cleanup() {
  sudo -n docker rm -f "$container_name" >/dev/null 2>&1 || true
  sudo -n docker rm -f "$repro_container_name" >/dev/null 2>&1 || true
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

verify_source_unchanged() {
  local current_revision current_tree current_status

  current_revision=$(git -C "$repo_root" rev-parse HEAD)
  current_tree=$(git -C "$repo_root" rev-parse 'HEAD^{tree}')
  current_status=$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)
  if [[ "$current_revision" != "$revision" ]] || \
     [[ "$current_tree" != "$source_tree" ]] || \
     [[ -n "$current_status" ]]
  then
    echo "source worktree changed during immutable image verification" >&2
    return 1
  fi
}

verify_tag_binding() {
  local tag=$1
  local expected_id=$2
  local actual_id

  actual_id=$(sudo -n docker image inspect "$tag" --format '{{.Id}}')
  if [[ "$actual_id" != "$expected_id" ]]; then
    echo "image tag drifted during immutable image verification: $tag" >&2
    echo "expected=$expected_id actual=$actual_id" >&2
    return 1
  fi
}

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
  sudo -n tar -tf "$scan_root/rootfs.tar" \
    | tee "$scan_root/rootfs.list" >/dev/null
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
    --arg binary_sha256 "$runtime_binary_authority_sha256" \
    '.bomFormat == "CycloneDX"
     and .specVersion == "1.5"
     and .metadata.properties == [{"name":"trnm:cargo-lock:sha256","value":$lock_sha256}]
     and (.components | length > 20)
     and (.dependencies | length > 20)
     and ([.components[]
       | select(.type == "file"
           and .["bom-ref"] == "file:/paper-raid-bff"
           and .name == "/paper-raid-bff"
           and .hashes == [{"alg":"SHA-256","content":$binary_sha256}])]
       | length == 1)' \
    "$scan_root/rootfs/usr/share/doc/paper-raid-bff/sbom.cdx.json" >/dev/null
  then
    echo "runtime image SBOM failed semantic validation" >&2
    return 2
  fi
  local runtime_binary_sha256
  runtime_binary_sha256=$(sha256sum "$scan_root/rootfs/paper-raid-bff" | cut -d' ' -f1)
  if [[ "$runtime_binary_sha256" != "$runtime_binary_authority_sha256" ]]; then
    echo "runtime image binary differs from the SBOM-bound pinned-builder authority" >&2
    echo "expected=$runtime_binary_authority_sha256 actual=$runtime_binary_sha256" >&2
    return 2
  fi
  local scan_status
  if sudo -n "$rg_bin" -a -n \
      'PAPER_RAID_BFF_IMAGE_SENTINEL_DO_NOT_SHIP|replace-with-|change-me@|paper-raid-test|alpha-subject-[123]' \
      "$scan_root/rootfs" 2>"$content_errors" \
      | tee "$content_matches" >/dev/null
  then
    scan_status=0
  else
    scan_status=${PIPESTATUS[0]}
  fi
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
mkdir -p "$source_context"
git -C "$repo_root" archive "$revision" | tar -xf - -C "$source_context"
verify_source_unchanged

cargo_lock="$source_context/services/paper-raid-bff/docker/Cargo.lock"
sbom="$source_context/services/paper-raid-bff/docker/sbom.cdx.json"
cargo_lock_sha256=$(sha256sum "$cargo_lock" | cut -d' ' -f1)
sbom_sha256=$(sha256sum "$sbom" | cut -d' ' -f1)
runtime_binary_authority_sha256=$(jq -er '
  [.components[]
    | select(.type == "file"
        and .["bom-ref"] == "file:/paper-raid-bff"
        and .name == "/paper-raid-bff"
        and .hashes == [{"alg":"SHA-256","content":.hashes[0].content}])
    | .hashes[0].content]
  | select(length == 1)
  | .[0]
  | select(test("^[0-9a-f]{64}$"))
' "$sbom")

mkdir -p "$(dirname "$buildx_plugin")"
bash "$source_context/services/paper-raid-bff/scripts/download-pinned-buildx.sh" \
  "$buildx_url" \
  "$buildx_sha256" \
  "$buildx_plugin" \
  "${PAPER_RAID_BUILDX_BIN:-}"
chmod 0500 "$buildx_plugin"
docker_cli=(sudo -n env "DOCKER_CONFIG=$docker_config" docker)
bounded_buildx=(
  sudo -n timeout --signal=TERM --kill-after=30s 600s
  env "DOCKER_CONFIG=$docker_config" docker buildx build
)
if ! "${docker_cli[@]}" buildx version | rg -q --fixed-strings "$buildx_version"; then
  echo "disposable buildx version gate failed" >&2
  exit 1
fi

(cd "$source_context" && \
  services/paper-raid-bff/scripts/check-docker-lock.sh)
(cd "$source_context" && \
  flock -n /tmp/trnm-paper-raid-cargo-gate.lock \
    cargo build --locked --release -p paper-raid-bff)
(cd "$source_context" && \
  services/paper-raid-bff/scripts/generate-sbom.sh \
  "$scratch_dir/generated-sbom.cdx.json" \
  --runtime-sha256 \
  "$runtime_binary_authority_sha256")
if ! cmp -s "$sbom" "$scratch_dir/generated-sbom.cdx.json"; then
  echo "checked-in CycloneDX SBOM is stale" >&2
  exit 1
fi
verify_source_unchanged

build_args=(
  # SOURCE_DATE_EPOCH is a BuildKit predefined build argument. The Dockerfile
  # deliberately does not declare it, and Cargo explicitly unsets it, so it
  # normalizes OCI timestamps without becoming a compiler input.
  --build-arg "SOURCE_DATE_EPOCH=$source_date_epoch"
  --build-arg "PAPER_RAID_BFF_RELEASE_EPOCH=$source_date_epoch"
  --build-arg "BUILDKIT_MULTI_PLATFORM=1"
  --build-arg "PAPER_RAID_BFF_REVISION=$revision"
  --build-arg "PAPER_RAID_BFF_SOURCE_TREE=$source_tree"
  --build-arg "PAPER_RAID_BFF_CREATED=$created"
  --build-arg "PAPER_RAID_BFF_SBOM_SHA256=$sbom_sha256"
  --build-arg "PAPER_RAID_BFF_CARGO_LOCK_SHA256=$cargo_lock_sha256"
  --build-arg "PAPER_RAID_BFF_RUNTIME_BINARY_SHA256=$runtime_binary_authority_sha256"
)

"${bounded_buildx[@]}" \
  --load \
  --no-cache \
  --provenance=false \
  --sbom=false \
  --platform linux/amd64 \
  --metadata-file "$scratch_dir/first-build.metadata.json" \
  --iidfile "$scratch_dir/first-build.iid" \
  --file "$source_context/services/paper-raid-bff/Dockerfile" \
  "${build_args[@]}" \
  --tag "$image_name" \
  "$source_context"
verify_source_unchanged

"${bounded_buildx[@]}" \
  --load \
  --no-cache \
  --provenance=false \
  --sbom=false \
  --platform linux/amd64 \
  --metadata-file "$scratch_dir/second-build.metadata.json" \
  --iidfile "$scratch_dir/second-build.iid" \
  --file "$source_context/services/paper-raid-bff/Dockerfile" \
  "${build_args[@]}" \
  --tag "$repro_image" \
  "$source_context"
verify_source_unchanged

image_id=$(sudo -n docker image inspect "$image_name" --format '{{.Id}}')
repro_image_id=$(sudo -n docker image inspect "$repro_image" --format '{{.Id}}')
verify_tag_binding "$image_name" "$image_id"
verify_tag_binding "$repro_image" "$repro_image_id"
if [[ "$image_id" != "$repro_image_id" ]]; then
  echo "independent no-cache image rebuild is not byte-deterministic" >&2
  echo "first=$image_id second=$repro_image_id" >&2
  exit 1
fi

digest_from_metadata() {
  local metadata=$1
  local key=$2
  jq -er \
    --arg key "$key" \
    '.[$key] | select(type == "string" and test("^sha256:[0-9a-f]{64}$"))' \
    "$metadata"
}

optional_digest_from_metadata() {
  local metadata=$1
  local key=$2

  if jq -e --arg key "$key" 'has($key)' "$metadata" >/dev/null; then
    digest_from_metadata "$metadata" "$key"
  fi
}

index_digest=$(digest_from_metadata "$scratch_dir/first-build.metadata.json" "containerimage.digest")
repro_index_digest=$(digest_from_metadata "$scratch_dir/second-build.metadata.json" "containerimage.digest")
metadata_config_digest=$(optional_digest_from_metadata \
  "$scratch_dir/first-build.metadata.json" \
  "containerimage.config.digest")
repro_metadata_config_digest=$(optional_digest_from_metadata \
  "$scratch_dir/second-build.metadata.json" \
  "containerimage.config.digest")
config_digest=$image_id
repro_config_digest=$repro_image_id
iid=$(tr -d '\n' <"$scratch_dir/first-build.iid")
repro_iid=$(tr -d '\n' <"$scratch_dir/second-build.iid")
if [[ ! "$iid" =~ ^sha256:[0-9a-f]{64}$ ]] || \
   [[ ! "$repro_iid" =~ ^sha256:[0-9a-f]{64}$ ]]
then
  echo "Buildx IID file is not a canonical sha256 digest" >&2
  exit 1
fi
if [[ "$metadata_config_digest" != "$repro_metadata_config_digest" ]]; then
  echo "independent no-cache Buildx config metadata is not deterministic" >&2
  exit 1
fi
if [[ -n "$metadata_config_digest" ]] && \
   [[ "$metadata_config_digest" != "$config_digest" ]]
then
  echo "Buildx config metadata differs from the loaded OCI config digest" >&2
  exit 1
fi
if [[ "$iid" != "$config_digest" && "$iid" != "$index_digest" ]]; then
  echo "Buildx IID is neither the verified OCI config nor index digest" >&2
  exit 1
fi
if [[ "$index_digest" != "$repro_index_digest" ]] || \
   [[ "$config_digest" != "$repro_config_digest" ]] || \
   [[ "$iid" != "$repro_iid" ]]
then
  echo "independent no-cache OCI metadata is not deterministic" >&2
  exit 1
fi

architecture=$(sudo -n docker image inspect "$image_id" --format '{{.Architecture}}')
configured_user=$(sudo -n docker image inspect "$image_id" --format '{{.Config.User}}')
entrypoint=$(sudo -n docker image inspect "$image_id" --format '{{json .Config.Entrypoint}}')
healthcheck=$(sudo -n docker image inspect "$image_id" --format '{{json .Config.Healthcheck.Test}}')
runtime_layers=$(sudo -n docker image inspect "$image_id" --format '{{len .RootFS.Layers}}')
label_revision=$(sudo -n docker image inspect "$image_id" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}')
label_source_tree=$(sudo -n docker image inspect "$image_id" --format '{{index .Config.Labels "org.trillionnium.source.tree"}}')
label_created=$(sudo -n docker image inspect "$image_id" --format '{{index .Config.Labels "org.opencontainers.image.created"}}')
label_sbom=$(sudo -n docker image inspect "$image_id" --format '{{index .Config.Labels "org.opencontainers.image.sbom"}}')
label_sbom_sha256=$(sudo -n docker image inspect "$image_id" --format '{{index .Config.Labels "org.trillionnium.sbom.sha256"}}')
label_lock_sha256=$(sudo -n docker image inspect "$image_id" --format '{{index .Config.Labels "org.trillionnium.cargo-lock.sha256"}}')
label_runtime_binary_sha256=$(sudo -n docker image inspect "$image_id" --format '{{index .Config.Labels "org.trillionnium.runtime-binary.sha256"}}')
[[ "$architecture" == "amd64" ]]
[[ "$configured_user" == "65532:65532" ]]
[[ "$entrypoint" == '["/paper-raid-bff"]' ]]
[[ "$healthcheck" == '["CMD","/paper-raid-bff","--probe-ready"]' ]]
[[ "$runtime_layers" -ge 3 ]]
[[ "$label_revision" == "$revision" ]]
[[ "$label_source_tree" == "$source_tree" ]]
[[ "$label_created" == "$created" ]]
[[ "$label_sbom" == "/usr/share/doc/paper-raid-bff/sbom.cdx.json" ]]
[[ "$label_sbom_sha256" == "$sbom_sha256" ]]
[[ "$label_lock_sha256" == "$cargo_lock_sha256" ]]
[[ "$label_runtime_binary_sha256" == "$runtime_binary_authority_sha256" ]]

if sudo -n docker image inspect "$image_id" --format '{{json .Config.Env}}' \
  | rg -n 'PAPER_RAID_BFF_(ALPHA|BIND|CAS|CONSUMER|DATABASE|EDGE|HEPTA|NAKAMA|PUBLIC|SESSION)|replace-with'
then
  echo "runtime image contains deployment configuration" >&2
  exit 1
fi

if sudo -n docker history --no-trunc --format '{{.CreatedBy}}' "$image_id" \
  | rg -n 'PAPER_RAID_BFF_(ALPHA|BIND|CAS|CONSUMER|DATABASE|EDGE|HEPTA|NAKAMA|PUBLIC|SESSION)|replace-with|login_key|secret_access_key'
then
  echo "runtime image history contains deployment configuration" >&2
  exit 1
fi

scan_runtime_image "$image_id" "$container_name" good
binary_sha256=$(sha256sum "$scratch_dir/good/rootfs/paper-raid-bff" | cut -d' ' -f1)
scan_runtime_image "$repro_image_id" "$repro_container_name" reproducibility
repro_binary_sha256=$(sha256sum \
  "$scratch_dir/reproducibility/rootfs/paper-raid-bff" \
  | cut -d' ' -f1)
[[ "$binary_sha256" == "$runtime_binary_authority_sha256" ]]
[[ "$repro_binary_sha256" == "$runtime_binary_authority_sha256" ]]
[[ "$binary_sha256" == "$repro_binary_sha256" ]]

jq -nr '"PAPER_RAID_BFF_IMAGE_SENTINEL_DO_NOT_SHIP"' >"$scratch_dir/sentinel.conf"
sudo -n docker create --name "$sentinel_container" "$image_id" >/dev/null
sudo -n docker cp \
  "$scratch_dir/sentinel.conf" \
  "$sentinel_container:/image-proof.txt"
sudo -n docker commit "$sentinel_container" "$sentinel_image" >/dev/null
sudo -n docker rm -f "$sentinel_container" >/dev/null
if scan_runtime_image "$sentinel_image" "$sentinel_scan_container" sentinel; then
  sentinel_status=0
else
  sentinel_status=$?
fi
if [[ "$sentinel_status" -ne 1 ]]; then
  echo "runtime rootfs gate did not reject the injected sentinel fixture" >&2
  exit 1
fi
sudo -n docker rm -f "$sentinel_scan_container" >/dev/null 2>&1 || true
sudo -n docker image rm -f "$sentinel_image" >/dev/null

if sudo -n docker run --rm --read-only --network none --entrypoint /bin/sh "$image_id"; then
  echo "runtime image unexpectedly contains a shell" >&2
  exit 1
fi

if runtime_output=$(sudo -n docker run --rm --read-only --network none \
  "$image_id" --probe-ready 2>&1)
then
  runtime_status=0
else
  runtime_status=$?
fi
[[ "$runtime_status" -eq 1 ]]
if printf '%s' "$runtime_output" | rg -n 'replace-with|login_key|secret_access_key'; then
  echo "runtime startup leaked a sensitive configuration value" >&2
  exit 1
fi

verify_tag_binding "$image_name" "$image_id"
verify_tag_binding "$repro_image" "$repro_image_id"
verify_source_unchanged

echo "paper-raid-bff immutable image gate: ok image_id=$image_id oci_index_digest=$index_digest iid=$iid config_digest=$config_digest revision=$revision tree=$source_tree binary_sha256=$binary_sha256 sbom_sha256=$sbom_sha256"

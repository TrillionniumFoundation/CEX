#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

if [[ -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
  echo "Hepta image builds require a clean checkout" >&2
  exit 1
fi

revision="$(git rev-parse HEAD)"
source_tree="$(git rev-parse 'HEAD^{tree}')"
source_date_epoch="$(git show -s --format=%ct HEAD)"
image_ref="${HEPTA_IMAGE_REF:-trnm/hepta-research-league:${revision}}"
docker_command=(docker)
if ! docker info >/dev/null 2>&1; then
  docker_command=(sudo -n docker)
fi

release_dir="$(mktemp -d)"
buildx_version=v0.36.1
buildx_url=https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64
buildx_sha256=48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778
docker_config="$release_dir/docker"
buildx_plugin="$docker_config/cli-plugins/docker-buildx"
iid_file="$release_dir/image-first.iid"
repro_iid_file="$release_dir/image-second.iid"
context_dir="$release_dir/context"
mkdir -p "$context_dir"
sbom_container=""
repro_ref="trnm/hepta-research-league:repro-${revision:0:12}-$$"
cleanup() {
  if [[ -n "$sbom_container" ]]; then
    "${docker_command[@]}" rm -f "$sbom_container" >/dev/null 2>&1 || true
  fi
  "${docker_command[@]}" image rm "$repro_ref" >/dev/null 2>&1 || true
  case "$release_dir" in
    /tmp/tmp.*)
      if [[ "${docker_command[0]}" == "sudo" ]]; then
        sudo -n rm -rf -- "$release_dir"
      else
        rm -rf -- "$release_dir"
      fi
      ;;
    *)
      echo "refusing to remove unexpected Hepta image-gate scratch path" >&2
      return 1
      ;;
  esac
}
trap cleanup EXIT

for command_name in curl docker jq rg sha256sum; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'Hepta image gate requires %s\n' "$command_name" >&2
    exit 1
  }
done
if [[ "$(uname -m)" != "x86_64" ]]; then
  echo "Hepta image gate requires an x86_64 host for the pinned Buildx binary" >&2
  exit 1
fi
[[ "$revision" =~ ^[0-9a-f]{40}$ \
  && "$source_tree" =~ ^[0-9a-f]{40}$ \
  && "$source_date_epoch" =~ ^[0-9]+$ ]] || {
  echo "Hepta image source metadata is not canonical" >&2
  exit 1
}

git archive --format=tar HEAD | tar -xf - -C "$context_dir"
sbom_sha256="$(sha256sum deploy/hepta-research-league/hepta-research-league.cdx.json | cut -d' ' -f1)"
[[ "$sbom_sha256" =~ ^[0-9a-f]{64}$ ]] || {
  echo "Hepta image SBOM digest is not canonical" >&2
  exit 1
}

mkdir -p "$(dirname "$buildx_plugin")"
curl --fail --location --proto '=https' --retry 5 --retry-all-errors \
  --retry-delay 2 --connect-timeout 15 --max-time 1200 \
  --show-error --silent --tlsv1.2 \
  "$buildx_url" --output "$buildx_plugin"
actual_buildx_sha256="$(sha256sum "$buildx_plugin" | cut -d' ' -f1)"
if [[ "$actual_buildx_sha256" != "$buildx_sha256" ]]; then
  echo "disposable Hepta Buildx checksum differs" >&2
  exit 1
fi
chmod 0500 "$buildx_plugin"
if [[ "${docker_command[0]}" == "sudo" ]]; then
  docker_build_command=(sudo -n env "DOCKER_CONFIG=$docker_config" docker)
else
  docker_build_command=(env "DOCKER_CONFIG=$docker_config" docker)
fi
"${docker_build_command[@]}" buildx version | rg -q --fixed-strings "$buildx_version"

build_image() {
  local target_ref="$1"
  local target_iid_file="$2"
  "${docker_build_command[@]}" buildx build \
    --load \
    --progress=plain \
    --pull=false \
    --no-cache \
    --provenance=false \
    --sbom=false \
    --platform linux/amd64 \
    --build-arg BUILDKIT_MULTI_PLATFORM=1 \
    --file services/hepta-research-league/Dockerfile \
    --build-arg "VCS_REF=${revision}" \
    --build-arg "SOURCE_TREE=${source_tree}" \
    --build-arg "SOURCE_DATE_EPOCH=${source_date_epoch}" \
    --build-arg "SBOM_SHA256=${sbom_sha256}" \
    --iidfile "$target_iid_file" \
    --tag "$target_ref" \
    "$context_dir"
}

build_image "$image_ref" "$iid_file"
build_image "$repro_ref" "$repro_iid_file"

image_id="$(<"$iid_file")"
repro_image_id="$(<"$repro_iid_file")"
if [[ "$image_id" != "$repro_image_id" ]]; then
  echo "independent no-cache Hepta image builds are not reproducible" >&2
  "${docker_command[@]}" image rm -f "$image_ref" "$repro_ref" >/dev/null 2>&1 || true
  exit 1
fi
"${docker_command[@]}" image rm "$repro_ref" >/dev/null
inspect_json="$("${docker_command[@]}" image inspect "$image_id")"
jq -e \
  --arg revision "$revision" \
  --arg source_tree "$source_tree" \
  --arg source_date_epoch "$source_date_epoch" \
  --arg sbom_sha256 "$sbom_sha256" '
  length == 1
  and .[0].Config.User == "65532:65532"
  and .[0].Config.Entrypoint == ["/usr/local/bin/hepta-research-league"]
  and .[0].Config.Healthcheck.Test == ["CMD", "/usr/local/bin/hepta-research-league", "--probe-ready"]
  and .[0].Config.Labels["org.opencontainers.image.revision"] == $revision
  and .[0].Config.Labels["org.opencontainers.image.source"] == "https://github.com/TrillionniumFoundation/CEX.git"
  and .[0].Config.Labels["org.trillionnium.source.tree"] == $source_tree
  and .[0].Config.Labels["org.trillionnium.sbom.sha256"] == $sbom_sha256
  and .[0].Config.Labels["io.trillionnium.hepta.source-tree"] == $source_tree
  and .[0].Config.Labels["io.trillionnium.hepta.source-date-epoch"] == $source_date_epoch
  and .[0].Config.Labels["io.trillionnium.hepta.application-sbom.sha256"] == $sbom_sha256
  and .[0].Config.Labels["io.trillionnium.hepta.runtime-base"] == "gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98"
' <<<"$inspect_json" >/dev/null

sbom_container="$("${docker_command[@]}" create "$image_id")"
"${docker_command[@]}" cp \
  "$sbom_container:/usr/share/doc/hepta-research-league/sbom.cdx.json" \
  "$release_dir/image-sbom.cdx.json"
"${docker_command[@]}" rm "$sbom_container" >/dev/null
sbom_container=""
test "$(sha256sum "$release_dir/image-sbom.cdx.json" | cut -d' ' -f1)" = "$sbom_sha256"

jq -n \
  --arg schema "hepta.release_image_provenance.v2" \
  --arg image_ref "$image_ref" \
  --arg image_id "$image_id" \
  --arg source_repository "https://github.com/TrillionniumFoundation/CEX.git" \
  --arg source_revision "$revision" \
  --arg source_tree "$source_tree" \
  --arg source_date_epoch "$source_date_epoch" \
  --arg dockerfile_frontend "docker/dockerfile:1@sha256:87999aa3d42bdc6bea60565083ee17e86d1f3339802f543c0d03998580f9cb89" \
  --arg buildx_version "$buildx_version" \
  --arg buildx_sha256 "$buildx_sha256" \
  --arg builder_base "rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb" \
  --arg runtime_base "gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98" \
  --arg vendor_manifest_sha256 "$(sha256sum vendor/trnm-chain-vendor-manifest.json | cut -d' ' -f1)" \
  --arg sbom_sha256 "$sbom_sha256" \
  '{
    schema: $schema,
    image_ref: $image_ref,
    image_id: $image_id,
    source_repository: $source_repository,
    source_revision: $source_revision,
    source_tree: $source_tree,
    source_date_epoch: ($source_date_epoch | tonumber),
    dockerfile_frontend: $dockerfile_frontend,
    buildx: {
      version: $buildx_version,
      binary_sha256: $buildx_sha256
    },
    builder_base: $builder_base,
    runtime_base: $runtime_base,
    vendor_manifest_sha256: $vendor_manifest_sha256,
    application_sbom: {
      format: "CycloneDX-1.5",
      path: "/usr/share/doc/hepta-research-league/sbom.cdx.json",
      sha256: $sbom_sha256
    },
    reproducibility: {
      independent_no_cache_builds: 2,
      identical_image_ids: true
    },
    healthcheck: ["/usr/local/bin/hepta-research-league", "--probe-ready"]
  }'

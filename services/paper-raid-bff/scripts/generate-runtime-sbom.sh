#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
output=${1:-}
scratch_dir=$(mktemp -d)
staged_output=
current_uid=$(id -u)
current_gid=$(id -g)
revision=$(git -C "$repo_root" rev-parse HEAD)
source_tree=$(git -C "$repo_root" rev-parse 'HEAD^{tree}')
tracked_sbom="$repo_root/services/paper-raid-bff/docker/sbom.cdx.json"
buildx_version=v0.36.1
buildx_url=https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64
buildx_sha256=48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778
docker_config="$scratch_dir/docker-config"
buildx_plugin="$docker_config/cli-plugins/docker-buildx"
source_context="$scratch_dir/source"

if [[ ! -d "$scratch_dir" ]] || \
   [[ -L "$scratch_dir" ]] || \
   [[ "$(stat -c '%u:%g' "$scratch_dir")" != "$current_uid:$current_gid" ]]
then
  echo "runtime generator scratch directory is not an owned regular directory" >&2
  exit 1
fi

cleanup() {
  if [[ -n "$staged_output" ]]; then
    case "$staged_output" in
      "$repo_root/services/paper-raid-bff/docker/".sbom.cdx.json.*)
        rm -f -- "$staged_output"
        ;;
      *)
        echo "refusing to remove unexpected staged SBOM path: $staged_output" >&2
        ;;
    esac
  fi
  case "$scratch_dir" in
    /tmp/tmp.*) sudo -n rm -rf -- "$scratch_dir" ;;
    *) echo "refusing to remove unexpected runtime generator scratch path: $scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT INT TERM

verify_source_unchanged() {
  if [[ "$(git -C "$repo_root" rev-parse HEAD)" != "$revision" ]] || \
     [[ "$(git -C "$repo_root" rev-parse 'HEAD^{tree}')" != "$source_tree" ]] || \
     [[ -n "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)" ]]
  then
    echo "pinned runtime source changed during generation" >&2
    exit 1
  fi
}

if [[ "$#" -ne 1 || -z "$output" ]]; then
  echo "usage: $0 services/paper-raid-bff/docker/sbom.cdx.json" >&2
  exit 2
fi
if [[ -L "$output" ]]; then
  echo "runtime SBOM output must not be a symlink" >&2
  exit 2
fi
resolved_output=$(realpath -m -- "$output")
resolved_tracked_sbom=$(realpath -m -- "$tracked_sbom")
if [[ "$resolved_output" != "$resolved_tracked_sbom" ]]; then
  echo "runtime SBOM output must be the exact tracked BFF SBOM path" >&2
  exit 2
fi
output=$resolved_tracked_sbom
verify_source_unchanged

mkdir -p "$source_context"
git -C "$repo_root" archive "$revision" | tar -x -C "$source_context"
verify_source_unchanged
bash "$source_context/services/paper-raid-bff/scripts/check-docker-lock.sh"

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

build_runtime_binary() {
  local destination=$1
  local platform_directory="$destination/linux_amd64"
  local runtime_binary="$platform_directory/paper-raid-bff"
  local exported_entries

  case "$destination" in
    "$scratch_dir/first"|"$scratch_dir/second") ;;
    *)
      echo "runtime binary destination escaped the exact generator scratch slots" >&2
      exit 1
      ;;
  esac
  if [[ -e "$destination" ]] || [[ -L "$destination" ]]; then
    echo "runtime binary destination must not exist before export" >&2
    exit 1
  fi
  verify_source_unchanged
  "${bounded_buildx[@]}" \
    --no-cache \
    --provenance=false \
    --sbom=false \
    --platform linux/amd64 \
    --target runtime-binary-export \
    --output "type=local,dest=$destination" \
    --build-arg "BUILDKIT_MULTI_PLATFORM=1" \
    --file "$source_context/services/paper-raid-bff/Dockerfile" \
    "$source_context"
  verify_source_unchanged
  if [[ ! -d "$destination" ]] || [[ -L "$destination" ]]; then
    echo "pinned builder did not create the exact regular export directory" >&2
    exit 1
  fi
  sudo -n chown -hR -- "$current_uid:$current_gid" "$destination"
  if find "$destination" -xdev \
    \( ! -uid "$current_uid" -o ! -gid "$current_gid" \) \
    -print -quit | rg -q .
  then
    echo "pinned builder export ownership normalization failed" >&2
    exit 1
  fi
  exported_entries=$(find "$destination" -mindepth 1 -printf '%P\n' | LC_ALL=C sort)
  if [[ ! -d "$platform_directory" ]] || \
     [[ -L "$platform_directory" ]] || \
     [[ ! -f "$runtime_binary" ]] || \
     [[ -L "$runtime_binary" ]] || \
     [[ ! -x "$runtime_binary" ]] || \
     [[ "$exported_entries" != $'linux_amd64\nlinux_amd64/paper-raid-bff' ]]
  then
    echo "pinned builder did not export exactly one regular executable linux/amd64 runtime binary" >&2
    exit 1
  fi
}

build_runtime_binary "$scratch_dir/first"
build_runtime_binary "$scratch_dir/second"
first_binary="$scratch_dir/first/linux_amd64/paper-raid-bff"
second_binary="$scratch_dir/second/linux_amd64/paper-raid-bff"
first_sha256=$(sha256sum "$first_binary" | cut -d' ' -f1)
second_sha256=$(sha256sum "$second_binary" | cut -d' ' -f1)
if [[ "$first_sha256" != "$second_sha256" ]] || \
   ! cmp -s "$first_binary" "$second_binary"
then
  echo "independent pinned-builder runtime binaries are not byte-deterministic" >&2
  echo "first=$first_sha256 second=$second_sha256" >&2
  exit 1
fi

tracked_sbom_sha256=$(sha256sum \
  "$source_context/services/paper-raid-bff/docker/sbom.cdx.json" \
  | cut -d' ' -f1)
for forbidden in \
  "$revision" \
  "$source_tree" \
  "$tracked_sbom_sha256" \
  "$first_sha256"
do
  if rg -a -q --fixed-strings "$forbidden" "$first_binary"; then
    echo "runtime binary embeds revision/tree/SBOM/self-hash material" >&2
    exit 1
  fi
done

bash "$source_context/services/paper-raid-bff/scripts/generate-sbom.sh" \
  "$scratch_dir/first-sbom.cdx.json" \
  --runtime-binary \
  "$first_binary"
bash "$source_context/services/paper-raid-bff/scripts/generate-sbom.sh" \
  "$scratch_dir/second-sbom.cdx.json" \
  --runtime-binary \
  "$second_binary"
if ! cmp -s \
  "$scratch_dir/first-sbom.cdx.json" \
  "$scratch_dir/second-sbom.cdx.json"
then
  echo "pinned-builder CycloneDX generation is not byte-deterministic" >&2
  exit 1
fi

verify_source_unchanged
staged_output=$(mktemp "$repo_root/services/paper-raid-bff/docker/.sbom.cdx.json.XXXXXX")
install -m 0644 "$scratch_dir/first-sbom.cdx.json" "$staged_output"
mv -f -- "$staged_output" "$output"
staged_output=
generated_sbom_sha256=$(sha256sum "$output" | cut -d' ' -f1)
echo "paper-raid-bff pinned runtime SBOM generation: ok binary_sha256=$first_sha256 sbom_sha256=$generated_sbom_sha256"

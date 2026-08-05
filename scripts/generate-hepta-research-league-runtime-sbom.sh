#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
mode=${1:-}
tracked_sbom="$repo_dir/deploy/hepta-research-league/hepta-research-league.cdx.json"
scratch=$(mktemp -d)
staged_output=
revision=$(git -C "$repo_dir" rev-parse HEAD)
source_tree=$(git -C "$repo_dir" rev-parse 'HEAD^{tree}')
buildx_version=v0.36.1
buildx_url=https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64
buildx_sha256=48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778
docker_config="$scratch/docker"
buildx_plugin="$docker_config/cli-plugins/docker-buildx"
source_context="$scratch/source"

cleanup() {
  if [[ -n "$staged_output" ]]; then
    case "$staged_output" in
      "$repo_dir/deploy/hepta-research-league/".hepta-research-league.cdx.json.*)
        rm -f -- "$staged_output"
        ;;
      *) echo "refusing to remove unexpected staged SBOM path" >&2 ;;
    esac
  fi
  case "$scratch" in
    /tmp/tmp.*)
      if [[ ${docker_command[0]:-} == sudo ]]; then
        sudo -n rm -rf -- "$scratch"
      else
        rm -rf -- "$scratch"
      fi
      ;;
    *) echo "refusing to remove unexpected runtime-SBOM scratch path" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

verify_source_unchanged() {
  if [[ "$(git -C "$repo_dir" rev-parse HEAD)" != "$revision" ]] || \
     [[ "$(git -C "$repo_dir" rev-parse 'HEAD^{tree}')" != "$source_tree" ]] || \
     [[ -n "$(git -C "$repo_dir" status --porcelain=v1 --untracked-files=all)" ]]; then
    echo "Hepta pinned runtime source changed during SBOM generation" >&2
    exit 1
  fi
}

case "$mode" in
  --write|--check) ;;
  *) echo "usage: $0 (--write|--check)" >&2; exit 2 ;;
esac
[[ "$revision" =~ ^[0-9a-f]{40}$ && "$source_tree" =~ ^[0-9a-f]{40}$ ]] || {
  echo "Hepta source identity is not canonical" >&2
  exit 1
}
verify_source_unchanged

for command_name in cmp curl cut docker find flock git install mktemp mv python3 rg \
  sha256sum sort tar timeout uname; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Hepta runtime SBOM generator requires $command_name" >&2
    exit 1
  }
done
release_lock=$(git -C "$repo_dir" rev-parse --path-format=absolute \
  --git-path hepta-release-authority.lock)
exec 9>"$release_lock"
flock -n 9 || {
  echo "another Hepta release-authority process is already running" >&2
  exit 1
}
if [[ "$(uname -m)" != x86_64 ]]; then
  echo "Hepta runtime SBOM generator requires x86_64" >&2
  exit 1
fi
docker_command=(docker)
if ! docker info >/dev/null 2>&1; then
  docker_command=(sudo -n docker)
fi
"${docker_command[@]}" info >/dev/null

mkdir -p "$source_context"
git -C "$repo_dir" archive "$revision" | tar -xf - -C "$source_context"
unexpected_archive_entry=$(find "$source_context" -mindepth 1 \
  \( -type l -o \! -type f -a \! -type d \) -print -quit)
[[ -z "$unexpected_archive_entry" ]] || {
  echo "Hepta immutable source archive contains a symlink or special file: $unexpected_archive_entry" >&2
  exit 1
}
verify_source_unchanged

bash "$source_context/scripts/download-pinned-buildx.sh" \
  "$buildx_url" "$buildx_sha256" "$buildx_plugin" \
  "${PAPER_RAID_BUILDX_BIN:-}"
if [[ ${docker_command[0]} == sudo ]]; then
  docker_build=(sudo -n env "DOCKER_CONFIG=$docker_config" docker)
  bounded_buildx=(
    sudo -n timeout --signal=TERM --kill-after=30s 1800s
    env "DOCKER_CONFIG=$docker_config" docker buildx build
  )
else
  docker_build=(env "DOCKER_CONFIG=$docker_config" docker)
  bounded_buildx=(
    timeout --signal=TERM --kill-after=30s 1800s
    env "DOCKER_CONFIG=$docker_config" docker buildx build
  )
fi
"${docker_build[@]}" buildx version | rg -q --fixed-strings "$buildx_version"

build_export() {
  local target=$1
  local destination=$2
  verify_source_unchanged
  "${bounded_buildx[@]}" \
    --no-cache --pull=false --provenance=false --sbom=false --platform linux/amd64 \
    --target "$target" --output "type=local,dest=$destination" \
    --build-arg BUILDKIT_MULTI_PLATFORM=1 \
    --file "$source_context/services/hepta-research-league/Dockerfile" \
    "$source_context"
  verify_source_unchanged
}

build_export runtime-binary-export "$scratch/first"
build_export runtime-binary-export "$scratch/second"
first_binary="$scratch/first/linux_amd64/hepta-research-league"
second_binary="$scratch/second/linux_amd64/hepta-research-league"
for directory in "$scratch/first" "$scratch/second"; do
  exported=$(find "$directory" -mindepth 1 -printf '%P\n' | LC_ALL=C sort)
  if [[ "$exported" != $'linux_amd64\nlinux_amd64/hepta-research-league' ]]; then
    echo "runtime-binary-export did not contain exactly one binary" >&2
    exit 1
  fi
done
for binary in "$first_binary" "$second_binary"; do
  if [[ -L "$binary" || ! -f "$binary" || ! -x "$binary" ]]; then
    echo "pinned builder exported a non-regular or non-executable binary" >&2
    exit 1
  fi
done
first_sha256=$(sha256sum "$first_binary" | cut -d' ' -f1)
second_sha256=$(sha256sum "$second_binary" | cut -d' ' -f1)
if [[ "$first_sha256" != "$second_sha256" ]] || ! cmp -s "$first_binary" "$second_binary"; then
  echo "independent pinned-builder Hepta binaries are not byte deterministic" >&2
  exit 1
fi

tracked_sbom_sha256=$(sha256sum \
  "$source_context/deploy/hepta-research-league/hepta-research-league.cdx.json" \
  | cut -d' ' -f1)
for forbidden in "$revision" "$source_tree" "$tracked_sbom_sha256" "$first_sha256"; do
  scan_status=0
  rg --hidden --no-ignore -a -q --fixed-strings "$forbidden" "$first_binary" || \
    scan_status=$?
  case "$scan_status" in
    0)
      echo "Hepta runtime binary embeds revision/tree/SBOM/self-hash material" >&2
      exit 1
      ;;
    1) ;;
    *)
      echo "Hepta runtime binary identity scan failed" >&2
      exit 1
      ;;
  esac
done

build_export sbom-metadata-export "$scratch/metadata"
metadata="$scratch/metadata/linux_amd64/cargo-metadata.json"
metadata_entries=$(find "$scratch/metadata" -mindepth 1 -printf '%P\n' | LC_ALL=C sort)
if [[ "$metadata_entries" != $'linux_amd64\nlinux_amd64/cargo-metadata.json' ]] || \
   [[ -L "$metadata" || ! -f "$metadata" ]]; then
  echo "sbom-metadata-export did not contain exactly one metadata document" >&2
  exit 1
fi

generator="$source_context/scripts/generate-hepta-research-league-sbom.py"
common=(
  --metadata "$metadata"
  --dockerfile "$source_context/services/hepta-research-league/Dockerfile"
  --cargo-lock "$source_context/Cargo.lock"
  --rust-toolchain "$source_context/services/hepta-research-league/docker/rust-toolchain.manifest"
)
python3 "$generator" "${common[@]}" --runtime-binary "$first_binary" \
  --output "$scratch/first.cdx.json"
python3 "$generator" "${common[@]}" --runtime-binary "$second_binary" \
  --output "$scratch/second.cdx.json"
cmp "$scratch/first.cdx.json" "$scratch/second.cdx.json"
python3 "$source_context/scripts/verify-hepta-research-league-sbom.py" \
  --sbom "$scratch/first.cdx.json" \
  --runtime-sha256 "$first_sha256" \
  --dockerfile "$source_context/services/hepta-research-league/Dockerfile" \
  --cargo-lock "$source_context/Cargo.lock" \
  --rust-toolchain "$source_context/services/hepta-research-league/docker/rust-toolchain.manifest"

verify_source_unchanged
if [[ "$mode" == --check ]]; then
  cmp "$tracked_sbom" "$scratch/first.cdx.json"
  verify_source_unchanged
else
  staged_output=$(mktemp \
    "$repo_dir/deploy/hepta-research-league/.hepta-research-league.cdx.json.XXXXXX")
  install -m 0644 "$scratch/first.cdx.json" "$staged_output"
  verify_source_unchanged
  mv -f -- "$staged_output" "$tracked_sbom"
  staged_output=
  cmp "$tracked_sbom" "$scratch/first.cdx.json"
  if [[ "$(git -C "$repo_dir" rev-parse HEAD)" != "$revision" ]] || \
     [[ "$(git -C "$repo_dir" rev-parse 'HEAD^{tree}')" != "$source_tree" ]]; then
    echo "Hepta source identity changed during the atomic SBOM write" >&2
    exit 1
  fi
  write_status=$(git -C "$repo_dir" status --porcelain=v1 --untracked-files=all)
  if [[ -n "$write_status" && \
        "$write_status" != " M deploy/hepta-research-league/hepta-research-league.cdx.json" ]]; then
    echo "unexpected worktree change raced the atomic Hepta SBOM write" >&2
    exit 1
  fi
  cmp "$tracked_sbom" "$scratch/first.cdx.json"
fi
printf 'Hepta pinned runtime SBOM %s: PASS binary_sha256=%s sbom_sha256=%s\n' \
  "${mode#--}" "$first_sha256" \
  "$(sha256sum "$scratch/first.cdx.json" | cut -d' ' -f1)"

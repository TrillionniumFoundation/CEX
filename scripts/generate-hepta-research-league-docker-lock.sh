#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
mode=${1:-}
lock_relative=services/hepta-research-league/docker/Cargo.lock
tracked_lock="$repo_dir/$lock_relative"
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
      "$repo_dir/services/hepta-research-league/docker/".Cargo.lock.*)
        rm -f -- "$staged_output"
        ;;
      *) echo "refusing to remove unexpected staged Docker lock path" >&2 ;;
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
    *) echo "refusing to remove unexpected Docker-lock scratch path" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

verify_source_unchanged() {
  if [[ "$(git -C "$repo_dir" rev-parse HEAD)" != "$revision" ]] || \
     [[ "$(git -C "$repo_dir" rev-parse 'HEAD^{tree}')" != "$source_tree" ]] || \
     [[ -n "$(git -C "$repo_dir" status --porcelain=v1 --untracked-files=all)" ]]; then
    echo "Hepta pinned Docker-lock source changed during generation" >&2
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

for command_name in bash cmp curl cut docker find flock git id install mktemp mv \
  python3 rg sha256sum sort tar timeout uname; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Hepta Docker-lock generator requires $command_name" >&2
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
[[ "$(uname -m)" == x86_64 ]] || {
  echo "Hepta Docker-lock generator requires x86_64" >&2
  exit 1
}
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
  local destination=$1
  verify_source_unchanged
  "${bounded_buildx[@]}" \
    --no-cache --pull=false --progress=plain \
    --provenance=false --sbom=false --platform linux/amd64 \
    --target cargo-lock-export --output "type=local,dest=$destination" \
    --build-arg BUILDKIT_MULTI_PLATFORM=1 \
    --file "$source_context/services/hepta-research-league/Dockerfile" \
    "$source_context"
  if [[ ${docker_command[0]} == sudo ]]; then
    case "$destination" in
      "$scratch"/*)
        sudo -n chown -R -- "$(id -u):$(id -g)" "$destination"
        ;;
      *) echo "refusing to change ownership outside Docker-lock scratch" >&2; exit 1 ;;
    esac
  fi
  verify_source_unchanged
}

build_export "$scratch/first"
build_export "$scratch/second"
first_lock="$scratch/first/linux_amd64/Cargo.lock"
second_lock="$scratch/second/linux_amd64/Cargo.lock"
for directory in "$scratch/first" "$scratch/second"; do
  exported=$(find "$directory" -mindepth 1 -printf '%P\n' | LC_ALL=C sort)
  if [[ "$exported" != $'linux_amd64\nlinux_amd64/Cargo.lock' ]]; then
    echo "cargo-lock-export did not contain exactly one Cargo.lock" >&2
    exit 1
  fi
done
for generated_lock in "$first_lock" "$second_lock"; do
  [[ -f "$generated_lock" && ! -L "$generated_lock" ]] || {
    echo "pinned builder exported a non-regular Docker lock" >&2
    exit 1
  }
done
cmp "$first_lock" "$second_lock"

python3 - "$first_lock" <<'PY'
import pathlib
import sys
import tomllib

path = pathlib.Path(sys.argv[1])
document = tomllib.loads(path.read_text(encoding="utf-8"))
if document.get("version") != 4 or set(document) != {"version", "package"}:
    raise SystemExit("generated Docker lock has a non-canonical top-level shape")
packages = document.get("package")
if not isinstance(packages, list) or not packages:
    raise SystemExit("generated Docker lock contains no packages")
identities = []
local = []
for package in packages:
    if not isinstance(package, dict):
        raise SystemExit("generated Docker lock package is not an object")
    name = package.get("name")
    version = package.get("version")
    source = package.get("source")
    if not isinstance(name, str) or not isinstance(version, str):
        raise SystemExit("generated Docker lock package identity is invalid")
    if isinstance(source, str) and source.startswith("git+"):
        raise SystemExit("generated Docker lock contains a Git dependency")
    identities.append((name, version, source))
    if source is None:
        local.append((name, version))
if len(identities) != len(set(identities)):
    raise SystemExit("generated Docker lock contains duplicate package identities")
expected_local = {
    ("hepta-paper-raid-contracts", "0.1.0"),
    ("hepta-research-league", "0.1.0"),
    ("trnm-finality-types", "0.1.0"),
    ("trnm-finality-verifier", "0.1.0"),
    ("trnm-protocol", "0.1.0"),
    ("trnm-research-protocol", "0.1.0"),
}
if set(local) != expected_local or len(local) != len(expected_local):
    raise SystemExit(f"generated Docker lock local package closure drifted: {local!r}")
PY

verify_source_unchanged
if [[ "$mode" == --check ]]; then
  [[ -f "$tracked_lock" && ! -L "$tracked_lock" ]] || {
    echo "tracked Hepta Docker lock is missing or non-regular" >&2
    exit 1
  }
  cmp "$tracked_lock" "$first_lock"
  verify_source_unchanged
else
  [[ ! -L "$tracked_lock" ]] || {
    echo "tracked Hepta Docker lock must not be a symlink" >&2
    exit 1
  }
  if git -C "$repo_dir" cat-file -e "$revision:$lock_relative" 2>/dev/null; then
    expected_status=" M $lock_relative"
  else
    expected_status="?? $lock_relative"
  fi
  staged_output=$(mktemp \
    "$repo_dir/services/hepta-research-league/docker/.Cargo.lock.XXXXXX")
  install -m 0644 "$first_lock" "$staged_output"
  cmp "$staged_output" "$first_lock"
  staged_relative=${staged_output#"$repo_dir/"}
  if [[ "$(git -C "$repo_dir" rev-parse HEAD)" != "$revision" ]] || \
     [[ "$(git -C "$repo_dir" rev-parse 'HEAD^{tree}')" != "$source_tree" ]] || \
     [[ "$(git -C "$repo_dir" status --porcelain=v1 --untracked-files=all)" != "?? $staged_relative" ]]; then
    echo "Hepta source changed while staging the atomic Docker-lock write" >&2
    exit 1
  fi
  mv -f -- "$staged_output" "$tracked_lock"
  staged_output=
  cmp "$tracked_lock" "$first_lock"
  if [[ "$(git -C "$repo_dir" rev-parse HEAD)" != "$revision" ]] || \
     [[ "$(git -C "$repo_dir" rev-parse 'HEAD^{tree}')" != "$source_tree" ]]; then
    echo "Hepta source identity changed during the atomic Docker-lock write" >&2
    exit 1
  fi
  write_status=$(git -C "$repo_dir" status --porcelain=v1 --untracked-files=all)
  if [[ -n "$write_status" && "$write_status" != "$expected_status" ]]; then
    echo "unexpected worktree change raced the atomic Hepta Docker-lock write" >&2
    exit 1
  fi
  cmp "$tracked_lock" "$first_lock"
fi
printf 'Hepta pinned Docker lock %s: PASS sha256=%s\n' \
  "${mode#--}" "$(sha256sum "$first_lock" | cut -d' ' -f1)"

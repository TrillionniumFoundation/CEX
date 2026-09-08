#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
docker_dir="$repo_root/services/paper-raid-bff/docker"
scratch_dir=$(mktemp -d)
expected_rust_toolchain=1.98.1
expected_rust_release_commit=48a229ceaefd4985c50990b14116b6d856af0985

cleanup() {
  rm -rf "$scratch_dir"
}
trap cleanup EXIT

exec 9>/tmp/trnm-paper-raid-cargo-gate.lock
if ! flock -n 9; then
  echo "shared Paper Raid Cargo gate is busy" >&2
  exit 75
fi

rustc_version=$(rustc --version)
[[ "$(awk '{print $2}' <<<"$rustc_version")" == "$expected_rust_toolchain" ]] || {
  echo "Docker lock gate requires rustc $expected_rust_toolchain, found: $rustc_version" >&2
  exit 1
}
rustc --version --verbose >"$scratch_dir/rustc-version.txt"
grep -Fx "commit-hash: $expected_rust_release_commit" "$scratch_dir/rustc-version.txt" >/dev/null || {
  echo "Docker lock gate rustc release commit drifted" >&2
  exit 1
}
cargo --version --verbose >"$scratch_dir/cargo-version.txt"

copy_minimal_workspace() {
  local destination=$1

  mkdir -p \
    "$destination/crates/hepta-paper-raid-contracts" \
    "$destination/services/paper-raid-bff"
  cp "$docker_dir/workspace.Cargo.toml" "$destination/Cargo.toml"
  cp "$repo_root/crates/hepta-paper-raid-contracts/Cargo.toml" \
    "$destination/crates/hepta-paper-raid-contracts/Cargo.toml"
  cp -a "$repo_root/crates/hepta-paper-raid-contracts/src" \
    "$destination/crates/hepta-paper-raid-contracts/src"
  cp "$repo_root/services/paper-raid-bff/Cargo.toml" \
    "$destination/services/paper-raid-bff/Cargo.toml"
  cp -a "$repo_root/services/paper-raid-bff/src" \
    "$destination/services/paper-raid-bff/src"
  cp -a "$repo_root/services/paper-raid-bff/migrations" \
    "$destination/services/paper-raid-bff/migrations"
}

locked_workspace="$scratch_dir/locked"
copy_minimal_workspace "$locked_workspace"
cp "$docker_dir/Cargo.lock" "$locked_workspace/Cargo.lock"

metadata_file="$scratch_dir/metadata.json"
cargo fetch \
  --locked \
  --manifest-path "$locked_workspace/Cargo.toml"
cargo metadata \
  --locked \
  --offline \
  --manifest-path "$locked_workspace/Cargo.toml" \
  --format-version 1 >"$metadata_file"

jq -e '
  [.packages[] as $package
    | select(.workspace_members | index($package.id))
    | $package.name]
  | sort == ["hepta-paper-raid-contracts", "paper-raid-bff"]
' "$metadata_file" >/dev/null

if jq -r '.packages[].source // empty' "$metadata_file" | rg -n '^git\+'; then
  echo "minimal Docker workspace resolved a Git dependency" >&2
  exit 1
fi

while IFS= read -r manifest_path; do
  case "$manifest_path" in
    "$locked_workspace"/*) ;;
    *)
      echo "path dependency escaped the minimal Docker workspace: $manifest_path" >&2
      exit 1
      ;;
  esac
done < <(jq -r '.packages[] | select(.source == null) | .manifest_path' "$metadata_file")

if rg -n 'source = "git\+' "$docker_dir/Cargo.lock"; then
  echo "minimal Docker lock contains a Git dependency" >&2
  exit 1
fi

echo "paper-raid-bff committed minimal Docker lock verification: ok"

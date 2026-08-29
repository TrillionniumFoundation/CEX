#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
docker_dir="$repo_root/services/paper-raid-bff/docker"
scratch_dir=$(mktemp -d)

cleanup() {
  rm -rf "$scratch_dir"
}
trap cleanup EXIT

exec 9>/tmp/trnm-paper-raid-cargo-gate.lock
if ! flock -n 9; then
  echo "shared Paper Raid Cargo gate is busy" >&2
  exit 75
fi

cargo_version=$(cargo --version)
case "$cargo_version" in
  "cargo 1.95.0 "*) ;;
  *)
    echo "Docker lock gate requires cargo 1.95.0, found: $cargo_version" >&2
    exit 1
    ;;
esac

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
cargo metadata \
  --locked \
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

regenerated_workspace="$scratch_dir/regenerated"
copy_minimal_workspace "$regenerated_workspace"
cargo generate-lockfile \
  --offline \
  --manifest-path "$regenerated_workspace/Cargo.toml"

if ! cmp -s "$docker_dir/Cargo.lock" "$regenerated_workspace/Cargo.lock"; then
  diff -u "$docker_dir/Cargo.lock" "$regenerated_workspace/Cargo.lock" || true
  echo "minimal Docker Cargo.lock drifted from cargo 1.95.0 output" >&2
  exit 1
fi

echo "paper-raid-bff minimal Docker lock gate: ok"

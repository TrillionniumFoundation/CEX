#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
tracked_sbom=$repo_root/services/paper-raid-bff/docker/accessctl.sbom.cdx.json
output=${1:-}
first_binary=${2:-}
second_binary=${3:-}
scratch_dir=$(mktemp -d)
staged_output=

cleanup() {
  if [[ -n "$staged_output" ]]; then
    case "$staged_output" in
      "$repo_root/services/paper-raid-bff/docker/".accessctl.sbom.cdx.json.*)
        rm -f -- "$staged_output"
        ;;
      *)
        echo "refusing to remove unexpected staged accessctl SBOM path: $staged_output" >&2
        ;;
    esac
  fi
  case "$scratch_dir" in
    /tmp/tmp.*) rm -rf -- "$scratch_dir" ;;
    *) echo "refusing to remove unexpected accessctl SBOM scratch path: $scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT INT TERM

if [[ $# -ne 3 ]]; then
  printf 'usage: %s services/paper-raid-bff/docker/accessctl.sbom.cdx.json FIRST_BINARY SECOND_BINARY\n' \
    "$0" >&2
  exit 2
fi
for binary in "$first_binary" "$second_binary"; do
  [[ -f "$binary" && ! -L "$binary" && -x "$binary" ]] || {
    printf 'accessctl runtime input is not a regular executable: %s\n' "$binary" >&2
    exit 2
  }
done
[[ ! -L "$output" ]] || {
  echo 'accessctl SBOM output must not be a symlink' >&2
  exit 2
}
resolved_output=$(realpath -m -- "$output")
resolved_tracked_sbom=$(realpath -m -- "$tracked_sbom")
[[ "$resolved_output" == "$resolved_tracked_sbom" ]] || {
  echo 'accessctl SBOM output must be the exact tracked accessctl SBOM path' >&2
  exit 2
}
[[ -z "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)" ]] || {
  echo 'accessctl SBOM binding requires a clean committed source tree' >&2
  exit 1
}
revision=$(git -C "$repo_root" rev-parse HEAD)
source_tree=$(git -C "$repo_root" rev-parse 'HEAD^{tree}')
first_sha256=$(sha256sum "$first_binary" | cut -d' ' -f1)
second_sha256=$(sha256sum "$second_binary" | cut -d' ' -f1)
if [[ "$first_sha256" != "$second_sha256" ]] || ! cmp -s "$first_binary" "$second_binary"; then
  echo 'independent pinned-builder accessctl binaries are not byte-deterministic' >&2
  exit 1
fi
for forbidden in "$revision" "$source_tree" "$first_sha256"; do
  if rg -a -q --fixed-strings "$forbidden" "$first_binary"; then
    echo 'accessctl runtime binary embeds revision/tree/self-hash material' >&2
    exit 1
  fi
done

generate() {
  local destination=$1
  jq --sort-keys --arg digest "$first_sha256" '
    if .bomFormat != "CycloneDX"
       or .specVersion != "1.5"
       or (.components | length) != 1
       or .components[0]["bom-ref"] != "file:/paper-raid-accessctl"
       or .components[0].name != "/paper-raid-accessctl"
       or .components[0].type != "file"
       or (.components[0].hashes | length) != 1
       or .components[0].hashes[0].alg != "SHA-256"
       or ([.metadata.properties[]
            | select(.name == "org.trillionnium.release-state")] | length) != 1
    then error("accessctl SBOM template contract drifted")
    else
      .components[0].hashes[0].content = $digest
      | (.metadata.properties[]
          | select(.name == "org.trillionnium.release-state")
          | .value) = "bound-release-binary"
    end
  ' "$tracked_sbom" >"$destination"
}

generate "$scratch_dir/first.cdx.json"
generate "$scratch_dir/second.cdx.json"
cmp -s "$scratch_dir/first.cdx.json" "$scratch_dir/second.cdx.json" || {
  echo 'accessctl CycloneDX binding is not byte-deterministic' >&2
  exit 1
}
[[ "$(git -C "$repo_root" rev-parse HEAD)" == "$revision" \
  && "$(git -C "$repo_root" rev-parse 'HEAD^{tree}')" == "$source_tree" \
  && -z "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)" ]] || {
  echo 'accessctl source changed during SBOM binding' >&2
  exit 1
}
staged_output=$(mktemp \
  "$repo_root/services/paper-raid-bff/docker/.accessctl.sbom.cdx.json.XXXXXX")
install -m 0644 "$scratch_dir/first.cdx.json" "$staged_output"
mv -f -- "$staged_output" "$resolved_output"
staged_output=
printf 'paper-raid-accessctl deterministic runtime SBOM binding: ok binary_sha256=%s\n' \
  "$first_sha256"

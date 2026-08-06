#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
docker_dir="$repo_root/services/paper-raid-bff/docker"
output=${1:-}
runtime_input_kind=${2:-}
runtime_input=${3:-}
scratch_dir=$(mktemp -d)

cleanup() {
  case "$scratch_dir" in
    /tmp/tmp.*) rm -rf -- "$scratch_dir" ;;
    *) echo "refusing to remove unexpected SBOM scratch path: $scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT

if [[ -z "$output" || "$#" -ne 3 ]]; then
  echo "usage: $0 OUTPUT (--runtime-binary FILE | --runtime-sha256 SHA256)" >&2
  exit 2
fi
case "$runtime_input_kind" in
  --runtime-binary)
    if [[ ! -f "$runtime_input" ]] || [[ ! -x "$runtime_input" ]]; then
      echo "runtime binary is missing or not executable: $runtime_input" >&2
      exit 2
    fi
    runtime_binary_sha256=$(sha256sum "$runtime_input" | cut -d' ' -f1)
    ;;
  --runtime-sha256)
    if [[ ! "$runtime_input" =~ ^[0-9a-f]{64}$ ]]; then
      echo "runtime binary SHA-256 is not canonical" >&2
      exit 2
    fi
    runtime_binary_sha256=$runtime_input
    ;;
  *)
    echo "usage: $0 OUTPUT (--runtime-binary FILE | --runtime-sha256 SHA256)" >&2
    exit 2
    ;;
esac

exec 9>/tmp/trnm-paper-raid-cargo-gate.lock
flock -x 9

workspace="$scratch_dir/workspace"
mkdir -p \
  "$workspace/crates/hepta-paper-raid-contracts" \
  "$workspace/services/paper-raid-bff"
cp "$docker_dir/workspace.Cargo.toml" "$workspace/Cargo.toml"
cp "$docker_dir/Cargo.lock" "$workspace/Cargo.lock"
cp "$repo_root/crates/hepta-paper-raid-contracts/Cargo.toml" \
  "$workspace/crates/hepta-paper-raid-contracts/Cargo.toml"
cp -a "$repo_root/crates/hepta-paper-raid-contracts/src" \
  "$workspace/crates/hepta-paper-raid-contracts/src"
cp "$repo_root/services/paper-raid-bff/Cargo.toml" \
  "$workspace/services/paper-raid-bff/Cargo.toml"
cp -a "$repo_root/services/paper-raid-bff/src" \
  "$workspace/services/paper-raid-bff/src"
cp -a "$repo_root/services/paper-raid-bff/migrations" \
  "$workspace/services/paper-raid-bff/migrations"

metadata="$scratch_dir/metadata.json"
cargo metadata \
  --locked \
  --offline \
  --manifest-path "$workspace/Cargo.toml" \
  --format-version 1 >"$metadata"

lock_sha256=$(sha256sum "$docker_dir/Cargo.lock" | cut -d' ' -f1)
generated="$scratch_dir/sbom.cdx.json"
jq --sort-keys \
  --arg lock_sha256 "$lock_sha256" \
  --arg runtime_binary_sha256 "$runtime_binary_sha256" '
  . as $metadata
  | def package($id): first($metadata.packages[] | select(.id == $id));
    def purl($package):
      "pkg:cargo/\($package.name | @uri)@\($package.version | @uri)";
    def component($package):
      {
        type: (if $package.name == "paper-raid-bff" then "application" else "library" end),
        "bom-ref": purl($package),
        name: $package.name,
        version: $package.version,
        purl: purl($package)
      }
      + (if $package.license == null then {} else {licenses: [{expression: $package.license}]} end);
    {
      bomFormat: "CycloneDX",
      specVersion: "1.5",
      version: 1,
      metadata: {
        tools: {components: [{type: "application", name: "paper-raid-bff-sbom-generator", version: "1"}]},
        component: component(first($metadata.packages[] | select(.name == "paper-raid-bff"))),
        properties: [{name: "trnm:cargo-lock:sha256", value: $lock_sha256}]
      },
      components: (
        [
          $metadata.packages[]
          | select(.name != "paper-raid-bff")
          | component(.)
        ]
        + [{
            type: "file",
            "bom-ref": "file:/paper-raid-bff",
            name: "/paper-raid-bff",
            hashes: [{alg: "SHA-256", content: $runtime_binary_sha256}]
          }]
        | sort_by(."bom-ref")
      ),
      dependencies: [
        $metadata.resolve.nodes[]
        | package(.id) as $package
        | {
            ref: purl($package),
            dependsOn: ([.deps[].pkg | package(.) | purl(.)] | unique | sort)
          }
      ] | sort_by(.ref)
    }
' "$metadata" >"$generated"

mkdir -p "$(dirname "$output")"
cp "$generated" "$output"

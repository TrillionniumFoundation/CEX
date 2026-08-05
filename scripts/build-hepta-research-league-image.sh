#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_dir"

revision=$(git rev-parse HEAD)
source_tree=$(git rev-parse 'HEAD^{tree}')
source_date_epoch=$(git show -s --format=%ct "$revision")
image_ref=${HEPTA_IMAGE_REF:-trnm/hepta-research-league:${revision}}
repro_ref="trnm/hepta-research-league:repro-${revision:0:12}-$$"
sentinel_ref="trnm/hepta-research-league:sentinel-${revision:0:12}-$$"
release_dir=$(mktemp -d)
source_context="$release_dir/context"
buildx_version=v0.36.1
buildx_url=https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64
buildx_sha256=48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778
docker_config="$release_dir/docker"
buildx_plugin="$docker_config/cli-plugins/docker-buildx"
first_iid="$release_dir/first.iid"
second_iid="$release_dir/second.iid"
first_metadata="$release_dir/first.metadata.json"
second_metadata="$release_dir/second.metadata.json"
containers=()
sentinel=HEPTA_IMAGE_SENTINEL_DO_NOT_SHIP_7e07109a
gate_succeeded=false
original_image_checked=false
original_image_id=

cleanup() {
  if [[ "$original_image_checked" == true && "$gate_succeeded" != true ]]; then
    current_image_id=$(
      "${docker_command[@]}" image inspect "$image_ref" --format '{{.Id}}' 2>/dev/null || true
    )
    if [[ "$current_image_id" != "$original_image_id" ]]; then
      [[ -z "$current_image_id" ]] || \
        "${docker_command[@]}" image rm -f "$image_ref" >/dev/null 2>&1 || true
      if [[ -n "$original_image_id" ]]; then
        "${docker_command[@]}" image tag "$original_image_id" "$image_ref" >/dev/null 2>&1 || \
          echo "failed to restore the previous Hepta image tag after gate failure" >&2
      fi
    fi
  fi
  for container in "${containers[@]:-}"; do
    [[ -n "$container" ]] && "${docker_command[@]:-docker}" rm -f "$container" >/dev/null 2>&1 || true
  done
  for disposable_ref in "${repro_ref:-}" "${sentinel_ref:-}"; do
    [[ -n "$disposable_ref" ]] && \
      "${docker_command[@]:-docker}" image rm -f "$disposable_ref" >/dev/null 2>&1 || true
  done
  case "$release_dir" in
    /tmp/tmp.*)
      if [[ ${docker_command[0]:-} == sudo ]]; then
        sudo -n rm -rf -- "$release_dir"
      else
        rm -rf -- "$release_dir"
      fi
      ;;
    *) echo "refusing to remove unexpected Hepta image-gate scratch path" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

verify_source_unchanged() {
  if [[ "$(git rev-parse HEAD)" != "$revision" ]] || \
     [[ "$(git rev-parse 'HEAD^{tree}')" != "$source_tree" ]] || \
     [[ -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
    echo "Hepta image source changed during immutable build" >&2
    exit 1
  fi
}

for command_name in cmp curl cut docker find flock git id jq mktemp python3 rg sed \
  sha256sum sort tar timeout tr uname; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Hepta image gate requires $command_name" >&2
    exit 1
  }
done
release_lock=$(git rev-parse --path-format=absolute --git-path hepta-release-authority.lock)
exec 9>"$release_lock"
flock -n 9 || {
  echo "another Hepta release-authority process is already running" >&2
  exit 1
}
[[ "$(uname -m)" == x86_64 ]] || {
  echo "Hepta image gate requires x86_64" >&2
  exit 1
}
[[ "$revision" =~ ^[0-9a-f]{40}$ \
  && "$source_tree" =~ ^[0-9a-f]{40}$ \
  && "$source_date_epoch" =~ ^[0-9]+$ ]] || {
  echo "Hepta image source identity is not canonical" >&2
  exit 1
}
verify_source_unchanged

docker_command=(docker)
if ! docker info >/dev/null 2>&1; then
  docker_command=(sudo -n docker)
fi
"${docker_command[@]}" info >/dev/null
original_inspect_status=0
"${docker_command[@]}" image inspect "$image_ref" --format '{{.Id}}' \
  >"$release_dir/original-image.id" 2>"$release_dir/original-image.errors" || \
  original_inspect_status=$?
case "$original_inspect_status" in
  0) original_image_id=$(<"$release_dir/original-image.id") ;;
  *)
    if rg -q --fixed-strings "No such image" "$release_dir/original-image.errors"; then
      original_image_id=
    else
      sed -n '1,20p' "$release_dir/original-image.errors" >&2
      echo "failed to capture the pre-gate Hepta image tag" >&2
      exit 1
    fi
    ;;
esac
original_image_checked=true

mkdir -p "$source_context"
git archive "$revision" | tar -xf - -C "$source_context"
unexpected_archive_entry=$(find "$source_context" -mindepth 1 \
  \( -type l -o \! -type f -a \! -type d \) -print -quit)
[[ -z "$unexpected_archive_entry" ]] || {
  echo "Hepta immutable source archive contains a symlink or special file: $unexpected_archive_entry" >&2
  exit 1
}
printf '%s\n' "$sentinel" >"$source_context/HEPTA_IMAGE_SENTINEL_DO_NOT_SHIP.txt"
verify_source_unchanged

dockerfile="$source_context/services/hepta-research-league/Dockerfile"
cargo_lock="$source_context/services/hepta-research-league/docker/Cargo.lock"
toolchain="$source_context/services/hepta-research-league/docker/rust-toolchain.manifest"
sbom="$source_context/deploy/hepta-research-league/hepta-research-league.cdx.json"
dockerfile_sha256=$(sha256sum "$dockerfile" | cut -d' ' -f1)
cargo_lock_sha256=$(sha256sum "$cargo_lock" | cut -d' ' -f1)
rust_toolchain_sha256=$(sha256sum "$toolchain" | cut -d' ' -f1)
sbom_sha256=$(sha256sum "$sbom" | cut -d' ' -f1)
runtime_binary_sha256=$(jq -er '
  [.components[]? | select(.type == "file")] as $files
  | if (($files | length) == 1
      and $files[0].name == "/usr/local/bin/hepta-research-league"
      and ($files[0].hashes | length) == 1
      and $files[0].hashes[0].alg == "SHA-256")
    then $files[0].hashes[0].content
    else error("SBOM must bind exactly one Hepta runtime file")
    end
' "$sbom")

verify_sbom() {
  python3 "$source_context/scripts/verify-hepta-research-league-sbom.py" \
    --sbom "$1" --runtime-sha256 "$runtime_binary_sha256" \
    --dockerfile "$dockerfile" --cargo-lock "$cargo_lock" \
    --rust-toolchain "$toolchain"
}
verify_sbom "$sbom"

expect_sbom_rejected() {
  if verify_sbom "$1" >/dev/null 2>&1; then
    echo "Hepta SBOM negative fixture was incorrectly accepted: $1" >&2
    exit 1
  fi
}
jq '.metadata.properties += [{"name":"trnm:unexpected","value":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}]' \
  "$sbom" >"$release_dir/sbom-extra-property.json"
expect_sbom_rejected "$release_dir/sbom-extra-property.json"
jq '.components += [.components[] | select(.type == "file")]' \
  "$sbom" >"$release_dir/sbom-extra-file.json"
expect_sbom_rejected "$release_dir/sbom-extra-file.json"
jq '.metadata.properties[0].value |= sub("^sha256:"; "")' \
  "$sbom" >"$release_dir/sbom-bad-prefix.json"
expect_sbom_rejected "$release_dir/sbom-bad-prefix.json"
jq '(.components[] | select(.type == "file") | .hashes[0].content) = "0000000000000000000000000000000000000000000000000000000000000000"' \
  "$sbom" >"$release_dir/sbom-bad-runtime.json"
expect_sbom_rejected "$release_dir/sbom-bad-runtime.json"

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

# Recompute the complete Cargo dependency closure in the same pinned builder
# authority used by the image, then require the tracked SBOM to be byte exact.
verify_source_unchanged
"${bounded_buildx[@]}" \
  --no-cache --progress=plain --pull=false \
  --provenance=false --sbom=false --platform linux/amd64 \
  --target sbom-metadata-export \
  --output "type=local,dest=$release_dir/sbom-metadata" \
  --build-arg BUILDKIT_MULTI_PLATFORM=1 \
  --file "$dockerfile" "$source_context"
if [[ ${docker_command[0]} == sudo ]]; then
  sudo -n chown -R -- "$(id -u):$(id -g)" "$release_dir/sbom-metadata"
fi
verify_source_unchanged
cargo_metadata="$release_dir/sbom-metadata/linux_amd64/cargo-metadata.json"
cargo_metadata_entries=$(find "$release_dir/sbom-metadata" -mindepth 1 \
  -printf '%P\n' | LC_ALL=C sort)
if [[ "$cargo_metadata_entries" != $'linux_amd64\nlinux_amd64/cargo-metadata.json' ]] || \
   [[ -L "$cargo_metadata" || ! -f "$cargo_metadata" ]]; then
  echo "pinned builder SBOM metadata export is not canonical" >&2
  exit 1
fi
python3 "$source_context/scripts/generate-hepta-research-league-sbom.py" \
  --metadata "$cargo_metadata" \
  --runtime-sha256 "$runtime_binary_sha256" \
  --dockerfile "$dockerfile" \
  --cargo-lock "$cargo_lock" \
  --rust-toolchain "$toolchain" \
  --output "$release_dir/regenerated.cdx.json"
cmp "$sbom" "$release_dir/regenerated.cdx.json"
verify_source_unchanged

build_args=(
  --build-arg BUILDKIT_MULTI_PLATFORM=1
  --build-arg "VCS_REF=$revision"
  --build-arg "SOURCE_TREE=$source_tree"
  --build-arg "SOURCE_DATE_EPOCH=$source_date_epoch"
  --build-arg "SBOM_SHA256=$sbom_sha256"
  --build-arg "RUNTIME_BINARY_SHA256=$runtime_binary_sha256"
  --build-arg "CARGO_LOCK_SHA256=$cargo_lock_sha256"
  --build-arg "DOCKERFILE_SHA256=$dockerfile_sha256"
  --build-arg "RUST_TOOLCHAIN_SHA256=$rust_toolchain_sha256"
)

build_image() {
  local target_ref=$1
  local iid=$2
  local metadata=$3
  verify_source_unchanged
  "${bounded_buildx[@]}" \
    --load --progress=plain --pull=false --no-cache \
    --provenance=false --sbom=false --platform linux/amd64 \
    --file "$dockerfile" "${build_args[@]}" \
    --iidfile "$iid" --metadata-file "$metadata" \
    --tag "$target_ref" "$source_context"
  verify_source_unchanged
}

build_image "$image_ref" "$first_iid" "$first_metadata"
build_image "$repro_ref" "$second_iid" "$second_metadata"
image_id=$("${docker_command[@]}" image inspect "$image_ref" --format '{{.Id}}')
repro_image_id=$("${docker_command[@]}" image inspect "$repro_ref" --format '{{.Id}}')
if [[ "$image_id" != "$repro_image_id" ]]; then
  echo "independent no-cache Hepta image builds differ" >&2
  exit 1
fi

digest_from_metadata() {
  local metadata=$1
  local key=$2
  jq -er --arg key "$key" \
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

index_digest=$(digest_from_metadata "$first_metadata" containerimage.digest)
repro_index_digest=$(digest_from_metadata "$second_metadata" containerimage.digest)
metadata_config_digest=$(optional_digest_from_metadata \
  "$first_metadata" containerimage.config.digest)
repro_metadata_config_digest=$(optional_digest_from_metadata \
  "$second_metadata" containerimage.config.digest)
first_iid_value=$(tr -d '\n' <"$first_iid")
second_iid_value=$(tr -d '\n' <"$second_iid")
for digest in "$image_id" "$repro_image_id" "$index_digest" \
  "$repro_index_digest" "$first_iid_value" "$second_iid_value"; do
  [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
    echo "Buildx returned a non-canonical Hepta OCI digest" >&2
    exit 1
  }
done
if [[ "$metadata_config_digest" != "$repro_metadata_config_digest" ]] || \
   [[ -n "$metadata_config_digest" && "$metadata_config_digest" != "$image_id" ]] || \
   [[ "$first_iid_value" != "$image_id" && "$first_iid_value" != "$index_digest" ]] || \
   [[ "$second_iid_value" != "$repro_image_id" && "$second_iid_value" != "$repro_index_digest" ]] || \
   [[ "$index_digest" != "$repro_index_digest" ]] || \
   [[ "$first_iid_value" != "$second_iid_value" ]]; then
  echo "independent no-cache Hepta OCI metadata is not deterministic" >&2
  exit 1
fi

inspect_json=$("${docker_command[@]}" image inspect "$image_id")
jq -e \
  --arg revision "$revision" \
  --arg source_tree "$source_tree" \
  --arg source_date_epoch "$source_date_epoch" \
  --arg sbom_sha256 "$sbom_sha256" \
  --arg runtime_binary_sha256 "$runtime_binary_sha256" \
  --arg cargo_lock_sha256 "$cargo_lock_sha256" \
  --arg dockerfile_sha256 "$dockerfile_sha256" \
  --arg rust_toolchain_sha256 "$rust_toolchain_sha256" '
  length == 1
  and .[0].Config.User == "65532:65532"
  and .[0].Config.Entrypoint == ["/usr/local/bin/hepta-research-league"]
  and .[0].Config.Healthcheck.Test == ["CMD", "/usr/local/bin/hepta-research-league", "--probe-ready"]
  and .[0].Config.Labels["org.opencontainers.image.revision"] == $revision
  and .[0].Config.Labels["org.opencontainers.image.source"] == "https://github.com/TrillionniumFoundation/CEX.git"
  and .[0].Config.Labels["org.trillionnium.source.tree"] == $source_tree
  and .[0].Config.Labels["org.trillionnium.sbom.sha256"] == $sbom_sha256
  and .[0].Config.Labels["org.trillionnium.cargo-lock.sha256"] == $cargo_lock_sha256
  and .[0].Config.Labels["org.trillionnium.dockerfile.sha256"] == $dockerfile_sha256
  and .[0].Config.Labels["org.trillionnium.rust-toolchain.sha256"] == $rust_toolchain_sha256
  and .[0].Config.Labels["io.trillionnium.hepta.source-date-epoch"] == $source_date_epoch
  and .[0].Config.Labels["io.trillionnium.hepta.runtime-binary.sha256"] == $runtime_binary_sha256
  and ([.[0].Config.Env[]? | select(test("^(HEPTA_|TRNM_|DATABASE_URL=|NAKAMA_)") )] | length) == 0
  ' <<<"$inspect_json" >/dev/null

scan_no_matches() {
  local label=$1
  local pattern=$2
  local target=$3
  local matches="$release_dir/$label.matches"
  local errors="$release_dir/$label.errors"
  local status=0
  rg --hidden --no-ignore -a -n -- "$pattern" "$target" \
    >"$matches" 2>"$errors" || status=$?
  case "$status" in
    0)
      sed -n '1,20p' "$matches" >&2
      return 10
      ;;
    1) return 0 ;;
    *)
      sed -n '1,20p' "$errors" >&2
      return 20
      ;;
  esac
}

if ! "${docker_command[@]}" history --no-trunc --format '{{.CreatedBy}}' "$image_id" \
  >"$release_dir/image.history" 2>"$release_dir/image.history.errors"; then
  sed -n '1,20p' "$release_dir/image.history.errors" >&2
  echo "failed to inspect Hepta image history" >&2
  exit 1
fi
history_status=0
scan_no_matches history \
  '(PASSWORD|PRIVATE_KEY|SECRET|SEED|TOKEN|DATABASE_URL|HEPTA_OPERATOR_TOKEN)=' \
  "$release_dir/image.history" || history_status=$?
case "$history_status" in
  0) ;;
  10) echo "Hepta image history contains deployment secrets" >&2; exit 1 ;;
  *) echo "Hepta image history scan failed" >&2; exit 1 ;;
esac

scan_image() {
  local candidate=$1
  local label=$2
  local scan="$release_dir/$label"
  mkdir -p "$scan/rootfs" || return 20
  local container
  container=$("${docker_command[@]}" create --read-only --network none "$candidate") || return 20
  containers+=("$container")
  "${docker_command[@]}" export "$container" --output "$scan/rootfs.tar" || return 20
  tar -tf "$scan/rootfs.tar" >"$scan/rootfs.list" || return 20
  local path_status=0
  scan_no_matches "rootfs-path-$label" \
    '(^|/)(\.git|Cargo\.toml|Cargo\.lock|rust-toolchain\.(toml|manifest))($|/)|(^|/)(src|target|migrations)(/|$)|(^|/)\.env($|[./])|(^|/)(id_rsa|auth-profiles\.json|[^/]*credentials[^/]*)$|\.(rs|sql|pem|key)$' \
    "$scan/rootfs.list" || path_status=$?
  case "$path_status" in
    0) ;;
    10)
      echo "Hepta runtime rootfs contains forbidden build or credential paths" >&2
      return 10
      ;;
    *)
      echo "Hepta runtime rootfs path scan failed" >&2
      return 20
      ;;
  esac
  tar -xf "$scan/rootfs.tar" -C "$scan/rootfs" || return 20
  cmp "$sbom" "$scan/rootfs/usr/share/doc/hepta-research-league/sbom.cdx.json" || return 20
  local binary="$scan/rootfs/usr/local/bin/hepta-research-league"
  [[ -f "$binary" && ! -L "$binary" && -x "$binary" ]] || {
    echo "Hepta runtime binary is not a regular executable" >&2
    return 20
  }
  local binary_sha
  binary_sha=$(sha256sum "$binary" | cut -d' ' -f1) || return 20
  [[ "$binary_sha" == "$runtime_binary_sha256" ]] || {
    echo "Hepta runtime image binary differs from the SBOM authority" >&2
    return 20
  }
  local content_status=0
  scan_no_matches "rootfs-content-$label" \
    'HEPTA_IMAGE_SENTINEL_DO_NOT_SHIP_7e07109a|HEPTA_GATE_SECRET_DO_NOT_SHIP|replace-with-real-secret|change-me@paper-raid' \
    "$scan/rootfs" || content_status=$?
  case "$content_status" in
    0) ;;
    10)
      echo "Hepta runtime rootfs contains forbidden credential content" >&2
      return 10
      ;;
    *)
      echo "Hepta runtime rootfs content scan failed" >&2
      return 20
      ;;
  esac
  printf '%s\n' "$binary_sha" >"$scan/binary.sha256" || return 20
  sha256sum "$scan/rootfs/usr/share/doc/hepta-research-league/sbom.cdx.json" \
    | cut -d' ' -f1 >"$scan/sbom.sha256" || return 20
  return 0
}

first_scan_status=0
scan_image "$image_id" first || first_scan_status=$?
[[ "$first_scan_status" -eq 0 ]] || {
  echo "primary Hepta image rootfs scan failed with status $first_scan_status" >&2
  exit 1
}
second_scan_status=0
scan_image "$repro_image_id" second || second_scan_status=$?
[[ "$second_scan_status" -eq 0 ]] || {
  echo "reproducibility Hepta image rootfs scan failed with status $second_scan_status" >&2
  exit 1
}
cmp "$release_dir/first/binary.sha256" "$release_dir/second/binary.sha256"
cmp "$release_dir/first/sbom.sha256" "$release_dir/second/sbom.sha256"
cmp "$release_dir/first/rootfs/usr/local/bin/hepta-research-league" \
  "$release_dir/second/rootfs/usr/local/bin/hepta-research-league"
cmp "$release_dir/first/rootfs/usr/share/doc/hepta-research-league/sbom.cdx.json" \
  "$release_dir/second/rootfs/usr/share/doc/hepta-research-league/sbom.cdx.json"

# Prove that the rootfs content scanner rejects an actually poisoned image,
# instead of merely producing a clean report for the expected image.
sentinel_seed_container=$("${docker_command[@]}" create --network none "$image_id")
containers+=("$sentinel_seed_container")
printf '%s\n' "$sentinel" >"$release_dir/$sentinel"
"${docker_command[@]}" cp \
  "$release_dir/$sentinel" \
  "$sentinel_seed_container:/$sentinel"
"${docker_command[@]}" commit "$sentinel_seed_container" "$sentinel_ref" >/dev/null
sentinel_scan_status=0
scan_image "$sentinel_ref" sentinel-negative >/dev/null 2>&1 || sentinel_scan_status=$?
if [[ "$sentinel_scan_status" -ne 10 ]]; then
  echo "Hepta sentinel image expected scanner status 10, got $sentinel_scan_status" >&2
  exit 1
fi

[[ "$("${docker_command[@]}" image inspect "$image_ref" --format '{{.Id}}')" == "$image_id" ]]
[[ "$("${docker_command[@]}" image inspect "$repro_ref" --format '{{.Id}}')" == "$repro_image_id" ]]
verify_source_unchanged

HEPTA_IMAGE="$image_ref" \
HEPTA_EXPECTED_IMAGE_ID="$image_id" \
  bash "$source_context/scripts/check-hepta-research-league-compose-smoke.sh"

[[ "$("${docker_command[@]}" image inspect "$image_ref" --format '{{.Id}}')" == "$image_id" ]]
verify_source_unchanged

jq -n \
  --arg schema hepta.release_image_provenance.v3 \
  --arg image_ref "$image_ref" \
  --arg image_id "$image_id" \
  --arg oci_index_digest "$index_digest" \
  --arg iid "$first_iid_value" \
  --arg source_revision "$revision" \
  --arg source_tree "$source_tree" \
  --arg source_date_epoch "$source_date_epoch" \
  --arg buildx_version "$buildx_version" \
  --arg buildx_sha256 "$buildx_sha256" \
  --arg dockerfile_sha256 "$dockerfile_sha256" \
  --arg cargo_lock_sha256 "$cargo_lock_sha256" \
  --arg rust_toolchain_sha256 "$rust_toolchain_sha256" \
  --arg sbom_sha256 "$sbom_sha256" \
  --arg runtime_binary_sha256 "$runtime_binary_sha256" \
  --arg vendor_manifest_sha256 "$(sha256sum "$source_context/vendor/trnm-chain-vendor-manifest.json" | cut -d' ' -f1)" '
  {
    schema: $schema,
    image_ref: $image_ref,
    image_id: $image_id,
    oci_index_digest: $oci_index_digest,
    iid: $iid,
    source_revision: $source_revision,
    source_tree: $source_tree,
    source_date_epoch: ($source_date_epoch | tonumber),
    buildx: {version: $buildx_version, binary_sha256: $buildx_sha256},
    dockerfile_sha256: $dockerfile_sha256,
    cargo_lock_sha256: $cargo_lock_sha256,
    rust_toolchain_sha256: $rust_toolchain_sha256,
    vendor_manifest_sha256: $vendor_manifest_sha256,
    application_sbom: {
      path: "/usr/share/doc/hepta-research-league/sbom.cdx.json",
      sha256: $sbom_sha256
    },
    runtime_binary: {
      path: "/usr/local/bin/hepta-research-league",
      sha256: $runtime_binary_sha256
    },
    reproducibility: {
      independent_no_cache_builds: 2,
      identical_image_ids: true,
      extracted_binaries_identical: true,
      extracted_sboms_identical: true
    },
    compose_postgres_sigkill_smoke: true
  }'
gate_succeeded=true

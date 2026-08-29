#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

while IFS='=' read -r environment_name _; do
  [[ "$environment_name" == GIT_* ]] && unset "$environment_name"
done < <(env)

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${HEPTA_IMAGE:?immutable HEPTA_IMAGE must be set}"
: "${HEPTA_EXPECTED_IMAGE_ID:?frozen HEPTA_EXPECTED_IMAGE_ID must be set}"
: "${HEPTA_IMAGE_BUILD_STDOUT:?successful immutable image-build stdout file must be set}"
: "${HEPTA_IMAGE_BUILD_STDERR:?successful immutable image-build stderr file must be set}"
: "${HEPTA_RESOURCE_GATE_FIXTURE_DIR:?fresh fixture bundle directory must be set}"
: "${HEPTA_RESOURCE_GATE_EVIDENCE_DIR:?new evidence directory path must be set}"

default_cap=32768
deployment_max=1048576
memory_limit_bytes=536870912
default_max_peak_bytes=402653184
max_peak_bytes=${HEPTA_RESOURCE_GATE_MAX_PEAK_BYTES:-$default_max_peak_bytes}
postgres_image='docker.io/library/postgres:17.6-alpine3.22@sha256:ef257d85f76e48da1c64832459b59fcaba1a4dac97bf5d7450c77753542eee94'
fixture_generator="$repo_dir/scripts/generate-hepta-receipt-v2-resource-fixtures.py"
image_build_admission="$repo_dir/scripts/admit-hepta-image-build-evidence.py"
image_builder_source="$repo_dir/scripts/build-hepta-research-league-image.sh"
clean_source_verifier="$repo_dir/scripts/verify-hepta-clean-source.py"
gate_source="$repo_dir/scripts/check-hepta-receipt-v2-resource-gate.sh"
compose_source="$repo_dir/deploy/hepta-research-league/compose.yaml"
migration_compose_source="$repo_dir/deploy/hepta-research-league/compose.migration.yaml"
dockerfile_source="$repo_dir/services/hepta-research-league/Dockerfile"
cargo_lock_source="$repo_dir/services/hepta-research-league/docker/Cargo.lock"
rust_toolchain_source="$repo_dir/services/hepta-research-league/docker/rust-toolchain.manifest"
sbom_source="$repo_dir/deploy/hepta-research-league/hepta-research-league.cdx.json"
vendor_manifest_source="$repo_dir/vendor/trnm-chain-vendor-manifest.json"
scratch_parent=/var/tmp
umask 077
command -v mktemp >/dev/null 2>&1 && command -v stat >/dev/null 2>&1 || {
  echo "Receipt V2 resource gate requires mktemp and stat" >&2
  exit 1
}
[[ -d "$scratch_parent" && ! -L "$scratch_parent" ]] || {
  echo "fixed resource-gate scratch parent is unavailable or is a symlink" >&2
  exit 2
}
scratch=$(mktemp -d -- "$scratch_parent/hepta-receipt-v2-resource.XXXXXXXXXX")
scratch_identity=
project="hepta-receipt-resource-${$}"
fixture_dir="$scratch/fixture-snapshot"
fixture_snapshot_identity=
default_env="$scratch/default.env"
max_env="$scratch/max.env"
override="$scratch/compose.resource.yaml"
migration_secret_file="$scratch/migration-owner.url"
started=false
holder_pid=
evidence_staging=
evidence_staging_prefix=
evidence_published=false
evidence_parent_fd=
evidence_staging_fd=
evidence_parent_identity=
evidence_staging_identity=
scratch_removed=false

stop_holder() {
  local stopped_pid
  [[ -n "$holder_pid" ]] || return 0
  stopped_pid=$holder_pid
  if kill -0 "$stopped_pid" >/dev/null 2>&1; then
    kill -TERM "$stopped_pid" >/dev/null 2>&1 || return 1
  fi
  wait "$stopped_pid" >/dev/null 2>&1 || true
  if kill -0 "$stopped_pid" >/dev/null 2>&1; then
    echo "resource-gate request holder did not terminate" >&2
    return 1
  fi
  holder_pid=
}

close_evidence_fds() {
  local close_failed=false
  if [[ -n "$evidence_staging_fd" ]]; then
    if ! exec {evidence_staging_fd}<&-; then
      close_failed=true
    fi
    evidence_staging_fd=
  fi
  if [[ -n "$evidence_parent_fd" ]]; then
    if ! exec {evidence_parent_fd}<&-; then
      close_failed=true
    fi
    evidence_parent_fd=
  fi
  [[ "$close_failed" == false ]]
}

remove_unpublished_evidence() {
  [[ -n "$evidence_staging" ]] || return 0
  if [[ -n "$evidence_parent_fd" && -n "$evidence_staging_fd" ]]; then
    python3 - "$evidence_parent_fd" "$evidence_staging_fd" \
      "$evidence_basename" "$(basename -- "$evidence_staging")" \
      "$evidence_parent_identity" "$evidence_staging_identity" <<'PY'
import os
import stat
import sys

parent_fd = int(sys.argv[1])
staging_fd = int(sys.argv[2])
target_name = sys.argv[3]
staging_name = sys.argv[4]
expected_parent = sys.argv[5]
expected_staging = sys.argv[6]


def identity(metadata):
    return (
        f"{metadata.st_dev}:{metadata.st_ino}:{metadata.st_uid}:"
        f"{metadata.st_gid}:{stat.S_IMODE(metadata.st_mode):o}"
    )


if identity(os.fstat(parent_fd)) != expected_parent:
    raise RuntimeError("resource evidence parent descriptor identity changed")
if identity(os.fstat(staging_fd)) != expected_staging:
    raise RuntimeError("resource evidence staging descriptor identity changed")
current_fd = os.open(
    staging_name,
    os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
    dir_fd=parent_fd,
)
try:
    if identity(os.fstat(current_fd)) != expected_staging:
        raise RuntimeError("resource evidence staging path was replaced")
finally:
    os.close(current_fd)
for name in os.listdir(staging_fd):
    metadata = os.stat(name, dir_fd=staging_fd, follow_symlinks=False)
    if stat.S_ISDIR(metadata.st_mode):
        raise RuntimeError(f"refusing to recursively remove unexpected evidence directory: {name}")
    os.unlink(name, dir_fd=staging_fd)
os.fsync(staging_fd)
try:
    os.stat(target_name, dir_fd=parent_fd, follow_symlinks=False)
except FileNotFoundError:
    pass
else:
    raise RuntimeError("resource evidence target appeared before failed-run cleanup")
os.rmdir(staging_name, dir_fd=parent_fd)
os.fsync(parent_fd)
PY
    return
  fi
  if [[ -e "$evidence_staging" || -L "$evidence_staging" ]]; then
    case "$evidence_staging" in
      "$evidence_staging_prefix"*)
        [[ ! -L "$evidence_staging" && -d "$evidence_staging" \
          && -n "$evidence_staging_identity" \
          && $(stat -c '%d:%i:%u:%g:%a' -- "$evidence_staging") == "$evidence_staging_identity" ]] || {
          echo "refusing to clean replaced resource-gate evidence staging path" >&2
          return 1
        }
        rm -rf -- "$evidence_staging"
        ;;
      *)
        echo "refusing to remove unexpected resource-gate evidence staging path" >&2
        return 1
        ;;
    esac
  fi
}

remove_resource_scratch() {
  local scratch_safe=true
  if [[ ! -e "$scratch" && ! -L "$scratch" ]]; then
    scratch_removed=true
    return 0
  fi
  case "$scratch" in
    /var/tmp/hepta-receipt-v2-resource.*)
      if [[ -L "$scratch" || ! -d "$scratch" || -z "$scratch_identity" \
        || $(stat -c '%d:%i' -- "$scratch" 2>/dev/null) != "$scratch_identity" ]]; then
        echo "refusing to clean replaced resource-gate scratch directory" >&2
        scratch_safe=false
      fi
      if [[ -e "$fixture_dir" || -L "$fixture_dir" ]]; then
        if [[ "$fixture_dir" != "$scratch/fixture-snapshot" || -L "$fixture_dir" \
          || ! -d "$fixture_dir" \
          || ( -n "$fixture_snapshot_identity" \
            && $(stat -c '%d:%i' -- "$fixture_dir" 2>/dev/null) != "$fixture_snapshot_identity" ) ]]; then
          echo "refusing to clean replaced resource fixture snapshot" >&2
          scratch_safe=false
        elif ! chmod u+rwx -- "$fixture_dir"; then
          echo "could not restore private snapshot write permission for cleanup" >&2
          scratch_safe=false
        fi
      fi
      if [[ "$scratch_safe" == true ]]; then
        chmod u+rwx -- "$scratch" || return 1
        rm -rf -- "$scratch" || return 1
        [[ ! -e "$scratch" && ! -L "$scratch" ]] || return 1
        scratch_removed=true
        return 0
      fi
      ;;
    *) echo "refusing to remove unexpected resource-gate scratch path" >&2 ;;
  esac
  return 1
}

compose_project_absent() {
  local remaining_containers remaining_networks remaining_volumes
  remaining_containers=$("${docker_command[@]}" ps -aq \
    --filter "label=com.docker.compose.project=$project") || return 1
  remaining_networks=$("${docker_command[@]}" network ls -q \
    --filter "label=com.docker.compose.project=$project") || return 1
  remaining_volumes=$("${docker_command[@]}" volume ls -q \
    --filter "label=com.docker.compose.project=$project") || return 1
  [[ -z "$remaining_containers" && -z "$remaining_networks" && -z "$remaining_volumes" ]]
}

teardown_compose_project() {
  stop_holder || return 1
  if [[ "$started" == true ]]; then
    "${compose_cleanup[@]}" down --volumes --remove-orphans --timeout 10 || return 1
    compose_project_absent || {
      echo "resource-gate Compose containers, networks, or volumes survived teardown" >&2
      return 1
    }
    started=false
  fi
}

cleanup() {
  local original_status=$?
  local cleanup_failed=false
  trap - EXIT
  set +e
  stop_holder || {
    echo "resource-gate cleanup could not stop the request holder" >&2
    cleanup_failed=true
  }
  if [[ "$started" == true ]] && ! teardown_compose_project; then
    echo "resource-gate cleanup could not remove Compose resources" >&2
    cleanup_failed=true
  fi
  if [[ "$evidence_published" != true && -n "$evidence_staging" \
    ]] && ! remove_unpublished_evidence; then
    echo "resource-gate cleanup could not remove unpublished evidence" >&2
    cleanup_failed=true
  fi
  if ! close_evidence_fds; then
    echo "resource-gate cleanup could not close retained evidence descriptors" >&2
    cleanup_failed=true
  fi
  if [[ "$scratch_removed" != true ]] && ! remove_resource_scratch; then
    echo "resource-gate cleanup could not remove private scratch/token material" >&2
    cleanup_failed=true
  fi
  if [[ "$original_status" -eq 0 && "$cleanup_failed" == true ]]; then
    exit 1
  fi
  exit "$original_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
[[ $(stat -c '%a' "$scratch") == 700 ]] || {
  echo "resource-gate scratch directory is not private" >&2
  exit 2
}
scratch_identity=$(stat -c '%d:%i' -- "$scratch")

for command_name in awk basename cmp cp curl cut dirname docker env flock git grep id jq mktemp python3 seq sha256sum sleep sort stat; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Receipt V2 resource gate requires $command_name" >&2
    exit 1
  }
done
git_binary=$(PATH=/usr/bin:/bin command -v git)
git_authority() {
  env -i \
    PATH=/usr/bin:/bin \
    HOME=/nonexistent \
    XDG_CONFIG_HOME=/nonexistent \
    LC_ALL=C \
    GIT_CONFIG_NOSYSTEM=1 \
    GIT_CONFIG_GLOBAL=/dev/null \
    GIT_NO_REPLACE_OBJECTS=1 \
    "$git_binary" --no-replace-objects "$@"
}
release_lock=$(git_authority -C "$repo_dir" rev-parse --path-format=absolute --git-path hepta-release-authority.lock)
exec 9>"$release_lock"
flock -n 9 || {
  echo "another Hepta release-authority process is already running" >&2
  exit 1
}
[[ "$HEPTA_EXPECTED_IMAGE_ID" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "HEPTA_EXPECTED_IMAGE_ID is not a canonical Docker config digest" >&2
  exit 2
}
[[ "$max_peak_bytes" =~ ^[1-9][0-9]*$ \
  && "$max_peak_bytes" -le "$default_max_peak_bytes" ]] || {
  echo "HEPTA_RESOURCE_GATE_MAX_PEAK_BYTES may only tighten the 384 MiB ceiling" >&2
  exit 2
}
release_revision=$(git_authority -C "$repo_dir" rev-parse HEAD)
release_tree=$(git_authority -C "$repo_dir" rev-parse 'HEAD^{tree}')
release_source_date_epoch=$(git_authority -C "$repo_dir" show -s --format=%ct "$release_revision")
[[ "$release_revision" =~ ^[0-9a-f]{40}$ \
  && "$release_tree" =~ ^[0-9a-f]{40}$ \
  && "$release_source_date_epoch" =~ ^[0-9]+$ \
  && -z $(git_authority -C "$repo_dir" status --porcelain=v1 --untracked-files=all) ]] || {
  echo "resource evidence requires one canonical clean Git source identity" >&2
  exit 2
}

committed_input_sha256() {
  git_authority -C "$repo_dir" show "$release_revision:$1" | sha256sum | cut -d' ' -f1
}

clean_source_verifier_sha256=$(sha256sum "$clean_source_verifier" | cut -d' ' -f1)
[[ "$clean_source_verifier_sha256" == \
  "$(committed_input_sha256 scripts/verify-hepta-clean-source.py)" ]] || {
  echo "resource evidence clean-source verifier differs from its committed blob" >&2
  exit 2
}
source_authority=$(python3 "$clean_source_verifier" \
  --repo-dir "$repo_dir" --revision "$release_revision" --tree "$release_tree")
jq -e --arg revision "$release_revision" --arg tree "$release_tree" '
  .revision == $revision and .tree == $tree and .tracked_files > 0
' <<<"$source_authority" >/dev/null
release_tracked_files=$(jq -er '.tracked_files' <<<"$source_authority")

dockerfile_sha256=$(sha256sum "$dockerfile_source" | cut -d' ' -f1)
cargo_lock_sha256=$(sha256sum "$cargo_lock_source" | cut -d' ' -f1)
rust_toolchain_sha256=$(sha256sum "$rust_toolchain_source" | cut -d' ' -f1)
sbom_sha256=$(sha256sum "$sbom_source" | cut -d' ' -f1)
vendor_manifest_sha256=$(sha256sum "$vendor_manifest_source" | cut -d' ' -f1)
[[ "$dockerfile_sha256" == \
    "$(committed_input_sha256 services/hepta-research-league/Dockerfile)" \
  && "$cargo_lock_sha256" == \
    "$(committed_input_sha256 services/hepta-research-league/docker/Cargo.lock)" \
  && "$rust_toolchain_sha256" == \
    "$(committed_input_sha256 services/hepta-research-league/docker/rust-toolchain.manifest)" \
  && "$sbom_sha256" == \
    "$(committed_input_sha256 deploy/hepta-research-league/hepta-research-league.cdx.json)" \
  && "$vendor_manifest_sha256" == \
    "$(committed_input_sha256 vendor/trnm-chain-vendor-manifest.json)" ]] || {
  echo "resource evidence release input differs from its committed blob" >&2
  exit 2
}
runtime_binary_sha256=$(jq -er '
  [.components[]? | select(.type == "file")] as $files
  | if (($files | length) == 1
      and $files[0].name == "/usr/local/bin/hepta-research-league"
      and ($files[0].hashes | length) == 1
      and $files[0].hashes[0].alg == "SHA-256")
    then $files[0].hashes[0].content
    else error("SBOM must bind exactly one Hepta runtime file")
    end
' "$sbom_source")
[[ "$runtime_binary_sha256" =~ ^[0-9a-f]{64}$ ]] || {
  echo "resource evidence SBOM runtime binary hash is not canonical" >&2
  exit 2
}

gate_source_sha256=$(sha256sum "$gate_source" | cut -d' ' -f1)
generator_source_sha256=$(sha256sum "$fixture_generator" | cut -d' ' -f1)
image_build_admission_source_sha256=$(sha256sum "$image_build_admission" | cut -d' ' -f1)
image_builder_source_sha256=$(sha256sum "$image_builder_source" | cut -d' ' -f1)
compose_source_sha256=$(sha256sum "$compose_source" | cut -d' ' -f1)
migration_compose_source_sha256=$(sha256sum "$migration_compose_source" | cut -d' ' -f1)
[[ "$gate_source_sha256" == \
    "$(committed_input_sha256 scripts/check-hepta-receipt-v2-resource-gate.sh)" \
  && "$generator_source_sha256" == \
    "$(committed_input_sha256 scripts/generate-hepta-receipt-v2-resource-fixtures.py)" \
  && "$image_build_admission_source_sha256" == \
    "$(committed_input_sha256 scripts/admit-hepta-image-build-evidence.py)" \
  && "$image_builder_source_sha256" == \
    "$(committed_input_sha256 scripts/build-hepta-research-league-image.sh)" \
  && "$clean_source_verifier_sha256" == \
    "$(committed_input_sha256 scripts/verify-hepta-clean-source.py)" \
  && "$compose_source_sha256" == \
    "$(committed_input_sha256 deploy/hepta-research-league/compose.yaml)" \
  && "$migration_compose_source_sha256" == \
    "$(committed_input_sha256 deploy/hepta-research-league/compose.migration.yaml)" ]] || {
  echo "resource evidence execution authority differs from its committed blob" >&2
  exit 2
}

verify_release_inputs_unchanged() {
  [[ $(git_authority -C "$repo_dir" rev-parse HEAD) == "$release_revision" \
    && $(git_authority -C "$repo_dir" rev-parse 'HEAD^{tree}') == "$release_tree" \
    && -z $(git_authority -C "$repo_dir" status --porcelain=v1 --untracked-files=all) \
    && $(sha256sum "$gate_source" | cut -d' ' -f1) == "$gate_source_sha256" \
    && $(sha256sum "$fixture_generator" | cut -d' ' -f1) == "$generator_source_sha256" \
    && $(sha256sum "$image_build_admission" | cut -d' ' -f1) == "$image_build_admission_source_sha256" \
    && $(sha256sum "$image_builder_source" | cut -d' ' -f1) == "$image_builder_source_sha256" \
    && $(sha256sum "$clean_source_verifier" | cut -d' ' -f1) == "$clean_source_verifier_sha256" \
    && $(sha256sum "$compose_source" | cut -d' ' -f1) == "$compose_source_sha256" \
    && $(sha256sum "$migration_compose_source" | cut -d' ' -f1) == "$migration_compose_source_sha256" \
    && $(sha256sum "$dockerfile_source" | cut -d' ' -f1) == "$dockerfile_sha256" \
    && $(sha256sum "$cargo_lock_source" | cut -d' ' -f1) == "$cargo_lock_sha256" \
    && $(sha256sum "$rust_toolchain_source" | cut -d' ' -f1) == "$rust_toolchain_sha256" \
    && $(sha256sum "$sbom_source" | cut -d' ' -f1) == "$sbom_sha256" \
    && $(sha256sum "$vendor_manifest_source" | cut -d' ' -f1) == "$vendor_manifest_sha256" ]] || {
    echo "resource-gate source authority changed while the gate was running" >&2
    return 1
  }
}

# Snapshot the caller-owned bundle through O_NOFOLLOW directory/file walks.
# Every subsequent verification and HTTP request consumes only this private,
# read-only snapshot; later changes to the external bundle are irrelevant.
snapshot_result=$($fixture_generator \
  --snapshot-bundle "$HEPTA_RESOURCE_GATE_FIXTURE_DIR" \
  --output "$fixture_dir")
fixture_snapshot_identity=$(stat -c '%d:%i' -- "$fixture_dir")
fixture_manifest=$(jq -cer '.manifest' <<<"$snapshot_result")
[[ $(jq -r '.default_cap_bytes' <<<"$fixture_manifest") == "$default_cap" ]]
[[ $(jq -r '.deployment_max_bytes' <<<"$fixture_manifest") == "$deployment_max" ]]
anchor_hash=$(jq -er '.anchor_hash_hex' <<<"$fixture_manifest")
paper_id=$(jq -er '.paper_id' <<<"$fixture_manifest")
legal_receipt_bytes=$(jq -er '.legal_receipt_bytes' <<<"$fixture_manifest")
legal_utilization_bps=$(jq -er '.legal_utilization_bps' <<<"$fixture_manifest")
[[ "$legal_receipt_bytes" -le "$default_cap" && "$legal_utilization_bps" -ge 4000 ]]
snapshot_manifest_sha256=$(sha256sum "$fixture_dir/manifest.json" | cut -d' ' -f1)
snapshot_tree_sha256=$(
  cd "$fixture_dir"
  sha256sum \
    canonical-shape-adversarial.json default-plus-one.body legal-receipt-v2.json \
    manifest.json max-plus-one.body trust-anchor.json | sha256sum | cut -d' ' -f1
)

verify_snapshot_unchanged() {
  local current current_manifest_sha current_tree_sha
  current=$($fixture_generator --verify-bundle "$fixture_dir")
  [[ "$current" == "$fixture_manifest" ]] || {
    echo "private resource fixture snapshot changed after admission" >&2
    return 1
  }
  current_manifest_sha=$(sha256sum "$fixture_dir/manifest.json" | cut -d' ' -f1)
  current_tree_sha=$(
    cd "$fixture_dir"
    sha256sum \
      canonical-shape-adversarial.json default-plus-one.body legal-receipt-v2.json \
      manifest.json max-plus-one.body trust-anchor.json | sha256sum | cut -d' ' -f1
  )
  [[ "$current_manifest_sha" == "$snapshot_manifest_sha256" \
    && "$current_tree_sha" == "$snapshot_tree_sha256" ]]
}

evidence_request=$HEPTA_RESOURCE_GATE_EVIDENCE_DIR
[[ "$evidence_request" == /* && "$evidence_request" != */ \
  && "$evidence_request" != *'/../'* && "$evidence_request" != *'/./'* ]] || {
  echo "resource evidence target must be an absolute normalized directory path" >&2
  exit 2
}
evidence_parent_request=$(dirname -- "$evidence_request")
evidence_basename=$(basename -- "$evidence_request")
[[ "$evidence_basename" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || {
  echo "resource evidence directory basename is not safe" >&2
  exit 2
}
[[ -d "$evidence_parent_request" && ! -L "$evidence_parent_request" ]] || {
  echo "resource evidence parent must be a real directory" >&2
  exit 2
}
evidence_parent=$(cd "$evidence_parent_request" && pwd -P)
[[ "$evidence_parent" == "$evidence_parent_request" ]] || {
  echo "resource evidence parent must not traverse symlinks or aliases" >&2
  exit 2
}
evidence_target="$evidence_parent/$evidence_basename"
evidence_staging_prefix="$evidence_parent/.${evidence_basename}.staging."
evidence_staging="${evidence_staging_prefix}${$}.${RANDOM}"
evidence_admission=$(python3 - "$evidence_parent" "$evidence_basename" \
  "$(basename -- "$evidence_staging")" "$(id -u)" <<'PY'
import json
import os
import stat
import sys

parent_path, target_name, staging_name, expected_uid_raw = sys.argv[1:]
expected_uid = int(expected_uid_raw)
flags = os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW
parent_fd = os.open(parent_path, flags)
created = False
try:
    parent = os.fstat(parent_fd)
    parent_mode = stat.S_IMODE(parent.st_mode)
    if not stat.S_ISDIR(parent.st_mode) or parent.st_uid != expected_uid:
        raise RuntimeError("resource evidence parent must be owned by the effective user")
    if parent_mode & 0o022:
        raise RuntimeError("resource evidence parent must not be group/world writable")
    for name in (target_name, staging_name):
        try:
            os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
        except FileNotFoundError:
            continue
        raise RuntimeError(f"resource evidence path already exists: {name}")
    os.mkdir(staging_name, mode=0o700, dir_fd=parent_fd)
    created = True
    staging_fd = os.open(staging_name, flags, dir_fd=parent_fd)
    try:
        staging = os.fstat(staging_fd)
        if (
            not stat.S_ISDIR(staging.st_mode)
            or staging.st_uid != expected_uid
            or stat.S_IMODE(staging.st_mode) != 0o700
        ):
            raise RuntimeError("resource evidence staging ownership or mode is unsafe")

        def identity(metadata):
            return (
                f"{metadata.st_dev}:{metadata.st_ino}:{metadata.st_uid}:"
                f"{metadata.st_gid}:{stat.S_IMODE(metadata.st_mode):o}"
            )

        print(
            json.dumps(
                {
                    "parent_identity": identity(parent),
                    "staging_identity": identity(staging),
                },
                sort_keys=True,
                separators=(",", ":"),
            )
        )
    finally:
        os.close(staging_fd)
    os.fsync(parent_fd)
except Exception:
    if created:
        try:
            os.rmdir(staging_name, dir_fd=parent_fd)
            os.fsync(parent_fd)
        except OSError:
            pass
    raise
finally:
    os.close(parent_fd)
PY
)
evidence_parent_identity=$(jq -er '.parent_identity' <<<"$evidence_admission")
evidence_staging_identity=$(jq -er '.staging_identity' <<<"$evidence_admission")
exec {evidence_parent_fd}<"$evidence_parent"
exec {evidence_staging_fd}<"$evidence_staging"
[[ $(stat -Lc '%d:%i:%u:%g:%a' -- "/proc/$$/fd/$evidence_parent_fd") == "$evidence_parent_identity" \
  && $(stat -Lc '%d:%i:%u:%g:%a' -- "/proc/$$/fd/$evidence_staging_fd") == "$evidence_staging_identity" ]] || {
  echo "resource evidence parent or staging changed during admission" >&2
  exit 2
}
# Every artifact write resolves through the retained staging descriptor. A path
# replacement can therefore only make final publication fail; it cannot divert
# bytes into a replacement directory and later pass the inode checks.
evidence_dir="/proc/$$/fd/$evidence_staging_fd"

verify_release_inputs_unchanged
image_build_snapshot=$(python3 "$image_build_admission" \
  --stdout "$HEPTA_IMAGE_BUILD_STDOUT" \
  --stderr "$HEPTA_IMAGE_BUILD_STDERR" \
  --repo-dir "$repo_dir" \
  --staging-fd "$evidence_staging_fd" \
  --image-ref "$HEPTA_IMAGE" \
  --image-id "$HEPTA_EXPECTED_IMAGE_ID" \
  --source-revision "$release_revision" \
  --source-tree "$release_tree" \
  --source-date-epoch "$release_source_date_epoch" \
  --dockerfile-sha256 "$dockerfile_sha256" \
  --cargo-lock-sha256 "$cargo_lock_sha256" \
  --rust-toolchain-sha256 "$rust_toolchain_sha256" \
  --sbom-sha256 "$sbom_sha256" \
  --vendor-manifest-sha256 "$vendor_manifest_sha256" \
  --runtime-binary-sha256 "$runtime_binary_sha256")
jq -e '
  (keys == [
    "provenance",
    "provenance_sha256",
    "stderr_identity",
    "stderr_sha256",
    "stdout_identity",
    "stdout_sha256"
  ])
  and (.stdout_sha256 | test("^[0-9a-f]{64}$"))
  and (.stderr_sha256 | test("^[0-9a-f]{64}$"))
  and (.provenance_sha256 | test("^[0-9a-f]{64}$"))
' <<<"$image_build_snapshot" >/dev/null
image_build_stdout_identity=$(jq -er '.stdout_identity' <<<"$image_build_snapshot")
image_build_stderr_identity=$(jq -er '.stderr_identity' <<<"$image_build_snapshot")
image_build_stdout_sha256=$(jq -er '.stdout_sha256' <<<"$image_build_snapshot")
image_build_stderr_sha256=$(jq -er '.stderr_sha256' <<<"$image_build_snapshot")
image_provenance_sha256=$(jq -er '.provenance_sha256' <<<"$image_build_snapshot")

docker_command=(docker)
if ! docker info >/dev/null 2>&1; then
  docker_command=(sudo -n docker)
fi
"${docker_command[@]}" info >/dev/null
"${docker_command[@]}" image inspect "$postgres_image" >/dev/null
frozen_image_id=$("${docker_command[@]}" image inspect "$HEPTA_IMAGE" --format '{{.Id}}')
[[ "$frozen_image_id" == "$HEPTA_EXPECTED_IMAGE_ID" ]] || {
  echo "Hepta image tag does not resolve to the frozen image ID" >&2
  exit 1
}
"${docker_command[@]}" image inspect "$HEPTA_EXPECTED_IMAGE_ID" | jq -S . \
  >"$evidence_dir/hepta-image-inspect.json"
jq -e \
  --arg image_id "$HEPTA_EXPECTED_IMAGE_ID" \
  --arg revision "$release_revision" \
  --arg source_tree "$release_tree" \
  --arg source_date_epoch "$release_source_date_epoch" \
  --arg sbom_sha256 "$sbom_sha256" \
  --arg runtime_binary_sha256 "$runtime_binary_sha256" \
  --arg cargo_lock_sha256 "$cargo_lock_sha256" \
  --arg dockerfile_sha256 "$dockerfile_sha256" \
  --arg rust_toolchain_sha256 "$rust_toolchain_sha256" \
  --slurpfile image_provenance "$evidence_dir/image-provenance.json" '
  length == 1
  and .[0].Id == $image_id
  and .[0].Config.Labels["org.opencontainers.image.revision"] == $revision
  and .[0].Config.Labels["org.opencontainers.image.source"] == "https://github.com/TrillionniumFoundation/CEX.git"
  and .[0].Config.Labels["org.trillionnium.source.tree"] == $source_tree
  and .[0].Config.Labels["org.trillionnium.sbom.sha256"] == $sbom_sha256
  and .[0].Config.Labels["org.trillionnium.cargo-lock.sha256"] == $cargo_lock_sha256
  and .[0].Config.Labels["org.trillionnium.dockerfile.sha256"] == $dockerfile_sha256
  and .[0].Config.Labels["org.trillionnium.rust-toolchain.sha256"] == $rust_toolchain_sha256
  and .[0].Config.Labels["io.trillionnium.hepta.source-date-epoch"] == $source_date_epoch
  and .[0].Config.Labels["io.trillionnium.hepta.source-tree"] == $source_tree
  and .[0].Config.Labels["io.trillionnium.hepta.application-sbom.path"] == "/usr/share/doc/hepta-research-league/sbom.cdx.json"
  and .[0].Config.Labels["io.trillionnium.hepta.application-sbom.sha256"] == $sbom_sha256
  and .[0].Config.Labels["io.trillionnium.hepta.runtime-binary.sha256"] == $runtime_binary_sha256
  and .[0].Config.Labels["io.trillionnium.hepta.runtime-base"] == "gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98"
  and .[0].Config.Labels["io.trillionnium.hepta.builder-base"] == "docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb"
  and ($image_provenance | length) == 1
  and $image_provenance[0].image_id == .[0].Id
  and $image_provenance[0].source_revision == .[0].Config.Labels["org.opencontainers.image.revision"]
  and $image_provenance[0].source_tree == .[0].Config.Labels["org.trillionnium.source.tree"]
  and $image_provenance[0].application_sbom.sha256 == .[0].Config.Labels["org.trillionnium.sbom.sha256"]
  and $image_provenance[0].cargo_lock_sha256 == .[0].Config.Labels["org.trillionnium.cargo-lock.sha256"]
  and $image_provenance[0].dockerfile_sha256 == .[0].Config.Labels["org.trillionnium.dockerfile.sha256"]
  and $image_provenance[0].rust_toolchain_sha256 == .[0].Config.Labels["org.trillionnium.rust-toolchain.sha256"]
  and ($image_provenance[0].source_date_epoch | tostring) == .[0].Config.Labels["io.trillionnium.hepta.source-date-epoch"]
  and $image_provenance[0].runtime_binary.sha256 == .[0].Config.Labels["io.trillionnium.hepta.runtime-binary.sha256"]
' "$evidence_dir/hepta-image-inspect.json" >/dev/null
image_inspect_sha256=$(sha256sum "$evidence_dir/hepta-image-inspect.json" | cut -d' ' -f1)

host_port=$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)

# The default phase intentionally omits HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES:
# Compose itself must supply the canonical 32 KiB default. The max phase adds
# one explicit 1 MiB override while keeping every other byte identical.
printf '%s\n' \
  'postgres://hepta_resource_migrator:hepta_resource_migrator_password@postgres:5432/hepta_resource' \
  >"$migration_secret_file"
chmod 0444 "$migration_secret_file"
cat >"$default_env" <<EOF
HEPTA_IMAGE=$HEPTA_IMAGE
HEPTA_HOST_PORT=$host_port
HEPTA_DATABASE_URL=postgres://hepta_resource_runtime:hepta_resource_runtime_password@postgres:5432/hepta_resource
HEPTA_FINALITY_DATABASE_URL=postgres://hepta_resource_finality:hepta_resource_finality_password@postgres:5432/hepta_resource
HEPTA_MIGRATION_DATABASE_URL_FILE=$migration_secret_file
HEPTA_RUNTIME_DATABASE_ROLE=hepta_resource_runtime
HEPTA_FINALITY_DATABASE_ROLE=hepta_resource_finality
HEPTA_OPERATOR_TOKEN=hepta-resource-operator-token
HEPTA_NAKAMA_TOKEN=hepta-resource-nakama-token
HEPTA_NAKAMA_AUTHORIZATION_ISSUER_KEY_ID=hepta-resource-authorization-v1
HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64=nWGxne/9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A=
HEPTA_NAKAMA_CONTROL_ISSUER_KEY_ID=hepta-resource-control-v1
HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64=TM0Imyj/ltqdtsNG7BFOD1uKMZ81q6Yk2oz27U+4pvs=
HEPTA_NAKAMA_BASE_URL=http://127.0.0.1:7350
HEPTA_NAKAMA_RUNTIME_HTTP_KEY=hepta-resource-runtime-http-key
HEPTA_CONSUMER_EDGE_ISSUER=hepta-resource-consumer
HEPTA_CONSUMER_EDGE_AUDIENCE=hepta-resource-api
HEPTA_CONSUMER_EDGE_ISSUER_KEY_ID=hepta-resource-consumer-v1
HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64=J4EX/BRMcjQPZ9DyMW6Dhs7/vyskKMnFH+98WX8dQm4=
HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEYS_JSON={"hepta-resource-consumer-v1":"J4EX/BRMcjQPZ9DyMW6Dhs7/vyskKMnFH+98WX8dQm4="}
TRNM_NAKAMA_AUTHORITY_KEY_ID=hepta-resource-nakama-authority-v1
TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64=/FHNjmIYoaONpH7QAjDwWAgW7RO6MwOsXeuRFUiQgCU=
TRNM_NAKAMA_AUTHORITY_PUBLIC_KEYS_JSON={"hepta-resource-nakama-authority-v1":"/FHNjmIYoaONpH7QAjDwWAgW7RO6MwOsXeuRFUiQgCU="}
HEPTA_TRNM_TOKEN=hepta-resource-trnm-token
HEPTA_FINALITY_MODE=verified
HEPTA_TRNM_VALIDATOR_SETS_JSON=[]
HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON=["$anchor_hash"]
HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT=1
EOF
cp -- "$default_env" "$max_env"
printf 'HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES=%s\n' "$deployment_max" >>"$max_env"

cat >"$override" <<EOF
services:
  postgres:
    image: $postgres_image
    pull_policy: never
    environment:
      POSTGRES_USER: hepta_resource_migrator
      POSTGRES_PASSWORD: hepta_resource_migrator_password
      POSTGRES_DB: hepta_resource
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U hepta_resource_migrator -d hepta_resource"]
      interval: 2s
      timeout: 2s
      retries: 30
    volumes:
      - pgdata:/var/lib/postgresql/data
    security_opt:
      - no-new-privileges:true
  hepta:
    pull_policy: never
    environment:
      HEPTA_DATABASE_URL: postgres://hepta_resource_runtime:hepta_resource_runtime_password@postgres:5432/hepta_resource
      HEPTA_FINALITY_DATABASE_URL: postgres://hepta_resource_finality:hepta_resource_finality_password@postgres:5432/hepta_resource
volumes:
  pgdata: {}
EOF

if [[ ${docker_command[0]} == sudo ]]; then
  compose_default=(sudo -n docker compose --project-name "$project" --env-file "$default_env" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" --file "$override")
  compose_max=(sudo -n docker compose --project-name "$project" --env-file "$max_env" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" --file "$override")
  migration_compose=(sudo -n docker compose --project-name "$project" --env-file "$default_env" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" \
    --file "$repo_dir/deploy/hepta-research-league/compose.migration.yaml" \
    --file "$override")
else
  compose_default=(docker compose --project-name "$project" --env-file "$default_env" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" --file "$override")
  compose_max=(docker compose --project-name "$project" --env-file "$max_env" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" --file "$override")
  migration_compose=(docker compose --project-name "$project" --env-file "$default_env" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" \
    --file "$repo_dir/deploy/hepta-research-league/compose.migration.yaml" \
    --file "$override")
fi
compose_cleanup=("${compose_max[@]}")

configured_default=$("${compose_default[@]}" config --format json)
configured_max=$("${compose_max[@]}" config --format json)
configured_migration=$("${migration_compose[@]}" --profile migration config --format json)
[[ $(jq -er '.services.hepta.image' <<<"$configured_default") == "$HEPTA_IMAGE" ]]
[[ $(jq -er '.services.hepta.environment.HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES' <<<"$configured_default") == "$default_cap" ]]
[[ $(jq -er '.services.hepta.environment.HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES' <<<"$configured_max") == "$deployment_max" ]]
[[ $(jq -er '.services.hepta.environment.HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT' <<<"$configured_default") == 1 ]]
[[ $(jq -er '.services.hepta.environment.HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT' <<<"$configured_max") == 1 ]]
[[ $(jq -r '.services.hepta.environment |
  (has("HEPTA_MIGRATION_DATABASE_URL") or has("HEPTA_MIGRATION_DATABASE_URL_FILE"))' \
  <<<"$configured_default") == false ]]
[[ $(jq -r '.services.hepta.environment.HEPTA_FINALITY_DATABASE_URL' <<<"$configured_default") == \
  'postgres://hepta_resource_finality:hepta_resource_finality_password@postgres:5432/hepta_resource' ]]
jq -e --arg secret_file "$migration_secret_file" '
  .services["hepta-migrate"].image == .services.hepta.image
  and .services["hepta-migrate"].restart == "no"
  and .services["hepta-migrate"].profiles == ["migration"]
  and .services["hepta-migrate"].command == ["--migrate"]
  and (.services["hepta-migrate"].environment | keys) == [
    "HEPTA_FINALITY_DATABASE_ROLE",
    "HEPTA_MIGRATION_DATABASE_URL_FILE",
    "HEPTA_RUNTIME_DATABASE_ROLE"
  ]
  and .services["hepta-migrate"].environment.HEPTA_MIGRATION_DATABASE_URL_FILE == "/run/secrets/hepta_migration_database_url"
  and .secrets.hepta_migration_database_url.file == $secret_file
' <<<"$configured_migration" >/dev/null
[[ $(jq -r '.services | has("hepta-migrate")' <<<"$configured_default") == false ]]
[[ $(jq -r 'has("secrets")' <<<"$configured_default") == false ]]
if grep -Fq 'postgres://hepta_resource_migrator:' <<<"$configured_migration"; then
  echo "rendered resource-gate Compose configuration exposed the migration-owner URL" >&2
  exit 1
fi
"${compose_default[@]}" config --quiet
"${compose_max[@]}" config --quiet
"${migration_compose[@]}" --profile migration config --quiet
printf '%s\n' "$configured_default" | jq -S . \
  >"$evidence_dir/compose-default-rendered.json"
printf '%s\n' "$configured_max" | jq -S . \
  >"$evidence_dir/compose-max-rendered.json"
compose_default_sha256=$(sha256sum "$evidence_dir/compose-default-rendered.json" | cut -d' ' -f1)
compose_max_sha256=$(sha256sum "$evidence_dir/compose-max-rendered.json" | cut -d' ' -f1)

compose=("${compose_default[@]}")

wait_postgres() {
  local attempt
  for attempt in $(seq 1 60); do
    if "${compose[@]}" exec -T postgres pg_isready -U hepta_resource_migrator -d hepta_resource >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "resource-gate PostgreSQL did not become ready" >&2
  return 1
}

wait_hepta() {
  local attempt
  for attempt in $(seq 1 60); do
    if "${compose[@]}" exec -T hepta /usr/local/bin/hepta-research-league --probe-ready >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "resource-gate Hepta did not become ready" >&2
  return 1
}

assert_ready_cap() {
  local expected=$1 output=$2
  curl --silent --show-error --fail --max-time 5 \
    "http://127.0.0.1:$host_port/ready" | jq -S . >"$output"
  jq -e \
    --argjson expected "$expected" \
    '.ready == true
      and .finality_mode == "verified"
      and .trnm_receipt_v2_max_body_bytes == $expected
      and .trnm_receipt_v2_max_in_flight == 1
      and .paper_chain_finality_v2_command_lane == "awaiting_chain_verifier_upgrade"
      and .paper_scientific_finality_policy == "hepta.paper_raid.scientific_finality_policy.v1"
      and .paper_no_appeal_window_seconds == 86400' "$output" >/dev/null
}

capture_db_snapshot() {
  local label=$1
  local state_raw="$scratch/$label.league-state.raw.json"
  local counts_raw="$scratch/$label.db-counts.raw.json"
  local rows_raw="$scratch/$label.db-rows.raw.json"
  local sequences_raw="$scratch/$label.db-sequences.raw.json"
  [[ "$label" =~ ^[a-z0-9-]+$ ]]
  "${compose[@]}" exec -T postgres psql -X -A -t \
    -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 -c \
    "select coalesce(
       jsonb_agg(to_jsonb(hepta_league_state) order by state_key),
       '[]'::jsonb
     )::text
     from hepta_league_state;" >"$state_raw"
  jq -S . "$state_raw" >"$evidence_dir/$label-league-state.json"

  "${compose[@]}" exec -T postgres psql -X -A -t \
    -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 -c \
    "select jsonb_build_object(
       'league_state_rows', (select count(*) from hepta_league_state),
       'league_revision_sum', (select coalesce(sum(revision), 0) from hepta_league_state),
       'outbox_rows', (select count(*) from hepta_outbox),
       'inbox_rows', (select count(*) from hepta_inbox),
       'module_receipt_rows', (select count(*) from hepta_module_receipts),
       'paper_room_event_rows', (select count(*) from hepta_paper_room_events),
       'nakama_control_command_rows', (select count(*) from hepta_nakama_research_control_commands),
       'trust_anchor_rows', (select count(*) from hepta_trnm_cometbft_trust_anchors),
       'finality_inbox_rows', (select count(*) from hepta_paper_chain_finality_inbox),
       'chain_receipt_rows', (select count(*) from hepta_paper_chain_receipts),
       'finality_projection_rows', (select count(*) from hepta_paper_chain_finality_projections),
       'chain_time_checkpoint_rows', (select count(*) from hepta_trnm_cometbft_time_checkpoints_v1),
       'finality_v2_window_arm_rows', (select count(*) from hepta_paper_chain_finality_window_arms_v2),
       'finality_v2_preparation_rows', (select count(*) from hepta_paper_chain_finality_preparations_v2)
     )::text;" >"$counts_raw"
  jq -S . "$counts_raw" >"$evidence_dir/$label-db-counts.json"

  "${compose[@]}" exec -T postgres psql -X -A -t \
    -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 -c \
    "select jsonb_build_object(
       'hepta_league_state', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by state_key)
          from hepta_league_state row_value), '[]'::jsonb),
       'hepta_outbox', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by event_id)
          from hepta_outbox row_value), '[]'::jsonb),
       'hepta_inbox', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by consumer, event_id)
          from hepta_inbox row_value), '[]'::jsonb),
       'hepta_module_receipts', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by receipt_id)
          from hepta_module_receipts row_value), '[]'::jsonb),
       'hepta_paper_room_events', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by cursor)
          from hepta_paper_room_events row_value), '[]'::jsonb),
       'hepta_nakama_research_control_commands', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by command_id)
          from hepta_nakama_research_control_commands row_value), '[]'::jsonb),
       'hepta_trnm_cometbft_trust_anchors', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by anchor_hash)
          from hepta_trnm_cometbft_trust_anchors row_value), '[]'::jsonb),
       'hepta_paper_chain_finality_inbox', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by receipt_hash)
          from hepta_paper_chain_finality_inbox row_value), '[]'::jsonb),
       'hepta_paper_chain_receipts', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by receipt_hash)
          from hepta_paper_chain_receipts row_value), '[]'::jsonb),
       'hepta_paper_chain_finality_projections', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by local_command_id)
          from hepta_paper_chain_finality_projections row_value), '[]'::jsonb),
       'hepta_trnm_cometbft_time_checkpoints_v1', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by checkpoint_hash)
          from hepta_trnm_cometbft_time_checkpoints_v1 row_value), '[]'::jsonb),
       'hepta_paper_chain_finality_window_arms_v2', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by arm_id)
          from hepta_paper_chain_finality_window_arms_v2 row_value), '[]'::jsonb),
       'hepta_paper_chain_finality_preparations_v2', coalesce(
         (select jsonb_agg(to_jsonb(row_value) order by preparation_id)
          from hepta_paper_chain_finality_preparations_v2 row_value), '[]'::jsonb)
     )::text;" >"$rows_raw"
  jq -S . "$rows_raw" >"$evidence_dir/$label-db-rows.json"

  # Sequence state is outside MVCC row rollback. A rejected request that calls
  # nextval() must therefore fail this invariant even when every table row is
  # byte-identical to the baseline.
  "${compose[@]}" exec -T postgres psql -X -A -t \
    -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 -c \
    "select jsonb_build_object(
       'hepta_paper_room_events_cursor_seq', (
         select jsonb_build_object('last_value', last_value, 'is_called', is_called)
         from hepta_paper_room_events_cursor_seq
       )
     )::text;" >"$sequences_raw"
  jq -S . "$sequences_raw" >"$evidence_dir/$label-db-sequences.json"
}

assert_db_unchanged() {
  local label=$1
  capture_db_snapshot "$label"
  cmp -s "$evidence_dir/db-baseline-league-state.json" \
    "$evidence_dir/$label-league-state.json" || {
    echo "resource probe changed Hepta league revision/state: $label" >&2
    return 1
  }
  cmp -s "$evidence_dir/db-baseline-counts.json" \
    "$evidence_dir/$label-db-counts.json" || {
    echo "resource probe changed Hepta outbox/event/command/finality rows: $label" >&2
    return 1
  }
  cmp -s "$evidence_dir/db-baseline-db-rows.json" \
    "$evidence_dir/$label-db-rows.json" || {
    echo "resource probe changed a protected database row: $label" >&2
    return 1
  }
  cmp -s "$evidence_dir/db-baseline-db-sequences.json" \
    "$evidence_dir/$label-db-sequences.json" || {
    echo "resource probe advanced a protected database sequence: $label" >&2
    return 1
  }
}

resolve_memory_peak_file() {
  local container_pid=$1
  local relative
  relative=$(awk -F: '$1=="0" {print $3}' "/proc/$container_pid/cgroup")
  [[ -n "$relative" && "$relative" == /* \
    && "$relative" != *'/../'* && "$relative" != */.. \
    && "$relative" != *'/./'* && "$relative" != */. ]]
  printf '/sys/fs/cgroup%s/memory.peak\n' "${relative%/}"
}

read_memory_peak() {
  local peak_file=$1
  local peak
  if [[ -r "$peak_file" ]]; then
    read -r peak <"$peak_file"
  else
    peak=$(sudo -n -- cat "$peak_file")
  fi
  printf '%s\n' "$peak"
}

assert_container_healthy() {
  local container_id=$1
  [[ $("${docker_command[@]}" inspect "$container_id" --format '{{.State.Running}}') == true ]]
  [[ $("${docker_command[@]}" inspect "$container_id" --format '{{.State.OOMKilled}}') == false ]]
  [[ $("${docker_command[@]}" inspect "$container_id" --format '{{.RestartCount}}') == 0 ]]
  "${compose[@]}" exec -T hepta /usr/local/bin/hepta-research-league --probe-ready >/dev/null
}

post_receipt() {
  local input=$1 output=$2
  curl --silent --show-error --max-time 90 \
    --header 'expect:' \
    --header 'x-hepta-trnm-token: hepta-resource-trnm-token' \
    --header "x-hepta-trnm-trust-anchor-hash: $anchor_hash" \
    --header 'content-type: application/json' \
    --data-binary "@$input" \
    --output "$output" --write-out '%{http_code}' \
    "http://127.0.0.1:$host_port/v2/hepta/papers/$paper_id/chain-finality"
}

# Phase 1: canonical Compose default (32 KiB), with no cap override.
started=true
"${compose[@]}" up -d postgres
wait_postgres
"${compose[@]}" exec -T postgres psql -X -v ON_ERROR_STOP=1 \
  -U hepta_resource_migrator -d hepta_resource -c \
  "create role hepta_resource_runtime login password 'hepta_resource_runtime_password'
     nosuperuser nocreatedb nocreaterole noinherit noreplication nobypassrls;
   create role hepta_resource_finality login password 'hepta_resource_finality_password'
     nosuperuser nocreatedb nocreaterole noinherit noreplication nobypassrls;"
"${migration_compose[@]}" --profile migration run --rm --no-deps hepta-migrate
[[ -z $("${migration_compose[@]}" --profile migration ps -a -q hepta-migrate) ]]
[[ -z $("${docker_command[@]}" ps -aq \
  --filter "label=com.docker.compose.project=$project" \
  --filter 'label=com.docker.compose.service=hepta-migrate') ]]
rm -f -- "$migration_secret_file"
[[ ! -e "$migration_secret_file" ]]
"${compose[@]}" config --quiet
"${compose[@]}" up -d --no-deps hepta
wait_hepta
resident_container=$("${compose[@]}" ps -q hepta)
if "${docker_command[@]}" inspect "$resident_container" \
  --format '{{range .Config.Env}}{{println .}}{{end}}' | grep -Eq '^HEPTA_MIGRATION_DATABASE_URL(_FILE)?='; then
  echo "resident resource-gate Hepta container retained the migration-owner credential" >&2
  exit 1
fi
if "${docker_command[@]}" inspect "$resident_container" \
  | grep -Fq 'postgres://hepta_resource_migrator:'; then
  echo "resident resource-gate Hepta metadata exposed the migration-owner URL" >&2
  exit 1
fi
"${docker_command[@]}" inspect "$resident_container" \
  --format '{{range .Config.Env}}{{println .}}{{end}}' \
  | grep -Fx 'HEPTA_FINALITY_DATABASE_URL=postgres://hepta_resource_finality:hepta_resource_finality_password@postgres:5432/hepta_resource' >/dev/null
runtime_role_boundary=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 -c \
  "select (
      not role.rolsuper
      and not role.rolinherit
      and not role.rolcreatedb
      and not role.rolcreaterole
      and not role.rolreplication
      and not role.rolbypassrls
      and not exists (
        select 1 from pg_auth_members as membership
        where membership.member=role.oid or membership.roleid=role.oid
      )
      and not exists (
        select 1 from pg_class as relation
        join pg_namespace as namespace on namespace.oid=relation.relnamespace
        where namespace.nspname='public' and relation.relowner=role.oid
      )
      and has_schema_privilege(role.rolname, 'public', 'USAGE')
      and not has_schema_privilege(role.rolname, 'public', 'CREATE')
      and has_database_privilege(role.rolname, current_database(), 'CONNECT')
      and not has_database_privilege(role.rolname, current_database(), 'CREATE')
      and not has_database_privilege(role.rolname, current_database(), 'TEMPORARY')
      and not has_function_privilege(
        role.rolname,
        'public.hepta_assert_paper_finality_v2_source_unsealed(uuid)',
        'EXECUTE'
      )
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_lock_window_arm()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_lock_preparation()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_apply_seal()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_reject_paper_finality_v2_source_mutation()', 'EXECUTE')
      and (
        select bool_and(
          has_table_privilege(role.rolname, relation.oid, 'SELECT')
          and not has_table_privilege(role.rolname, relation.oid, 'TRUNCATE')
          and not has_table_privilege(role.rolname, relation.oid, 'REFERENCES')
          and not has_table_privilege(role.rolname, relation.oid, 'TRIGGER')
          and (
            case when relation.relname = any(array[
              'hepta_trnm_cometbft_time_checkpoints_v1',
              'hepta_paper_chain_finality_window_arms_v2',
              'hepta_paper_chain_finality_preparations_v2'
            ]) then
              not has_table_privilege(role.rolname, relation.oid, 'INSERT')
              and not has_table_privilege(role.rolname, relation.oid, 'UPDATE')
              and not has_table_privilege(role.rolname, relation.oid, 'DELETE')
            else
              has_table_privilege(role.rolname, relation.oid, 'INSERT')
              and has_table_privilege(role.rolname, relation.oid, 'UPDATE')
              and has_table_privilege(role.rolname, relation.oid, 'DELETE')
            end
          )
        )
        from pg_class as relation
        join pg_namespace as namespace on namespace.oid=relation.relnamespace
        where namespace.nspname='public' and relation.relkind in ('r','p')
      )
    )::text
   from pg_roles as role where role.rolname='hepta_resource_runtime';")
[[ "$runtime_role_boundary" == t ]]
finality_role_boundary=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 -c \
  "select (
      not role.rolsuper
      and not role.rolinherit
      and not role.rolcreatedb
      and not role.rolcreaterole
      and not role.rolreplication
      and not role.rolbypassrls
      and not exists (
        select 1 from pg_auth_members as membership
        where membership.member=role.oid or membership.roleid=role.oid
      )
      and not exists (
        select 1 from pg_class as relation
        join pg_namespace as namespace on namespace.oid=relation.relnamespace
        where namespace.nspname='public' and relation.relowner=role.oid
      )
      and has_schema_privilege(role.rolname, 'public', 'USAGE')
      and not has_schema_privilege(role.rolname, 'public', 'CREATE')
      and has_database_privilege(role.rolname, current_database(), 'CONNECT')
      and not has_database_privilege(role.rolname, current_database(), 'CREATE')
      and not has_database_privilege(role.rolname, current_database(), 'TEMPORARY')
      and has_function_privilege(
        role.rolname,
        'public.hepta_assert_paper_finality_v2_source_unsealed(uuid)',
        'EXECUTE'
      )
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_lock_window_arm()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_lock_preparation()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_apply_seal()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_reject_paper_finality_v2_source_mutation()', 'EXECUTE')
      and (
        select bool_and(
          has_table_privilege(role.rolname, relation.oid, 'SELECT')
          and (
            has_table_privilege(role.rolname, relation.oid, 'INSERT') =
            (relation.relname = any(array[
              'hepta_trnm_cometbft_time_checkpoints_v1',
              'hepta_paper_chain_finality_window_arms_v2',
              'hepta_paper_chain_finality_preparations_v2'
            ]))
          )
          and not has_table_privilege(role.rolname, relation.oid, 'UPDATE')
          and not has_table_privilege(role.rolname, relation.oid, 'DELETE')
          and not has_table_privilege(role.rolname, relation.oid, 'TRUNCATE')
          and not has_table_privilege(role.rolname, relation.oid, 'REFERENCES')
          and not has_table_privilege(role.rolname, relation.oid, 'TRIGGER')
        )
        from pg_class as relation
        join pg_namespace as namespace on namespace.oid=relation.relnamespace
        where namespace.nspname='public' and relation.relkind in ('r','p')
      )
    )::text
   from pg_roles as role where role.rolname='hepta_resource_finality';")
[[ "$finality_role_boundary" == t ]]
definer_public_execute_count=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 -c \
  "select count(*)
   from pg_proc as function
   where function.oid = any(array[
     to_regprocedure('public.hepta_paper_finality_v2_lock_window_arm()'),
     to_regprocedure('public.hepta_paper_finality_v2_lock_preparation()'),
     to_regprocedure('public.hepta_paper_finality_v2_apply_seal()'),
     to_regprocedure('public.hepta_assert_paper_finality_v2_source_unsealed(uuid)'),
     to_regprocedure('public.hepta_reject_paper_finality_v2_source_mutation()')
   ])
   and exists (
     select 1
     from aclexplode(coalesce(function.proacl, acldefault('f', function.proowner))) as privilege
     where privilege.grantee=0 and privilege.privilege_type='EXECUTE'
   );")
[[ "$definer_public_execute_count" == 0 ]]
verified_definer_count=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 -c \
  "select count(*)
   from (values
     ('public.hepta_paper_finality_v2_lock_window_arm()'),
     ('public.hepta_paper_finality_v2_lock_preparation()'),
     ('public.hepta_paper_finality_v2_apply_seal()'),
     ('public.hepta_assert_paper_finality_v2_source_unsealed(uuid)'),
     ('public.hepta_reject_paper_finality_v2_source_mutation()')
   ) as expected(signature)
   join pg_proc as function on function.oid=to_regprocedure(expected.signature)
   where function.prosecdef
     and function.proconfig=array['search_path=pg_catalog']::text[];")
[[ "$verified_definer_count" == 5 ]]
assert_ready_cap "$default_cap" "$evidence_dir/default-ready.json"
default_ready_sha256=$(sha256sum "$evidence_dir/default-ready.json" | cut -d' ' -f1)
default_container=$("${compose[@]}" ps -q hepta)
[[ $("${docker_command[@]}" inspect "$default_container" --format '{{.Image}}') == "$HEPTA_EXPECTED_IMAGE_ID" ]]
[[ $("${docker_command[@]}" inspect "$default_container" --format '{{.HostConfig.Memory}}') == "$memory_limit_bytes" ]]
default_pid=$("${docker_command[@]}" inspect "$default_container" --format '{{.State.Pid}}')
default_cgroup_path=$(resolve_memory_peak_file "$default_pid")
printf '%s\n' "$default_cgroup_path" >"$evidence_dir/default-cgroup-memory-peak-path.txt"
default_baseline_peak=$(read_memory_peak "$default_cgroup_path")
[[ "$default_baseline_peak" =~ ^[0-9]+$ ]]

verify_snapshot_unchanged
anchor_status=$(curl --silent --show-error --max-time 30 \
  --header 'x-hepta-operator-token: hepta-resource-operator-token' \
  --header 'content-type: application/json' \
  --data-binary "@$fixture_dir/trust-anchor.json" \
  --output "$evidence_dir/default-anchor-response.json" --write-out '%{http_code}' \
  "http://127.0.0.1:$host_port/v2/hepta/operator/trnm/trust-anchors")
[[ "$anchor_status" == 201 ]]
[[ $("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 \
  -c "select count(*) from hepta_trnm_cometbft_trust_anchors;") == 1 ]]
capture_db_snapshot db-baseline
jq -e \
  '.trust_anchor_rows == 1
    and .outbox_rows == 0
    and .inbox_rows == 0
    and .module_receipt_rows == 0
    and .paper_room_event_rows == 0
    and .nakama_control_command_rows == 0
    and .finality_inbox_rows == 0
    and .chain_receipt_rows == 0
    and .finality_projection_rows == 0
    and .chain_time_checkpoint_rows == 0
    and .finality_v2_window_arm_rows == 0
    and .finality_v2_preparation_rows == 0' \
  "$evidence_dir/db-baseline-db-counts.json" >/dev/null
cp -- "$evidence_dir/db-baseline-db-counts.json" \
  "$evidence_dir/db-baseline-counts.json"
db_baseline_counts_sha256=$(sha256sum "$evidence_dir/db-baseline-counts.json" | cut -d' ' -f1)
db_baseline_state_sha256=$(sha256sum "$evidence_dir/db-baseline-league-state.json" | cut -d' ' -f1)
db_baseline_rows_sha256=$(sha256sum "$evidence_dir/db-baseline-db-rows.json" | cut -d' ' -f1)
db_baseline_sequences_sha256=$(sha256sum "$evidence_dir/db-baseline-db-sequences.json" | cut -d' ' -f1)

verify_snapshot_unchanged
legal_status=$(post_receipt \
  "$fixture_dir/legal-receipt-v2.json" "$evidence_dir/default-legal-response.json")
[[ "$legal_status" == 404 ]]
jq -e '.code == "queued_trnm_command_not_found"' \
  "$evidence_dir/default-legal-response.json" >/dev/null
assert_db_unchanged default-after-legal
default_legal_peak=$(read_memory_peak "$default_cgroup_path")
[[ "$default_legal_peak" =~ ^[0-9]+$ && "$default_legal_peak" -le "$max_peak_bytes" ]]

verify_snapshot_unchanged
default_plus_one_status=$(post_receipt \
  "$fixture_dir/default-plus-one.body" "$evidence_dir/default-plus-one-response.json")
[[ "$default_plus_one_status" == 413 ]]
jq -e '.code == "request_body_too_large"' \
  "$evidence_dir/default-plus-one-response.json" >/dev/null
assert_db_unchanged default-after-plus-one
default_final_peak=$(read_memory_peak "$default_cgroup_path")
[[ "$default_final_peak" =~ ^[0-9]+$ && "$default_final_peak" -le "$max_peak_bytes" ]]
assert_container_healthy "$default_container"
default_oom_killed=$("${docker_command[@]}" inspect "$default_container" --format '{{.State.OOMKilled}}')
default_restart_count=$("${docker_command[@]}" inspect "$default_container" --format '{{.RestartCount}}')
"${docker_command[@]}" inspect "$default_container" | jq -S . \
  >"$evidence_dir/default-container-inspect.json"
default_inspect_sha256=$(sha256sum "$evidence_dir/default-container-inspect.json" | cut -d' ' -f1)
"${docker_command[@]}" logs "$default_container" >"$evidence_dir/default-hepta.log" 2>&1

# Phase 2: recreate only Hepta with an explicit 1 MiB deployment-max override.
compose=("${compose_max[@]}")
"${compose[@]}" up -d --no-deps --force-recreate hepta
wait_hepta
assert_ready_cap "$deployment_max" "$evidence_dir/max-ready.json"
max_ready_sha256=$(sha256sum "$evidence_dir/max-ready.json" | cut -d' ' -f1)
max_container=$("${compose[@]}" ps -q hepta)
[[ "$max_container" != "$default_container" ]]
[[ -z $("${compose[@]}" ps -a -q hepta-migrate) ]]
[[ -z $("${docker_command[@]}" ps -aq \
  --filter "label=com.docker.compose.project=$project" \
  --filter 'label=com.docker.compose.service=hepta-migrate') ]]
[[ $("${docker_command[@]}" inspect "$max_container" --format '{{.Image}}') == "$HEPTA_EXPECTED_IMAGE_ID" ]]
[[ $("${docker_command[@]}" inspect "$max_container" --format '{{.HostConfig.Memory}}') == "$memory_limit_bytes" ]]
max_pid=$("${docker_command[@]}" inspect "$max_container" --format '{{.State.Pid}}')
max_cgroup_path=$(resolve_memory_peak_file "$max_pid")
printf '%s\n' "$max_cgroup_path" >"$evidence_dir/max-cgroup-memory-peak-path.txt"
max_baseline_peak=$(read_memory_peak "$max_cgroup_path")
[[ "$max_baseline_peak" =~ ^[0-9]+$ ]]
[[ $("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_resource_migrator -d hepta_resource -v ON_ERROR_STOP=1 \
  -c "select count(*) from hepta_trnm_cometbft_trust_anchors;") == 1 ]]
assert_db_unchanged max-after-recreate

verify_snapshot_unchanged
adversarial_status=$(post_receipt \
  "$fixture_dir/canonical-shape-adversarial.json" \
  "$evidence_dir/max-adversarial-response.json")
[[ "$adversarial_status" == 400 ]]
jq -e '.code == "trnm_receipt_v2_structural_invalid"' \
  "$evidence_dir/max-adversarial-response.json" >/dev/null
assert_db_unchanged max-after-adversarial
max_adversarial_peak=$(read_memory_peak "$max_cgroup_path")
[[ "$max_adversarial_peak" =~ ^[0-9]+$ && "$max_adversarial_peak" -le "$max_peak_bytes" ]]

verify_snapshot_unchanged
max_plus_one_status=$(post_receipt \
  "$fixture_dir/max-plus-one.body" "$evidence_dir/max-plus-one-response.json")
[[ "$max_plus_one_status" == 413 ]]
jq -e '.code == "request_body_too_large"' \
  "$evidence_dir/max-plus-one-response.json" >/dev/null
assert_db_unchanged max-after-plus-one
assert_container_healthy "$max_container"

# Hold one authenticated request after its headers and first body byte. The
# second probe is the readiness signal: only an observed 503 proves that the
# service, rather than the client socket, actually owns the sole permit.
holder_ready="$scratch/holder.connected"
python3 - "$host_port" "$paper_id" "$anchor_hash" "$deployment_max" "$holder_ready" <<'PY' &
import pathlib
import socket
import sys
import time

port, paper_id, anchor_hash, length, ready = sys.argv[1:]
request = (
    f"POST /v2/hepta/papers/{paper_id}/chain-finality HTTP/1.1\r\n"
    f"Host: 127.0.0.1:{port}\r\n"
    "x-hepta-trnm-token: hepta-resource-trnm-token\r\n"
    f"x-hepta-trnm-trust-anchor-hash: {anchor_hash}\r\n"
    "content-type: application/json\r\n"
    f"content-length: {length}\r\n"
    "connection: close\r\n\r\n{"
).encode()
last_error = None
for _ in range(50):
    try:
        with socket.create_connection(("127.0.0.1", int(port)), timeout=5) as stream:
            stream.sendall(request)
            pathlib.Path(ready).write_text("connected\n", encoding="ascii")
            time.sleep(30)
            raise SystemExit(0)
    except OSError as error:
        last_error = error
        time.sleep(0.1)
raise SystemExit(f"holder could not connect: {last_error}")
PY
holder_pid=$!
for _ in $(seq 1 50); do
  [[ -f "$holder_ready" ]] && kill -0 "$holder_pid" >/dev/null 2>&1 && break
  sleep 0.1
done
[[ -f "$holder_ready" ]]
kill -0 "$holder_pid" >/dev/null 2>&1

python3 - "$host_port" "$paper_id" "$anchor_hash" "$deployment_max" \
  "$evidence_dir/max-busy-response.raw" <<'PY'
import pathlib
import socket
import sys
import time

port, paper_id, anchor_hash, length, output = sys.argv[1:]
request = (
    f"POST /v2/hepta/papers/{paper_id}/chain-finality HTTP/1.1\r\n"
    f"Host: 127.0.0.1:{port}\r\n"
    "x-hepta-trnm-token: hepta-resource-trnm-token\r\n"
    f"x-hepta-trnm-trust-anchor-hash: {anchor_hash}\r\n"
    "content-type: application/json\r\n"
    f"content-length: {length}\r\n"
    "connection: close\r\n\r\n"
).encode()
last = b""
last_error = None
for _ in range(50):
    try:
        with socket.create_connection(("127.0.0.1", int(port)), timeout=5) as stream:
            stream.sendall(request)
            stream.settimeout(5)
            chunks = []
            while True:
                chunk = stream.recv(65536)
                if not chunk:
                    break
                chunks.append(chunk)
            last = b"".join(chunks)
        if last.startswith(b"HTTP/1.1 503 "):
            pathlib.Path(output).write_bytes(last)
            raise SystemExit(0)
    except OSError as error:
        last_error = error
    time.sleep(0.1)
pathlib.Path(output).write_bytes(last)
raise SystemExit(f"Receipt V2 busy probe never observed HTTP 503: {last_error}")
PY
grep -a -q '^HTTP/1.1 503 Service Unavailable' "$evidence_dir/max-busy-response.raw"
grep -a -q '"code":"trnm_receipt_v2_verification_busy"' \
  "$evidence_dir/max-busy-response.raw"
stop_holder
assert_db_unchanged max-after-busy
max_final_peak=$(read_memory_peak "$max_cgroup_path")
[[ "$max_final_peak" =~ ^[0-9]+$ && "$max_final_peak" -le "$max_peak_bytes" ]]
assert_container_healthy "$max_container"
verify_snapshot_unchanged
[[ $("${docker_command[@]}" image inspect "$HEPTA_IMAGE" --format '{{.Id}}') == "$HEPTA_EXPECTED_IMAGE_ID" ]]
max_oom_killed=$("${docker_command[@]}" inspect "$max_container" --format '{{.State.OOMKilled}}')
max_restart_count=$("${docker_command[@]}" inspect "$max_container" --format '{{.RestartCount}}')
"${docker_command[@]}" inspect "$max_container" | jq -S . \
  >"$evidence_dir/max-container-inspect.json"
max_inspect_sha256=$(sha256sum "$evidence_dir/max-container-inspect.json" | cut -d' ' -f1)
"${docker_command[@]}" logs "$max_container" >"$evidence_dir/max-hepta.log" 2>&1
cp -- "$fixture_dir/manifest.json" "$evidence_dir/fixture-snapshot-manifest.json"
db_final_counts_sha256=$(sha256sum "$evidence_dir/max-after-busy-db-counts.json" | cut -d' ' -f1)
db_final_state_sha256=$(sha256sum "$evidence_dir/max-after-busy-league-state.json" | cut -d' ' -f1)
db_final_rows_sha256=$(sha256sum "$evidence_dir/max-after-busy-db-rows.json" | cut -d' ' -f1)
db_final_sequences_sha256=$(sha256sum "$evidence_dir/max-after-busy-db-sequences.json" | cut -d' ' -f1)
verify_snapshot_unchanged
verify_release_inputs_unchanged

# PASS is impossible while any container, network, named volume, fixture copy,
# environment file, token, or other private scratch artifact remains. Failure
# cleanup retries these operations but never converts the original error to a
# successful result.
teardown_compose_project
compose_teardown_verified=true
remove_resource_scratch
[[ "$scratch_removed" == true && ! -e "$scratch" && ! -L "$scratch" ]]
scratch_cleanup_verified=true
verify_release_inputs_unchanged

expected_payload_artifacts() {
  printf '%s\n' \
    compose-default-rendered.json \
    compose-max-rendered.json \
    db-baseline-counts.json \
    default-anchor-response.json \
    default-cgroup-memory-peak-path.txt \
    default-container-inspect.json \
    default-hepta.log \
    default-legal-response.json \
    default-plus-one-response.json \
    default-ready.json \
    fixture-snapshot-manifest.json \
    hepta-image-inspect.json \
    image-build.stderr \
    image-build.stdout \
    image-provenance.json \
    max-adversarial-response.json \
    max-busy-response.raw \
    max-cgroup-memory-peak-path.txt \
    max-container-inspect.json \
    max-hepta.log \
    max-plus-one-response.json \
    max-ready.json
  local label
  for label in \
    db-baseline \
    default-after-legal \
    default-after-plus-one \
    max-after-adversarial \
    max-after-busy \
    max-after-plus-one \
    max-after-recreate; do
    printf '%s\n' \
      "$label-db-counts.json" \
      "$label-db-rows.json" \
      "$label-db-sequences.json" \
      "$label-league-state.json"
  done
}

mapfile -t payload_artifacts < <(expected_payload_artifacts | sort)
python3 - "$evidence_staging_fd" "${payload_artifacts[@]}" <<'PY'
import os
import stat
import sys

staging_fd = int(sys.argv[1])
for name in sys.argv[2:]:
    descriptor = os.open(
        name,
        os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK,
        dir_fd=staging_fd,
    )
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
            raise RuntimeError(f"non-regular evidence payload artifact: {name}")
        os.fchmod(descriptor, 0o600)
    finally:
        os.close(descriptor)
PY

# Bind every raw response, log, rendered configuration, inspection and DB
# snapshot before the summary is created. The summary binds this manifest;
# SHA256SUMS then binds the summary as the final transport manifest.
(cd "$evidence_dir" && sha256sum -- "${payload_artifacts[@]}" >PAYLOAD.SHA256)
payload_manifest_sha256=$(sha256sum "$evidence_dir/PAYLOAD.SHA256" | cut -d' ' -f1)

jq -n \
  --slurpfile db_baseline "$evidence_dir/db-baseline-counts.json" \
  --slurpfile db_final "$evidence_dir/max-after-busy-db-counts.json" \
  --slurpfile image_build_provenance "$evidence_dir/image-provenance.json" \
  --arg image "$HEPTA_IMAGE" \
  --arg image_id "$HEPTA_EXPECTED_IMAGE_ID" \
  --arg source_revision "$release_revision" \
  --arg source_tree "$release_tree" \
  --argjson source_date_epoch "$release_source_date_epoch" \
  --argjson tracked_files "$release_tracked_files" \
  --arg gate_source_sha256 "$gate_source_sha256" \
  --arg generator_source_sha256 "$generator_source_sha256" \
  --arg image_build_admission_source_sha256 "$image_build_admission_source_sha256" \
  --arg image_builder_source_sha256 "$image_builder_source_sha256" \
  --arg clean_source_verifier_sha256 "$clean_source_verifier_sha256" \
  --arg compose_source_sha256 "$compose_source_sha256" \
  --arg migration_compose_source_sha256 "$migration_compose_source_sha256" \
  --arg dockerfile_sha256 "$dockerfile_sha256" \
  --arg cargo_lock_sha256 "$cargo_lock_sha256" \
  --arg rust_toolchain_sha256 "$rust_toolchain_sha256" \
  --arg sbom_sha256 "$sbom_sha256" \
  --arg vendor_manifest_sha256 "$vendor_manifest_sha256" \
  --arg runtime_binary_sha256 "$runtime_binary_sha256" \
  --arg image_build_stdout_identity "$image_build_stdout_identity" \
  --arg image_build_stderr_identity "$image_build_stderr_identity" \
  --arg image_build_stdout_sha256 "$image_build_stdout_sha256" \
  --arg image_build_stderr_sha256 "$image_build_stderr_sha256" \
  --arg image_provenance_sha256 "$image_provenance_sha256" \
  --arg compose_default_sha256 "$compose_default_sha256" \
  --arg compose_max_sha256 "$compose_max_sha256" \
  --arg image_inspect_sha256 "$image_inspect_sha256" \
  --arg default_ready_sha256 "$default_ready_sha256" \
  --arg max_ready_sha256 "$max_ready_sha256" \
  --arg default_inspect_sha256 "$default_inspect_sha256" \
  --arg max_inspect_sha256 "$max_inspect_sha256" \
  --arg evidence_parent_identity "$evidence_parent_identity" \
  --arg evidence_staging_identity "$evidence_staging_identity" \
  --arg evidence_target_basename "$evidence_basename" \
  --arg payload_manifest_sha256 "$payload_manifest_sha256" \
  --arg snapshot_manifest_sha256 "$snapshot_manifest_sha256" \
  --arg snapshot_tree_sha256 "$snapshot_tree_sha256" \
  --arg anchor_hash "$anchor_hash" \
  --arg paper_id "$paper_id" \
  --arg default_container "$default_container" \
  --arg max_container "$max_container" \
  --arg default_cgroup_path "$default_cgroup_path" \
  --arg max_cgroup_path "$max_cgroup_path" \
  --arg db_baseline_counts_sha256 "$db_baseline_counts_sha256" \
  --arg db_baseline_state_sha256 "$db_baseline_state_sha256" \
  --arg db_baseline_rows_sha256 "$db_baseline_rows_sha256" \
  --arg db_baseline_sequences_sha256 "$db_baseline_sequences_sha256" \
  --arg db_final_counts_sha256 "$db_final_counts_sha256" \
  --arg db_final_state_sha256 "$db_final_state_sha256" \
  --arg db_final_rows_sha256 "$db_final_rows_sha256" \
  --arg db_final_sequences_sha256 "$db_final_sequences_sha256" \
  --argjson legal_receipt_bytes "$legal_receipt_bytes" \
  --argjson legal_utilization_bps "$legal_utilization_bps" \
  --argjson memory_limit_bytes "$memory_limit_bytes" \
  --argjson memory_peak_policy_ceiling_bytes "$default_max_peak_bytes" \
  --argjson max_peak_bytes "$max_peak_bytes" \
  --argjson default_baseline_peak "$default_baseline_peak" \
  --argjson default_legal_peak "$default_legal_peak" \
  --argjson default_final_peak "$default_final_peak" \
  --arg default_oom_killed "$default_oom_killed" \
  --argjson default_restart_count "$default_restart_count" \
  --argjson max_baseline_peak "$max_baseline_peak" \
  --argjson max_adversarial_peak "$max_adversarial_peak" \
  --argjson max_final_peak "$max_final_peak" \
  --arg max_oom_killed "$max_oom_killed" \
  --argjson max_restart_count "$max_restart_count" \
  '{
    schema:"hepta.receipt_v2.resource_gate_evidence.v3",
    result:"pass",
    image:$image,
    image_id:$image_id,
    source_revision:$source_revision,
    source_tree:$source_tree,
    source_date_epoch:$source_date_epoch,
    git_status_clean:true,
    commit_index_worktree_identical:true,
    tracked_files:$tracked_files,
    publication:"private_sibling_staging_then_atomic_noreplace_rename",
    publication_authority:{
      parent_dev_inode_owner_mode:$evidence_parent_identity,
      evidence_dev_inode_owner_mode:$evidence_staging_identity,
      target_basename:$evidence_target_basename,
      retained_dirfds:true,
      exact_artifact_set_verified:true,
      manifests_verified:true,
      post_rename_inode_verified:true
    },
    provenance:{
      gate_source_sha256:$gate_source_sha256,
      generator_source_sha256:$generator_source_sha256,
      image_build_admission_source_sha256:$image_build_admission_source_sha256,
      image_builder_source_sha256:$image_builder_source_sha256,
      clean_source_verifier_sha256:$clean_source_verifier_sha256,
      compose_source_sha256:$compose_source_sha256,
      migration_compose_source_sha256:$migration_compose_source_sha256,
      compose_default_rendered_sha256:$compose_default_sha256,
      compose_max_rendered_sha256:$compose_max_sha256,
      image_inspect_sha256:$image_inspect_sha256,
      default_ready_sha256:$default_ready_sha256,
      max_ready_sha256:$max_ready_sha256,
      default_container_inspect_sha256:$default_inspect_sha256,
      max_container_inspect_sha256:$max_inspect_sha256,
      payload_manifest_sha256:$payload_manifest_sha256
    },
    image_build:{
      stdout:{
        artifact:"image-build.stdout",
        admitted_source_identity:$image_build_stdout_identity,
        sha256:$image_build_stdout_sha256
      },
      stderr:{
        artifact:"image-build.stderr",
        admitted_source_identity:$image_build_stderr_identity,
        sha256:$image_build_stderr_sha256
      },
      canonical_provenance:{
        artifact:"image-provenance.json",
        sha256:$image_provenance_sha256,
        document:$image_build_provenance[0]
      },
      committed_inputs:{
        dockerfile_sha256:$dockerfile_sha256,
        cargo_lock_sha256:$cargo_lock_sha256,
        rust_toolchain_sha256:$rust_toolchain_sha256,
        sbom_sha256:$sbom_sha256,
        vendor_manifest_sha256:$vendor_manifest_sha256,
        runtime_binary_sha256:$runtime_binary_sha256
      }
    },
    fixture:{
      mode:"private_o_nofollow_snapshot",
      manifest_sha256:$snapshot_manifest_sha256,
      tree_sha256:$snapshot_tree_sha256,
      anchor_hash:$anchor_hash,
      paper_id:$paper_id,
      legal_receipt_bytes:$legal_receipt_bytes,
      legal_utilization_bps:$legal_utilization_bps
    },
    memory_limit_bytes:$memory_limit_bytes,
    memory_peak_policy_ceiling_bytes:$memory_peak_policy_ceiling_bytes,
    enforced_max_peak_bytes:$max_peak_bytes,
    max_peak_bytes:$max_peak_bytes,
    max_in_flight:1,
    teardown:{
      compose_project_absent:true,
      named_volumes_absent:true,
      private_scratch_and_tokens_removed:true
    },
    database:{
      invariant:"enumerated_sorted_full_rows_counts_and_protected_sequences_exactly_unchanged_after_anchor_admission",
      baseline_counts:$db_baseline[0],
      final_counts:$db_final[0],
      baseline_counts_sha256:$db_baseline_counts_sha256,
      baseline_league_state_sha256:$db_baseline_state_sha256,
      baseline_protected_rows_sha256:$db_baseline_rows_sha256,
      baseline_protected_sequences_sha256:$db_baseline_sequences_sha256,
      final_counts_sha256:$db_final_counts_sha256,
      final_league_state_sha256:$db_final_state_sha256,
      final_protected_rows_sha256:$db_final_rows_sha256,
      final_protected_sequences_sha256:$db_final_sequences_sha256
    },
    phases:{
      canonical_default:{
        configured_cap_bytes:32768,
        configuration_source:"compose_default_without_env_override",
        readiness_evidence:"default-ready.json",
        container_id:$default_container,
        cgroup_memory_peak_path:$default_cgroup_path,
        legal_status:404,
        default_plus_one_status:413,
        baseline_peak_bytes:$default_baseline_peak,
        legal_peak_bytes:$default_legal_peak,
        final_peak_bytes:$default_final_peak,
        partial_projection_rows:0,
        oom_killed:($default_oom_killed == "true"),
        restart_count:$default_restart_count
      },
      deployment_max_override:{
        configured_cap_bytes:1048576,
        configuration_source:"explicit_resource_gate_override",
        readiness_evidence:"max-ready.json",
        container_id:$max_container,
        cgroup_memory_peak_path:$max_cgroup_path,
        adversarial_status:400,
        max_plus_one_status:413,
        busy_status:503,
        baseline_peak_bytes:$max_baseline_peak,
        adversarial_peak_bytes:$max_adversarial_peak,
        final_peak_bytes:$max_final_peak,
        partial_projection_rows:0,
        oom_killed:($max_oom_killed == "true"),
        restart_count:$max_restart_count
      }
    }
  }' >"$evidence_dir/summary.json"

mapfile -t final_manifest_artifacts < <(
  printf '%s\n' "${payload_artifacts[@]}" PAYLOAD.SHA256 summary.json | sort
)
(cd "$evidence_dir" && sha256sum -- "${final_manifest_artifacts[@]}" >SHA256SUMS)
closure_manifest_sha256=$(sha256sum "$evidence_dir/SHA256SUMS" | cut -d' ' -f1)
publication_contract=$(jq -cn \
  --arg image "$HEPTA_IMAGE" \
  --arg image_id "$HEPTA_EXPECTED_IMAGE_ID" \
  --arg source_revision "$release_revision" \
  --arg source_tree "$release_tree" \
  --argjson source_date_epoch "$release_source_date_epoch" \
  --argjson tracked_files "$release_tracked_files" \
  --arg image_build_stdout_identity "$image_build_stdout_identity" \
  --arg image_build_stderr_identity "$image_build_stderr_identity" \
  --arg image_build_stdout_sha256 "$image_build_stdout_sha256" \
  --arg image_build_stderr_sha256 "$image_build_stderr_sha256" \
  --arg image_provenance_sha256 "$image_provenance_sha256" \
  --arg dockerfile_sha256 "$dockerfile_sha256" \
  --arg cargo_lock_sha256 "$cargo_lock_sha256" \
  --arg rust_toolchain_sha256 "$rust_toolchain_sha256" \
  --arg sbom_sha256 "$sbom_sha256" \
  --arg vendor_manifest_sha256 "$vendor_manifest_sha256" \
  --arg runtime_binary_sha256 "$runtime_binary_sha256" \
  --arg gate_source_sha256 "$gate_source_sha256" \
  --arg generator_source_sha256 "$generator_source_sha256" \
  --arg image_build_admission_source_sha256 "$image_build_admission_source_sha256" \
  --arg image_builder_source_sha256 "$image_builder_source_sha256" \
  --arg clean_source_verifier_sha256 "$clean_source_verifier_sha256" \
  --arg compose_source_sha256 "$compose_source_sha256" \
  --arg migration_compose_source_sha256 "$migration_compose_source_sha256" \
  --arg compose_default_sha256 "$compose_default_sha256" \
  --arg compose_max_sha256 "$compose_max_sha256" \
  --arg image_inspect_sha256 "$image_inspect_sha256" \
  --arg default_ready_sha256 "$default_ready_sha256" \
  --arg max_ready_sha256 "$max_ready_sha256" \
  --arg default_inspect_sha256 "$default_inspect_sha256" \
  --arg max_inspect_sha256 "$max_inspect_sha256" '
  {
    image:$image,
    image_id:$image_id,
    source_revision:$source_revision,
    source_tree:$source_tree,
    source_date_epoch:$source_date_epoch,
    tracked_files:$tracked_files,
    image_build:{
      stdout_identity:$image_build_stdout_identity,
      stderr_identity:$image_build_stderr_identity,
      stdout_sha256:$image_build_stdout_sha256,
      stderr_sha256:$image_build_stderr_sha256,
      provenance_sha256:$image_provenance_sha256,
      committed_inputs:{
        dockerfile_sha256:$dockerfile_sha256,
        cargo_lock_sha256:$cargo_lock_sha256,
        rust_toolchain_sha256:$rust_toolchain_sha256,
        sbom_sha256:$sbom_sha256,
        vendor_manifest_sha256:$vendor_manifest_sha256,
        runtime_binary_sha256:$runtime_binary_sha256
      }
    },
    provenance:{
      gate_source_sha256:$gate_source_sha256,
      generator_source_sha256:$generator_source_sha256,
      image_build_admission_source_sha256:$image_build_admission_source_sha256,
      image_builder_source_sha256:$image_builder_source_sha256,
      clean_source_verifier_sha256:$clean_source_verifier_sha256,
      compose_source_sha256:$compose_source_sha256,
      migration_compose_source_sha256:$migration_compose_source_sha256,
      compose_default_rendered_sha256:$compose_default_sha256,
      compose_max_rendered_sha256:$compose_max_sha256,
      image_inspect_sha256:$image_inspect_sha256,
      default_ready_sha256:$default_ready_sha256,
      max_ready_sha256:$max_ready_sha256,
      default_container_inspect_sha256:$default_inspect_sha256,
      max_container_inspect_sha256:$max_inspect_sha256
    }
  }')

verify_release_inputs_unchanged
final_source_authority=$(python3 "$clean_source_verifier" \
  --repo-dir "$repo_dir" --revision "$release_revision" --tree "$release_tree")
[[ "$final_source_authority" == "$source_authority" ]] || {
  echo "resource evidence Git source authority changed before publication" >&2
  exit 1
}
closure_manifest_sha256=$(python3 - \
  "$evidence_parent_fd" \
  "$evidence_staging_fd" \
  "$evidence_parent" \
  "$(basename -- "$evidence_staging")" \
  "$evidence_basename" \
  "$evidence_parent_identity" \
  "$evidence_staging_identity" \
  "$(id -u)" \
  "$closure_manifest_sha256" \
  "$publication_contract" \
  "${payload_artifacts[@]}" <<'PY'
import ctypes
import hashlib
import json
import os
import re
import stat
import sys

parent_fd = int(sys.argv[1])
staging_fd = int(sys.argv[2])
parent_path = sys.argv[3]
staging_name = sys.argv[4]
target_name = sys.argv[5]
expected_parent_identity = sys.argv[6]
expected_staging_identity = sys.argv[7]
expected_uid = int(sys.argv[8])
expected_closure_manifest_sha256 = sys.argv[9]
expected_contract = json.loads(sys.argv[10])
payload_names = sys.argv[11:]
if payload_names != sorted(payload_names) or len(payload_names) != len(set(payload_names)):
    raise RuntimeError("resource evidence payload authority is not sorted and unique")
payload_set = set(payload_names)
sha_entries = sorted(payload_set | {"PAYLOAD.SHA256", "summary.json"})
expected_all = set(sha_entries) | {"SHA256SUMS"}
safe_name = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
if any(not safe_name.fullmatch(name) for name in expected_all):
    raise RuntimeError("resource evidence artifact authority contains an unsafe name")


def identity(metadata):
    return (
        f"{metadata.st_dev}:{metadata.st_ino}:{metadata.st_uid}:"
        f"{metadata.st_gid}:{stat.S_IMODE(metadata.st_mode):o}"
    )


def open_artifact(name):
    descriptor = os.open(
        name,
        os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK,
        dir_fd=staging_fd,
    )
    metadata = os.fstat(descriptor)
    if (
        not stat.S_ISREG(metadata.st_mode)
        or metadata.st_nlink != 1
        or metadata.st_uid != expected_uid
        or stat.S_IMODE(metadata.st_mode) != 0o600
    ):
        os.close(descriptor)
        raise RuntimeError(f"resource evidence artifact metadata is unsafe: {name}")
    return descriptor


def read_artifact(name, maximum=4 * 1024 * 1024):
    descriptor = open_artifact(name)
    try:
        chunks = []
        total = 0
        while True:
            chunk = os.read(descriptor, 65536)
            if not chunk:
                break
            total += len(chunk)
            if total > maximum:
                raise RuntimeError(f"resource evidence manifest is too large: {name}")
            chunks.append(chunk)
        os.fsync(descriptor)
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def digest_artifact(name):
    descriptor = open_artifact(name)
    try:
        digest = hashlib.sha256()
        while True:
            chunk = os.read(descriptor, 1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
        os.fsync(descriptor)
        return digest.hexdigest()
    finally:
        os.close(descriptor)


manifest_line = re.compile(r"^([0-9a-f]{64})  ([A-Za-z0-9][A-Za-z0-9._-]*)$")


def parse_manifest(name, expected_names):
    raw = read_artifact(name)
    if not raw.endswith(b"\n"):
        raise RuntimeError(f"resource evidence manifest has no final newline: {name}")
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeDecodeError as error:
        raise RuntimeError(f"resource evidence manifest is not ASCII: {name}") from error
    entries = []
    for line in lines:
        match = manifest_line.fullmatch(line)
        if match is None:
            raise RuntimeError(f"resource evidence manifest line is not canonical: {name}")
        entries.append((match.group(2), match.group(1)))
    if [entry_name for entry_name, _ in entries] != expected_names:
        raise RuntimeError(f"resource evidence manifest file set/order differs: {name}")
    if len(entries) != len({entry_name for entry_name, _ in entries}):
        raise RuntimeError(f"resource evidence manifest contains duplicate names: {name}")
    for entry_name, expected_digest in entries:
        if digest_artifact(entry_name) != expected_digest:
            raise RuntimeError(f"resource evidence digest differs: {entry_name}")
    return raw


parent_metadata = os.fstat(parent_fd)
staging_metadata = os.fstat(staging_fd)
if identity(parent_metadata) != expected_parent_identity:
    raise RuntimeError("retained resource evidence parent identity changed")
if identity(staging_metadata) != expected_staging_identity:
    raise RuntimeError("retained resource evidence staging identity changed")
if (
    not stat.S_ISDIR(parent_metadata.st_mode)
    or parent_metadata.st_uid != expected_uid
    or stat.S_IMODE(parent_metadata.st_mode) & 0o022
):
    raise RuntimeError("retained resource evidence parent ownership/mode is unsafe")
if (
    not stat.S_ISDIR(staging_metadata.st_mode)
    or staging_metadata.st_uid != expected_uid
    or stat.S_IMODE(staging_metadata.st_mode) != 0o700
):
    raise RuntimeError("retained resource evidence staging ownership/mode is unsafe")

directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW
path_parent_fd = os.open(parent_path, directory_flags)
current_staging_fd = os.open(staging_name, directory_flags, dir_fd=parent_fd)
try:
    if identity(os.fstat(path_parent_fd)) != expected_parent_identity:
        raise RuntimeError("resource evidence parent path was replaced")
    if identity(os.fstat(current_staging_fd)) != expected_staging_identity:
        raise RuntimeError("resource evidence staging path was replaced")
finally:
    os.close(current_staging_fd)
    os.close(path_parent_fd)

try:
    os.stat(target_name, dir_fd=parent_fd, follow_symlinks=False)
except FileNotFoundError:
    pass
else:
    raise RuntimeError("resource evidence target appeared before publication")

actual_names = set(os.listdir(staging_fd))
if actual_names != expected_all:
    raise RuntimeError(
        "resource evidence artifact set differs: "
        f"missing={sorted(expected_all - actual_names)!r} "
        f"extra={sorted(actual_names - expected_all)!r}"
    )
for artifact_name in sorted(expected_all):
    descriptor = open_artifact(artifact_name)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)

payload_manifest = parse_manifest("PAYLOAD.SHA256", payload_names)
parse_manifest("SHA256SUMS", sha_entries)


def reject_duplicate_json(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise RuntimeError(f"duplicate summary JSON key: {key}")
        result[key] = value
    return result


summary = json.loads(
    read_artifact("summary.json").decode("utf-8"),
    object_pairs_hook=reject_duplicate_json,
)
if set(expected_contract) != {
    "image",
    "image_id",
    "source_revision",
    "source_tree",
    "source_date_epoch",
    "tracked_files",
    "image_build",
    "provenance",
}:
    raise RuntimeError("resource evidence publication contract shape differs")
for key in ("image", "image_id", "source_revision", "source_tree", "source_date_epoch", "tracked_files"):
    if summary.get(key) != expected_contract.get(key) or type(summary.get(key)) is not type(
        expected_contract.get(key)
    ):
        raise RuntimeError(f"resource evidence summary differs from publication contract: {key}")
expected_summary_provenance = dict(expected_contract["provenance"])
expected_summary_provenance["payload_manifest_sha256"] = hashlib.sha256(
    payload_manifest
).hexdigest()
if summary.get("provenance") != expected_summary_provenance:
    raise RuntimeError("resource evidence summary provenance differs from publication contract")
for field, artifact in {
    "compose_default_rendered_sha256": "compose-default-rendered.json",
    "compose_max_rendered_sha256": "compose-max-rendered.json",
    "image_inspect_sha256": "hepta-image-inspect.json",
    "default_ready_sha256": "default-ready.json",
    "max_ready_sha256": "max-ready.json",
    "default_container_inspect_sha256": "default-container-inspect.json",
    "max_container_inspect_sha256": "max-container-inspect.json",
}.items():
    if expected_contract["provenance"].get(field) != digest_artifact(artifact):
        raise RuntimeError(f"resource evidence payload hash differs: {artifact}")
if (
    summary.get("schema") != "hepta.receipt_v2.resource_gate_evidence.v3"
    or summary.get("result") != "pass"
    or summary.get("publication")
    != "private_sibling_staging_then_atomic_noreplace_rename"
    or summary.get("memory_peak_policy_ceiling_bytes") != 402653184
    or not 0 < summary.get("enforced_max_peak_bytes", 0) <= 402653184
    or summary.get("phases", {}).get("canonical_default", {}).get(
        "readiness_evidence"
    )
    != "default-ready.json"
    or summary.get("phases", {}).get("deployment_max_override", {}).get(
        "readiness_evidence"
    )
    != "max-ready.json"
):
    raise RuntimeError("resource evidence summary release contract differs")
git_identity = re.compile(r"^[0-9a-f]{40}$")
sha256_digest = re.compile(r"^[0-9a-f]{64}$")
docker_digest = re.compile(r"^sha256:[0-9a-f]{64}$")
if (
    not git_identity.fullmatch(summary.get("source_revision", ""))
    or not git_identity.fullmatch(summary.get("source_tree", ""))
    or type(summary.get("source_date_epoch")) is not int
    or summary.get("source_date_epoch", 0) <= 0
    or summary.get("git_status_clean") is not True
    or summary.get("commit_index_worktree_identical") is not True
    or type(summary.get("tracked_files")) is not int
    or summary.get("tracked_files", 0) <= 0
    or not docker_digest.fullmatch(summary.get("image_id", ""))
):
    raise RuntimeError("resource evidence summary Git/image identity differs")
image_build = summary.get("image_build", {})
if set(image_build) != {
    "stdout",
    "stderr",
    "canonical_provenance",
    "committed_inputs",
}:
    raise RuntimeError("resource evidence image-build contract differs")
for stream_name in ("stdout", "stderr"):
    stream = image_build.get(stream_name, {})
    expected_artifact = f"image-build.{stream_name}"
    if (
        set(stream) != {"artifact", "admitted_source_identity", "sha256"}
        or stream.get("artifact") != expected_artifact
        or not isinstance(stream.get("admitted_source_identity"), str)
        or not sha256_digest.fullmatch(stream.get("sha256", ""))
        or stream.get("sha256") != digest_artifact(expected_artifact)
        or stream.get("admitted_source_identity")
        != expected_contract["image_build"].get(f"{stream_name}_identity")
        or stream.get("sha256")
        != expected_contract["image_build"].get(f"{stream_name}_sha256")
    ):
        raise RuntimeError(f"resource evidence image-build {stream_name} differs")
canonical_provenance = image_build.get("canonical_provenance", {})
if (
    set(canonical_provenance) != {"artifact", "sha256", "document"}
    or canonical_provenance.get("artifact") != "image-provenance.json"
    or not sha256_digest.fullmatch(canonical_provenance.get("sha256", ""))
    or canonical_provenance.get("sha256")
    != digest_artifact("image-provenance.json")
    or canonical_provenance.get("sha256")
    != expected_contract["image_build"].get("provenance_sha256")
):
    raise RuntimeError("resource evidence canonical image provenance differs")
provenance_bytes = read_artifact("image-provenance.json")
provenance = json.loads(
    provenance_bytes.decode("utf-8"), object_pairs_hook=reject_duplicate_json
)
expected_canonical_provenance = (
    json.dumps(provenance, sort_keys=True, separators=(",", ":")) + "\n"
).encode("utf-8")
reproducibility = provenance.get("reproducibility", {})
if (
    provenance_bytes != expected_canonical_provenance
    or canonical_provenance.get("document") != provenance
    or provenance.get("schema") != "hepta.release_image_provenance.v3"
    or provenance.get("image_ref") != summary.get("image")
    or provenance.get("image_id") != summary.get("image_id")
    or provenance.get("source_revision") != summary.get("source_revision")
    or provenance.get("source_tree") != summary.get("source_tree")
    or provenance.get("source_date_epoch") != summary.get("source_date_epoch")
    or type(reproducibility.get("independent_no_cache_builds")) is not int
    or type(reproducibility.get("identical_image_ids")) is not bool
    or type(reproducibility.get("extracted_binaries_identical")) is not bool
    or type(reproducibility.get("extracted_sboms_identical")) is not bool
    or reproducibility
    != {
        "independent_no_cache_builds": 2,
        "identical_image_ids": True,
        "extracted_binaries_identical": True,
        "extracted_sboms_identical": True,
    }
    or provenance.get("compose_postgres_sigkill_smoke") is not True
):
    raise RuntimeError("resource evidence image provenance release identity differs")
committed_inputs = image_build.get("committed_inputs", {})
expected_inputs = {
    "dockerfile_sha256": provenance.get("dockerfile_sha256"),
    "cargo_lock_sha256": provenance.get("cargo_lock_sha256"),
    "rust_toolchain_sha256": provenance.get("rust_toolchain_sha256"),
    "sbom_sha256": provenance.get("application_sbom", {}).get("sha256"),
    "vendor_manifest_sha256": provenance.get("vendor_manifest_sha256"),
    "runtime_binary_sha256": provenance.get("runtime_binary", {}).get("sha256"),
}
if committed_inputs != expected_inputs or committed_inputs != expected_contract["image_build"].get(
    "committed_inputs"
) or any(
    not sha256_digest.fullmatch(value or "") for value in committed_inputs.values()
):
    raise RuntimeError("resource evidence committed input hashes differ")
image_inspect = json.loads(
    read_artifact("hepta-image-inspect.json").decode("utf-8"),
    object_pairs_hook=reject_duplicate_json,
)
if not isinstance(image_inspect, list) or len(image_inspect) != 1:
    raise RuntimeError("resource evidence image inspection shape differs")
image = image_inspect[0]
labels = image.get("Config", {}).get("Labels", {})
if (
    image.get("Id") != summary.get("image_id")
    or labels.get("org.opencontainers.image.revision") != summary.get("source_revision")
    or labels.get("org.opencontainers.image.source")
    != "https://github.com/TrillionniumFoundation/CEX.git"
    or labels.get("org.trillionnium.source.tree") != summary.get("source_tree")
    or labels.get("org.trillionnium.sbom.sha256") != committed_inputs["sbom_sha256"]
    or labels.get("org.trillionnium.cargo-lock.sha256")
    != committed_inputs["cargo_lock_sha256"]
    or labels.get("org.trillionnium.dockerfile.sha256")
    != committed_inputs["dockerfile_sha256"]
    or labels.get("org.trillionnium.rust-toolchain.sha256")
    != committed_inputs["rust_toolchain_sha256"]
    or labels.get("io.trillionnium.hepta.source-date-epoch")
    != str(summary.get("source_date_epoch"))
    or labels.get("io.trillionnium.hepta.source-tree") != summary.get("source_tree")
    or labels.get("io.trillionnium.hepta.application-sbom.path")
    != "/usr/share/doc/hepta-research-league/sbom.cdx.json"
    or labels.get("io.trillionnium.hepta.application-sbom.sha256")
    != committed_inputs["sbom_sha256"]
    or labels.get("io.trillionnium.hepta.runtime-binary.sha256")
    != committed_inputs["runtime_binary_sha256"]
    or labels.get("io.trillionnium.hepta.runtime-base")
    != "gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98"
    or labels.get("io.trillionnium.hepta.builder-base")
    != "docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb"
):
    raise RuntimeError("resource evidence image labels differ from the closure")
teardown = summary.get("teardown", {})
if teardown != {
    "compose_project_absent": True,
    "named_volumes_absent": True,
    "private_scratch_and_tokens_removed": True,
}:
    raise RuntimeError("resource evidence summary does not prove teardown")
if summary.get("publication_authority") != {
    "parent_dev_inode_owner_mode": expected_parent_identity,
    "evidence_dev_inode_owner_mode": expected_staging_identity,
    "target_basename": target_name,
    "retained_dirfds": True,
    "exact_artifact_set_verified": True,
    "manifests_verified": True,
    "post_rename_inode_verified": True,
}:
    raise RuntimeError("resource evidence summary publication authority differs")
if summary.get("provenance", {}).get("payload_manifest_sha256") != hashlib.sha256(
    payload_manifest
).hexdigest():
    raise RuntimeError("resource evidence summary does not bind PAYLOAD.SHA256")

os.fsync(staging_fd)
libc = ctypes.CDLL(None, use_errno=True)
renameat2 = libc.renameat2
renameat2.argtypes = [
    ctypes.c_int,
    ctypes.c_char_p,
    ctypes.c_int,
    ctypes.c_char_p,
    ctypes.c_uint,
]
renameat2.restype = ctypes.c_int


def rename_noreplace(source, destination):
    if renameat2(
        parent_fd,
        os.fsencode(source),
        parent_fd,
        os.fsencode(destination),
        1,
    ) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), destination)


renamed = False
try:
    rename_noreplace(staging_name, target_name)
    renamed = True
    target_fd = os.open(target_name, directory_flags, dir_fd=parent_fd)
    try:
        if identity(os.fstat(target_fd)) != expected_staging_identity:
            raise RuntimeError("published evidence target inode differs from staging")
        if set(os.listdir(target_fd)) != expected_all:
            raise RuntimeError("published evidence target artifact set differs")
        actual_closure_manifest_sha256 = digest_artifact("SHA256SUMS")
        if actual_closure_manifest_sha256 != expected_closure_manifest_sha256:
            raise RuntimeError("published evidence closure manifest digest differs")
        os.fsync(target_fd)
    finally:
        os.close(target_fd)
    try:
        os.stat(staging_name, dir_fd=parent_fd, follow_symlinks=False)
    except FileNotFoundError:
        pass
    else:
        raise RuntimeError("staging name survived atomic publication")
    os.fsync(parent_fd)
except Exception:
    if renamed:
        try:
            rename_noreplace(target_name, staging_name)
            os.fsync(parent_fd)
        except Exception as rollback_error:
            raise RuntimeError(
                "resource evidence publication failed and rollback also failed"
            ) from rollback_error
    raise
print(actual_closure_manifest_sha256)
PY
)
evidence_published=true
evidence_dir=$evidence_target
close_evidence_fds

echo "Hepta Receipt V2 512 MiB resource gate: PASS evidence=$evidence_dir closure_manifest_sha256=$closure_manifest_sha256 default_peak=$default_final_peak max_peak=$max_final_peak"

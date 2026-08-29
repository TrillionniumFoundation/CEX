#!/usr/bin/env bash
set -euo pipefail
set +x

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
service_root="$repo_root/services/paper-raid-bff"
runner_root="$service_root/browser-e2e"
allow_dirty=${PAPER_RAID_BFF_BROWSER_ALLOW_DIRTY:-0}
evidence_output_dir=${PAPER_RAID_BFF_BROWSER_EVIDENCE_OUTPUT_DIR:-}
run_id="paper-raid-mobile-a11y-$$"
scratch_dir=$(mktemp -d)
evidence_dir="$scratch_dir/evidence"
credentials="$scratch_dir/credentials.json"
runtime_env="$scratch_dir/bff.env"
network_name="$run_id-net"
postgres_name="$run_id-postgres"
mock_name="$run_id-hepta"
bff_name="$run_id-bff"
runner_name="$run_id-chromium"
runner_image="$run_id-runner:gate"
postgres_image=postgres@sha256:ef257d85f76e48da1c64832459b59fcaba1a4dac97bf5d7450c77753542eee94
playwright_base=mcr.microsoft.com/playwright@sha256:2f29369043d81d6d69a815ceb80760f55e85f5020371ad06a4d996f18503ad1c
rust_builder_image=docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb
buildx_version=v0.36.1
buildx_url=https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64
buildx_sha256=48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778
buildx_source=${PAPER_RAID_BUILDX_BIN:-}
player_id=00000000-0000-4000-8000-000000000011
binding_id=00000000-0000-4000-8000-000000000021
grant_id=00000000-0000-4000-8000-000000000031
login_key=paper-raid-mobile-a11y-author-captain-login-key-0001
agent_digest="sha256:${buildx_sha256}"

cleanup() {
  sudo -n docker rm -f "$runner_name" >/dev/null 2>&1 || true
  sudo -n docker rm -f "$bff_name" "$mock_name" "$postgres_name" >/dev/null 2>&1 || true
  sudo -n docker network rm "$network_name" >/dev/null 2>&1 || true
  sudo -n docker image rm -f "$runner_image" >/dev/null 2>&1 || true
  case "$scratch_dir" in
    /tmp/tmp.*) sudo -n rm -rf -- "$scratch_dir" >/dev/null 2>&1 || true ;;
    *) echo "refusing to remove unexpected browser accessibility scratch path: $scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT
umask 077

case "$allow_dirty" in
  0 | 1) ;;
  *) echo "PAPER_RAID_BFF_BROWSER_ALLOW_DIRTY must be 0 or 1" >&2; exit 2 ;;
esac
if [[ ! -f "$repo_root/PROJECT_ID" ]] || [[ "$(<"$repo_root/PROJECT_ID")" != "hepta-control-plane" ]]; then
  echo "browser accessibility gate is outside the Hepta physical root" >&2
  exit 1
fi
source_revision=$(git -C "$repo_root" rev-parse HEAD)
source_tree=$(git -C "$repo_root" rev-parse HEAD^{tree})
source_status=$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)
source_state=clean
if [[ -n "$source_status" ]]; then
  if [[ "$allow_dirty" != "1" ]]; then
    echo "browser accessibility release evidence requires a clean source tree" >&2
    exit 1
  fi
  source_state=dirty-development-only
fi
source_snapshot="$scratch_dir/source-snapshot"
install -d -m 0700 "$source_snapshot"
hash_file_set() {
  local root=$1
  local list=$2
  (
    while IFS= read -r -d '' path; do
      printf '%s\0' "$path"
      if [[ -L "$root/$path" ]]; then
        printf 'symlink\0%s\0' "$(readlink -- "$root/$path")"
      elif [[ -f "$root/$path" ]]; then
        printf 'file\0%s\0' "$(sha256sum "$root/$path" | awk '{print $1}')"
      else
        echo "browser accessibility source file vanished: $path" >&2
        exit 1
      fi
    done <"$list"
  ) | sha256sum | awk '{print $1}'
}
if [[ "$source_state" == "clean" ]]; then
  git -C "$repo_root" archive --format=tar "$source_revision" | tar -xf - -C "$source_snapshot"
else
  source_files_before="$scratch_dir/source-files-before.list"
  source_files_after="$scratch_dir/source-files-after.list"
  git -C "$repo_root" ls-files --cached --others --exclude-standard -z | sort -z >"$source_files_before"
  source_digest_before=$(hash_file_set "$repo_root" "$source_files_before")
  tar -C "$repo_root" --null -T "$source_files_before" -cf - | tar -xf - -C "$source_snapshot"
  git -C "$repo_root" ls-files --cached --others --exclude-standard -z | sort -z >"$source_files_after"
  cmp -s "$source_files_before" "$source_files_after" || {
    echo "browser accessibility source file set changed while snapshotting" >&2
    exit 1
  }
  source_digest_after=$(hash_file_set "$repo_root" "$source_files_after")
  source_snapshot_file_digest=$(hash_file_set "$source_snapshot" "$source_files_before")
  if [[ "$source_digest_before" != "$source_digest_after" || \
        "$source_digest_before" != "$source_snapshot_file_digest" ]]; then
    echo "browser accessibility source bytes changed while snapshotting" >&2
    exit 1
  fi
fi
find "$source_snapshot" -type d -exec chmod a+rx,a-w {} +
find "$source_snapshot" -type f -exec chmod a+r,a-w {} +
source_snapshot_sha256=$(
  tar -C "$source_snapshot" --sort=name --mtime='UTC 1970-01-01' \
    --owner=0 --group=0 --numeric-owner -cf - . | sha256sum | awk '{print $1}'
)
[[ "$source_snapshot_sha256" =~ ^[0-9a-f]{64}$ ]] || {
  echo "browser accessibility source snapshot digest is invalid" >&2
  exit 1
}
input_service_root="$source_snapshot/services/paper-raid-bff"
input_runner_root="$input_service_root/browser-e2e"
if [[ "$(git -C "$repo_root" rev-parse HEAD)" != "$source_revision" || \
      "$(git -C "$repo_root" rev-parse HEAD^{tree})" != "$source_tree" || \
      "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)" != "$source_status" ]]; then
  echo "browser accessibility source identity changed while snapshotting" >&2
  exit 1
fi
if [[ -n "$evidence_output_dir" ]]; then
  case "$evidence_output_dir" in
    /*) ;;
    *) echo "PAPER_RAID_BFF_BROWSER_EVIDENCE_OUTPUT_DIR must be absolute" >&2; exit 2 ;;
  esac
  if [[ -L "$evidence_output_dir" || ( -e "$evidence_output_dir" && ! -d "$evidence_output_dir" ) ]]; then
    echo "browser accessibility evidence output must be a real directory" >&2
    exit 2
  fi
  if [[ -d "$evidence_output_dir" && -n "$(find "$evidence_output_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
    echo "browser accessibility evidence output must start empty" >&2
    exit 2
  fi
fi

for required in sudo docker jq openssl sha256sum git rg cargo rustc flock node tar; do
  command -v "$required" >/dev/null || { echo "missing browser accessibility dependency: $required" >&2; exit 1; }
done
for required_file in \
  "$input_runner_root/Dockerfile" \
  "$input_runner_root/mobile-a11y.mjs" \
  "$input_runner_root/mock-hepta.mjs" \
  "$input_service_root/docker/workspace.Cargo.toml" \
  "$input_service_root/docker/Cargo.lock" \
  "$input_service_root/scripts/download-pinned-buildx.sh"
do
  [[ -f "$required_file" && ! -L "$required_file" ]] || {
    echo "browser accessibility source is absent or a symlink: $required_file" >&2
    exit 1
  }
done
rg -q --fixed-strings "FROM $playwright_base" "$input_runner_root/Dockerfile" || {
  echo "browser accessibility runner base digest drifted" >&2
  exit 1
}

mkdir -m 0700 "$evidence_dir"
printf '%s\n' "$(jq -cn --arg login_key "$login_key" '{
  schema:"hepta.paper_raid.browser_mobile_a11y.credentials.v1",
  login_key:$login_key
}')" >"$credentials"
chmod 0600 "$credentials"

session_key=$(openssl rand -base64 32 | tr -d '\n')
signing_seed=$(openssl rand -base64 32 | tr -d '\n')
consumer_public_key=$(
  PAPER_RAID_BFF_A11Y_SIGNING_SEED_B64="$signing_seed" node -e '
    const { createPrivateKey, createPublicKey } = require("node:crypto");
    const seed = Buffer.from(process.env.PAPER_RAID_BFF_A11Y_SIGNING_SEED_B64 || "", "base64");
    if (seed.length !== 32 || seed.toString("base64") !== process.env.PAPER_RAID_BFF_A11Y_SIGNING_SEED_B64) process.exit(1);
    const privateKey = createPrivateKey({
      key: Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), seed]),
      format: "der",
      type: "pkcs8",
    });
    const publicDer = createPublicKey(privateKey).export({ format: "der", type: "spki" });
    process.stdout.write(publicDer.subarray(-32).toString("base64"));
  '
)
[[ "$(printf '%s' "$consumer_public_key" | openssl base64 -d -A | wc -c)" -eq 32 ]] || {
  echo "browser accessibility Consumer public key derivation failed" >&2
  exit 1
}
alpha_identities=$(jq -cn \
  --arg first "$login_key" \
  '[
    {login_key:$first,subject_id:"alpha-author-captain",display_name:"Author Captain",nakama_user_id:"00000000-0000-4000-8000-000000000001",player_id:"00000000-0000-4000-8000-000000000011",scopes:["author"],author_roles:["captain"]},
    {login_key:"paper-raid-mobile-a11y-author-evidence-login-key-0002",subject_id:"alpha-author-evidence",display_name:"Author Evidence",nakama_user_id:"00000000-0000-4000-8000-000000000002",player_id:"00000000-0000-4000-8000-000000000012",scopes:["author"],author_roles:["evidence"]},
    {login_key:"paper-raid-mobile-a11y-author-experiment-login-key-0003",subject_id:"alpha-author-experiment",display_name:"Author Experiment",nakama_user_id:"00000000-0000-4000-8000-000000000003",player_id:"00000000-0000-4000-8000-000000000013",scopes:["author"],author_roles:["experiment"]},
    {login_key:"paper-raid-mobile-a11y-evaluator-login-key-0004",subject_id:"alpha-evaluator",display_name:"Independent Evaluator",nakama_user_id:"00000000-0000-4000-8000-000000000004",player_id:"00000000-0000-4000-8000-000000000014",scopes:["evaluator"],author_roles:[]},
    {login_key:"paper-raid-mobile-a11y-reviewer-one-login-key-0005",subject_id:"alpha-reviewer-1",display_name:"Independent Reviewer One",nakama_user_id:"00000000-0000-4000-8000-000000000005",player_id:"00000000-0000-4000-8000-000000000015",scopes:["reviewer"],author_roles:[]},
    {login_key:"paper-raid-mobile-a11y-reviewer-two-login-key-0006",subject_id:"alpha-reviewer-2",display_name:"Independent Reviewer Two",nakama_user_id:"00000000-0000-4000-8000-000000000006",player_id:"00000000-0000-4000-8000-000000000016",scopes:["reviewer"],author_roles:[]},
    {login_key:"paper-raid-mobile-a11y-reproducer-login-key-0007",subject_id:"alpha-reproducer",display_name:"Independent Reproducer",nakama_user_id:"00000000-0000-4000-8000-000000000007",player_id:"00000000-0000-4000-8000-000000000017",scopes:["reproducer"],author_roles:[]}
  ]')
{
  printf 'PAPER_RAID_BFF_BIND=0.0.0.0:7020\n'
  printf 'PAPER_RAID_BFF_EDGE_SCOPE=container_loopback_publish\n'
  printf 'PAPER_RAID_BFF_PUBLIC_ORIGIN=http://127.0.0.1:7020\n'
  printf 'PAPER_RAID_BFF_DATABASE_URL=postgres://paper_raid_bff:paper-raid-browser-test@postgres/paper_raid_bff\n'
  printf 'PAPER_RAID_BFF_SESSION_KEY_B64=%s\n' "$session_key"
  printf 'PAPER_RAID_BFF_IDENTITY_MODE=fixed_alpha\n'
  printf 'PAPER_RAID_BFF_ALPHA_IDENTITIES_JSON=%s\n' "$alpha_identities"
  printf 'PAPER_RAID_BFF_HEPTA_BASE_URL=http://hepta:7011\n'
  printf 'PAPER_RAID_BFF_CONSUMER_ISSUER=browser-mobile-a11y\n'
  printf 'PAPER_RAID_BFF_HEPTA_AUDIENCE=hepta-research-league\n'
  printf 'PAPER_RAID_BFF_CONSUMER_KEY_ID=browser-mobile-a11y-key-1\n'
  printf 'PAPER_RAID_BFF_CONSUMER_SIGNING_SEED_B64=%s\n' "$signing_seed"
  printf 'PAPER_RAID_BFF_NAKAMA_BASE_URL=http://hepta:7011\n'
  printf 'PAPER_RAID_BFF_NAKAMA_HTTP_KEY=browser-mobile-a11y-nakama-key\n'
  printf 'PAPER_RAID_BFF_CAS_ENDPOINT=http://hepta:7011\n'
  printf 'PAPER_RAID_BFF_CAS_BUCKET=browser-mobile-a11y\n'
  printf 'PAPER_RAID_BFF_CAS_REGION=us-east-1\n'
  printf 'PAPER_RAID_BFF_CAS_ACCESS_KEY_ID=browser-mobile-a11y-access\n'
  printf 'PAPER_RAID_BFF_CAS_SECRET_ACCESS_KEY=browser-mobile-a11y-secret\n'
  printf 'PAPER_RAID_BFF_CAS_READY_DIGEST=sha256:%064d\n' 0
  printf 'PAPER_RAID_BFF_CAS_READY_MEDIA_TYPE=application/json\n'
} >"$runtime_env"
chmod 0600 "$runtime_env"

[[ "$(rustc --version)" == "rustc 1.95.0 (59807616e 2026-04-14)" ]] || {
  echo "browser accessibility host rustc does not match the pinned builder toolchain" >&2
  exit 1
}
[[ "$(cargo --version)" == "cargo 1.95.0 (f2d3ce0bd 2026-03-21)" ]] || {
  echo "browser accessibility host Cargo does not match the pinned builder toolchain" >&2
  exit 1
}
build_root="$scratch_dir/reduced-workspace"
target_root="$scratch_dir/cargo-target"
if [[ -n "${PAPER_RAID_BFF_BROWSER_CARGO_TARGET_DIR:-}" ]]; then
  if [[ "$allow_dirty" != "1" ]]; then
    echo "an external Cargo target is allowed only for dirty development runs" >&2
    exit 2
  fi
  target_root=$PAPER_RAID_BFF_BROWSER_CARGO_TARGET_DIR
fi
if [[ -L "$target_root" || ( -e "$target_root" && ! -d "$target_root" ) ]]; then
  echo "browser accessibility Cargo target must be a real directory" >&2
  exit 1
fi
install -d -m 0700 \
  "$build_root/crates/hepta-paper-raid-contracts" \
  "$build_root/crates/hepta-paper-raid-contracts/assets" \
  "$build_root/services/paper-raid-bff/src" \
  "$build_root/services/paper-raid-bff/migrations" \
  "$target_root"
if [[ "$(stat -c %u "$target_root")" != "$(id -u)" ]] || \
   [[ -n "$(find "$target_root" -maxdepth 0 -perm /022 -print -quit)" ]]; then
  echo "browser accessibility Cargo target must be owner-only" >&2
  exit 1
fi
cp "$input_service_root/docker/workspace.Cargo.toml" "$build_root/Cargo.toml"
cp "$input_service_root/docker/Cargo.lock" "$build_root/Cargo.lock"
cp "$source_snapshot/crates/hepta-paper-raid-contracts/Cargo.toml" \
  "$build_root/crates/hepta-paper-raid-contracts/Cargo.toml"
cp -a "$source_snapshot/crates/hepta-paper-raid-contracts/src/." \
  "$build_root/crates/hepta-paper-raid-contracts/src"
cp -a "$source_snapshot/crates/hepta-paper-raid-contracts/assets/." \
  "$build_root/crates/hepta-paper-raid-contracts/assets"
cp "$input_service_root/Cargo.toml" "$build_root/services/paper-raid-bff/Cargo.toml"
cp -a "$input_service_root/src/." "$build_root/services/paper-raid-bff/src"
cp -a "$input_service_root/migrations/." "$build_root/services/paper-raid-bff/migrations"
if [[ -n "$(find "$build_root" -type l -print -quit)" ]]; then
  echo "browser accessibility reduced Cargo workspace contains a symlink" >&2
  exit 1
fi
exec 9>/tmp/trnm-paper-raid-cargo-gate.lock
flock 9
(
  cd "$build_root"
  CARGO_TARGET_DIR="$target_root" \
    env -u SOURCE_DATE_EPOCH cargo build --locked --offline --release -p paper-raid-bff
)
flock -u 9
candidate_binary="$target_root/release/paper-raid-bff"
[[ -f "$candidate_binary" && -x "$candidate_binary" && ! -L "$candidate_binary" ]] || {
  echo "browser accessibility candidate binary was not produced" >&2
  exit 1
}
chmod 0555 "$candidate_binary"
candidate_binary_sha256=$(sha256sum "$candidate_binary" | awk '{print $1}')
[[ "$candidate_binary_sha256" =~ ^[0-9a-f]{64}$ ]] || {
  echo "browser accessibility candidate binary digest is invalid" >&2
  exit 1
}
if [[ "$source_snapshot_sha256" != "$(tar -C "$source_snapshot" --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner -cf - . | sha256sum | awk '{print $1}')" ]]; then
  echo "browser accessibility immutable source snapshot changed while compiling the candidate" >&2
  exit 1
fi
echo "browser accessibility stage: candidate-compiled-with-pinned-host-toolchain" >&2

docker_config="$scratch_dir/docker-config"
buildx_plugin="$docker_config/cli-plugins/docker-buildx"
mkdir -p "$(dirname "$buildx_plugin")"
if [[ -n "$buildx_source" ]]; then
  bash "$input_service_root/scripts/download-pinned-buildx.sh" \
    "$buildx_url" "$buildx_sha256" "$buildx_plugin" "$buildx_source"
else
  bash "$input_service_root/scripts/download-pinned-buildx.sh" \
    "$buildx_url" "$buildx_sha256" "$buildx_plugin"
fi
chmod 0500 "$buildx_plugin"
docker_cli=(sudo -n env "DOCKER_CONFIG=$docker_config" docker)
"${docker_cli[@]}" buildx version | rg -q --fixed-strings "$buildx_version" || {
  echo "browser accessibility disposable Buildx version gate failed" >&2
  exit 1
}

"${docker_cli[@]}" buildx build \
  --pull=false \
  --progress plain \
  --load \
  --provenance=false \
  --sbom=false \
  --platform linux/amd64 \
  --file "$input_runner_root/Dockerfile" \
  --tag "$runner_image" \
  "$source_snapshot" >&2
echo "browser accessibility stage: runner-built" >&2

actual_base=$(sudo -n docker image inspect "$runner_image" --format '{{index .Config.Labels "org.trillionnium.playwright.base.digest"}}')
[[ "$actual_base" == sha256:2f29369043d81d6d69a815ceb80760f55e85f5020371ad06a4d996f18503ad1c ]] || {
  echo "browser accessibility runner provenance label mismatch" >&2
  exit 1
}
echo "browser accessibility stage: runner-provenance-verified" >&2

sudo -n docker network create "$network_name" >/dev/null
sudo -n docker run -d \
  --name "$postgres_name" \
  --network "$network_name" \
  --network-alias postgres \
  --tmpfs /var/lib/postgresql/data:rw,nosuid,nodev,size=768m \
  --env POSTGRES_USER=paper_raid_bff \
  --env POSTGRES_PASSWORD=paper-raid-browser-test \
  --env POSTGRES_DB=paper_raid_bff \
  "$postgres_image" >/dev/null
for _ in $(seq 1 120); do
  if sudo -n docker exec "$postgres_name" pg_isready -U paper_raid_bff -d paper_raid_bff >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
sudo -n docker exec "$postgres_name" pg_isready -U paper_raid_bff -d paper_raid_bff >/dev/null
echo "browser accessibility stage: postgres-ready" >&2

sudo -n docker run -d \
  --name "$mock_name" \
  --network "$network_name" \
  --network-alias hepta \
  --read-only \
  --tmpfs /tmp:rw,nosuid,nodev,size=32m \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --pids-limit 128 \
  --memory 256m \
  --env "PAPER_RAID_BFF_A11Y_PLAYER_ID=$player_id" \
  --env "PAPER_RAID_BFF_A11Y_BINDING_ID=$binding_id" \
  --env "PAPER_RAID_BFF_A11Y_AGENT_DIGEST=$agent_digest" \
  --env "PAPER_RAID_BFF_A11Y_CONSUMER_PUBLIC_KEY_B64=$consumer_public_key" \
  --entrypoint node \
  "$runner_image" /runner/mock-hepta.mjs >/dev/null
for _ in $(seq 1 120); do
  if sudo -n docker logs "$mock_name" 2>&1 | rg -q --fixed-strings 'hepta.paper_raid.browser_mobile_a11y.mock_ready.v1'; then
    break
  fi
  sleep 0.25
done
sudo -n docker logs "$mock_name" 2>&1 | rg -q --fixed-strings 'hepta.paper_raid.browser_mobile_a11y.mock_ready.v1'
echo "browser accessibility stage: typed-hepta-ready" >&2

sudo -n docker run -d \
  --name "$bff_name" \
  --network "$network_name" \
  --user 65532:65532 \
  --read-only \
  --tmpfs /tmp:rw,nosuid,nodev,size=64m \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --pids-limit 256 \
  --memory 1g \
  --env-file "$runtime_env" \
  --mount "type=bind,src=$candidate_binary,dst=/candidate/paper-raid-bff,readonly" \
  --entrypoint /candidate/paper-raid-bff \
  "$rust_builder_image" >/dev/null
for _ in $(seq 1 240); do
  if [[ "$(sudo -n docker inspect "$bff_name" --format '{{.State.Running}}')" != "true" ]]; then
    sudo -n docker logs "$bff_name" >&2 || true
    echo "browser accessibility candidate BFF exited before /health" >&2
    exit 1
  fi
  if sudo -n docker exec "$bff_name" bash -c 'exec 3<>/dev/tcp/127.0.0.1/7020; printf "GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n" >&3; grep -q "200 OK" <&3' >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
sudo -n docker exec "$bff_name" bash -c 'exec 3<>/dev/tcp/127.0.0.1/7020; printf "GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n" >&3; grep -q "200 OK" <&3'
echo "browser accessibility stage: candidate-bff-ready" >&2

sudo -n docker exec -i "$postgres_name" psql -v ON_ERROR_STOP=1 -U paper_raid_bff -d paper_raid_bff >/dev/null <<SQL
INSERT INTO paper_raid_bff_agent_pairing_grants(
  grant_id, subject_id, player_id, code_hash, state,
  pinned_request_hash, pinned_binding_id, created_at, expires_at,
  pinned_at, consumed_at, pair_response_status, pair_response_body, updated_at
) VALUES (
  '$grant_id', 'alpha-author-captain', '$player_id', decode(repeat('31', 32), 'hex'), 'consumed',
  decode(repeat('32', 32), 'hex'), '$binding_id', now() - interval '2 minutes', now() + interval '3 minutes',
  now() - interval '1 minute', now() - interval '1 minute', 200, convert_to('{}', 'UTF8'), now() - interval '1 minute'
);
WITH disclosure AS (
  SELECT '{"schema":"hepta.paper_raid.agent_capability_disclosure.v1","assurance":"self_declared_unverified","capabilities":["experiment_execution"],"resource_classes":["cpu","sandbox"],"max_parallel_tasks":1}'::jsonb AS value
)
INSERT INTO paper_raid_bff_agent_bridge_bindings(
  binding_id, grant_id, last_pairing_grant_id, subject_id, player_id,
  agent_id, agent_key_id, capability_disclosure_hash,
  capability_disclosure, binding_record, paired_at, last_verified_at
)
SELECT
  '$binding_id', '$grant_id', '$grant_id', 'alpha-author-captain', '$player_id',
  'did:trnm:agent:browser-a11y-prequalified-v1', '${agent_digest}', '${agent_digest}',
  value,
  jsonb_build_object(
    'binding_id', '$binding_id',
    'player_id', '$player_id',
    'agent_id', 'did:trnm:agent:browser-a11y-prequalified-v1',
    'agent_key_id', '${agent_digest}',
    'agent_public_key_hash', '${agent_digest}',
    'capability_disclosure_hash', '${agent_digest}',
    'capability_disclosure', value,
    'status', 'active',
    'version', 1,
    'created_at', '2026-08-18T00:00:00Z',
    'updated_at', '2026-08-18T00:00:00Z'
  ),
  now() - interval '1 minute', now() - interval '1 minute'
FROM disclosure;
SQL
echo "browser accessibility stage: prequalified-binding-seeded" >&2

sudo -n docker run --rm \
  --name "$runner_name" \
  --network "container:$bff_name" \
  --user "$(id -u):$(id -g)" \
  --read-only \
  --shm-size 1g \
  --tmpfs /tmp:rw,nosuid,nodev,size=1g \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --pids-limit 512 \
  --memory 2g \
  --cpus 2 \
  --env HOME=/tmp \
  --env PAPER_RAID_BFF_BROWSER_BASE_URL=http://127.0.0.1:7020 \
  --env PAPER_RAID_BFF_BROWSER_CREDENTIALS_FILE=/run/secrets/credentials.json \
  --env PAPER_RAID_BFF_BROWSER_EVIDENCE_DIR=/evidence \
  --env "PAPER_RAID_BFF_BROWSER_SOURCE_REVISION=$source_revision" \
  --env "PAPER_RAID_BFF_BROWSER_SOURCE_TREE=$source_tree" \
  --env "PAPER_RAID_BFF_BROWSER_SOURCE_STATE=$source_state" \
  --env "PAPER_RAID_BFF_BROWSER_SOURCE_SNAPSHOT_SHA256=$source_snapshot_sha256" \
  --env "PAPER_RAID_BFF_BROWSER_CANDIDATE_BINARY_SHA256=$candidate_binary_sha256" \
  --mount "type=bind,src=$credentials,dst=/run/secrets/credentials.json,readonly" \
  --mount "type=bind,src=$evidence_dir,dst=/evidence" \
  --entrypoint node \
  "$runner_image" /runner/mobile-a11y.mjs

if [[ "$source_snapshot_sha256" != "$(tar -C "$source_snapshot" --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner -cf - . | sha256sum | awk '{print $1}')" || \
      "$(git -C "$repo_root" rev-parse HEAD)" != "$source_revision" || \
      "$(git -C "$repo_root" rev-parse HEAD^{tree})" != "$source_tree" || \
      "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)" != "$source_status" ]]; then
  echo "browser accessibility source identity changed before evidence sealing" >&2
  exit 1
fi

jq -e \
  --arg revision "$source_revision" \
  --arg tree "$source_tree" \
  --arg state "$source_state" \
  --arg snapshot_sha256 "$source_snapshot_sha256" \
  --arg binary_sha256 "$candidate_binary_sha256" \
  '.schema == "hepta.paper_raid.browser_mobile_a11y.result.v1"
   and .source_revision == $revision
   and .source_tree == $tree
   and .source_state == $state
   and .source_snapshot_sha256 == $snapshot_sha256
   and .runtime_kind == "pinned_host_toolchain_container_test_only"
   and .candidate_binary_sha256 == $binary_sha256
   and .developer_json_fallback_used == false
   and .production_bridge_pairing_proved == false
   and .production_bridge_execution_proved == false
   and .post_agent_focus_transition_real_e2e_proved == false
   and .manual_screen_reader_certification == false
   and .passed == true
   and (.artifacts | length) == 24' \
  "$evidence_dir/result.json" >/dev/null
[[ "$(find "$evidence_dir" -maxdepth 1 -type f -name '*.png' | wc -l)" -eq 12 ]]
[[ "$(find "$evidence_dir" -maxdepth 1 -type f -name '*.json' ! -name result.json | wc -l)" -eq 12 ]]
sha256sum "$evidence_dir"/* >&2
if [[ -n "$evidence_output_dir" ]]; then
  install -d -m 0700 "$evidence_output_dir"
  if [[ "$(realpath -e "$evidence_output_dir")" != "$evidence_output_dir" ]] || \
     [[ "$(stat -c %u "$evidence_output_dir")" != "$(id -u)" ]] || \
     [[ -n "$(find "$evidence_output_dir" -maxdepth 0 -perm /077 -print -quit)" ]]; then
    echo "browser accessibility evidence output ownership or canonical path is invalid" >&2
    exit 1
  fi
  cp -a "$evidence_dir/." "$evidence_output_dir/"
  find "$evidence_output_dir" -type f -exec chmod 0600 {} +
  echo "browser accessibility evidence retained at: $evidence_output_dir" >&2
fi
echo "paper-raid-bff real 390/430 Chromium accessibility gate: ok" >&2

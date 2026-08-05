#!/usr/bin/env bash
set -euo pipefail
set +x

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
runner_root="$repo_root/services/paper-raid-bff/browser-e2e"
base_url=${PAPER_RAID_BFF_BROWSER_BASE_URL:-http://127.0.0.1:17020}
require_lobby=${PAPER_RAID_BFF_BROWSER_REQUIRE_LOBBY:-1}
runtime_env=${PAPER_RAID_BFF_BROWSER_RUNTIME_ENV:-/etc/trillionnium-paper-raid/runtime.env}
agent_bindings_file=${PAPER_RAID_BFF_BROWSER_AGENT_BINDINGS_FILE:-}
supplied_credentials=${PAPER_RAID_BFF_BROWSER_CREDENTIALS_FILE:-}
scratch_dir=$(mktemp -d)
credentials="$scratch_dir/credentials.json"
image_name=paper-raid-bff-browser-e2e:gate-$$
buildx_version=v0.36.1
buildx_url=https://github.com/docker/buildx/releases/download/v0.36.1/buildx-v0.36.1.linux-amd64
buildx_sha256=48af8a397ebd60178778bf63611dbcebe5f5e7a9be90eb9147b24b9587455778
playwright_version=1.49.1
playwright_base_digest=sha256:ad57c625d284e8d287abcd40d18434582b2e354de71c8428cb080112b0c45960

cleanup() {
  sudo -n docker image rm -f "$image_name" >/dev/null 2>&1 || true
  case "$scratch_dir" in
    /tmp/tmp.*) sudo -n rm -rf -- "$scratch_dir" >/dev/null 2>&1 || true ;;
    *) echo "refusing to remove unexpected browser E2E scratch path: $scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT
umask 077

case "$require_lobby" in
  0 | 1) ;;
  *) echo "PAPER_RAID_BFF_BROWSER_REQUIRE_LOBBY must be 0 or 1" >&2; exit 2 ;;
esac

if [[ -n "$supplied_credentials" ]]; then
  if [[ ! -f "$supplied_credentials" || ! -r "$supplied_credentials" ]]; then
    echo "browser credentials file is not a readable regular file" >&2
    exit 2
  fi
  if [[ -n "$(find "$supplied_credentials" -maxdepth 0 -perm /077 -print -quit)" ]]; then
    echo "browser credentials file must be mode 0600 or stricter" >&2
    exit 2
  fi
  cp "$supplied_credentials" "$credentials"
else
  if [[ ! -f "$runtime_env" || ! -r "$runtime_env" ]]; then
    echo "root-only Paper Raid runtime env is unavailable; provide PAPER_RAID_BFF_BROWSER_CREDENTIALS_FILE" >&2
    exit 2
  fi
  if [[ -n "$(find "$runtime_env" -maxdepth 0 -perm /077 -print -quit)" ]]; then
    echo "Paper Raid runtime env must be mode 0600 or stricter" >&2
    exit 2
  fi
  if [[ -z "$agent_bindings_file" ]]; then
    if [[ "$require_lobby" == "1" ]]; then
      echo "PAPER_RAID_BFF_BROWSER_AGENT_BINDINGS_FILE is required for the Lobby gate" >&2
      exit 2
    fi
    agent_bindings_file="$scratch_dir/no-agent-bindings.json"
    printf 'null\n' >"$agent_bindings_file"
  fi
  if [[ ! -f "$agent_bindings_file" || ! -r "$agent_bindings_file" ]]; then
    echo "Agent bindings file is not a readable regular file" >&2
    exit 2
  fi
  if [[ -n "$(find "$agent_bindings_file" -maxdepth 0 -perm /077 -print -quit)" ]]; then
    echo "Agent bindings file must be mode 0600 or stricter" >&2
    exit 2
  fi
  if [[ "$(stat -c %s "$agent_bindings_file")" -gt 262144 ]]; then
    echo "Agent bindings file exceeds the 256 KiB cap" >&2
    exit 2
  fi
  (
    set +u
    # shellcheck disable=SC1090
    source "$runtime_env"
    set -u
    : "${PAPER_RAID_ALPHA_1_LOGIN_KEY:?missing alpha login key 1}"
    : "${PAPER_RAID_ALPHA_2_LOGIN_KEY:?missing alpha login key 2}"
    : "${PAPER_RAID_ALPHA_3_LOGIN_KEY:?missing alpha login key 3}"
    {
      printf '%s\0' \
        "$PAPER_RAID_ALPHA_1_LOGIN_KEY" \
        "$PAPER_RAID_ALPHA_2_LOGIN_KEY" \
        "$PAPER_RAID_ALPHA_3_LOGIN_KEY"
    } | jq -Rs \
      --slurpfile agent_bindings "$agent_bindings_file" \
      --arg paper_id "${PAPER_RAID_BFF_BROWSER_PAPER_ID:-}" \
      '{
        schema: "hepta.paper_raid.browser_e2e.credentials.v1",
        login_keys: (split("\u0000") | .[0:3]),
        agent_bindings: $agent_bindings[0],
        paper_id: (if $paper_id == "" then null else $paper_id end)
      }' >"$credentials"
  )
fi
chmod 0600 "$credentials"
if [[ "$(stat -c %s "$credentials")" -gt 262144 ]]; then
  echo "browser credentials file exceeds the 256 KiB cap" >&2
  exit 2
fi

jq -e \
  --argjson require_lobby "$require_lobby" \
  '.schema == "hepta.paper_raid.browser_e2e.credentials.v1"
   and (.login_keys | type == "array" and length == 3 and all(type == "string" and length >= 32))
   and (($require_lobby == 0) or (.agent_bindings | type == "array" and length == 3))
   and (.paper_id == null or (.paper_id | type == "string"))' \
  "$credentials" >/dev/null

if [[ "$(jq -r '.dependencies.playwright' "$runner_root/package.json")" != "$playwright_version" ]] || \
   [[ "$(jq -r '.packages["node_modules/playwright"].version' "$runner_root/package-lock.json")" != "$playwright_version" ]] || \
   ! rg -q --fixed-strings \
     "FROM mcr.microsoft.com/playwright@$playwright_base_digest" \
     "$runner_root/Dockerfile"
then
  echo "Playwright package lock and immutable base image are out of sync" >&2
  exit 1
fi

docker_config="$scratch_dir/docker-config"
buildx_plugin="$docker_config/cli-plugins/docker-buildx"
mkdir -p "$(dirname "$buildx_plugin")"
curl \
  --fail \
  --location \
  --proto '=https' \
  --retry 3 \
  --retry-all-errors \
  --retry-delay 2 \
  --show-error \
  --silent \
  --tlsv1.2 \
  "$buildx_url" \
  --output "$buildx_plugin"
actual_buildx_sha256=$(sha256sum "$buildx_plugin" | cut -d' ' -f1)
if [[ "$actual_buildx_sha256" != "$buildx_sha256" ]]; then
  echo "disposable buildx checksum mismatch" >&2
  exit 1
fi
chmod 0500 "$buildx_plugin"
docker_cli=(sudo -n env "DOCKER_CONFIG=$docker_config" docker)
if ! "${docker_cli[@]}" buildx version | rg -q --fixed-strings "$buildx_version"; then
  echo "disposable buildx version gate failed" >&2
  exit 1
fi

"${docker_cli[@]}" buildx build \
  --progress plain \
  --load \
  --provenance=false \
  --sbom=false \
  --platform linux/amd64 \
  --file "$runner_root/Dockerfile" \
  --tag "$image_name" \
  "$repo_root" >&2

actual_base_digest=$(sudo -n docker image inspect "$image_name" \
  --format '{{index .Config.Labels "org.trillionnium.playwright.base.digest"}}')
if [[ "$actual_base_digest" != "$playwright_base_digest" ]]; then
  echo "browser runner base provenance label mismatch" >&2
  exit 1
fi

sudo -n docker run \
  --rm \
  --read-only \
  --network host \
  --shm-size 1g \
  --pids-limit 512 \
  --memory 2g \
  --cpus 2 \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --tmpfs /tmp:rw,nosuid,nodev,size=1g \
  --env HOME=/tmp \
  --env "PAPER_RAID_BFF_BROWSER_BASE_URL=$base_url" \
  --env PAPER_RAID_BFF_BROWSER_CREDENTIALS_FILE=/run/secrets/paper-raid-browser.json \
  --env "PAPER_RAID_BFF_BROWSER_REQUIRE_LOBBY=$require_lobby" \
  --mount "type=bind,src=$credentials,dst=/run/secrets/paper-raid-browser.json,readonly" \
  "$image_name"

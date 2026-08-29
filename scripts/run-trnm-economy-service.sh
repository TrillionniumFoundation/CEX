#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

# This lane is production-like by contract. The repository .env commonly
# carries APP_ENV=dev for the general local stack; leaving that value in the
# environment would make the shared startup guard reject the explicit TRNM
# profile as a conflict. Set the compatibility alias before selecting the
# canonical profile and launching either service.
export APP_ENV=production
export DATABASE_URL="$(cex_effective_database_url)"
export LEDGER_FAIL_FAST=true
export LEDGER_DATABASE_MAX_CONNECTIONS="${LEDGER_DATABASE_MAX_CONNECTIONS:-8}"
export CEX_RUNTIME_PROFILE=trnm-economy
cex_select_runtime_profile
export CONSUMER_ENTRY_RUNTIME_PROFILE=production
export LEDGER_BASE_URL="${LEDGER_BASE_URL:-http://127.0.0.1:7002}"
export CONSUMER_ENTRY_BIND_ADDR="${CONSUMER_ENTRY_BIND_ADDR:-127.0.0.1:8090}"

require_secret() {
  local name="$1"
  local value="${!name:-}"
  if [[ -z "${value//[[:space:]]/}" ]]; then
    echo "$name is required for the production-like TRNM economy lane" >&2
    exit 78
  fi
  if ((${#value} < 32)); then
    echo "$name must contain at least 32 characters" >&2
    exit 78
  fi
  case "$value" in
    REPLACE_*|*REPLACE_WITH_*|local-dev-key|trnm-economy-local-production-*)
      echo "$name still contains a placeholder or local-development credential" >&2
      exit 78
      ;;
  esac
}

require_distinct_secrets() {
  local -a names=("$@")
  local left_name right_name
  local i j
  for ((i = 0; i < ${#names[@]}; i++)); do
    left_name="${names[$i]}"
    for ((j = i + 1; j < ${#names[@]}; j++)); do
      right_name="${names[$j]}"
      if [[ "${!left_name}" == "${!right_name}" ]]; then
        echo "$left_name and $right_name must use distinct credentials" >&2
        exit 78
      fi
    done
  done
}

TRNM_SECRET_ENV_NAMES=(
  LEDGER_ADMIN_TOKEN
  TRNM_VALUE_ENTITLEMENT_SIGNING_SECRET
  TRNM_GAME_AUTHORITY_TOKEN
  TRNM_PLAYER_SESSION_SIGNING_SECRET
  CONSUMER_ENTRY_INGRESS_TOKEN
  CONSUMER_ENTRY_SESSION_AUTH_SECRET
  CEX_GATEWAY_API_KEY
  CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET
)
for secret_name in "${TRNM_SECRET_ENV_NAMES[@]}"; do
  require_secret "$secret_name"
  export "$secret_name"
done
require_distinct_secrets "${TRNM_SECRET_ENV_NAMES[@]}"

: "${TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH:?TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH is required}"
if [[ "$TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH" != /* ]]; then
  echo "TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH must be an absolute mounted path" >&2
  exit 78
fi
if [[ ! -f "$TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH" || ! -r "$TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH" ]]; then
  echo "TRNM entitlement issuer registry is not a readable regular file" >&2
  exit 78
fi
export TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH

export TRNM_REQUIRE_PLAYER_SESSION=true
export TRNM_ALLOW_SYSTEM_ECONOMY_OPERATIONS=true
export TRNM_PRODUCT_ORG_ID="${TRNM_PRODUCT_ORG_ID:-00000000-0000-0000-0000-00000000ce01}"
export CONSUMER_ENTRY_LEDGER_ADMIN_TOKEN="$LEDGER_ADMIN_TOKEN"
export CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS="${CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS:-trnm-native-client}"
export CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE="${CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE:-consumer-entry-api}"
export CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true
export CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true
export CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH="$CEX_PROJECT_ROOT/deploy/trnm-economy/identity-bindings.json"
export CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH="$CEX_PROJECT_ROOT/deploy/trnm-economy/identity-registry.json"
export CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH="$CEX_PROJECT_ROOT/deploy/trnm-economy/identity-approved-revisions.json"
export CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH="$CEX_PROJECT_ROOT/run/trnm-economy/identity-audit.jsonl"
export CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION=true
export CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION=true
export CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR=true
export CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS=cex-trnm-economy-service
export CONSUMER_ENTRY_REPLAY_STORE_PATH="$CEX_PROJECT_ROOT/run/trnm-economy/replay.json"
export CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH="$CEX_PROJECT_ROOT/run/trnm-economy/rate-limit.json"
export CONSUMER_ENTRY_LEAGUE_STATE_PATH="$CEX_PROJECT_ROOT/run/trnm-economy/league-projection.json"
export CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL="$DATABASE_URL"
export CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED=false
export CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED=false
export CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED=false

mkdir -p "$CEX_PROJECT_ROOT/run/trnm-economy"
touch "$CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH"
chmod 700 "$CEX_PROJECT_ROOT/run/trnm-economy"
chmod 600 "$CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH"

cex_wait_postgres 60 1

case "${1:-}" in
  ledger)
    exec "$CEX_PROJECT_ROOT/target/release/ledger-service"
    ;;
  consumer)
    for _ in $(seq 1 60); do
      if curl -fsS "$LEDGER_BASE_URL/v1/trnm/economy/readiness" >/dev/null 2>&1; then
        exec "$CEX_PROJECT_ROOT/target/release/consumer-entry-api"
      fi
      sleep 1
    done
    echo "persistent ledger did not become ready" >&2
    exit 1
    ;;
  *)
    echo "usage: $0 <ledger|consumer>" >&2
    exit 64
    ;;
esac

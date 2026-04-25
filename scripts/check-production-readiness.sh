#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"
EXECUTION_ADMIN_TOKEN="${EXECUTION_ADMIN_TOKEN:-${CEX_EXECUTION_ADMIN_TOKEN:-${LOCAL_DEV_ADMIN_TOKEN:-local-dev-admin-token}}}"
CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
MATRIX_ENTRY_BASE_URL="${MATRIX_ENTRY_BASE_URL:-http://127.0.0.1:8091}"
CEX_PROVIDER_PROBE_REQUIRED="${CEX_PROVIDER_PROBE_REQUIRED:-1}"
CEX_PROVIDER_PROBE_MODEL="${CEX_PROVIDER_PROBE_MODEL:-}"
CEX_READINESS_MODE="${CEX_READINESS_MODE:-production}"
CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED="${CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED:-}"
CEX_DB_BACKUP_RESTORE_DRILL_SUMMARY_PATH="${CEX_DB_BACKUP_RESTORE_DRILL_SUMMARY_PATH:-}"
CEX_DB_BACKUP_RESTORE_DRILL_MAX_AGE_SECONDS="${CEX_DB_BACKUP_RESTORE_DRILL_MAX_AGE_SECONDS:-86400}"
CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES="${CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES:-}"

case "$CEX_READINESS_MODE" in
  local|production) ;;
  *)
    echo "invalid CEX_READINESS_MODE: $CEX_READINESS_MODE (expected local|production)" >&2
    exit 64
    ;;
esac

if [[ -z "$CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED" ]]; then
  if [[ "$CEX_READINESS_MODE" == "production" ]]; then
    CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED="1"
  else
    CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED="0"
  fi
fi

cex_require_cmd bash curl jq

failures=0

section() {
  printf '\n==> %s\n' "$*"
}

fail() {
  failures=$((failures + 1))
  printf 'FAIL %s\n' "$*" >&2
}

pass() {
  printf 'OK %s\n' "$*"
}

secret_is_default_or_empty() {
  local value="$1"
  [[ -z "$value" || "$value" == "local-dev-key" || "$value" == "local-dev-admin-token" ]]
}

required_bool_true() {
  local value="$1"
  [[ "${value,,}" == "true" || "$value" == "1" ]]
}

file_has_group_or_other_permissions() {
  local path="$1"
  local mode
  mode="$(stat -c '%a' "$path")"
  (( (8#$mode & 077) != 0 ))
}

csv_items() {
  local raw="$1"
  tr ',' '\n' <<<"$raw" | sed 's/^ *//;s/ *$//' | awk 'length > 0'
}

section 'deployment posture'
printf 'readiness mode=%s\n' "$CEX_READINESS_MODE"
if [[ "$CEX_READINESS_MODE" == "local" ]]; then
  pass 'local readiness posture selected (production secret/profile checks skipped)'
else
  posture_failures=0
  if [[ -n "${CEX_ENV_FILE:-}" && -f "$CEX_ENV_FILE" ]] && file_has_group_or_other_permissions "$CEX_ENV_FILE"; then
    posture_failures=$((posture_failures + 1))
    fail "production posture requires CEX_ENV_FILE to be owner-only readable ($CEX_ENV_FILE)"
  fi
  if secret_is_default_or_empty "${CEX_GATEWAY_API_KEY:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires non-default CEX_GATEWAY_API_KEY'
  fi
  if secret_is_default_or_empty "${EXECUTION_ADMIN_TOKEN:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires non-default execution admin token'
  fi
  if [[ -z "${CONSUMER_ENTRY_INGRESS_TOKEN:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_INGRESS_TOKEN'
  fi
  if [[ -z "${MATRIX_ENTRY_INGRESS_TOKEN:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires MATRIX_ENTRY_INGRESS_TOKEN'
  fi
  if ! required_bool_true "${CONSUMER_ENTRY_REQUIRE_SESSION_AUTH:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true'
  fi
  if ! required_bool_true "${CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true'
  fi
  if [[ -z "${CONSUMER_ENTRY_REPLAY_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable CONSUMER_ENTRY_REPLAY_STORE_PATH'
  fi
  if [[ -z "${CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH'
  fi
  if [[ -z "${MATRIX_ENTRY_RECENT_EVENT_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable MATRIX_ENTRY_RECENT_EVENT_STORE_PATH'
  fi

  if [[ "$posture_failures" -eq 0 ]]; then
    pass 'production posture checks clear'
  fi
fi

section 'entry runtime posture'
if [[ "$CEX_READINESS_MODE" == "local" ]]; then
  pass 'local readiness posture selected (entry runtime posture checks skipped)'
else
  consumer_health_file="$(mktemp)"
  matrix_health_file="$(mktemp)"
  consumer_health_ok=0
  matrix_health_ok=0
  curl -fsS "$CONSUMER_ENTRY_BASE_URL/health" >"$consumer_health_file" || consumer_health_ok=$?
  curl -fsS "$MATRIX_ENTRY_BASE_URL/health" >"$matrix_health_file" || matrix_health_ok=$?
  if [[ "$consumer_health_ok" -ne 0 ]]; then
    fail "production posture cannot read consumer-entry health ($CONSUMER_ENTRY_BASE_URL/health)"
  else
    if [[ "$(jq -r '.ingress_protected // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry ingress_protected=true'
    fi
    if [[ "$(jq -r '.require_session_auth // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry require_session_auth=true'
    fi
    if [[ "$(jq -r '.require_identity_binding // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry require_identity_binding=true'
    fi
    if [[ "$(jq -r '.replay_store_enabled // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry replay_store_enabled=true'
    fi
    if [[ "$(jq -r '.rate_limit_store_enabled // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry rate_limit_store_enabled=true'
    fi
    if [[ "$(jq -r '.identity_governance_overview.valid // false' "$consumer_health_file")" != "true" ]]; then
      fail 'production runtime requires consumer-entry identity governance valid=true'
    fi
  fi

  if [[ "$matrix_health_ok" -ne 0 ]]; then
    fail "production posture cannot read matrix-entry health ($MATRIX_ENTRY_BASE_URL/health)"
  else
    if [[ "$(jq -r '.ingress_protected // false' "$matrix_health_file")" != "true" ]]; then
      fail 'production runtime requires matrix-entry ingress_protected=true'
    fi
    if [[ "$(jq -r '.consumer_entry_protected // false' "$matrix_health_file")" != "true" ]]; then
      fail 'production runtime requires matrix-entry consumer_entry_protected=true'
    fi
    if [[ "$(jq -r '.recent_event_store_enabled // false' "$matrix_health_file")" != "true" ]]; then
      fail 'production runtime requires matrix-entry recent_event_store_enabled=true'
    fi
  fi
  rm -f "$consumer_health_file" "$matrix_health_file"
fi

section 'runtime health/status'
if bash "$SCRIPT_DIR/runtime-manager-linux.sh" status; then
  pass 'runtime status'
else
  fail 'runtime status'
fi

section 'execution policy posture'
if [[ "$CEX_READINESS_MODE" == "local" ]]; then
  pass 'local readiness posture selected (execution policy posture checks skipped)'
else
  policy_info_file="$(mktemp)"
  if curl -fsS "$EXECUTION_BASE_URL/v1/info" >"$policy_info_file"; then
    policy_status="$(jq -r '.policy.policy_bundle_load_status // "unknown"' "$policy_info_file")"
    if [[ "$policy_status" == "loaded" ]]; then
      pass 'execution policy bundle loaded'
    else
      fail "execution policy bundle is not loaded (status=$policy_status)"
    fi
    if [[ -n "$CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES" ]]; then
      missing_policy_prefixes=0
      while IFS= read -r required_prefix; do
        if jq -e --arg prefix "$required_prefix" '.policy.block_capability_prefixes // [] | index($prefix) != null' "$policy_info_file" >/dev/null; then
          :
        else
          missing_policy_prefixes=$((missing_policy_prefixes + 1))
          fail "execution policy must block non-launch capability prefix: $required_prefix"
        fi
      done < <(csv_items "$CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES")
      if [[ "$missing_policy_prefixes" -eq 0 ]]; then
        pass 'required non-launch capability prefixes blocked'
      fi
    fi
  else
    fail 'execution info endpoint unreachable for policy posture'
  fi
  rm -f "$policy_info_file"
fi

section 'native metrics smoke'
if bash "$SCRIPT_DIR/smoke-runtime-metrics.sh"; then
  pass 'native metrics smoke'
else
  fail 'native metrics smoke'
fi

section 'operator signals'
operator_json_file="$(mktemp)"
operator_status=0
bash "$SCRIPT_DIR/check-operator-signals.sh" --compact >"$operator_json_file" || operator_status=$?
operator_overall="$(jq -r '.overall // "unknown"' "$operator_json_file")"
operator_warns="$(jq -r '.summary.warn_count // 0' "$operator_json_file")"
operator_criticals="$(jq -r '.summary.critical_count // 0' "$operator_json_file")"
printf 'operator overall=%s warn=%s critical=%s exit=%s\n' \
  "$operator_overall" "$operator_warns" "$operator_criticals" "$operator_status"
if [[ "$operator_status" -eq 0 && "$operator_overall" == "ok" ]]; then
  pass 'operator signals clear'
else
  fail "operator signals not clear (overall=$operator_overall, exit=$operator_status)"
  jq -r '.alerts[]? | "  - \(.severity // "unknown") \(.service):\(.name) value=\(.value // "n/a") threshold=\(.threshold // "n/a")"' "$operator_json_file" >&2
fi
rm -f "$operator_json_file"

section 'provider dead-letter readiness'
dead_letters_json_file="$(mktemp)"
if curl -fsS -H "x-admin-token: $EXECUTION_ADMIN_TOKEN" \
  "$EXECUTION_BASE_URL/v1/executions/provider-dead-letters?limit=200" >"$dead_letters_json_file"; then
  dead_letter_count="$(jq 'length' "$dead_letters_json_file")"
  billing_count="$(jq '[.[] | select(.provider_failure_kind == "billing")] | length' "$dead_letters_json_file")"
  retry_exhausted_count="$(jq '[.[] | select(.retry_budget_exhausted == true)] | length' "$dead_letters_json_file")"
  printf 'provider dead_letters=%s billing=%s retry_budget_exhausted=%s\n' \
    "$dead_letter_count" "$billing_count" "$retry_exhausted_count"
  if [[ "$dead_letter_count" -eq 0 ]]; then
    pass 'provider dead-letter queue clear'
  else
    fail 'provider dead-letter queue is not clear'
    jq -r '.[:10][] | "  - \(.provider_failure_kind) \(.dead_letter_reason) \(.execution_id) \(.provider_target // "unknown") :: \(.error // "")"' "$dead_letters_json_file" >&2
  fi
else
  fail 'provider dead-letter endpoint unreachable'
fi
rm -f "$dead_letters_json_file"

section 'live provider probe'
if [[ "$CEX_PROVIDER_PROBE_REQUIRED" == "0" ]]; then
  pass 'live provider probe not required by environment'
elif [[ -z "$CEX_PROVIDER_PROBE_MODEL" ]]; then
  fail 'live provider probe model is not configured (set CEX_PROVIDER_PROBE_MODEL)'
else
  provider_probe_json_file="$(mktemp)"
  provider_probe_status=0
  bash "$SCRIPT_DIR/probe-openclaw-provider.sh" --model "$CEX_PROVIDER_PROBE_MODEL" --compact \
    >"$provider_probe_json_file" || provider_probe_status=$?
  provider_probe_ok="$(jq -r '.ok // false' "$provider_probe_json_file")"
  provider_probe_status_text="$(jq -r '.status // "unknown"' "$provider_probe_json_file")"
  if [[ "$provider_probe_status" -eq 0 && "$provider_probe_ok" == "true" ]]; then
    pass "live provider probe succeeded ($CEX_PROVIDER_PROBE_MODEL)"
  else
    provider_probe_error="$(jq -r '.error // "unknown provider probe error"' "$provider_probe_json_file")"
    fail "live provider probe failed ($CEX_PROVIDER_PROBE_MODEL status=$provider_probe_status_text): $provider_probe_error"
  fi
  rm -f "$provider_probe_json_file"
fi

section 'db backup/restore drill evidence'
if [[ "$CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED" == "0" ]]; then
  pass 'db backup/restore drill evidence not required by environment'
else
  drill_summary_path="$CEX_DB_BACKUP_RESTORE_DRILL_SUMMARY_PATH"
  if [[ -z "$drill_summary_path" ]]; then
    drill_summary_path="$(find "$SCRIPT_DIR/../run/drills" -maxdepth 1 -type f -name 'db-backup-restore-*.summary.json' -printf '%T@ %p\n' 2>/dev/null | sort -nr | awk 'NR==1 { $1=""; sub(/^ /, ""); print }')"
  fi
  if [[ -z "$drill_summary_path" || ! -f "$drill_summary_path" ]]; then
    fail 'db backup/restore drill evidence is missing (run scripts/drill-db-backup-restore.sh)'
  else
    drill_ok="$(jq -r '.ok // false' "$drill_summary_path")"
    drill_kind="$(jq -r '.kind // "unknown"' "$drill_summary_path")"
    drill_ended="$(jq -r '.ended_at_epoch // 0' "$drill_summary_path")"
    drill_age=$(( $(date +%s) - drill_ended ))
    if [[ "$drill_ok" != "true" || "$drill_kind" != "db_backup_restore_drill" ]]; then
      fail "db backup/restore drill summary is not successful ($drill_summary_path)"
    elif [[ "$drill_age" -lt 0 || "$drill_age" -gt "$CEX_DB_BACKUP_RESTORE_DRILL_MAX_AGE_SECONDS" ]]; then
      fail "db backup/restore drill summary is stale (age=${drill_age}s max=${CEX_DB_BACKUP_RESTORE_DRILL_MAX_AGE_SECONDS}s path=$drill_summary_path)"
    else
      pass "db backup/restore drill evidence fresh (${drill_age}s old, $drill_summary_path)"
    fi
  fi
fi

section 'production readiness verdict'
if [[ "$failures" -eq 0 ]]; then
  echo "READY $CEX_READINESS_MODE readiness smoke passed"
  exit 0
fi

echo "NOT_READY $CEX_READINESS_MODE readiness smoke found $failures blocker(s)" >&2
exit 2

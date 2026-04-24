#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

GATEWAY_INFO_URL="${GATEWAY_INFO_URL:-http://127.0.0.1:8080/v1/info}"
EXECUTION_INFO_URL="${EXECUTION_INFO_URL:-http://127.0.0.1:7003/v1/info}"
CONSUMER_ENTRY_HEALTH_URL="${CONSUMER_ENTRY_HEALTH_URL-http://127.0.0.1:8090/health}"
MATRIX_ENTRY_HEALTH_URL="${MATRIX_ENTRY_HEALTH_URL-http://127.0.0.1:8091/health}"
READ_MONITORING_DEPLOY_STATUS_SCRIPT="${READ_MONITORING_DEPLOY_STATUS_SCRIPT:-$SCRIPT_DIR/read-monitoring-deploy-status.sh}"
MONITORING_DEPLOY_METADATA_FILE="${MONITORING_DEPLOY_METADATA_FILE-$PROJECT_ROOT/run/monitoring-live-target/metadata/monitoring-deploy-metadata.yml}"
MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY="${MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY:-0}"
ALERT_CONSUMER_ENTRY_RATE_LIMITED_THRESHOLD="${ALERT_CONSUMER_ENTRY_RATE_LIMITED_THRESHOLD:-20}"
ALERT_CONSUMER_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD="${ALERT_CONSUMER_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD:-5}"
ALERT_MATRIX_ENTRY_RATE_LIMITED_THRESHOLD="${ALERT_MATRIX_ENTRY_RATE_LIMITED_THRESHOLD:-20}"
ALERT_MATRIX_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD="${ALERT_MATRIX_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD:-5}"
ALERT_MATRIX_ENTRY_DUPLICATE_EVENT_THRESHOLD="${ALERT_MATRIX_ENTRY_DUPLICATE_EVENT_THRESHOLD:-25}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-5}"
OUTPUT_MODE="pretty"

usage() {
  cat <<'EOF'
Usage: scripts/check-operator-signals.sh [--compact] [--pretty]

Fetch gateway/execution core /v1/info snapshots plus optional product-edge /health snapshots,
and optionally fold in monitoring deploy verdict metadata, summarize operator signals,
and exit with a monitoring-friendly status code:

  0 = ok
  1 = warn
  2 = critical

Config via env:
  GATEWAY_INFO_URL
  EXECUTION_INFO_URL
  CONSUMER_ENTRY_HEALTH_URL   (set empty to skip)
  MATRIX_ENTRY_HEALTH_URL     (set empty to skip)
  READ_MONITORING_DEPLOY_STATUS_SCRIPT
  MONITORING_DEPLOY_METADATA_FILE
  MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY=0|1
  ALERT_CONSUMER_ENTRY_RATE_LIMITED_THRESHOLD
  ALERT_CONSUMER_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD
  ALERT_MATRIX_ENTRY_RATE_LIMITED_THRESHOLD
  ALERT_MATRIX_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD
  ALERT_MATRIX_ENTRY_DUPLICATE_EVENT_THRESHOLD
  TIMEOUT_SECONDS
EOF
}

require_non_negative_integer() {
  local name="$1"
  local value="$2"
  if ! [[ "$value" =~ ^[0-9]+$ ]]; then
    echo "invalid $name: $value" >&2
    exit 64
  fi
}

while (($#)); do
  case "$1" in
    --compact)
      OUTPUT_MODE="compact"
      ;;
    --pretty)
      OUTPUT_MODE="pretty"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
  shift
done

require_non_negative_integer ALERT_CONSUMER_ENTRY_RATE_LIMITED_THRESHOLD "$ALERT_CONSUMER_ENTRY_RATE_LIMITED_THRESHOLD"
require_non_negative_integer ALERT_CONSUMER_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD "$ALERT_CONSUMER_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD"
require_non_negative_integer ALERT_MATRIX_ENTRY_RATE_LIMITED_THRESHOLD "$ALERT_MATRIX_ENTRY_RATE_LIMITED_THRESHOLD"
require_non_negative_integer ALERT_MATRIX_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD "$ALERT_MATRIX_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD"
require_non_negative_integer ALERT_MATRIX_ENTRY_DUPLICATE_EVENT_THRESHOLD "$ALERT_MATRIX_ENTRY_DUPLICATE_EVENT_THRESHOLD"
if [[ "$MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY" != "0" && "$MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY" != "1" ]]; then
  echo "invalid MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY: $MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY" >&2
  exit 64
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

fetch_json() {
  local url="$1"
  local body_file="$2"
  local err_file="$3"

  if local http_code; http_code="$(curl -sS -m "$TIMEOUT_SECONDS" -w '%{http_code}' -o "$body_file" "$url" 2>"$err_file")"; then
    if [[ "$http_code" =~ ^2[0-9][0-9]$ ]] && jq empty "$body_file" >/dev/null 2>&1; then
      printf '{"ok":true,"http_code":"%s"}' "$http_code"
      return 0
    fi

    local message
    message="$(cat "$err_file" 2>/dev/null || true)"
    if [[ -z "$message" ]]; then
      if [[ ! "$http_code" =~ ^2[0-9][0-9]$ ]]; then
        message="unexpected http status $http_code"
      else
        message="response was not valid json"
      fi
    fi
    printf '{"ok":false,"http_code":"%s","error":%s}' "$http_code" "$(jq -Rn --arg v "$message" '$v')"
    return 0
  fi

  local message
  message="$(cat "$err_file" 2>/dev/null || true)"
  if [[ -z "$message" ]]; then
    message="curl request failed"
  fi
  printf '{"ok":false,"http_code":"000","error":%s}' "$(jq -Rn --arg v "$message" '$v')"
}

severity_for_signal() {
  local service="$1"
  local name="$2"
  case "$service:$name" in
    execution:refund_failures|gateway:endpoint_unreachable|execution:endpoint_unreachable|execution:runtime_error)
      printf 'critical'
      ;;
    gateway:invocation_create_upstream_failures|execution:approval_backlog|execution:queued_worker_lease_expired|execution:queued_worker_retry_budget_exhausted|execution:audit_failures|consumer_entry:endpoint_unreachable|consumer_entry:health_status_not_ok|consumer_entry:rate_limited_requests|consumer_entry:ingress_auth_failures|consumer_entry:identity_governance_invalid|consumer_entry:identity_binding_not_loaded|consumer_entry:identity_registry_not_loaded|consumer_entry:identity_ref_integrity_not_ok|consumer_entry:identity_actor_gate_invalid|consumer_entry:identity_approval_source_invalid|consumer_entry:identity_approval_coverage_invalid|consumer_entry:session_auth_issuer_registry_governance_invalid|consumer_entry:session_auth_issuer_registry_not_loaded|consumer_entry:session_auth_issuer_registry_actor_gate_invalid|consumer_entry:session_auth_issuer_registry_approval_source_invalid|consumer_entry:session_auth_issuer_registry_approval_coverage_invalid|consumer_entry:session_auth_issuer_registry_revision_missing|matrix_entry:endpoint_unreachable|matrix_entry:health_status_not_ok|matrix_entry:rate_limited_requests|matrix_entry:ingress_auth_failures|matrix_entry:duplicate_events|matrix_entry:consumer_entry_session_auth_governance_invalid|matrix_entry:consumer_entry_session_auth_selection_invalid|matrix_entry:consumer_entry_session_auth_issuer_registry_not_loaded|matrix_entry:consumer_entry_session_auth_issuer_registry_revision_missing|matrix_entry:consumer_entry_session_auth_approval_source_invalid|matrix_entry:consumer_entry_session_auth_approval_coverage_invalid)
      printf 'warn'
      ;;
    *)
      printf 'warn'
      ;;
  esac
}

append_source_object() {
  local key="$1"
  local url="$2"
  local fetch_json="$3"
  jq -cn \
    --arg key "$key" \
    --arg url "$url" \
    --argjson fetch "$fetch_json" \
    '{($key):{url:$url,fetch:$fetch}}' >> "$sources_jsonl"
}

append_file_source_object() {
  local key="$1"
  local path="$2"
  local fetch_json="$3"
  jq -cn \
    --arg key "$key" \
    --arg path "$path" \
    --argjson fetch "$fetch_json" \
    '{($key):{path:$path,fetch:$fetch}}' >> "$sources_jsonl"
}

append_endpoint_alert() {
  local service="$1"
  local severity="$2"
  local fetch_json="$3"
  jq -cn \
    --arg service "$service" \
    --arg name endpoint_unreachable \
    --arg severity "$severity" \
    --arg message "$(jq -r '.error // "unknown error"' <<<"$fetch_json")" \
    '{service:$service,name:$name,severity:$severity,message:$message}' >> "$alerts_jsonl"
}

collect_operator_alerts() {
  local service="$1"
  local body_file="$2"
  jq -c '.operator_signals // {} | to_entries[] | select(.value.alert == true) | {name:.key,value:(.value.value // 0),threshold:(.value.threshold // 0)}' "$body_file" |
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    local name value threshold severity
    name="$(jq -r '.name' <<<"$line")"
    value="$(jq -r '.value' <<<"$line")"
    threshold="$(jq -r '.threshold' <<<"$line")"
    severity="$(severity_for_signal "$service" "$name")"
    jq -cn \
      --arg service "$service" \
      --arg name "$name" \
      --arg severity "$severity" \
      --argjson value "$value" \
      --argjson threshold "$threshold" \
      '{service:$service,name:$name,severity:$severity,value:$value,threshold:$threshold}'
  done
}

collect_supporting_health_alerts() {
  local service="$1"
  local body_file="$2"
  local status
  status="$(jq -r '.status // "unknown"' "$body_file")"
  if [[ "$status" != "ok" ]]; then
    jq -cn \
      --arg service "$service" \
      --arg name health_status_not_ok \
      --arg severity "$(severity_for_signal "$service" health_status_not_ok)" \
      --arg message "status=$status" \
      '{service:$service,name:$name,severity:$severity,message:$message}'
  fi
}

collect_supporting_metric_alerts() {
  local service="$1"
  local body_file="$2"

  local metrics thresholds_json
  metrics="$(jq -c '.metrics // {}' "$body_file")"

  case "$service" in
    consumer_entry)
      thresholds_json="$(jq -cn \
        --argjson rate_limited "$ALERT_CONSUMER_ENTRY_RATE_LIMITED_THRESHOLD" \
        --argjson ingress_auth_failures "$ALERT_CONSUMER_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD" \
        '{rate_limited_requests:$rate_limited,ingress_auth_failures:$ingress_auth_failures}')"
      ;;
    matrix_entry)
      thresholds_json="$(jq -cn \
        --argjson rate_limited "$ALERT_MATRIX_ENTRY_RATE_LIMITED_THRESHOLD" \
        --argjson ingress_auth_failures "$ALERT_MATRIX_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD" \
        --argjson duplicate_events "$ALERT_MATRIX_ENTRY_DUPLICATE_EVENT_THRESHOLD" \
        '{rate_limited_requests:$rate_limited,ingress_auth_failures:$ingress_auth_failures,duplicate_events:$duplicate_events}')"
      ;;
    *)
      return 0
      ;;
  esac

  jq -cn \
    --arg service "$service" \
    --argjson metrics "$metrics" \
    --argjson thresholds "$thresholds_json" '
      [ $thresholds | to_entries[]
        | . as $entry
        | ($metrics[$entry.key] // 0) as $value
        | select(($entry.value // 0) > 0 and $value >= $entry.value)
        | {
            service: $service,
            name: $entry.key,
            severity: "warn",
            value: $value,
            threshold: $entry.value,
            message: ("metric=" + $entry.key + " value=" + ($value|tostring) + " threshold=" + ($entry.value|tostring))
          }
      ][]'
}

collect_consumer_entry_audit_alerts() {
  local body_file="$1"
  local audit_status
  audit_status="$(jq -r '.identity_binding_audit.last_status // ""' "$body_file")"
  local path
  path="$(jq -r '.identity_binding_audit.path // empty' "$body_file")"

  if [[ "$audit_status" != "write_error" && "$audit_status" != "serialize_error" ]]; then
    return 0
  fi

  jq -cn \
    --arg service "consumer_entry" \
    --arg name "identity_binding_audit_last_status" \
    --arg severity "warn" \
    --arg path "$path" \
    --arg status "$audit_status" \
    --arg message "identity_binding_audit.last_status=$audit_status path=$path" \
    '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}'
}

collect_consumer_entry_governance_alerts() {
  local body_file="$1"
  local overview
  overview="$(jq -c '.identity_governance_overview // {}' "$body_file")"

  if [[ "$(jq -r 'if type == "object" then (length > 0) else false end' <<<"$overview")" != "true" ]]; then
    return 0
  fi

  if [[ "$(jq -r '.valid // false' <<<"$overview")" != "false" ]]; then
    return 0
  fi

  local status effective_revision failed_checks
  status="$(jq -r '.status // "invalid"' <<<"$overview")"
  effective_revision="$(jq -r '.effective_revision // empty' <<<"$overview")"
  failed_checks="$(jq -r '[.checks // {} | to_entries[] | select(.value == false) | .key] | join(",")' <<<"$overview")"

  jq -cn \
    --arg service "consumer_entry" \
    --arg name "identity_governance_invalid" \
    --arg severity "warn" \
    --arg status "$status" \
    --arg effective_revision "$effective_revision" \
    --arg failed_checks "$failed_checks" \
    --arg message "identity_governance status=$status failed_checks=${failed_checks:-none} effective_revision=${effective_revision:-unknown}" \
    '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}'

  jq -r '.checks // {} | to_entries[] | select(.value == false) | .key' <<<"$overview" |
  while IFS= read -r check; do
    [[ -z "$check" ]] && continue
    local signal_name message
    case "$check" in
      binding_loaded)
        signal_name="identity_binding_not_loaded"
        message="identity governance check failed: binding_loaded=false"
        ;;
      registry_loaded)
        signal_name="identity_registry_not_loaded"
        message="identity governance check failed: registry_loaded=false"
        ;;
      ref_integrity_ok)
        signal_name="identity_ref_integrity_not_ok"
        message="identity governance check failed: ref_integrity_ok=false"
        ;;
      actor_gate_valid)
        signal_name="identity_actor_gate_invalid"
        message="identity governance check failed: actor_gate_valid=false"
        ;;
      approval_source_valid)
        signal_name="identity_approval_source_invalid"
        message="identity governance check failed: approval_source_valid=false"
        ;;
      approval_coverage_valid)
        signal_name="identity_approval_coverage_invalid"
        message="identity governance check failed: approval_coverage_valid=false"
        ;;
      *)
        signal_name="identity_governance_check_failed"
        message="identity governance check failed: ${check}=false"
        ;;
    esac

    jq -cn \
      --arg service "consumer_entry" \
      --arg name "$signal_name" \
      --arg severity "$(severity_for_signal "consumer_entry" "$signal_name")" \
      --arg check "$check" \
      --arg status "$status" \
      --arg effective_revision "$effective_revision" \
      --arg message "$message status=$status effective_revision=${effective_revision:-unknown}" \
      '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}'
  done
}

collect_consumer_entry_session_auth_registry_alerts() {
  local body_file="$1"
  local overview
  overview="$(jq -c '.session_auth_issuer_registry_governance_overview // {}' "$body_file")"

  if [[ "$(jq -r 'if type == "object" then (length > 0) else false end' <<<"$overview")" != "true" ]]; then
    return 0
  fi

  if [[ "$(jq -r '.configured // false' <<<"$overview")" != "true" ]]; then
    return 0
  fi

  if [[ "$(jq -r '.valid // false' <<<"$overview")" != "false" ]]; then
    return 0
  fi

  local status current_revision failed_checks
  status="$(jq -r '.status // "invalid"' <<<"$overview")"
  current_revision="$(jq -r '.current_revision // empty' <<<"$overview")"
  failed_checks="$(jq -r '[.checks // {} | to_entries[] | select(.value == false) | .key] | join(",")' <<<"$overview")"

  jq -cn \
    --arg service "consumer_entry" \
    --arg name "session_auth_issuer_registry_governance_invalid" \
    --arg severity "warn" \
    --arg status "$status" \
    --arg current_revision "$current_revision" \
    --arg failed_checks "$failed_checks" \
    --arg message "session_auth_issuer_registry_governance status=$status failed_checks=${failed_checks:-none} current_revision=${current_revision:-unknown}" \
    '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}'

  jq -r '.checks // {} | to_entries[] | select(.value == false) | .key' <<<"$overview" |
  while IFS= read -r check; do
    [[ -z "$check" ]] && continue
    local signal_name message
    case "$check" in
      registry_loaded)
        signal_name="session_auth_issuer_registry_not_loaded"
        message="session auth issuer registry governance check failed: registry_loaded=false"
        ;;
      revision_present)
        signal_name="session_auth_issuer_registry_revision_missing"
        message="session auth issuer registry governance check failed: revision_present=false"
        ;;
      actor_gate_valid)
        signal_name="session_auth_issuer_registry_actor_gate_invalid"
        message="session auth issuer registry governance check failed: actor_gate_valid=false"
        ;;
      approval_source_valid)
        signal_name="session_auth_issuer_registry_approval_source_invalid"
        message="session auth issuer registry governance check failed: approval_source_valid=false"
        ;;
      approval_coverage_valid)
        signal_name="session_auth_issuer_registry_approval_coverage_invalid"
        message="session auth issuer registry governance check failed: approval_coverage_valid=false"
        ;;
      *)
        signal_name="session_auth_issuer_registry_governance_check_failed"
        message="session auth issuer registry governance check failed: ${check}=false"
        ;;
    esac

    jq -cn \
      --arg service "consumer_entry" \
      --arg name "$signal_name" \
      --arg severity "$(severity_for_signal "consumer_entry" "$signal_name")" \
      --arg check "$check" \
      --arg status "$status" \
      --arg current_revision "$current_revision" \
      --arg message "$message status=$status current_revision=${current_revision:-unknown}" \
      '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}'
  done
}

collect_matrix_entry_session_auth_alerts() {
  local body_file="$1"
  local overview
  overview="$(jq -c '.consumer_entry_session_auth_governance_overview // {}' "$body_file")"

  if [[ "$(jq -r 'if type == "object" then (length > 0) else false end' <<<"$overview")" != "true" ]]; then
    return 0
  fi

  if [[ "$(jq -r '.expected // false' <<<"$overview")" != "true" ]]; then
    return 0
  fi

  if [[ "$(jq -r '.valid // false' <<<"$overview")" != "false" ]]; then
    return 0
  fi

  local status selection_status selection_source failed_checks
  status="$(jq -r '.status // "invalid"' <<<"$overview")"
  selection_status="$(jq -r '.selection.status // empty' <<<"$overview")"
  selection_source="$(jq -r '.selection.source // empty' <<<"$overview")"
  failed_checks="$(jq -r '[.checks // {} | to_entries[] | select(.value == false) | .key] | join(",")' <<<"$overview")"

  jq -cn \
    --arg service "matrix_entry" \
    --arg name "consumer_entry_session_auth_governance_invalid" \
    --arg severity "warn" \
    --arg status "$status" \
    --arg selection_status "$selection_status" \
    --arg selection_source "$selection_source" \
    --arg failed_checks "$failed_checks" \
    --arg message "consumer_entry_session_auth_governance status=$status selection=${selection_source:-unknown}/${selection_status:-unknown} failed_checks=${failed_checks:-none}" \
    '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}'

  jq -r '.checks // {} | to_entries[] | select(.value == false) | .key' <<<"$overview" |
  while IFS= read -r check; do
    [[ -z "$check" ]] && continue
    local signal_name message
    case "$check" in
      selection_ok)
        signal_name="consumer_entry_session_auth_selection_invalid"
        message="matrix session-auth signer selection invalid: selection_ok=false"
        ;;
      registry_loaded)
        signal_name="consumer_entry_session_auth_issuer_registry_not_loaded"
        message="matrix session-auth issuer registry governance check failed: registry_loaded=false"
        ;;
      revision_present)
        signal_name="consumer_entry_session_auth_issuer_registry_revision_missing"
        message="matrix session-auth issuer registry governance check failed: revision_present=false"
        ;;
      approval_source_valid)
        signal_name="consumer_entry_session_auth_approval_source_invalid"
        message="matrix session-auth approval source invalid: approval_source_valid=false"
        ;;
      approval_coverage_valid)
        signal_name="consumer_entry_session_auth_approval_coverage_invalid"
        message="matrix session-auth approval coverage invalid: approval_coverage_valid=false"
        ;;
      *)
        signal_name="consumer_entry_session_auth_governance_check_failed"
        message="matrix session-auth governance check failed: ${check}=false"
        ;;
    esac

    jq -cn \
      --arg service "matrix_entry" \
      --arg name "$signal_name" \
      --arg severity "$(severity_for_signal "matrix_entry" "$signal_name")" \
      --arg status "$status" \
      --arg selection_status "$selection_status" \
      --arg selection_source "$selection_source" \
      --arg message "$message status=$status selection=${selection_source:-unknown}/${selection_status:-unknown}" \
      '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}'
  done
}

collect_monitoring_deploy_alerts() {
  local body_file="$1"
  local status severity operator_display next_action metadata_file
  status="$(jq -r '.overall.status // "unknown"' "$body_file")"
  severity="$(jq -r '.overall.severity // "unknown"' "$body_file")"
  operator_display="$(jq -r '.overall.operatorDisplay // .overall.summaryDisplay // empty' "$body_file")"
  next_action="$(jq -r '.overall.nextActionHint // empty' "$body_file")"
  metadata_file="$(jq -r '.metadataFile // empty' "$body_file")"

  if [[ "$severity" == "ok" ]]; then
    return 0
  fi

  if [[ "$status" == "deploy_only" && "$MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY" != "1" ]]; then
    return 0
  fi

  jq -cn \
    --arg service "monitoring_deploy" \
    --arg name "post_action_status" \
    --arg severity "warn" \
    --arg status "$status" \
    --arg verdict_severity "$severity" \
    --arg operator_display "$operator_display" \
    --arg next_action "$next_action" \
    --arg metadata_file "$metadata_file" \
    --arg message "monitoring deploy verdict status=$status severity=$severity summary=${operator_display:-unknown} next=${next_action:-none} metadata=${metadata_file:-unknown}" \
    '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}'
}

append_supporting_summary() {
  local key="$1"
  local body_file="$2"
  case "$key" in
    consumer_entry)
      jq -cn \
        --arg key "$key" \
        --slurpfile body "$body_file" \
        '{($key):{
          status: ($body[0].status // "unknown"),
          service: ($body[0].service // "consumer-entry-api"),
          ingress_protected: ($body[0].ingress_protected // false),
          max_text_chars: ($body[0].max_text_chars // null),
          rate_limit_window_secs: ($body[0].rate_limit_window_secs // null),
          rate_limit_max_requests: ($body[0].rate_limit_max_requests // null),
          identity_binding_audit: ($body[0].identity_binding_audit // {}),
          identity_governance_valid: (try $body[0].profile_validation.checks.identity_governance_valid catch null),
          identity_governance_overview: ($body[0].identity_governance_overview // {}),
          metrics: ($body[0].metrics // {})
        }}' >> "$supporting_jsonl"
      ;;
    matrix_entry)
      jq -cn \
        --arg key "$key" \
        --slurpfile body "$body_file" \
        '{($key):{
          status: ($body[0].status // "unknown"),
          service: ($body[0].service // "matrix-entry-adapter"),
          ingress_protected: ($body[0].ingress_protected // false),
          consumer_entry_protected: ($body[0].consumer_entry_protected // false),
          max_text_chars: ($body[0].max_text_chars // null),
          rate_limit_window_secs: ($body[0].rate_limit_window_secs // null),
          rate_limit_max_requests: ($body[0].rate_limit_max_requests // null),
          recent_event_window_secs: ($body[0].recent_event_window_secs // null),
          recent_event_cache_size: ($body[0].recent_event_cache_size // null),
          consumer_entry_session_auth_selection_ok: (try $body[0].profile_validation.checks.consumer_entry_session_auth_selection_ok catch null),
          consumer_entry_session_auth_governance_valid: (try $body[0].profile_validation.checks.consumer_entry_session_auth_governance_valid catch null),
          consumer_entry_session_auth_issuer_registry_approval_loaded: (try $body[0].profile_validation.checks.consumer_entry_session_auth_issuer_registry_approval_loaded catch null),
          consumer_entry_session_auth_issuer_registry_revision_approved: (try $body[0].profile_validation.checks.consumer_entry_session_auth_issuer_registry_revision_approved catch null),
          consumer_entry_session_auth_governance_overview: ($body[0].consumer_entry_session_auth_governance_overview // {}),
          consumer_entry_session_auth: ($body[0].consumer_entry_session_auth // {}),
          metrics: ($body[0].metrics // {})
        }}' >> "$supporting_jsonl"
      ;;
    monitoring_deploy)
      jq -cn \
        --arg key "$key" \
        --slurpfile body "$body_file" \
        '{($key):{
          metadata_file: ($body[0].metadataFile // null),
          deployed_at: ($body[0].deployedAt // null),
          post_actions_updated_at: ($body[0].postActionsUpdatedAt // null),
          overall: ($body[0].overall // {}),
          reload: ($body[0].reload // {}),
          verify: ($body[0].verify // {})
        }}' >> "$supporting_jsonl"
      ;;
  esac
}

process_supporting_source() {
  local key="$1"
  local url="$2"
  local body_file="$3"
  local err_file="$4"

  if [[ -z "$url" ]]; then
    return 0
  fi

  local fetch
  fetch="$(fetch_json "$url" "$body_file" "$err_file")"
  append_source_object "$key" "$url" "$fetch"

  if [[ "$(jq -r '.ok' <<<"$fetch")" == "true" ]]; then
    collect_supporting_health_alerts "$key" "$body_file" >> "$alerts_jsonl"
    collect_supporting_metric_alerts "$key" "$body_file" >> "$alerts_jsonl"
    if [[ "$key" == "consumer_entry" ]]; then
      collect_consumer_entry_audit_alerts "$body_file" >> "$alerts_jsonl"
      collect_consumer_entry_governance_alerts "$body_file" >> "$alerts_jsonl"
      collect_consumer_entry_session_auth_registry_alerts "$body_file" >> "$alerts_jsonl"
    fi
    if [[ "$key" == "matrix_entry" ]]; then
      collect_matrix_entry_session_auth_alerts "$body_file" >> "$alerts_jsonl"
    fi
    append_supporting_summary "$key" "$body_file"
  else
    append_endpoint_alert "$key" warn "$fetch"
  fi
}

process_monitoring_deploy_source() {
  local metadata_file="$1"
  local body_file="$2"
  local err_file="$3"

  if [[ -z "$metadata_file" || ! -f "$metadata_file" ]]; then
    return 0
  fi

  local result
  if result="$("$READ_MONITORING_DEPLOY_STATUS_SCRIPT" --metadata-file "$metadata_file" --json 2>"$err_file")"; then
    printf '%s\n' "$result" > "$body_file"
    append_file_source_object monitoring_deploy "$metadata_file" '{"ok":true,"kind":"file"}'
    collect_monitoring_deploy_alerts "$body_file" >> "$alerts_jsonl"
    append_supporting_summary monitoring_deploy "$body_file"
  else
    local message
    message="$(cat "$err_file" 2>/dev/null || true)"
    if [[ -z "$message" ]]; then
      message="read-monitoring-deploy-status.sh failed"
    fi
    append_file_source_object monitoring_deploy "$metadata_file" "$(jq -cn --arg error "$message" '{ok:false,kind:"file",error:$error}')"
    jq -cn \
      --arg service "monitoring_deploy" \
      --arg name "metadata_unreadable" \
      --arg severity "warn" \
      --arg message "monitoring deploy metadata unreadable: $message" \
      '{service:$service,name:$name,severity:$severity,value:1,threshold:1,message:$message}' >> "$alerts_jsonl"
  fi
}

sources_jsonl="$TMP_DIR/sources.jsonl"
alerts_jsonl="$TMP_DIR/alerts.jsonl"
supporting_jsonl="$TMP_DIR/supporting.jsonl"
: > "$sources_jsonl"
: > "$alerts_jsonl"
: > "$supporting_jsonl"

gateway_body="$TMP_DIR/gateway.json"
gateway_err="$TMP_DIR/gateway.err"
execution_body="$TMP_DIR/execution.json"
execution_err="$TMP_DIR/execution.err"
consumer_entry_body="$TMP_DIR/consumer-entry.json"
consumer_entry_err="$TMP_DIR/consumer-entry.err"
matrix_entry_body="$TMP_DIR/matrix-entry.json"
matrix_entry_err="$TMP_DIR/matrix-entry.err"
monitoring_deploy_body="$TMP_DIR/monitoring-deploy.json"
monitoring_deploy_err="$TMP_DIR/monitoring-deploy.err"

GATEWAY_FETCH="$(fetch_json "$GATEWAY_INFO_URL" "$gateway_body" "$gateway_err")"
EXECUTION_FETCH="$(fetch_json "$EXECUTION_INFO_URL" "$execution_body" "$execution_err")"

append_source_object gateway "$GATEWAY_INFO_URL" "$GATEWAY_FETCH"
append_source_object execution "$EXECUTION_INFO_URL" "$EXECUTION_FETCH"

if [[ "$(jq -r '.ok' <<<"$GATEWAY_FETCH")" == "true" ]]; then
  collect_operator_alerts gateway "$gateway_body" >> "$alerts_jsonl"
else
  append_endpoint_alert gateway critical "$GATEWAY_FETCH"
fi

if [[ "$(jq -r '.ok' <<<"$EXECUTION_FETCH")" == "true" ]]; then
  collect_operator_alerts execution "$execution_body" >> "$alerts_jsonl"
  runtime_error="$(jq -r '.runtime_error // empty' "$execution_body")"
  if [[ -n "$runtime_error" ]]; then
    jq -cn \
      --arg service execution \
      --arg name runtime_error \
      --arg severity critical \
      --arg message "$runtime_error" \
      '{service:$service,name:$name,severity:$severity,message:$message}' >> "$alerts_jsonl"
  fi
else
  append_endpoint_alert execution critical "$EXECUTION_FETCH"
fi

process_supporting_source consumer_entry "$CONSUMER_ENTRY_HEALTH_URL" "$consumer_entry_body" "$consumer_entry_err"
process_supporting_source matrix_entry "$MATRIX_ENTRY_HEALTH_URL" "$matrix_entry_body" "$matrix_entry_err"
process_monitoring_deploy_source "$MONITORING_DEPLOY_METADATA_FILE" "$monitoring_deploy_body" "$monitoring_deploy_err"

alerts_json="$TMP_DIR/alerts.json"
if [[ -s "$alerts_jsonl" ]]; then
  jq -s '.' "$alerts_jsonl" > "$alerts_json"
else
  printf '[]' > "$alerts_json"
fi

sources_json="$TMP_DIR/sources.json"
if [[ -s "$sources_jsonl" ]]; then
  jq -s 'add' "$sources_jsonl" > "$sources_json"
else
  printf '{}' > "$sources_json"
fi

supporting_json="$TMP_DIR/supporting.json"
if [[ -s "$supporting_jsonl" ]]; then
  jq -s 'add' "$supporting_jsonl" > "$supporting_json"
else
  printf '{}' > "$supporting_json"
fi

warn_count="$(jq '[.[] | select(.severity == "warn")] | length' "$alerts_json")"
critical_count="$(jq '[.[] | select(.severity == "critical")] | length' "$alerts_json")"

overall="ok"
exit_code=0
if [[ "$critical_count" -gt 0 ]]; then
  overall="critical"
  exit_code=2
elif [[ "$warn_count" -gt 0 ]]; then
  overall="warn"
  exit_code=1
fi

result_json="$TMP_DIR/result.json"
jq -n \
  --arg checked_at "$(date -Iseconds)" \
  --arg overall "$overall" \
  --argjson exit_code "$exit_code" \
  --slurpfile sources "$sources_json" \
  --slurpfile alerts "$alerts_json" \
  --slurpfile supporting "$supporting_json" \
  '{
    checked_at:$checked_at,
    overall:$overall,
    exit_code:$exit_code,
    sources:$sources[0],
    alerts:$alerts[0],
    supporting:$supporting[0],
    summary:{
      warn_count:([$alerts[0][] | select(.severity == "warn")] | length),
      critical_count:([$alerts[0][] | select(.severity == "critical")] | length),
      source_count: ($sources[0] | keys | length),
      supporting_surface_count: ($supporting[0] | keys | length)
    }
  }' > "$result_json"

if [[ "$OUTPUT_MODE" == "compact" ]]; then
  jq -c '.' "$result_json"
else
  jq '.' "$result_json"
fi

exit "$exit_code"

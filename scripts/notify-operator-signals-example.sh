#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
DEFAULT_STATE_DIR="${OPERATOR_SIGNAL_STATE_DIR:-$PROJECT_ROOT/run/operator-signals}"
LOG_DIR="${OPERATOR_SIGNAL_NOTIFY_LOG_DIR:-$DEFAULT_STATE_DIR}"
LOG_PATH="${OPERATOR_SIGNAL_NOTIFY_LOG_PATH:-$LOG_DIR/notifications.log}"
WEBHOOK_URL="${OPERATOR_SIGNAL_NOTIFY_WEBHOOK_URL:-}"
WEBHOOK_MODE="${OPERATOR_SIGNAL_NOTIFY_WEBHOOK_MODE:-json}"
ECHO_SUMMARY="${OPERATOR_SIGNAL_NOTIFY_ECHO:-0}"
POLICY_SUMMARY_LEVEL="${OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_LEVEL:-short}"
POLICY_SUMMARY_LEVEL_KEY="short"
DEFAULT_ALERT_FAMILY_RULES_JSON='[{"name":"refund","matchAny":["refund"]},{"name":"audit","matchAny":["audit"]},{"name":"upstream","matchAny":["upstream","bad_gateway","gateway"]},{"name":"monitoring-deploy","matchAny":["monitoring_deploy","deploy_only","reload_failed","verify_failed","post_action","metadata_unreadable"]},{"name":"entry-identity","matchAny":["identity_","identity-","governance","session_auth","actor_gate","approval_source","approval_coverage","ref_integrity","product_user","registry"]},{"name":"approval","matchAny":["approval","awaiting_approval"]},{"name":"worker","matchAny":["lease","retry_budget","retryable","queue","queued_worker","claim"]},{"name":"entry-abuse","matchAny":["duplicate","rate_limit","auth_failure","ingress_auth","abuse"]},{"name":"entry","matchAny":["matrix_entry","consumer_entry","entry"]}]'
ALERT_FAMILY_RULES_JSON="${OPERATOR_SIGNAL_NOTIFY_ALERT_FAMILY_RULES_JSON:-$DEFAULT_ALERT_FAMILY_RULES_JSON}"

mkdir -p "$LOG_DIR"

TMP_JSON="$(mktemp "$LOG_DIR/notify-input.XXXXXX.json")"
cleanup() {
  rm -f "$TMP_JSON"
}
trap cleanup EXIT

cat > "$TMP_JSON"

if ! jq empty "$TMP_JSON" >/dev/null 2>&1; then
  echo "notify-operator-signals-example.sh expected JSON on stdin" >&2
  exit 64
fi

if ! jq -e 'type == "array" and all(.[]?; type == "object" and ((.name // "") | type == "string") and ((.name // "") | length > 0) and ((.matchAny // []) | type == "array") and all((.matchAny // [])[]?; type == "string" and length > 0))' >/dev/null 2>&1 <<<"$ALERT_FAMILY_RULES_JSON"; then
  echo "invalid OPERATOR_SIGNAL_NOTIFY_ALERT_FAMILY_RULES_JSON: must be a json array of {name, matchAny[]} rules" >&2
  exit 64
fi

overall="$(jq -r '.overall // env.OPERATOR_SIGNAL_OVERALL // "unknown"' "$TMP_JSON")"
exit_code="$(jq -r '.exit_code // env.OPERATOR_SIGNAL_EXIT_CODE // "unknown"' "$TMP_JSON")"
notify_reason="${OPERATOR_SIGNAL_NOTIFY_REASON:-direct}"
notify_severity="${OPERATOR_SIGNAL_NOTIFY_SEVERITY:-$overall}"
notify_route="${OPERATOR_SIGNAL_NOTIFY_ROUTE:-default}"
notify_policy_name="${OPERATOR_SIGNAL_NOTIFY_POLICY_NAME:-}"
notify_policy_summary="${OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY:-}"
notify_policy_summary_short="$notify_policy_summary"
notify_policy_summary_ultra_short="${OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_ULTRA_SHORT:-}"
notify_policy_summary_full="${OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_FULL:-}"
notify_policy_escalated="${OPERATOR_SIGNAL_NOTIFY_POLICY_ESCALATED:-false}"
notify_policy_group_escalated="${OPERATOR_SIGNAL_NOTIFY_POLICY_GROUP_ESCALATED:-false}"
notify_policy_occurrences="${OPERATOR_SIGNAL_NOTIFY_POLICY_OCCURRENCES:-0}"
notify_policy_active_seconds="${OPERATOR_SIGNAL_NOTIFY_POLICY_ACTIVE_SECONDS:-0}"
notify_signal_key="${OPERATOR_SIGNAL_NOTIFY_SIGNAL_KEY:-}"
notify_signal_keys="${OPERATOR_SIGNAL_NOTIFY_SIGNAL_KEYS:-}"
previous_severity="${OPERATOR_SIGNAL_NOTIFY_PREVIOUS_SEVERITY:-unknown}"
previous_signal_key="${OPERATOR_SIGNAL_NOTIFY_PREVIOUS_SIGNAL_KEY:-}"
checked_at="$(jq -r '.checked_at // now | tostring' "$TMP_JSON")"
alert_count="$(jq '(.alerts // []) | length' "$TMP_JSON")"
case "$POLICY_SUMMARY_LEVEL" in
  ultra_short|ultra-short)
    POLICY_SUMMARY_LEVEL_KEY="ultra_short"
    notify_policy_summary="$notify_policy_summary_ultra_short"
    ;;
  full)
    POLICY_SUMMARY_LEVEL_KEY="full"
    notify_policy_summary="$notify_policy_summary_full"
    ;;
  short|*)
    POLICY_SUMMARY_LEVEL_KEY="short"
    notify_policy_summary="$notify_policy_summary"
    ;;
esac

if [[ -z "$notify_policy_summary_short" ]]; then
  notify_policy_summary_short="$(jq -r '.notify.policy_summary_levels.short // .notify.policy_summary // .notify.policy_selection_trace.summary_text // ""' "$TMP_JSON")"
fi
if [[ -z "$notify_policy_summary_ultra_short" ]]; then
  notify_policy_summary_ultra_short="$(jq -r '.notify.policy_summary_levels.ultra_short // .notify.policy_summary // .notify.policy_selection_trace.summary_text // ""' "$TMP_JSON")"
fi
if [[ -z "$notify_policy_summary_full" ]]; then
  notify_policy_summary_full="$(jq -r '.notify.policy_summary_levels.full // .notify.policy_summary // .notify.policy_selection_trace.summary_text // ""' "$TMP_JSON")"
fi

if [[ "$POLICY_SUMMARY_LEVEL_KEY" == "ultra_short" ]]; then
  notify_policy_summary="$notify_policy_summary_ultra_short"
elif [[ "$POLICY_SUMMARY_LEVEL_KEY" == "full" ]]; then
  notify_policy_summary="$notify_policy_summary_full"
else
  notify_policy_summary="$notify_policy_summary_short"
fi
alerts_text="$(jq -r '(.alerts // []) | if length == 0 then "- no alerts" else .[] | "- [\(.severity)] \(.service):\(.name) value=\(.value // "?") threshold=\(.threshold // "?")\(if .message then " message=\(.message)" else "" end)" end' "$TMP_JSON")"
family_grouped_alerts_json="$(jq -c --argjson familyRules "$ALERT_FAMILY_RULES_JSON" '
  def severity_rank($s): if $s == "critical" then 0 elif $s == "warn" then 1 else 2 end;
  def classify_family($rules; $service; $name):
    (($service // "") + " " + ($name // "") | ascii_downcase) as $k
    | (first(
        $rules[]?
        | select((((.matchAny // []) | map(. as $pattern | ($k | test($pattern; "i"))) | any) == true))
        | .name
      ) // ($service // "unknown"));
  (.alerts // [])
  | map({
      service: (.service // "unknown"),
      name: (.name // "unknown"),
      severity: (.severity // "unknown"),
      severity_rank: severity_rank(.severity // "unknown"),
      family: classify_family($familyRules; .service; .name)
    })
  | sort_by(.severity_rank, .severity, .family, .service, .name)
  | group_by(.severity)
  | map({
      severity: .[0].severity,
      severity_rank: .[0].severity_rank,
      signal_count: length,
      family_count: (group_by(.family) | length),
      families: (
        group_by(.family)
        | map({
            family: .[0].family,
            signal_count: length,
            service_count: (group_by(.service) | length),
            services: (
              group_by(.service)
              | map({
                  service: .[0].service,
                  signal_count: length,
                  names: (map(.name) | unique)
                })
            )
          })
      )
    })' "$TMP_JSON")"
alerts_brief="$(jq -r '
  if length == 0 then "none"
  else
    map(
      .severity as $severity
      | (
          (.families // [])
          | map(
              .family as $family
              | (
                  (.services // [])
                  | map(
                      if (.names | length) <= 1 then
                        (.service + ":" + (.names[0] // "unknown"))
                      else
                        (.service + "(" + (.names | join(",")) + ")")
                      end
                    )
                  | join(",")
                ) as $serviceSummary
              | ($family + "(" + $serviceSummary + ")")
            )
          | join("; ")
        ) as $familySummary
      | ($severity + " " + $familySummary)
    )
    | join(" | ")
  end' <<<"$family_grouped_alerts_json")"
family_grouped_alerts_text="$(jq -r '
  if length == 0 then "- none"
  else
    map(
      .severity as $severity
      | (.families // [])
      | map(
          .family as $family
          | (
              (.services // [])
              | map(
                  if (.names | length) <= 1 then
                    (.service + ":" + (.names[0] // "unknown"))
                  else
                    (.service + "(" + (.names | join(",")) + ")")
                  end
                )
              | join(", ")
            ) as $serviceSummary
          | ("- [" + $severity + "] " + $family + " -> " + $serviceSummary)
        )
    )
    | add
    | join("\n")
  end' <<<"$family_grouped_alerts_json")"
family_brief="$(jq -r '
  if length == 0 then "none"
  else
    map(
      .severity as $severity
      | (
          (.families // [])
          | map(
              .family as $family
              | ($family + "=" + ((.signal_count // 0) | tostring))
            )
          | join(",")
        ) as $familyCounts
      | ($severity + "{" + $familyCounts + "}")
    )
    | join(" | ")
  end' <<<"$family_grouped_alerts_json")"
summary_views_json="$(jq -c -n \
  --argjson version 1 \
  --arg policy_summary "$notify_policy_summary" \
  --arg family_brief "$family_brief" \
  --arg alerts_brief "$alerts_brief" \
  --argjson family_grouped_alerts "$family_grouped_alerts_json" \
  '{version:$version,policy_summary:$policy_summary,family_brief:$family_brief,alerts_brief:$alerts_brief,family_grouped_alerts:$family_grouped_alerts}')"

build_summary_text_for_level() {
  local level="$1"
  local level_policy_summary="$2"

  case "$level" in
    ultra_short)
      cat <<EOF
[CEX operator signals]
checked_at: $checked_at
overall: $overall
notify_policy_summary: $level_policy_summary
family_brief: $family_brief
alerts_brief: $alerts_brief
EOF
      ;;
    short)
      cat <<EOF
[CEX operator signals]
checked_at: $checked_at
overall: $overall
notify_reason: $notify_reason
notify_route: $notify_route
notify_policy_summary_level: $level
notify_policy_summary: $level_policy_summary
family_brief: $family_brief
alerts: $alert_count
alerts_brief: $alerts_brief
EOF
      ;;
    full|*)
      cat <<EOF
[CEX operator signals]
checked_at: $checked_at
overall: $overall
exit_code: $exit_code
notify_reason: $notify_reason
notify_severity: $notify_severity
notify_route: $notify_route
notify_policy_name: $notify_policy_name
notify_policy_summary_level: $level
notify_policy_summary: $level_policy_summary
notify_policy_escalated: $notify_policy_escalated
notify_policy_group_escalated: $notify_policy_group_escalated
notify_policy_occurrences: $notify_policy_occurrences
notify_policy_active_seconds: $notify_policy_active_seconds
notify_signal_key: $notify_signal_key
notify_signal_keys: $notify_signal_keys
previous_severity: $previous_severity
previous_signal_key: $previous_signal_key
family_brief: $family_brief
alerts: $alert_count
$alerts_text
family_grouped_alerts:
$family_grouped_alerts_text
summary_views_json: $summary_views_json
EOF
      ;;
  esac
}

summary_text_ultra_short="$(build_summary_text_for_level ultra_short "${notify_policy_summary_ultra_short:-$notify_policy_summary_short}")"
summary_text_short="$(build_summary_text_for_level short "${notify_policy_summary_short:-$notify_policy_summary}")"
summary_text_full="$(build_summary_text_for_level full "${notify_policy_summary_full:-$notify_policy_summary}")"

case "$POLICY_SUMMARY_LEVEL_KEY" in
  ultra_short)
    summary_text="$summary_text_ultra_short"
    ;;
  short)
    summary_text="$summary_text_short"
    ;;
  full|*)
    summary_text="$summary_text_full"
    ;;
esac

printf '%s\n' "$summary_text" >> "$LOG_PATH"
printf '%s\n\n' '---' >> "$LOG_PATH"

if [[ -n "$WEBHOOK_URL" ]]; then
  case "$WEBHOOK_MODE" in
    json)
      jq -n \
        --arg text "$summary_text" \
        --arg selected_level "$POLICY_SUMMARY_LEVEL_KEY" \
        --arg text_ultra_short "$summary_text_ultra_short" \
        --arg text_short "$summary_text_short" \
        --arg text_full "$summary_text_full" \
        --arg alerts_brief "$alerts_brief" \
        --arg family_brief "$family_brief" \
        --argjson family_grouped_alerts "$family_grouped_alerts_json" \
        --arg checked_at "$checked_at" \
        --arg overall "$overall" \
        --arg exit_code "$exit_code" \
        --arg notify_reason "$notify_reason" \
        --arg notify_severity "$notify_severity" \
        --arg notify_route "$notify_route" \
        --arg notify_policy_name "$notify_policy_name" \
        --arg notify_policy_summary "$notify_policy_summary" \
        --argjson notify_policy_escalated "$notify_policy_escalated" \
        --argjson notify_policy_group_escalated "$notify_policy_group_escalated" \
        --arg notify_signal_key "$notify_signal_key" \
        --arg notify_signal_keys "$notify_signal_keys" \
        --arg previous_severity "$previous_severity" \
        --arg previous_signal_key "$previous_signal_key" \
        --argjson notify_policy_occurrences "$notify_policy_occurrences" \
        --argjson notify_policy_active_seconds "$notify_policy_active_seconds" \
        --argjson alert_count "$alert_count" \
        --slurpfile payload "$TMP_JSON" \
        '{
          text:$text,
          selected_level:$selected_level,
          selected_body:{
            level:$selected_level,
            text:$text
          },
          body_variants:{
            ultra_short:$text_ultra_short,
            short:$text_short,
            full:$text_full
          },
          rendered_text:$text,
          alerts_brief:$alerts_brief,
          family_brief:$family_brief,
          render:{
            selected_level:$selected_level,
            rendered_text:$text,
            alerts_brief:$alerts_brief,
            family_brief:$family_brief,
            family_grouped_alerts:$family_grouped_alerts,
            summary_views:{
              version:1,
              policy_summary:$notify_policy_summary,
              family_brief:$family_brief,
              alerts_brief:$alerts_brief,
              family_grouped_alerts:$family_grouped_alerts
            }
          },
          summary:{
            checked_at:$checked_at,
            overall:$overall,
            exit_code:$exit_code,
            notify_reason:$notify_reason,
            notify_severity:$notify_severity,
            notify_route:$notify_route,
            notify_policy_name:$notify_policy_name,
            notify_policy_summary:$notify_policy_summary,
            notify_policy_escalated:$notify_policy_escalated,
            notify_policy_group_escalated:$notify_policy_group_escalated,
            notify_signal_key:$notify_signal_key,
            notify_signal_keys:$notify_signal_keys,
            previous_severity:$previous_severity,
            previous_signal_key:$previous_signal_key,
            notify_policy_occurrences:$notify_policy_occurrences,
            notify_policy_active_seconds:$notify_policy_active_seconds,
            alert_count:$alert_count,
            alerts_brief:$alerts_brief,
            family_brief:$family_brief,
            family_grouped_alerts:$family_grouped_alerts,
            summary_views:{
              version:1,
              policy_summary:$notify_policy_summary,
              family_brief:$family_brief,
              alerts_brief:$alerts_brief,
              family_grouped_alerts:$family_grouped_alerts
            }
          },
          payload:$payload[0]
        }' |
      curl -fsS -X POST "$WEBHOOK_URL" \
        -H 'content-type: application/json' \
        --data-binary @- >/dev/null
      ;;
    text)
      curl -fsS -X POST "$WEBHOOK_URL" \
        -H 'content-type: text/plain; charset=utf-8' \
        --data-binary "$summary_text" >/dev/null
      ;;
    *)
      echo "invalid OPERATOR_SIGNAL_NOTIFY_WEBHOOK_MODE: $WEBHOOK_MODE" >&2
      exit 64
      ;;
  esac
fi

if [[ "$ECHO_SUMMARY" == "1" ]]; then
  printf '%s\n' "$summary_text" >&2
fi

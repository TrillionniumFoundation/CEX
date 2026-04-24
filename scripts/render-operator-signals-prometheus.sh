#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/render-operator-signals-prometheus.sh [result-json-path|-]

Render Prometheus text exposition from the JSON output of:
  ./scripts/check-operator-signals.sh --compact
or from:
  run/operator-signals/last.json

Examples:
  ./scripts/check-operator-signals.sh --compact | ./scripts/render-operator-signals-prometheus.sh
  ./scripts/render-operator-signals-prometheus.sh run/operator-signals/last.json
EOF
}

escape_label_value() {
  local value="${1-}"
  value=${value//\\/\\\\}
  value=${value//"/\\"}
  value=${value//$'\n'/\\n}
  printf '%s' "$value"
}

emit_metric() {
  local name="$1"
  local labels="$2"
  local value="$3"
  if [[ -n "$labels" ]]; then
    printf '%s{%s} %s\n' "$name" "$labels" "$value"
  else
    printf '%s %s\n' "$name" "$value"
  fi
}

if (($# > 1)); then
  usage >&2
  exit 64
fi

if (($# == 1)); then
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    -)
      json_input="$(cat)"
      ;;
    *)
      if [[ ! -f "$1" ]]; then
        echo "input json file not found: $1" >&2
        exit 66
      fi
      json_input="$(cat "$1")"
      ;;
  esac
else
  json_input="$(cat)"
fi

if [[ -z "$json_input" ]]; then
  echo "expected operator-signal json on stdin or as a file path" >&2
  exit 64
fi

if ! jq empty >/dev/null 2>&1 <<<"$json_input"; then
  echo "invalid operator-signal json" >&2
  exit 65
fi

checked_at="$(jq -r '.checked_at // empty' <<<"$json_input")"
checked_at_epoch="0"
if [[ -n "$checked_at" ]]; then
  if parsed_epoch="$(date -d "$checked_at" +%s 2>/dev/null)"; then
    checked_at_epoch="$parsed_epoch"
  fi
fi

overall="$(jq -r '.overall // "unknown"' <<<"$json_input")"
exit_code="$(jq -r '.exit_code // 0' <<<"$json_input")"
warn_count="$(jq -r '.summary.warn_count // 0' <<<"$json_input")"
critical_count="$(jq -r '.summary.critical_count // 0' <<<"$json_input")"
source_count="$(jq -r '.summary.source_count // 0' <<<"$json_input")"
supporting_surface_count="$(jq -r '.summary.supporting_surface_count // 0' <<<"$json_input")"

echo '# HELP cex_operator_signal_last_checked_unixtime Unix timestamp of the last operator-signal snapshot.'
echo '# TYPE cex_operator_signal_last_checked_unixtime gauge'
emit_metric 'cex_operator_signal_last_checked_unixtime' '' "$checked_at_epoch"

echo '# HELP cex_operator_signal_overall_code Overall operator-signal severity encoded as ok=0,warn=1,critical=2.'
echo '# TYPE cex_operator_signal_overall_code gauge'
emit_metric 'cex_operator_signal_overall_code' '' "$exit_code"

echo '# HELP cex_operator_signal_overall_level Overall operator-signal state as one-hot gauge labels.'
echo '# TYPE cex_operator_signal_overall_level gauge'
for level in ok warn critical; do
  value=0
  if [[ "$overall" == "$level" ]]; then
    value=1
  fi
  emit_metric 'cex_operator_signal_overall_level' "level=\"$(escape_label_value "$level")\"" "$value"
done

echo '# HELP cex_operator_signal_alert_count Number of active operator signals by severity.'
echo '# TYPE cex_operator_signal_alert_count gauge'
emit_metric 'cex_operator_signal_alert_count' 'severity="warn"' "$warn_count"
emit_metric 'cex_operator_signal_alert_count' 'severity="critical"' "$critical_count"

echo '# HELP cex_operator_signal_source_count Number of source endpoints represented in the snapshot.'
echo '# TYPE cex_operator_signal_source_count gauge'
emit_metric 'cex_operator_signal_source_count' '' "$source_count"

echo '# HELP cex_operator_signal_supporting_surface_count Number of supporting surfaces included in the snapshot.'
echo '# TYPE cex_operator_signal_supporting_surface_count gauge'
emit_metric 'cex_operator_signal_supporting_surface_count' '' "$supporting_surface_count"

echo '# HELP cex_operator_signal_source_fetch_ok Whether a source endpoint fetch succeeded.'
echo '# TYPE cex_operator_signal_source_fetch_ok gauge'
echo '# HELP cex_operator_signal_source_http_status Last HTTP status code seen for a source endpoint (0 when unavailable).' 
echo '# TYPE cex_operator_signal_source_http_status gauge'
while IFS=$'\t' read -r source ok http_code; do
  [[ -z "$source" ]] && continue
  ok_value=0
  if [[ "$ok" == "true" ]]; then
    ok_value=1
  fi
  http_status=0
  if [[ "$http_code" =~ ^[0-9]+$ ]]; then
    http_status="$((10#$http_code))"
  fi
  labels="source=\"$(escape_label_value "$source")\""
  emit_metric 'cex_operator_signal_source_fetch_ok' "$labels" "$ok_value"
  emit_metric 'cex_operator_signal_source_http_status' "$labels" "$http_status"
done < <(jq -r '.sources // {} | to_entries[] | [ .key, (.value.fetch.ok // false), (.value.fetch.http_code // 0) ] | @tsv' <<<"$json_input")

echo '# HELP cex_operator_signal_active Active operator signals from the unified wrapper output.'
echo '# TYPE cex_operator_signal_active gauge'
echo '# HELP cex_operator_signal_value Current operator signal value from the wrapper output.'
echo '# TYPE cex_operator_signal_value gauge'
echo '# HELP cex_operator_signal_threshold Current operator signal threshold from the wrapper output.'
echo '# TYPE cex_operator_signal_threshold gauge'
while IFS=$'\t' read -r service name severity value threshold; do
  [[ -z "$service" || -z "$name" ]] && continue
  labels="service=\"$(escape_label_value "$service")\",name=\"$(escape_label_value "$name")\",severity=\"$(escape_label_value "$severity")\""
  emit_metric 'cex_operator_signal_active' "$labels" 1
  emit_metric 'cex_operator_signal_value' "$labels" "$value"
  emit_metric 'cex_operator_signal_threshold' "$labels" "$threshold"
done < <(jq -r '.alerts // [] | .[] | [(.service // "unknown"), (.name // "unknown"), (.severity // "unknown"), (.value // 0), (.threshold // 0)] | @tsv' <<<"$json_input")

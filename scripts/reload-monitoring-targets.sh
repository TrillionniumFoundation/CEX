#!/usr/bin/env bash
set -euo pipefail

PROMETHEUS_URL="${PROMETHEUS_RELOAD_URL:-http://127.0.0.1:9090/-/reload}"
ALERTMANAGER_URL="${ALERTMANAGER_RELOAD_URL:-http://127.0.0.1:9093/-/reload}"
PROMETHEUS_COMMAND="${PROMETHEUS_RELOAD_COMMAND:-}"
ALERTMANAGER_COMMAND="${ALERTMANAGER_RELOAD_COMMAND:-}"
PROMETHEUS_RESTART_COMMAND="${PROMETHEUS_RESTART_COMMAND:-}"
ALERTMANAGER_RESTART_COMMAND="${ALERTMANAGER_RESTART_COMMAND:-}"
MODE="auto"
TIMEOUT_SECS="10"
FAILURE_POLICY="${MONITORING_RELOAD_FAILURE_POLICY:-fail}"
SKIP_PROMETHEUS="false"
SKIP_ALERTMANAGER="false"
DRY_RUN="false"
SUMMARY_FILE=""

PROMETHEUS_STATUS="pending"
PROMETHEUS_SUCCESS="false"
PROMETHEUS_ACTION="none"
PROMETHEUS_TRANSPORT=""
PROMETHEUS_FALLBACK_USED="false"
PROMETHEUS_SKIPPED="false"

ALERTMANAGER_STATUS="pending"
ALERTMANAGER_SUCCESS="false"
ALERTMANAGER_ACTION="none"
ALERTMANAGER_TRANSPORT=""
ALERTMANAGER_FALLBACK_USED="false"
ALERTMANAGER_SKIPPED="false"

usage() {
  cat <<'EOF'
Usage: scripts/reload-monitoring-targets.sh [--mode auto|http|command] [--timeout-secs <n>] [--failure-policy fail|restart] [--prometheus-url <url>] [--alertmanager-url <url>] [--prometheus-command <cmd>] [--alertmanager-command <cmd>] [--prometheus-restart-command <cmd>] [--alertmanager-restart-command <cmd>] [--skip-prometheus] [--skip-alertmanager] [--dry-run] [--summary-file <path>]

Triggers Prometheus / Alertmanager configuration reload after monitoring bundle deployment.

Default behavior:
  - mode=auto
  - if a reload command is configured, run the command
  - otherwise POST to the reload URL
  - default URLs are Prometheus http://127.0.0.1:9090/-/reload and Alertmanager http://127.0.0.1:9093/-/reload
  - failure-policy=fail (stop on reload failure unless restart fallback is enabled)

Environment overrides:
  PROMETHEUS_RELOAD_URL
  ALERTMANAGER_RELOAD_URL
  PROMETHEUS_RELOAD_COMMAND
  ALERTMANAGER_RELOAD_COMMAND
  PROMETHEUS_RESTART_COMMAND
  ALERTMANAGER_RESTART_COMMAND
  MONITORING_RELOAD_FAILURE_POLICY

Options:
  --mode <kind>                  One of: auto, http, command (default: auto)
  --timeout-secs <n>             HTTP timeout in seconds (default: 10)
  --failure-policy <kind>        One of: fail, restart (default: fail)
  --prometheus-url <url>         Override Prometheus reload URL
  --alertmanager-url <url>       Override Alertmanager reload URL
  --prometheus-command <c>       Shell command to reload Prometheus
  --alertmanager-command <c>     Shell command to reload Alertmanager
  --prometheus-restart-command <c> Shell command to restart Prometheus if reload fails
  --alertmanager-restart-command <c> Shell command to restart Alertmanager if reload fails
  --skip-prometheus              Skip Prometheus reload
  --skip-alertmanager            Skip Alertmanager reload
  --dry-run                      Print what would run without executing
  --summary-file <path>          Write a machine-readable YAML summary to the given path
  -h, --help                     Show this help

Examples:
  ./scripts/reload-monitoring-targets.sh
  ./scripts/reload-monitoring-targets.sh --dry-run
  ./scripts/reload-monitoring-targets.sh --mode http --prometheus-url http://127.0.0.1:9090/-/reload
  ./scripts/reload-monitoring-targets.sh --mode command --prometheus-command 'systemctl reload prometheus' --alertmanager-command 'systemctl reload alertmanager'
  ./scripts/reload-monitoring-targets.sh --mode command --failure-policy restart --prometheus-command 'systemctl reload prometheus' --prometheus-restart-command 'systemctl restart prometheus'
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      [[ $# -ge 2 ]] || { echo "Error: --mode requires auto|http|command" >&2; exit 2; }
      MODE="$2"
      shift 2
      ;;
    --timeout-secs)
      [[ $# -ge 2 ]] || { echo "Error: --timeout-secs requires a value" >&2; exit 2; }
      TIMEOUT_SECS="$2"
      shift 2
      ;;
    --failure-policy)
      [[ $# -ge 2 ]] || { echo "Error: --failure-policy requires fail|restart" >&2; exit 2; }
      FAILURE_POLICY="$2"
      shift 2
      ;;
    --prometheus-url)
      [[ $# -ge 2 ]] || { echo "Error: --prometheus-url requires a value" >&2; exit 2; }
      PROMETHEUS_URL="$2"
      shift 2
      ;;
    --alertmanager-url)
      [[ $# -ge 2 ]] || { echo "Error: --alertmanager-url requires a value" >&2; exit 2; }
      ALERTMANAGER_URL="$2"
      shift 2
      ;;
    --prometheus-command)
      [[ $# -ge 2 ]] || { echo "Error: --prometheus-command requires a value" >&2; exit 2; }
      PROMETHEUS_COMMAND="$2"
      shift 2
      ;;
    --alertmanager-command)
      [[ $# -ge 2 ]] || { echo "Error: --alertmanager-command requires a value" >&2; exit 2; }
      ALERTMANAGER_COMMAND="$2"
      shift 2
      ;;
    --prometheus-restart-command)
      [[ $# -ge 2 ]] || { echo "Error: --prometheus-restart-command requires a value" >&2; exit 2; }
      PROMETHEUS_RESTART_COMMAND="$2"
      shift 2
      ;;
    --alertmanager-restart-command)
      [[ $# -ge 2 ]] || { echo "Error: --alertmanager-restart-command requires a value" >&2; exit 2; }
      ALERTMANAGER_RESTART_COMMAND="$2"
      shift 2
      ;;
    --skip-prometheus)
      SKIP_PROMETHEUS="true"
      shift
      ;;
    --skip-alertmanager)
      SKIP_ALERTMANAGER="true"
      shift
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    --summary-file)
      [[ $# -ge 2 ]] || { echo "Error: --summary-file requires a path" >&2; exit 2; }
      SUMMARY_FILE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$MODE" in
  auto|http|command) ;;
  *)
    echo "Error: --mode must be auto, http, or command" >&2
    exit 2
    ;;
esac

case "$FAILURE_POLICY" in
  fail|restart) ;;
  *)
    echo "Error: --failure-policy must be fail or restart" >&2
    exit 2
    ;;
esac

if ! [[ "$TIMEOUT_SECS" =~ ^[0-9]+$ ]]; then
  echo "Error: --timeout-secs must be an integer" >&2
  exit 2
fi

set_service_field() {
  local service="$1"
  local field="$2"
  local value="$3"
  printf -v "${service^^}_${field}" '%s' "$value"
}

resolve_reload_transport() {
  local command="$1"
  if [[ "$MODE" == "http" ]]; then
    printf 'http'
  elif [[ "$MODE" == "command" ]]; then
    printf 'command'
  elif [[ -n "$command" ]]; then
    printf 'command'
  else
    printf 'http'
  fi
}

write_summary_file() {
  local exit_code="$1"
  [[ -n "$SUMMARY_FILE" ]] || return 0
  python3 - "$SUMMARY_FILE" "$exit_code" "$MODE" "$TIMEOUT_SECS" "$FAILURE_POLICY" "$DRY_RUN" \
    "$PROMETHEUS_URL" "$PROMETHEUS_COMMAND" "$PROMETHEUS_RESTART_COMMAND" "$PROMETHEUS_STATUS" "$PROMETHEUS_SUCCESS" "$PROMETHEUS_ACTION" "$PROMETHEUS_TRANSPORT" "$PROMETHEUS_FALLBACK_USED" "$PROMETHEUS_SKIPPED" \
    "$ALERTMANAGER_URL" "$ALERTMANAGER_COMMAND" "$ALERTMANAGER_RESTART_COMMAND" "$ALERTMANAGER_STATUS" "$ALERTMANAGER_SUCCESS" "$ALERTMANAGER_ACTION" "$ALERTMANAGER_TRANSPORT" "$ALERTMANAGER_FALLBACK_USED" "$ALERTMANAGER_SKIPPED" <<'PY'
from pathlib import Path
import sys, yaml
summary_path = Path(sys.argv[1])
exit_code = int(sys.argv[2])
mode = sys.argv[3]
timeout_secs = int(sys.argv[4])
failure_policy = sys.argv[5]
dry_run = sys.argv[6] == 'true'
services = {
    'prometheus': {
        'url': sys.argv[7],
        'commandConfigured': bool(sys.argv[8]),
        'restartCommandConfigured': bool(sys.argv[9]),
        'status': sys.argv[10],
        'success': sys.argv[11] == 'true',
        'action': sys.argv[12],
        'transport': sys.argv[13],
        'fallbackUsed': sys.argv[14] == 'true',
        'skipped': sys.argv[15] == 'true',
    },
    'alertmanager': {
        'url': sys.argv[16],
        'commandConfigured': bool(sys.argv[17]),
        'restartCommandConfigured': bool(sys.argv[18]),
        'status': sys.argv[19],
        'success': sys.argv[20] == 'true',
        'action': sys.argv[21],
        'transport': sys.argv[22],
        'fallbackUsed': sys.argv[23] == 'true',
        'skipped': sys.argv[24] == 'true',
    },
}
summary = {
    'version': 1,
    'kind': 'reload',
    'mode': mode,
    'timeoutSecs': timeout_secs,
    'failurePolicy': failure_policy,
    'dryRun': dry_run,
    'exitCode': exit_code,
    'overallSuccess': exit_code == 0,
    'failureCount': sum(1 for item in services.values() if not item['skipped'] and not item['success']),
    'services': services,
}
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary_path.write_text('# Generated by reload-monitoring-targets.sh\n\n' + yaml.safe_dump(summary, sort_keys=False, allow_unicode=True), encoding='utf-8')
PY
}

run_http_reload() {
  local service="$1"
  local url="$2"
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "dry-run: [$service] POST $url"
    return 0
  fi
  local code
  if ! code="$(curl -fsS -o /dev/null -w '%{http_code}' --max-time "$TIMEOUT_SECS" -X POST "$url")"; then
    echo "Error: [$service] reload via http failed: $url" >&2
    return 1
  fi
  echo "reloaded [$service] via http $url (status=$code)"
}

run_shell_action() {
  local service="$1"
  local action="$2"
  local command="$3"
  if [[ -z "$command" ]]; then
    echo "Error: [$service] ${action} command is empty" >&2
    return 1
  fi
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "dry-run: [$service] ${action} command: $command"
    return 0
  fi
  if ! bash -lc "$command"; then
    echo "Error: [$service] ${action} command failed" >&2
    return 1
  fi
  echo "${action}ed [$service] via command"
}

run_reload_action() {
  local service="$1"
  local url="$2"
  local command="$3"
  case "$MODE" in
    http)
      run_http_reload "$service" "$url"
      ;;
    command)
      run_shell_action "$service" reload "$command"
      ;;
    auto)
      if [[ -n "$command" ]]; then
        run_shell_action "$service" reload "$command"
      else
        run_http_reload "$service" "$url"
      fi
      ;;
  esac
}

run_restart_fallback() {
  local service="$1"
  local command="$2"
  if [[ "$FAILURE_POLICY" != "restart" ]]; then
    return 1
  fi
  if [[ -z "$command" ]]; then
    echo "Error: [$service] reload failed and no restart command is configured" >&2
    return 1
  fi
  echo "warning: [$service] reload failed, attempting restart fallback" >&2
  run_shell_action "$service" restart "$command"
}

reload_service() {
  local service="$1"
  local url="$2"
  local reload_command="$3"
  local restart_command="$4"

  if run_reload_action "$service" "$url" "$reload_command"; then
    set_service_field "$service" STATUS "success"
    set_service_field "$service" SUCCESS "true"
    set_service_field "$service" ACTION "reload"
    set_service_field "$service" TRANSPORT "$(resolve_reload_transport "$reload_command")"
    set_service_field "$service" FALLBACK_USED "false"
    return 0
  fi

  if run_restart_fallback "$service" "$restart_command"; then
    set_service_field "$service" STATUS "restart_fallback"
    set_service_field "$service" SUCCESS "true"
    set_service_field "$service" ACTION "restart"
    set_service_field "$service" TRANSPORT "command"
    set_service_field "$service" FALLBACK_USED "true"
    return 0
  fi

  set_service_field "$service" STATUS "failed"
  set_service_field "$service" SUCCESS "false"
  set_service_field "$service" ACTION "reload"
  set_service_field "$service" TRANSPORT "$(resolve_reload_transport "$reload_command")"
  set_service_field "$service" FALLBACK_USED "false"
  return 1
}

failure_count=0

if [[ "$SKIP_PROMETHEUS" != "true" ]]; then
  if ! reload_service "prometheus" "$PROMETHEUS_URL" "$PROMETHEUS_COMMAND" "$PROMETHEUS_RESTART_COMMAND"; then
    failure_count=$((failure_count + 1))
  fi
else
  PROMETHEUS_STATUS="skipped"
  PROMETHEUS_SKIPPED="true"
fi

if [[ "$SKIP_ALERTMANAGER" != "true" ]]; then
  if ! reload_service "alertmanager" "$ALERTMANAGER_URL" "$ALERTMANAGER_COMMAND" "$ALERTMANAGER_RESTART_COMMAND"; then
    failure_count=$((failure_count + 1))
  fi
else
  ALERTMANAGER_STATUS="skipped"
  ALERTMANAGER_SKIPPED="true"
fi

if [[ "$failure_count" -gt 0 ]]; then
  write_summary_file 1
  echo "Error: monitoring reload actions failed for $failure_count target(s)" >&2
  exit 1
fi

write_summary_file 0

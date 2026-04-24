#!/usr/bin/env bash
set -euo pipefail

PROMETHEUS_URL="${PROMETHEUS_VERIFY_URL:-http://127.0.0.1:9090/-/healthy}"
ALERTMANAGER_URL="${ALERTMANAGER_VERIFY_URL:-http://127.0.0.1:9093/-/healthy}"
PROMETHEUS_COMMAND="${PROMETHEUS_VERIFY_COMMAND:-}"
ALERTMANAGER_COMMAND="${ALERTMANAGER_VERIFY_COMMAND:-}"
MODE="auto"
TIMEOUT_SECS="10"
ATTEMPTS="${MONITORING_VERIFY_ATTEMPTS:-1}"
DELAY_SECS="${MONITORING_VERIFY_DELAY_SECS:-1}"
SKIP_PROMETHEUS="false"
SKIP_ALERTMANAGER="false"
DRY_RUN="false"
SUMMARY_FILE=""

PROMETHEUS_STATUS="pending"
PROMETHEUS_SUCCESS="false"
PROMETHEUS_TRANSPORT=""
PROMETHEUS_SKIPPED="false"
PROMETHEUS_ATTEMPTS_USED="0"

ALERTMANAGER_STATUS="pending"
ALERTMANAGER_SUCCESS="false"
ALERTMANAGER_TRANSPORT=""
ALERTMANAGER_SKIPPED="false"
ALERTMANAGER_ATTEMPTS_USED="0"

usage() {
  cat <<'EOF'
Usage: scripts/verify-monitoring-targets.sh [--mode auto|http|command] [--timeout-secs <n>] [--attempts <n>] [--delay-secs <n>] [--prometheus-url <url>] [--alertmanager-url <url>] [--prometheus-command <cmd>] [--alertmanager-command <cmd>] [--skip-prometheus] [--skip-alertmanager] [--dry-run] [--summary-file <path>]

Verifies Prometheus / Alertmanager health after monitoring bundle deploy/reload.

Default behavior:
  - mode=auto
  - if a verify command is configured, run the command
  - otherwise GET the verify URL
  - default URLs are Prometheus http://127.0.0.1:9090/-/healthy and Alertmanager http://127.0.0.1:9093/-/healthy
  - attempts=1, delay-secs=1

Environment overrides:
  PROMETHEUS_VERIFY_URL
  ALERTMANAGER_VERIFY_URL
  PROMETHEUS_VERIFY_COMMAND
  ALERTMANAGER_VERIFY_COMMAND
  MONITORING_VERIFY_ATTEMPTS
  MONITORING_VERIFY_DELAY_SECS

Options:
  --mode <kind>               One of: auto, http, command (default: auto)
  --timeout-secs <n>          HTTP timeout in seconds (default: 10)
  --attempts <n>              Verification attempts per service (default: 1)
  --delay-secs <n>            Delay between failed attempts (default: 1)
  --prometheus-url <url>      Override Prometheus verify URL
  --alertmanager-url <url>    Override Alertmanager verify URL
  --prometheus-command <cmd>  Shell command to verify Prometheus health
  --alertmanager-command <cmd> Shell command to verify Alertmanager health
  --skip-prometheus           Skip Prometheus verification
  --skip-alertmanager         Skip Alertmanager verification
  --dry-run                   Print what would run without executing
  --summary-file <path>       Write a machine-readable YAML summary to the given path
  -h, --help                  Show this help

Examples:
  ./scripts/verify-monitoring-targets.sh
  ./scripts/verify-monitoring-targets.sh --dry-run
  ./scripts/verify-monitoring-targets.sh --attempts 5 --delay-secs 2
  ./scripts/verify-monitoring-targets.sh --mode command --prometheus-command 'curl -fsS http://127.0.0.1:9090/-/healthy >/dev/null'
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
    --attempts)
      [[ $# -ge 2 ]] || { echo "Error: --attempts requires a value" >&2; exit 2; }
      ATTEMPTS="$2"
      shift 2
      ;;
    --delay-secs)
      [[ $# -ge 2 ]] || { echo "Error: --delay-secs requires a value" >&2; exit 2; }
      DELAY_SECS="$2"
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

for value_name in TIMEOUT_SECS ATTEMPTS DELAY_SECS; do
  value="${!value_name}"
  if ! [[ "$value" =~ ^[0-9]+$ ]]; then
    echo "Error: ${value_name,,} must be an integer" >&2
    exit 2
  fi
done

set_service_field() {
  local service="$1"
  local field="$2"
  local value="$3"
  printf -v "${service^^}_${field}" '%s' "$value"
}

resolve_verify_transport() {
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
  python3 - "$SUMMARY_FILE" "$exit_code" "$MODE" "$TIMEOUT_SECS" "$ATTEMPTS" "$DELAY_SECS" "$DRY_RUN" \
    "$PROMETHEUS_URL" "$PROMETHEUS_COMMAND" "$PROMETHEUS_STATUS" "$PROMETHEUS_SUCCESS" "$PROMETHEUS_TRANSPORT" "$PROMETHEUS_SKIPPED" "$PROMETHEUS_ATTEMPTS_USED" \
    "$ALERTMANAGER_URL" "$ALERTMANAGER_COMMAND" "$ALERTMANAGER_STATUS" "$ALERTMANAGER_SUCCESS" "$ALERTMANAGER_TRANSPORT" "$ALERTMANAGER_SKIPPED" "$ALERTMANAGER_ATTEMPTS_USED" <<'PY'
from pathlib import Path
import sys, yaml
summary_path = Path(sys.argv[1])
exit_code = int(sys.argv[2])
mode = sys.argv[3]
timeout_secs = int(sys.argv[4])
attempts = int(sys.argv[5])
delay_secs = int(sys.argv[6])
dry_run = sys.argv[7] == 'true'
services = {
    'prometheus': {
        'url': sys.argv[8],
        'commandConfigured': bool(sys.argv[9]),
        'status': sys.argv[10],
        'success': sys.argv[11] == 'true',
        'transport': sys.argv[12],
        'skipped': sys.argv[13] == 'true',
        'attemptsUsed': int(sys.argv[14]),
    },
    'alertmanager': {
        'url': sys.argv[15],
        'commandConfigured': bool(sys.argv[16]),
        'status': sys.argv[17],
        'success': sys.argv[18] == 'true',
        'transport': sys.argv[19],
        'skipped': sys.argv[20] == 'true',
        'attemptsUsed': int(sys.argv[21]),
    },
}
summary = {
    'version': 1,
    'kind': 'verify',
    'mode': mode,
    'timeoutSecs': timeout_secs,
    'attempts': attempts,
    'delaySecs': delay_secs,
    'dryRun': dry_run,
    'exitCode': exit_code,
    'overallSuccess': exit_code == 0,
    'failureCount': sum(1 for item in services.values() if not item['skipped'] and not item['success']),
    'services': services,
}
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary_path.write_text('# Generated by verify-monitoring-targets.sh\n\n' + yaml.safe_dump(summary, sort_keys=False, allow_unicode=True), encoding='utf-8')
PY
}

run_http_verify() {
  local service="$1"
  local url="$2"
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "dry-run: [$service] GET $url"
    return 0
  fi
  local code
  if ! code="$(curl -fsS -o /dev/null -w '%{http_code}' --max-time "$TIMEOUT_SECS" "$url")"; then
    echo "Error: [$service] verify via http failed: $url" >&2
    return 1
  fi
  echo "verified [$service] via http $url (status=$code)"
}

run_command_verify() {
  local service="$1"
  local command="$2"
  if [[ -z "$command" ]]; then
    echo "Error: [$service] verify command is empty" >&2
    return 1
  fi
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "dry-run: [$service] verify command: $command"
    return 0
  fi
  if ! bash -lc "$command"; then
    echo "Error: [$service] verify command failed" >&2
    return 1
  fi
  echo "verified [$service] via command"
}

run_verify_action() {
  local service="$1"
  local url="$2"
  local command="$3"
  case "$MODE" in
    http)
      run_http_verify "$service" "$url"
      ;;
    command)
      run_command_verify "$service" "$command"
      ;;
    auto)
      if [[ -n "$command" ]]; then
        run_command_verify "$service" "$command"
      else
        run_http_verify "$service" "$url"
      fi
      ;;
  esac
}

verify_service() {
  local service="$1"
  local url="$2"
  local command="$3"
  local attempt=1
  while (( attempt <= ATTEMPTS )); do
    set_service_field "$service" ATTEMPTS_USED "$attempt"
    if run_verify_action "$service" "$url" "$command"; then
      set_service_field "$service" STATUS "success"
      set_service_field "$service" SUCCESS "true"
      set_service_field "$service" TRANSPORT "$(resolve_verify_transport "$command")"
      return 0
    fi
    if (( attempt < ATTEMPTS )); then
      echo "warning: [$service] verification attempt ${attempt}/${ATTEMPTS} failed, retrying after ${DELAY_SECS}s" >&2
      if [[ "$DRY_RUN" != "true" ]]; then
        sleep "$DELAY_SECS"
      fi
    fi
    attempt=$((attempt + 1))
  done
  set_service_field "$service" STATUS "failed"
  set_service_field "$service" SUCCESS "false"
  set_service_field "$service" TRANSPORT "$(resolve_verify_transport "$command")"
  echo "Error: [$service] verification failed after ${ATTEMPTS} attempt(s)" >&2
  return 1
}

failure_count=0

if [[ "$SKIP_PROMETHEUS" != "true" ]]; then
  if ! verify_service "prometheus" "$PROMETHEUS_URL" "$PROMETHEUS_COMMAND"; then
    failure_count=$((failure_count + 1))
  fi
else
  PROMETHEUS_STATUS="skipped"
  PROMETHEUS_SKIPPED="true"
fi

if [[ "$SKIP_ALERTMANAGER" != "true" ]]; then
  if ! verify_service "alertmanager" "$ALERTMANAGER_URL" "$ALERTMANAGER_COMMAND"; then
    failure_count=$((failure_count + 1))
  fi
else
  ALERTMANAGER_STATUS="skipped"
  ALERTMANAGER_SKIPPED="true"
fi

if [[ "$failure_count" -gt 0 ]]; then
  write_summary_file 1
  echo "Error: monitoring verification failed for $failure_count target(s)" >&2
  exit 1
fi

write_summary_file 0

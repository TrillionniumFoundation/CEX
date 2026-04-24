#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
HELPER="$ROOT_DIR/scripts/install-operator-signal-cron.sh"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/operator-signal-cron-cli-surfaces.sh"
MODE="forward"
PASS_THROUGH=()

set_mode() {
  local requested="$1"
  if [[ "$MODE" != "forward" && "$MODE" != "$requested" ]]; then
    echo "Error: only one of --status, --status-json, --examples, --doctor, --doctor-json, --help-json, or --schema may be used at a time" >&2
    exit 2
  fi
  MODE="$requested"
}

usage() {
  osc_install_print_usage
}

print_examples() {
  osc_install_print_examples
}

json_array_from_args() {
  if [[ "$#" -eq 0 ]]; then
    printf '[]\n'
    return 0
  fi
  printf '%s\n' "$@" | jq -R . | jq -s .
}

collect_status() {
  status_output="$($HELPER --show 2>&1 || true)"
  if [[ "$status_output" == "No matching OpenClaw operator signal cron job found." ]]; then
    cron_status="absent"
    matching_jobs_json='[]'
    matching_count='0'
  elif [[ "$status_output" == \[* ]]; then
    cron_status="present"
    matching_jobs_json="$status_output"
    matching_count="$(jq 'length' <<<"$matching_jobs_json")"
  else
    cron_status="unknown"
    matching_jobs_json='[]'
    matching_count='0'
  fi
}

collect_doctor() {
  doctor_kind="$OSC_INSTALL_DOCTOR_KIND"
  doctor_schema_version="$OSC_INSTALL_DOCTOR_SCHEMA_VERSION"
  default_policy_profile="$OSC_INSTALL_DEFAULT_POLICY_PROFILE"

  recommended_install_command="./install-operator-signal-cron.sh"
  recommended_dry_run_command="./install-operator-signal-cron.sh --dry-run"
  recommended_status_command="./install-operator-signal-cron.sh --status"
  recommended_doctor_command="./install-operator-signal-cron.sh --doctor"
  recommended_doctor_json_command="./install-operator-signal-cron.sh --doctor-json"

  profile_install_command_default="./install-operator-signal-cron.sh --policy-profile default"
  profile_install_command_identity="./install-operator-signal-cron.sh --policy-profile identity"
  profile_install_command_deploy="./install-operator-signal-cron.sh --policy-profile deploy"

  openclaw_path="$(command -v openclaw 2>/dev/null || true)"
  if [[ -n "$openclaw_path" ]]; then
    openclaw_available="true"
  else
    openclaw_available="false"
  fi

  helper_args_json="$(json_array_from_args "${PASS_THROUGH[@]}")"
  default_run_command="$($HELPER --print-run-command)"
  effective_run_command="$($HELPER --print-run-command "${PASS_THROUGH[@]}")"
  collect_status
}

print_help_json() {
  osc_install_print_help_json "$HELPER"
}

print_schema_json() {
  osc_install_print_schema_json
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      set_mode help
      shift
      ;;
    --status)
      set_mode status
      shift
      ;;
    --status-json)
      set_mode status-json
      shift
      ;;
    --examples)
      set_mode examples
      shift
      ;;
    --doctor)
      set_mode doctor
      shift
      ;;
    --doctor-json)
      set_mode doctor-json
      shift
      ;;
    --help-json)
      set_mode help-json
      shift
      ;;
    --schema)
      set_mode schema
      shift
      ;;
    *)
      PASS_THROUGH+=("$1")
      shift
      ;;
  esac
done

case "$MODE" in
  help)
    usage
    ;;
  status)
    collect_status
    osc_install_print_status_text "$status_output"
    ;;
  status-json)
    collect_status
    osc_install_print_status_json "$ROOT_DIR" "$HELPER" "$cron_status" "$matching_count" "$matching_jobs_json" "$status_output"
    ;;
  examples)
    print_examples
    ;;
  doctor)
    collect_doctor
    osc_install_print_doctor_text \
      "$doctor_kind" \
      "$doctor_schema_version" \
      "$ROOT_DIR" \
      "$HELPER" \
      "$default_policy_profile" \
      "$openclaw_available" \
      "${openclaw_path:-}" \
      "$cron_status" \
      "$default_run_command" \
      "$effective_run_command" \
      "$recommended_install_command" \
      "$recommended_dry_run_command" \
      "$recommended_status_command" \
      "$recommended_doctor_command" \
      "$recommended_doctor_json_command" \
      "$profile_install_command_default" \
      "$profile_install_command_identity" \
      "$profile_install_command_deploy" \
      "$status_output"
    ;;
  doctor-json)
    collect_doctor
    osc_install_print_doctor_json \
      "$ROOT_DIR" \
      "$HELPER" \
      "$default_policy_profile" \
      "$openclaw_available" \
      "${openclaw_path:-}" \
      "$cron_status" \
      "$default_run_command" \
      "$effective_run_command" \
      "$recommended_install_command" \
      "$recommended_dry_run_command" \
      "$recommended_status_command" \
      "$recommended_doctor_command" \
      "$recommended_doctor_json_command" \
      "$profile_install_command_default" \
      "$profile_install_command_identity" \
      "$profile_install_command_deploy" \
      "$status_output" \
      "$helper_args_json"
    ;;
  help-json)
    print_help_json
    ;;
  schema)
    print_schema_json
    ;;
  forward)
    exec "$HELPER" "${PASS_THROUGH[@]}"
    ;;
esac

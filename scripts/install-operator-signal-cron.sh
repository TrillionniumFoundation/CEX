#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}" )" && pwd)"
# shellcheck source=/dev/null
source "$SCRIPT_DIR/operator-signal-cron-cli-surfaces.sh"

ACTION="install"
PASS_THROUGH=()

usage() {
  osc_script_install_print_usage
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      PASS_THROUGH+=(--dry-run)
      shift
      ;;
    --show)
      ACTION="show"
      shift
      ;;
    --print-run-command)
      PASS_THROUGH+=(--print-run-command)
      shift
      ;;
    --print-message)
      PASS_THROUGH+=(--print-message)
      shift
      ;;
    --remove)
      ACTION="remove"
      shift
      ;;
    --run-now)
      ACTION="run-now"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      PASS_THROUGH+=("$1")
      shift
      ;;
  esac
done

register_args=(--action "$ACTION")
if ! osc_operator_signal_has_explicit_policy_flags "${PASS_THROUGH[@]}"; then
  register_args+=(--policy-profile "$OSC_OPERATOR_SIGNAL_DEFAULT_POLICY_PROFILE")
fi
register_args+=("${PASS_THROUGH[@]}")

exec "$SCRIPT_DIR/register-openclaw-operator-signal-cron.sh" "${register_args[@]}"

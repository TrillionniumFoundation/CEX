#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}" )" && pwd)"
# shellcheck source=/dev/null
source "$SCRIPT_DIR/operator-signal-cron-cli-surfaces.sh"

ACTION="install"
RECREATE="false"
DRY_RUN="false"
PRINT_RUN_COMMAND="false"
PRINT_MESSAGE="false"
USE_ENTRY_IDENTITY_POLICY_EXAMPLE="false"
USE_MONITORING_DEPLOY_POLICY_EXAMPLE="false"
POLICY_BUNDLES=()
POLICY_PROFILES=()

JOB_NAME="CEX Operator Signal Monitor"
BASE_JOB_DESCRIPTION="Run the repo-local operator signal wrapper every 5 minutes with OpenClaw cron."
RUN_COMMAND="$OSC_OPERATOR_SIGNAL_RUN_COMMAND_BASE"

usage() {
  osc_register_print_usage
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --action)
      [[ $# -ge 2 ]] || { echo "Error: --action requires a value" >&2; exit 2; }
      ACTION="$2"
      shift 2
      ;;
    --recreate)
      RECREATE="true"
      shift
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    --print-run-command)
      PRINT_RUN_COMMAND="true"
      shift
      ;;
    --print-message)
      PRINT_MESSAGE="true"
      shift
      ;;
    --use-entry-identity-policy-example)
      USE_ENTRY_IDENTITY_POLICY_EXAMPLE="true"
      shift
      ;;
    --use-monitoring-deploy-policy-example)
      USE_MONITORING_DEPLOY_POLICY_EXAMPLE="true"
      shift
      ;;
    --policy-bundle)
      [[ $# -ge 2 ]] || { echo "Error: --policy-bundle requires a value" >&2; exit 2; }
      POLICY_BUNDLES+=("$2")
      shift 2
      ;;
    --policy-profile)
      [[ $# -ge 2 ]] || { echo "Error: --policy-profile requires a value" >&2; exit 2; }
      POLICY_PROFILES+=("$2")
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

case "$ACTION" in
  install|show|remove|run-now) ;;
  *)
    echo "Error: --action must be install, show, remove, or run-now" >&2
    exit 2
    ;;
esac

if [[ "$USE_ENTRY_IDENTITY_POLICY_EXAMPLE" == "true" ]]; then
  POLICY_BUNDLES+=("entry-identity")
fi
if [[ "$USE_MONITORING_DEPLOY_POLICY_EXAMPLE" == "true" ]]; then
  POLICY_BUNDLES+=("monitoring-deploy")
fi

if [[ "${#POLICY_BUNDLES[@]}" -eq 0 && "${#POLICY_PROFILES[@]}" -eq 0 ]]; then
  POLICY_PROFILES+=("$OSC_OPERATOR_SIGNAL_DEFAULT_POLICY_PROFILE")
fi

osc_operator_signal_validate_unique_list POLICY_BUNDLES
osc_operator_signal_validate_unique_list POLICY_PROFILES

for bundle in "${POLICY_BUNDLES[@]}"; do
  if ! osc_operator_signal_validate_policy_bundle "$bundle"; then
    echo "Error: unsupported --policy-bundle: $bundle" >&2
    exit 2
  fi
done

for profile in "${POLICY_PROFILES[@]}"; do
  if ! osc_operator_signal_validate_policy_profile "$profile"; then
    echo "Error: unsupported --policy-profile: $profile" >&2
    exit 2
  fi
done

if [[ "${#POLICY_PROFILES[@]}" -gt 0 ]]; then
  IFS=',' read -r -a _unused <<< "$(printf '%s,' "${POLICY_PROFILES[@]}")"
  profile_value="$(IFS=,; echo "${POLICY_PROFILES[*]}")"
  RUN_COMMAND="OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE=\"$profile_value\" ./scripts/run-operator-signal-check.sh --compact"
elif [[ "${#POLICY_BUNDLES[@]}" -gt 0 ]]; then
  bundle_value="$(IFS=,; echo "${POLICY_BUNDLES[*]}")"
  RUN_COMMAND="OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE=\"$bundle_value\" ./scripts/run-operator-signal-check.sh --compact"
fi

JOB_DESCRIPTION="$BASE_JOB_DESCRIPTION"
if [[ "${#POLICY_PROFILES[@]}" -gt 0 ]]; then
  JOB_DESCRIPTION+=" Uses the repo-local policy profiles: $(osc_operator_signal_join_by_comma_space "${POLICY_PROFILES[@]}")."
elif [[ "${#POLICY_BUNDLES[@]}" -gt 0 ]]; then
  JOB_DESCRIPTION+=" Uses the repo-local policy bundles: $(osc_operator_signal_join_by_comma_space "${POLICY_BUNDLES[@]}")."
fi

MESSAGE=$(cat <<EOF
In /data/home-data/CEX, run exactly this command from the repo root:

$RUN_COMMAND

Rules:
- Do not edit source files.
- This run is only for operator-signal monitoring.
- If the result is ok, keep the internal summary short.
- If the result is warn or critical, summarize the triggered signals briefly.
- If the script fails, report the exact failure briefly.
EOF
)

get_cron_list_json() {
  local raw
  raw="$(openclaw cron list --json)"
  [[ -n "$raw" ]] || { echo "openclaw cron list returned empty output" >&2; exit 1; }
  printf '%s\n' "$raw"
}

get_matching_jobs() {
  get_cron_list_json | jq --arg name "$JOB_NAME" '[.jobs[] | select(.name == $name)]'
}

remove_job_by_id() {
  local id="$1"
  openclaw cron rm "$id" --json >/dev/null
}

add_job() {
  openclaw cron add --json \
    --name "$JOB_NAME" \
    --description "$JOB_DESCRIPTION" \
    --every 5m \
    --session isolated \
    --agent main \
    --message "$MESSAGE" \
    --tools 'exec,read' \
    --thinking minimal \
    --light-context \
    --no-deliver
}

print_dry_run() {
  jq -cn \
    --arg action "$ACTION" \
    --arg jobName "$JOB_NAME" \
    --arg jobDescription "$JOB_DESCRIPTION" \
    --arg runCommand "$RUN_COMMAND" \
    --arg message "$MESSAGE" \
    --arg recreate "$RECREATE" \
    --argjson policyBundles "$(printf '%s\n' "${POLICY_BUNDLES[@]}" | jq -R . | jq -s .)" \
    --argjson policyProfiles "$(printf '%s\n' "${POLICY_PROFILES[@]}" | jq -R . | jq -s .)" \
    '{action:$action,jobName:$jobName,jobDescription:$jobDescription,runCommand:$runCommand,message:$message,recreate:($recreate=="true"),policyBundles:$policyBundles,policyProfiles:$policyProfiles}'
}

if [[ "$PRINT_RUN_COMMAND" == "true" ]]; then
  printf '%s\n' "$RUN_COMMAND"
  exit 0
fi

if [[ "$PRINT_MESSAGE" == "true" ]]; then
  printf '%s\n' "$MESSAGE"
  exit 0
fi

if [[ "$DRY_RUN" == "true" ]]; then
  print_dry_run
  exit 0
fi

matches_json="$(get_matching_jobs)"
matches_count="$(jq 'length' <<<"$matches_json")"

case "$ACTION" in
  show)
    if [[ "$matches_count" == "0" ]]; then
      echo 'No matching OpenClaw operator signal cron job found.'
      exit 0
    fi
    printf '%s\n' "$matches_json"
    ;;
  remove)
    if [[ "$matches_count" == "0" ]]; then
      echo 'No matching OpenClaw operator signal cron job found.'
      exit 0
    fi
    while IFS= read -r id; do
      [[ -n "$id" ]] || continue
      remove_job_by_id "$id"
    done < <(jq -r '.[].id' <<<"$matches_json")
    echo "Removed $matches_count cron job(s) named '$JOB_NAME'."
    ;;
  install)
    if [[ "$RECREATE" == "true" && "$matches_count" != "0" ]]; then
      while IFS= read -r id; do
        [[ -n "$id" ]] || continue
        remove_job_by_id "$id"
      done < <(jq -r '.[].id' <<<"$matches_json")
      matches_json='[]'
      matches_count='0'
    fi
    if [[ "$matches_count" -gt 1 ]]; then
      echo "Found multiple matching cron jobs named '$JOB_NAME'. Run with --action remove or --recreate first." >&2
      exit 1
    fi
    if [[ "$matches_count" == "1" ]]; then
      echo 'OpenClaw operator signal cron job already exists:'
      printf '%s\n' "$matches_json"
      exit 0
    fi
    echo 'Installed OpenClaw operator signal cron job:'
    add_job
    ;;
  run-now)
    if [[ "$matches_count" == "0" ]]; then
      echo 'No matching cron job found. Install it first.' >&2
      exit 1
    fi
    if [[ "$matches_count" -gt 1 ]]; then
      echo "Found multiple matching cron jobs named '$JOB_NAME'. Clean them up first." >&2
      exit 1
    fi
    job_id="$(jq -r '.[0].id' <<<"$matches_json")"
    openclaw cron run "$job_id"
    ;;
esac

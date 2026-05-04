#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

PROJECT_ROOT="$CEX_PROJECT_ROOT"
OPENCLAW_SCOPE_ROOT_DEFAULT="$PROJECT_ROOT/run/openclaw-cex"
OPENCLAW_SCOPE_CONFIG_DEFAULT="$OPENCLAW_SCOPE_ROOT_DEFAULT/openclaw.json"
OPENCLAW_SCOPE_AGENT_DIR_DEFAULT="$OPENCLAW_SCOPE_ROOT_DEFAULT/agents/cex/agent"

MODEL="${CEX_PROVIDER_PROBE_MODEL:-}"
PROMPT="${CEX_PROVIDER_PROBE_PROMPT:-Return exactly: CEX_PROVIDER_PROBE_OK}"
EXPECTED_TEXT="${CEX_PROVIDER_PROBE_EXPECTED_TEXT:-CEX_PROVIDER_PROBE_OK}"
TIMEOUT_SECONDS="${CEX_PROVIDER_PROBE_TIMEOUT_SECONDS:-90}"
TRANSPORT="${CEX_PROVIDER_PROBE_TRANSPORT:-local}"
OUTPUT_MODE="pretty"

usage() {
  cat <<'EOF'
Usage: scripts/probe-openclaw-provider.sh --model <provider/model> [--local|--gateway] [--compact]

Runs a repo-local OpenClaw provider smoke probe and returns JSON. This is a live
provider call; it may consume provider quota if the selected model succeeds.

Env:
  CEX_PROVIDER_PROBE_MODEL
  CEX_PROVIDER_PROBE_TRANSPORT=local|gateway
  CEX_PROVIDER_PROBE_PROMPT
  CEX_PROVIDER_PROBE_EXPECTED_TEXT
  CEX_PROVIDER_PROBE_TIMEOUT_SECONDS
  OPENCLAW_STATE_DIR / OPENCLAW_CONFIG_PATH / OPENCLAW_AGENT_DIR
EOF
}

while (($#)); do
  case "$1" in
    --model)
      MODEL="${2:-}"
      shift
      ;;
    --compact)
      OUTPUT_MODE="compact"
      ;;
    --gateway)
      TRANSPORT="gateway"
      ;;
    --local)
      TRANSPORT="local"
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

if [[ -z "$MODEL" ]]; then
  jq -n '{ok:false,status:"not_configured",error:"CEX_PROVIDER_PROBE_MODEL or --model is required"}'
  exit 64
fi
if ! [[ "$TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [[ "$TIMEOUT_SECONDS" -eq 0 ]]; then
  jq -n --arg value "$TIMEOUT_SECONDS" '{ok:false,status:"invalid_timeout",error:("invalid CEX_PROVIDER_PROBE_TIMEOUT_SECONDS: " + $value)}'
  exit 64
fi
if [[ "$TRANSPORT" != "local" && "$TRANSPORT" != "gateway" ]]; then
  jq -n --arg value "$TRANSPORT" '{ok:false,status:"invalid_transport",error:("invalid CEX_PROVIDER_PROBE_TRANSPORT: " + $value)}'
  exit 64
fi

if [[ "$TRANSPORT" == "local" ]]; then
  if [[ -z "${OPENCLAW_STATE_DIR:-}" && -f "$OPENCLAW_SCOPE_CONFIG_DEFAULT" ]]; then
    export OPENCLAW_STATE_DIR="$OPENCLAW_SCOPE_ROOT_DEFAULT"
  fi
  if [[ -z "${OPENCLAW_CONFIG_PATH:-}" && -f "$OPENCLAW_SCOPE_CONFIG_DEFAULT" ]]; then
    export OPENCLAW_CONFIG_PATH="$OPENCLAW_SCOPE_CONFIG_DEFAULT"
  fi
  if [[ -z "${OPENCLAW_AGENT_DIR:-}" && -d "$OPENCLAW_SCOPE_AGENT_DIR_DEFAULT" ]]; then
    export OPENCLAW_AGENT_DIR="$OPENCLAW_SCOPE_AGENT_DIR_DEFAULT"
  fi
elif [[ "${CEX_PROVIDER_PROBE_USE_SCOPED_OPENCLAW:-0}" != "1" ]]; then
  unset OPENCLAW_STATE_DIR OPENCLAW_CONFIG_PATH OPENCLAW_AGENT_DIR
fi

cex_require_cmd openclaw jq timeout

stdout_file="$(mktemp)"
stderr_file="$(mktemp)"
cleanup() {
  rm -f "$stdout_file" "$stderr_file"
}
trap cleanup EXIT

set +e
transport_flag="--local"
if [[ "$TRANSPORT" == "gateway" ]]; then
  transport_flag="--gateway"
fi
timeout --signal=TERM --kill-after=5s "${TIMEOUT_SECONDS}s" \
  openclaw infer model run "$transport_flag" --json --model "$MODEL" --prompt "$PROMPT" \
  >"$stdout_file" 2>"$stderr_file"
exit_code=$?
set -e

render() {
  if [[ "$OUTPUT_MODE" == "compact" ]]; then
    jq -c .
  else
    jq .
  fi
}

if [[ "$exit_code" -eq 124 || "$exit_code" -eq 137 ]]; then
  jq -n \
    --arg model "$MODEL" \
    --arg transport "$TRANSPORT" \
    --argjson timeout "$TIMEOUT_SECONDS" \
    '{ok:false,status:"timeout",model:$model,transport:$transport,timeout_seconds:$timeout,error:("provider probe timed out after " + ($timeout|tostring) + "s")}' | render
  exit 2
fi

if [[ ! -s "$stdout_file" ]]; then
  stderr_excerpt="$(head -c 800 "$stderr_file" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  jq -n \
    --arg model "$MODEL" \
    --arg transport "$TRANSPORT" \
    --argjson exit_code "$exit_code" \
    --arg stderr "$stderr_excerpt" \
    '{ok:false,status:"provider_error",model:$model,transport:$transport,exit_code:$exit_code,error:(if ($stderr|length) > 0 then $stderr else "openclaw provider probe returned no stdout" end)}' | render
  exit 2
fi

if ! jq empty "$stdout_file" >/dev/null 2>&1; then
  stderr_excerpt="$(head -c 800 "$stderr_file" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  stdout_excerpt="$(head -c 800 "$stdout_file" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  jq -n \
    --arg model "$MODEL" \
    --arg transport "$TRANSPORT" \
    --argjson exit_code "$exit_code" \
    --arg stdout "$stdout_excerpt" \
    --arg stderr "$stderr_excerpt" \
    '{ok:false,status:"invalid_json",model:$model,transport:$transport,exit_code:$exit_code,error:"openclaw provider probe did not return valid JSON",stdout_excerpt:$stdout,stderr_excerpt:$stderr}' | render
  exit 2
fi

if [[ "$exit_code" -ne 0 ]]; then
  stderr_excerpt="$(head -c 800 "$stderr_file" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  stdout_excerpt="$(head -c 800 "$stdout_file" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  jq -n \
    --arg model "$MODEL" \
    --arg transport "$TRANSPORT" \
    --argjson exit_code "$exit_code" \
    --arg stdout "$stdout_excerpt" \
    --arg stderr "$stderr_excerpt" \
    '{ok:false,status:"provider_error",model:$model,transport:$transport,exit_code:$exit_code,error:(if ($stderr|length) > 0 then $stderr else "openclaw provider probe exited non-zero" end),stdout_excerpt:$stdout}' | render
  exit 2
fi

probe_json="$(cat "$stdout_file")"
output_text="$(jq -r '[.outputs[]?.text // empty] | join("\n")' <<<"$probe_json")"
stderr_text="$(cat "$stderr_file")"
combined_text="$output_text
$stderr_text"

bad_pattern='⚠️|billing error|insufficient balance|usage limit|quota|RESOURCE_EXHAUSTED|free plan|auth|unauthorized|invalid api key|api key|rate limit|timed out|timeout|unavailable|provider probe timed out|returned .*error|rawError|isError=true'
if grep -Eiq "$bad_pattern" <<<"$combined_text"; then
  surface_error="$(printf '%s' "$output_text" | head -c 800 | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  if [[ -z "$surface_error" ]]; then
    surface_error="$(printf '%s' "$stderr_text" | head -c 800 | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  fi
  jq -n \
    --arg model "$MODEL" \
    --arg transport "$TRANSPORT" \
    --argjson exit_code "$exit_code" \
    --arg error "$surface_error" \
    '{ok:false,status:"provider_error",model:$model,transport:$transport,exit_code:$exit_code,error:$error}' | render
  exit 2
fi

if ! grep -Fq "$EXPECTED_TEXT" <<<"$output_text"; then
  excerpt="$(printf '%s' "$output_text" | head -c 800 | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  jq -n \
    --arg model "$MODEL" \
    --arg transport "$TRANSPORT" \
    --arg expected "$EXPECTED_TEXT" \
    --arg output "$excerpt" \
    '{ok:false,status:"unexpected_output",model:$model,transport:$transport,expected:$expected,error:"provider probe completed but did not return expected sentinel text",output_excerpt:$output}' | render
  exit 2
fi

jq -n \
  --arg model "$MODEL" \
  --arg transport "$TRANSPORT" \
  --arg expected "$EXPECTED_TEXT" \
  '{ok:true,status:"provider_success",model:$model,transport:$transport,expected:$expected}' | render

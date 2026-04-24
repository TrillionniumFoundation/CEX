#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}" )" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=/dev/null
source "$SCRIPT_DIR/operator-signal-cron-cli-surfaces.sh"

COMPACT="false"
FIELD=""
SCHEMA="false"
HELP_JSON="false"

usage() {
  osc_operator_signal_print_catalog_reader_usage
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --compact)
      COMPACT="true"
      shift
      ;;
    --field)
      [[ $# -ge 2 ]] || { echo "Error: --field requires a value" >&2; exit 2; }
      FIELD="$2"
      shift 2
      ;;
    --help-json)
      HELP_JSON="true"
      shift
      ;;
    --schema)
      SCHEMA="true"
      shift
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

if [[ "$SCHEMA" == "true" && -n "$FIELD" ]]; then
  echo "Error: --schema cannot be combined with --field" >&2
  exit 2
fi

if [[ "$HELP_JSON" == "true" && -n "$FIELD" ]]; then
  echo "Error: --help-json cannot be combined with --field" >&2
  exit 2
fi

if [[ "$HELP_JSON" == "true" && "$SCHEMA" == "true" ]]; then
  echo "Error: --help-json cannot be combined with --schema" >&2
  exit 2
fi

if [[ "$HELP_JSON" == "true" ]]; then
  help_json="$(osc_operator_signal_print_catalog_reader_help_json)"
  if [[ "$COMPACT" == "true" ]]; then
    jq -c . <<<"$help_json"
  else
    jq . <<<"$help_json"
  fi
  exit 0
fi

if [[ "$SCHEMA" == "true" ]]; then
  schema_json="$(osc_operator_signal_print_catalog_reader_schema_json)"
  if [[ "$COMPACT" == "true" ]]; then
    jq -c . <<<"$schema_json"
  else
    jq . <<<"$schema_json"
  fi
  exit 0
fi

catalog_json="$(osc_operator_signal_print_catalog_json "$REPO_ROOT")"

if [[ -n "$FIELD" ]]; then
  jq -c ".${FIELD}" <<<"$catalog_json"
  exit 0
fi

if [[ "$COMPACT" == "true" ]]; then
  jq -c . <<<"$catalog_json"
else
  jq . <<<"$catalog_json"
fi

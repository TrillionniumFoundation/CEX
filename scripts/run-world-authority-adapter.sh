#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

profile="${WORLD_AUTHORITY_ADAPTER_RUNTIME_PROFILE:-${CONSUMER_ENTRY_RUNTIME_PROFILE:-${CEX_RUNTIME_PROFILE:-${APP_ENV:-local_dev}}}}"
profile="$(printf '%s' "$profile" | tr '[:upper:]' '[:lower:]')"

case "$profile" in
  beta|staging|stage|production|prod)
    : "${CEX_WORLD_AUTHORITY_MODE:?CEX_WORLD_AUTHORITY_MODE=remote is required}"
    : "${TRILLIONNIUM_WORLD_BASE_URL:?TRILLIONNIUM_WORLD_BASE_URL is required}"
    : "${TRILLIONNIUM_WORLD_API_CONTRACT:?TRILLIONNIUM_WORLD_API_CONTRACT is required}"
    : "${TRILLIONNIUM_WORLD_AUTH_TOKEN:?TRILLIONNIUM_WORLD_AUTH_TOKEN is required}"
    [[ "$CEX_WORLD_AUTHORITY_MODE" == "remote" ]] || {
      echo "CEX_WORLD_AUTHORITY_MODE must be remote" >&2
      exit 78
    }
    [[ "$TRILLIONNIUM_WORLD_API_CONTRACT" == "trillionnium_world_api_v1" ]] || {
      echo "TRILLIONNIUM_WORLD_API_CONTRACT must be trillionnium_world_api_v1" >&2
      exit 78
    }
    case "${TRILLIONNIUM_WORLD_BASE_URL,,}" in
      *localhost*|*127.0.0.1*|*0.0.0.0*|*change-me*|*replace_me*|*example.com*)
        echo "production-like World authority URL cannot be loopback or placeholder" >&2
        exit 78
        ;;
    esac
    case "${TRILLIONNIUM_WORLD_AUTH_TOKEN,,}" in
      *change-me*|*replace_me*|*password*|*example*|secret)
        echo "production-like World authority token is placeholder-like" >&2
        exit 78
        ;;
    esac
    if (( ${#TRILLIONNIUM_WORLD_AUTH_TOKEN} < 24 )); then
      echo "production-like World authority token must be at least 24 characters" >&2
      exit 78
    fi
    ;;
esac

exec cargo run --locked --release -p consumer-entry-api --bin world-authority-adapter

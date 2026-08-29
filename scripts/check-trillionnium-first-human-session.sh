#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$ROOT_DIR/scripts/_dev-helpers.sh"
cex_load_env
SCRIPT_PATH="$ROOT_DIR/scripts/playwright/trillionnium-browser-e2e.mjs"
BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
CHROME_BIN="${PLAYWRIGHT_CHROMIUM_EXECUTABLE:-${CHROME_BIN:-/usr/bin/google-chrome-stable}}"

if [[ ! -x "$CHROME_BIN" ]]; then
  echo "Chrome executable not found or not executable: $CHROME_BIN" >&2
  exit 64
fi

export CEX_PROJECT_ROOT="$ROOT_DIR"
export CONSUMER_ENTRY_BASE_URL="$BASE_URL"
export PLAYWRIGHT_CHROMIUM_EXECUTABLE="$CHROME_BIN"
export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
export TRILLIONNIUM_BROWSER_E2E_OUT_DIR="${TRILLIONNIUM_FIRST_HUMAN_SESSION_OUT_DIR:-$ROOT_DIR/run/first-human-session}"
export TRILLIONNIUM_BROWSER_E2E_SCRIPT="$SCRIPT_PATH"
export TRILLIONNIUM_BROWSER_E2E_MODE="first-human-session"

# The first-human path is intentionally mutating: it submits a real tactics
# command and then opens the reward/next-route action loop. Ensure the
# normalized Trillionnium tactics tables exist before the browser run, because
# runtime-manager restart does not apply migrations.
cex_wait_postgres 60 1 >/dev/null
cex_apply_migrations >/dev/null 2>&1

npm exec --yes --package=playwright@1.49.1 -- bash -lc '
  set -euo pipefail
  export NODE_PATH="$(dirname "$(dirname "$(command -v playwright)")")"
  node "$TRILLIONNIUM_BROWSER_E2E_SCRIPT"
'

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$ROOT_DIR/scripts/_dev-helpers.sh"
cex_load_env
SCRIPT_PATH="$ROOT_DIR/scripts/playwright/trillionnium-ui-audit.mjs"
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
export TRILLIONNIUM_UI_AUDIT_OUT_DIR="${TRILLIONNIUM_UI_AUDIT_OUT_DIR:-$ROOT_DIR/run/trillionnium-ui-audit}"
export TRILLIONNIUM_UI_AUDIT_SCRIPT="$SCRIPT_PATH"

npm exec --yes --package=playwright@1.55.1 -- bash -lc '
  set -euo pipefail
  export NODE_PATH="$(dirname "$(dirname "$(command -v playwright)")")"
  node "$TRILLIONNIUM_UI_AUDIT_SCRIPT"
'

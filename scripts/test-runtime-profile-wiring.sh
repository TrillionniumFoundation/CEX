#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$ROOT_DIR/scripts/_dev-helpers.sh"

assert_profile() {
  local requested="$1" app_env="$2" expected_profile="$3" expected_lane="$4" expected_app_env="$5"
  (
    export APP_ENV="$app_env"
    export CEX_RUNTIME_PROFILE="$requested"
    export CEX_RUNTIME_SERVICE_PROFILE=""
    export CEX_RUNTIME_LANE=""
    cex_select_runtime_profile
    [[ "$CEX_RUNTIME_PROFILE" == "$expected_profile" ]] || {
      echo "profile mismatch for requested=$requested: $CEX_RUNTIME_PROFILE" >&2
      exit 1
    }
    [[ "$CEX_RUNTIME_LANE" == "$expected_lane" ]] || {
      echo "lane mismatch for requested=$requested: $CEX_RUNTIME_LANE" >&2
      exit 1
    }
    [[ "$APP_ENV" == "$expected_app_env" ]] || {
      echo "APP_ENV mismatch for requested=$requested: $APP_ENV" >&2
      exit 1
    }
  )
}

assert_profile "trnm-economy" "dev" "trnm-economy" "trnm-economy" "production"
assert_profile "trnm_economy" "dev" "trnm-economy" "trnm-economy" "production"
assert_profile "full" "dev" "dev" "full" "dev"
assert_profile "full" "production" "production" "full" "production"

echo "runtime profile wiring: PASS"

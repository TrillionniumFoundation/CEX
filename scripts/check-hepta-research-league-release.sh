#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

: "${HEPTA_TEST_DATABASE_URL:?HEPTA_TEST_DATABASE_URL is required for the live PostgreSQL release gate}"

python3 - <<'PY'
import yaml

with open("docs/openapi/hepta-research-league-v1.yaml", encoding="utf-8") as stream:
    yaml.safe_load(stream)
PY
jq -e \
  '.fixture_version == "hepta_sdk_fixtures_v1"
   and .agent_execution_mode == "external_only"
   and .top_level_modules == ["hepta","nakama","trnm"]
   and .invariants.nakama_match_authorization_is_ed25519_signed == true
   and .invariants.nakama_match_authorization_bearer_token == false' \
  docs/sdk-fixtures/hepta-research-league-v1.json >/dev/null

if rg -n \
  'model[_ -]?api[_ -]?key hosting|platform-owned agent|hosted agent loop|agent_execution_mode["=: ]+internal' \
  services/hepta-research-league docs/openapi/hepta-research-league-v1.yaml \
  docs/sdk-fixtures/hepta-research-league-v1.json; then
  echo "forbidden platform-hosted Agent semantics found" >&2
  exit 1
fi

test "$(rg -o '\"hepta\"|\"nakama\"|\"trnm\"' \
  docs/sdk-fixtures/hepta-research-league-v1.json | sort -u | wc -l)" -eq 3

cargo fmt --all -- --check
cargo test --locked -p hepta-research-league
cargo check --locked --workspace

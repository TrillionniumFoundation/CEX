#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

: "${HEPTA_TEST_DATABASE_URL:?HEPTA_TEST_DATABASE_URL is required for the live PostgreSQL release gate}"
cargo_gate="${HEPTA_CARGO_LOCK_FILE:-/tmp/trnm-paper-raid-cargo-gate.lock}"

cargo_locked() {
  flock -n "$cargo_gate" cargo "$@"
}

python3 - <<'PY'
import yaml
import json

for path in (
    "docs/openapi/hepta-research-league-v1.yaml",
    "docs/openapi/hepta-paper-raid-v2.yaml",
):
    with open(path, encoding="utf-8") as stream:
        yaml.safe_load(stream)

with open("docs/openapi/vendor/integration-artifact-bundle-v1.schema.json", encoding="utf-8") as stream:
    json.load(stream)
with open("docs/sdk-fixtures/integration-paper-raid-artifact-bundle-v1.json", encoding="utf-8") as stream:
    bundle = json.load(stream)
    assert bundle["schema"] == "paper-raid.artifact-bundle.v1"
PY

test "$(sha256sum docs/openapi/vendor/integration-artifact-bundle-v1.schema.json | cut -d' ' -f1)" = \
  "aba8fd6d1059c59f63cdb258a2e507de1bed3ff74f935b7dd0214e4640ad9bb6"
test "$(sha256sum docs/sdk-fixtures/integration-paper-raid-artifact-bundle-v1.json | cut -d' ' -f1)" = \
  "9c2234c2c677307b262faa6be52a9855958003212fa0733e3c991af86df1555d"
test "$(sha256sum docs/sdk-fixtures/hepta-paper-collaboration-v3.json | cut -d' ' -f1)" = \
  "6a8c20dabaf2ff723a1db7e9742bcbd24f4d18bb17938f3695cac099c29d84ce"
test "$(sha256sum docs/sdk-fixtures/hepta-paper-review-v4.json | cut -d' ' -f1)" = \
  "b25dcbfcf3f9d5830ab8d2b32bdd36b2c073bca8a0bff1da6ba05fb85f6f17b4"
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

node scripts/verify-hepta-paper-raid-v2-fixture.mjs \
  docs/sdk-fixtures/hepta-paper-raid-v2.json
node scripts/verify-hepta-paper-collaboration-v3-fixture.mjs \
  docs/sdk-fixtures/hepta-paper-collaboration-v3.json
node scripts/verify-hepta-paper-review-v4-fixture.mjs \
  docs/sdk-fixtures/hepta-paper-review-v4.json

cargo_locked fmt --all -- --check
cargo_locked test --locked -p hepta-research-league
cargo_locked check --locked --workspace
cargo_locked clippy --locked --workspace --all-targets -- -D warnings

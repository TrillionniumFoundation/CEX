#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MAIN="services/consumer-entry-api/src/main.rs"
ADAPTER="services/consumer-entry-api/src/bin/world-authority-adapter.rs"
READINESS="services/consumer-entry-api/src/trillionnium_world_adapters.rs"
EVIDENCE="docs/traceability/world-authority-cutover-v1.json"
ENV_EXAMPLE="config/world-authority-adapter.env.example"
DOC="docs/world-authority-cutover-v1.md"
RUNNER="scripts/run-world-authority-adapter.sh"

python3 - "$MAIN" "$ADAPTER" "$READINESS" "$EVIDENCE" "$ENV_EXAMPLE" "$DOC" "$RUNNER" <<'PY'
import json
import pathlib
import re
import sys

main_path, adapter_path, readiness_path, evidence_path, env_path, doc_path, runner_path = map(pathlib.Path, sys.argv[1:])
for path in (main_path, adapter_path, readiness_path, evidence_path, env_path, doc_path, runner_path):
    if not path.is_file():
        raise SystemExit(f"missing World authority cutover artifact: {path}")

main = main_path.read_text()
adapter = adapter_path.read_text()
readiness = readiness_path.read_text()
doc = doc_path.read_text()
runner = runner_path.read_text()

required_main = [
    "CEX_WORLD_AUTHORITY_MODE",
    "TRILLIONNIUM_WORLD_BASE_URL",
    "TRILLIONNIUM_WORLD_API_CONTRACT",
    "trillionnium_world_api_v1",
    "local_world_writer_quarantined",
    "remote_world_authority_required",
    "world_authority_write_fence",
    "std::process::exit(78)",
    "TrillionniumFoundation/Trillionnium-World",
]
for marker in required_main:
    if marker not in main:
        raise SystemExit(f"consumer-entry production World write fence missing marker: {marker}")

for marker in (
    "/action",
    "/move",
    "/tactics",
    "/company",
    "/shop",
    "/listing",
    "/buy",
    "/work-deliver",
    "/work-accept",
    "/work-reject",
    "/work-reopen",
    "/work-cancel",
):
    if marker not in main:
        raise SystemExit(f"authoritative World route family is not fenced: {marker}")

required_adapter = [
    "cex_trillionnium_world_authority_adapter_v1",
    "trillionnium_world_api_v1",
    "trillionnium_world_authority_cutover_v1",
    "redirect(reqwest::redirect::Policy::none())",
    "MAX_PROXY_BODY_BYTES",
    "TRILLIONNIUM_WORLD_AUTH_TOKEN",
    "idempotency-key",
    "local_fallback_used",
    "world_authority_contract_mismatch",
    "world_authority_unavailable",
]
for marker in required_adapter:
    if marker not in adapter:
        raise SystemExit(f"World authority edge adapter missing marker: {marker}")

for forbidden in (
    "LeagueState",
    "WorldState",
    "sqlx::",
    "std::fs",
    "FileWorldRepository",
    "build_router(state.clone())",
):
    if forbidden in adapter:
        raise SystemExit(f"remote-only adapter contains local authority dependency: {forbidden}")

required_readiness = [
    "production_authorization\": \"not_granted",
    "production_status\": \"quarantined_by_consumer_entry_router_fence",
    "world_pull_request\": 60",
    "production_adapter_trait_ready\": false",
]
for marker in required_readiness:
    if marker not in readiness:
        raise SystemExit(f"World readiness truth boundary missing marker: {marker}")

if "production_authorization: `not_granted`" not in doc:
    raise SystemExit("World cutover document must preserve production not_granted truth")
if "never falls back to CEX-local World state" not in doc:
    raise SystemExit("World cutover document must state the no-local-fallback rule")
if "cargo run --locked --release -p consumer-entry-api --bin world-authority-adapter" not in runner:
    raise SystemExit("World adapter runner does not launch the bounded binary")

pairs = {}
for raw in env_path.read_text().splitlines():
    line = raw.strip()
    if not line or line.startswith("#") or "=" not in line:
        continue
    key, value = line.split("=", 1)
    pairs[key.strip()] = value.strip()
expected_env = {
    "WORLD_AUTHORITY_ADAPTER_RUNTIME_PROFILE": "production",
    "CEX_WORLD_AUTHORITY_MODE": "remote",
    "TRILLIONNIUM_WORLD_API_CONTRACT": "trillionnium_world_api_v1",
    "TRILLIONNIUM_WORLD_CUTOVER_CONTRACT": "trillionnium_world_authority_cutover_v1",
    "TRILLIONNIUM_WORLD_PRODUCTION_AUTHORIZATION": "not_granted",
}
for key, expected in expected_env.items():
    if pairs.get(key) != expected:
        raise SystemExit(f"World adapter env contract drift: {key}={pairs.get(key)!r}, expected {expected!r}")
if not pairs.get("TRILLIONNIUM_WORLD_BASE_URL", "").startswith("https://"):
    raise SystemExit("production example must use HTTPS World authority URL")
if "change-me" not in pairs.get("TRILLIONNIUM_WORLD_AUTH_TOKEN", ""):
    raise SystemExit("checked-in production example must remain intentionally non-runnable")

evidence = json.loads(evidence_path.read_text())
if evidence.get("schema") != "cex.world.authority.cutover.evidence.v1":
    raise SystemExit("World cutover evidence schema mismatch")
if evidence.get("production_authorization") != "not_granted":
    raise SystemExit("World cutover source evidence cannot grant production authorization")
if evidence.get("authority_owner") != "TrillionniumFoundation/Trillionnium-World":
    raise SystemExit("World authority owner repository drift")
world_source = evidence.get("world_source", {})
if world_source.get("pull_request") != 60:
    raise SystemExit("World source PR is not pinned to #60")
if world_source.get("branch") != "feat/p0-world-authority-cutover-20260906":
    raise SystemExit("World source branch drift")
head_sha = world_source.get("head_sha")
if not isinstance(head_sha, str) or re.fullmatch(r"[0-9a-f]{40}", head_sha) is None:
    raise SystemExit("World source head_sha is not an exact commit SHA")
if world_source.get("historical_source_parent") != "d44d8930c917b55da7b23eb19e9645feb8f4ee59":
    raise SystemExit("World historical source parent drift")
if world_source.get("restored_server_crate_count") != 7:
    raise SystemExit("World restored server crate count drift")
if world_source.get("restored_historical_file_count") != 15:
    raise SystemExit("World restored historical file count drift")
if world_source.get("provenance_manifest") != "docs/contracts/trillionnium-world-authority-provenance-v1.json":
    raise SystemExit("World provenance manifest path drift")
if evidence.get("cex_candidate", {}).get("local_world_writer_production_status") != "quarantined":
    raise SystemExit("CEX production World writer quarantine is not recorded")
controls = evidence.get("implemented_controls", {})
required_true_controls = (
    "production_startup_requires_remote_mode",
    "production_startup_requires_non_loopback_world_url",
    "production_startup_requires_exact_api_contract",
    "production_adapter_requires_strong_service_token",
    "production_local_world_mutations_return_503",
    "cross_repository_exact_sha_checkout_gate_defined",
    "development_restart_and_rollback_smoke_defined",
    "historical_tree_and_blob_provenance_bound",
    "provenance_hostile_fixtures_defined",
    "world_lockfile_committed_and_immutable_during_gate",
    "github_actions_pinned_to_commit_sha",
)
for key in required_true_controls:
    if controls.get(key) is not True:
        raise SystemExit(f"World cutover control is not true: {key}")
if controls.get("adapter_has_local_world_state") is not False:
    raise SystemExit("World adapter must not carry local World state")
if controls.get("adapter_failure_local_fallback") is not False:
    raise SystemExit("World adapter must not use a local fallback")
if controls.get("rust_toolchain_pinned") != "1.98.1":
    raise SystemExit("World cutover Rust toolchain pin drift")
blocking = [row for row in evidence.get("evidence_matrix", []) if row.get("blocking")]
if not blocking:
    raise SystemExit("evidence matrix must retain unresolved production blockers")
print(f"World authority static contracts and truth boundaries: ok ({head_sha})")
PY

cargo fmt --all -- --check
cargo test --locked -p consumer-entry-api \
  --bin consumer-entry-api \
  --bin world-authority-adapter
cargo clippy --locked -p consumer-entry-api \
  --bin consumer-entry-api \
  --bin world-authority-adapter \
  -- -D warnings

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

set +e
CONSUMER_ENTRY_RUNTIME_PROFILE=production \
CEX_RUNTIME_PROFILE= \
APP_ENV= \
CEX_WORLD_AUTHORITY_MODE=embedded \
cargo run --quiet --locked -p consumer-entry-api --bin consumer-entry-api \
  >"$TMP/consumer-entry-negative.log" 2>&1
consumer_entry_status=$?

WORLD_AUTHORITY_ADAPTER_RUNTIME_PROFILE=production \
TRILLIONNIUM_WORLD_BASE_URL=http://127.0.0.1:8787 \
TRILLIONNIUM_WORLD_API_CONTRACT=trillionnium_world_api_v1 \
TRILLIONNIUM_WORLD_AUTH_TOKEN=Qm9uZGVkLXdvcmxkLWF1dGhvcml0eS10b2tlbg \
cargo run --quiet --locked -p consumer-entry-api --bin world-authority-adapter \
  >"$TMP/adapter-negative.log" 2>&1
adapter_status=$?
set -e

if [[ "$consumer_entry_status" -ne 78 ]]; then
  cat "$TMP/consumer-entry-negative.log" >&2
  echo "consumer-entry production local-writer guard returned $consumer_entry_status, expected 78" >&2
  exit 1
fi
if [[ "$adapter_status" -ne 78 ]]; then
  cat "$TMP/adapter-negative.log" >&2
  echo "World adapter production loopback guard returned $adapter_status, expected 78" >&2
  exit 1
fi

echo "World authority negative startup guards: ok"

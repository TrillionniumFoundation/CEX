#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

cargo_gate="${HEPTA_CARGO_LOCK_FILE:-/tmp/trnm-paper-raid-cargo-gate.lock}"
cargo_locked() {
  flock -n "$cargo_gate" cargo "$@"
}

bash scripts/project-preflight.sh --dev
bash scripts/check-hepta-research-league-release-structure.sh
python3 scripts/check-hepta-route-openapi-parity.py
bash services/paper-raid-bff/scripts/check-boundaries.sh
cargo_locked fmt --all -- --check
cargo_locked test --locked -p hepta-research-league
cargo_locked test --locked -p paper-raid-bff
cargo_locked check --locked -p hepta-research-league -p paper-raid-bff
cargo_locked clippy --locked -p hepta-research-league -p paper-raid-bff --all-targets -- -D warnings

echo "Paper Raid alpha candidate source gate: PASS"

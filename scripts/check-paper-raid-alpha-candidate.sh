#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

cargo_gate="${HEPTA_CARGO_LOCK_FILE:-/tmp/trnm-paper-raid-cargo-gate.lock}"
cargo_locked() {
  flock -n "$cargo_gate" cargo "$@"
}

bash scripts/project-preflight.sh --dev
bash scripts/check-paper-raid-player-path.sh
bash scripts/check-hepta-research-league-release-structure.sh
python3 scripts/check-hepta-route-openapi-parity.py
bash services/paper-raid-bff/scripts/check-boundaries.sh
bash services/paper-raid-bff/scripts/check-agent-bridge-boundary.sh
bash services/paper-raid-bff/scripts/check-invite-alpha-boundary.sh
bash services/paper-raid-bff/scripts/check-observability-boundary.sh
npm --prefix tools/paper-raid-agent-bridge test
cargo_locked fmt --all -- --check
cargo_locked check --locked -p hepta-research-league -p paper-raid-bff
cargo_locked clippy --locked -p hepta-research-league -p paper-raid-bff --all-targets -- -D warnings
cargo_locked test --locked -p hepta-research-league
cargo_locked test --locked -p paper-raid-bff

if [[ ${PAPER_RAID_ALPHA_REQUIRE_POSTGRES:-1} == 1 ]]; then
  bash scripts/check-paper-raid-alpha-candidate-postgres.sh
elif [[ ${PAPER_RAID_ALPHA_REQUIRE_POSTGRES:-1} != 0 ]]; then
  echo "ERROR: PAPER_RAID_ALPHA_REQUIRE_POSTGRES must be 0 or 1" >&2
  exit 64
fi

echo "Paper Raid alpha candidate source gate: PASS"

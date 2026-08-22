#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

# This is a source/fixture gate only.  It deliberately does not start a BFF,
# mutate a database, contact Hepta/Nakama/CAS/Chain, or claim live playability.
bash scripts/project-preflight.sh --dev

summary_file="${PAPER_RAID_PLAYER_PATH_SUMMARY_FILE:-$root/run/paper-raid-player-path/last.json}"
args=(--repo "$root" --summary-file "$summary_file")
if [[ "${PAPER_RAID_PLAYER_PATH_SKIP_NODE:-0}" == "1" ]]; then
  args+=(--skip-node)
elif [[ "${PAPER_RAID_PLAYER_PATH_SKIP_NODE:-0}" != "0" ]]; then
  echo "PAPER_RAID_PLAYER_PATH_SKIP_NODE must be 0 or 1" >&2
  exit 64
fi

python3 scripts/check-paper-raid-player-path.py "${args[@]}"
echo "Paper Raid ordinary seven-identity player-path source gate: PASS (live phases remain not_run)"

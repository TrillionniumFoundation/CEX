#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is required}"
repo="${GITHUB_REPOSITORY:-TrillionniumFoundation/CEX}"
main_before="$(gh api "repos/$repo/git/ref/heads/main" --jq .object.sha)"
probe="ruleset-negative-probe-${GITHUB_RUN_ID:-manual}-$$"
cleanup() {
  gh api -X DELETE "repos/$repo/git/refs/heads/$probe" >/dev/null 2>&1 || true
}
trap cleanup EXIT

gh api -X POST "repos/$repo/git/refs" -f ref="refs/heads/$probe" -f sha="$main_before" >/dev/null
probe_sha="$(gh api "repos/$repo/git/ref/heads/$probe" --jq .object.sha)"

set +e
direct_output="$(gh api -X PATCH "repos/$repo/git/refs/heads/main" -f sha="$probe_sha" -F force=false 2>&1)"
direct_status=$?
force_output="$(gh api -X PATCH "repos/$repo/git/refs/heads/main" -f sha="$probe_sha" -F force=true 2>&1)"
force_status=$?
delete_output="$(gh api -X DELETE "repos/$repo/git/refs/heads/main" 2>&1)"
delete_status=$?
set -e

main_after="$(gh api "repos/$repo/git/ref/heads/main" --jq .object.sha)"
python3 - "$main_before" "$main_after" "$direct_status" "$force_status" "$delete_status" \
  "$direct_output" "$force_output" "$delete_output" <<'PY'
import json, sys
before, after, direct, force, delete, direct_out, force_out, delete_out = sys.argv[1:]
value = {
    "schema": "cex.main-ruleset-negative-probes.v1",
    "main_before": before,
    "main_after": after,
    "direct_update_rejected": int(direct) != 0,
    "non_fast_forward_update_rejected": int(force) != 0,
    "deletion_rejected": int(delete) != 0,
    "server_outputs": {
        "direct_update": direct_out,
        "non_fast_forward_update": force_out,
        "deletion": delete_out,
    },
    "production_authorization": "not_granted",
}
print(json.dumps(value, indent=2, sort_keys=True))
if before != after or not all(
    value[key]
    for key in (
        "direct_update_rejected",
        "non_fast_forward_update_rejected",
        "deletion_rejected",
    )
):
    raise SystemExit(1)
PY

#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is required}"
repo="${GITHUB_REPOSITORY:-TrillionniumFoundation/CEX}"
main_before="$(gh api "repos/$repo/git/ref/heads/main" --jq .object.sha)"
main_tree="$(gh api "repos/$repo/git/commits/$main_before" --jq .tree.sha)"
probe="ruleset-negative-probe-${GITHUB_RUN_ID:-manual}-$$"
cleanup() {
  gh api -X DELETE "repos/$repo/git/refs/heads/$probe" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# Create a distinct commit with the same tree. Updating main to this commit is a
# real fast-forward mutation while leaving repository content unchanged.
probe_commit="$(
  gh api -X POST "repos/$repo/git/commits" \
    -f message='CEX ruleset negative probe; never merge' \
    -f tree="$main_tree" \
    -f "parents[]=$main_before" \
    --jq .sha
)"
[[ "$probe_commit" =~ ^[0-9a-f]{40}$ ]]
gh api -X POST "repos/$repo/git/refs" \
  -f ref="refs/heads/$probe" \
  -f sha="$probe_commit" >/dev/null

set +e
direct_output="$(
  gh api -X PATCH "repos/$repo/git/refs/heads/main" \
    -f sha="$probe_commit" -F force=false 2>&1
)"
direct_status=$?
force_output="$(
  gh api -X PATCH "repos/$repo/git/refs/heads/main" \
    -f sha="$probe_commit" -F force=true 2>&1
)"
force_status=$?
delete_output="$(gh api -X DELETE "repos/$repo/git/refs/heads/main" 2>&1)"
delete_status=$?
set -e

main_after="$(gh api "repos/$repo/git/ref/heads/main" --jq .object.sha)"
python3 - "$main_before" "$main_after" "$probe_commit" \
  "$direct_status" "$force_status" "$delete_status" \
  "$direct_output" "$force_output" "$delete_output" <<'PY'
import json
import sys

(
    before,
    after,
    probe_commit,
    direct,
    force,
    delete,
    direct_out,
    force_out,
    delete_out,
) = sys.argv[1:]
value = {
    "schema": "cex.main-ruleset-negative-probes.v2",
    "main_before": before,
    "main_after": after,
    "distinct_probe_commit": probe_commit,
    "direct_fast_forward_update_rejected": int(direct) != 0,
    "force_update_rejected": int(force) != 0,
    "deletion_rejected": int(delete) != 0,
    "server_outputs": {
        "direct_fast_forward_update": direct_out,
        "force_update": force_out,
        "deletion": delete_out,
    },
    "production_authorization": "not_granted",
}
print(json.dumps(value, indent=2, sort_keys=True))
if before != after or not all(
    value[key]
    for key in (
        "direct_fast_forward_update_rejected",
        "force_update_rejected",
        "deletion_rejected",
    )
):
    raise SystemExit(1)
PY

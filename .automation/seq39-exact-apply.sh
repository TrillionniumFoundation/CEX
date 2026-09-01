#!/usr/bin/env bash
set -euo pipefail

BASE_SHA='e4a8bb9f3a2c7425d16e0b13a41b0d73517f1ffb'
TARGET_BRANCH='fix/hepta-v12-gap-closure-20260829'
STAGING_BRANCH='automation/seq39-validated'
PATCH_SHA256='710158c63183d90b9dc80f9e93c520c1aa28ddbadabfbe930abf19d5300d432f'
PATCH_GZIP_SHA256='7d133cebc8ff8fcc3c7833fce48a1ebf497ef469d42676cee4064e70018c462a'
PART_SHAS=(
  ea2432da83881c5bbaabd55f8ebf1ff55e42ed14
  8b64564e580876a17ca72ad2bb4e4fbea81b8c44
  83ab4aac4bbf1782c158a6be2eeb67db2d4b612d
  12922fae8d7d443a9f9b2a05fc8d04cfab3ede75
  4ad14f6562275ba63e5b2d3d6be02edb9fc38e6b
  dedf6655c47e5cbda01ef289201b50a2b32c31b8
  b0b4f55649592b0e57103f0bb2841896eccc7f8f
  0499e3aa95546579600145414b55d9fc079bf4ac
)

fail() {
  echo "::error::$*" >&2
  exit 1
}

check_eq() {
  local observed=$1 expected=$2 label=$3
  [[ "$observed" == "$expected" ]] \
    || fail "$label: expected=$expected observed=${observed:-<empty>}"
}

phase() {
  echo
  echo "===== $* ====="
}

phase "preflight immutable refs"
check_eq "${GITHUB_REF_TYPE:-}" branch GITHUB_REF_TYPE
check_eq "${GITHUB_REF_NAME:-}" automation/seq39-exact-apply GITHUB_REF_NAME
remote_target=$(git ls-remote --heads origin "refs/heads/$TARGET_BRANCH" | awk '{print $1}')
remote_staging=$(git ls-remote --heads origin "refs/heads/$STAGING_BRANCH" | awk '{print $1}')
check_eq "$remote_target" "$BASE_SHA" target_branch_sha
check_eq "$remote_staging" "$BASE_SHA" staging_branch_sha
git cat-file -e "$BASE_SHA^{commit}" || fail "base commit is unavailable"

phase "reassemble and verify exact patch"
parts=()
for index in {0..7}; do
  part=$(printf '.automation/seq39.patch.b64.part%02d' "$index")
  [[ -f "$part" ]] || fail "missing patch part: $part"
  check_eq "$(git hash-object "$part")" "${PART_SHAS[$index]}" "patch_part_$index"
  parts+=("$part")
done
cat "${parts[@]}" > "$RUNNER_TEMP/seq39.patch.gz.b64"
base64 --decode "$RUNNER_TEMP/seq39.patch.gz.b64" > "$RUNNER_TEMP/seq39.patch.gz" \
  || fail "base64 patch reconstruction failed"
check_eq \
  "$(sha256sum "$RUNNER_TEMP/seq39.patch.gz" | awk '{print $1}')" \
  "$PATCH_GZIP_SHA256" \
  patch_gzip_sha256
gzip -dc "$RUNNER_TEMP/seq39.patch.gz" > "$RUNNER_TEMP/seq39.patch" \
  || fail "gzip patch decompression failed"
check_eq \
  "$(sha256sum "$RUNNER_TEMP/seq39.patch" | awk '{print $1}')" \
  "$PATCH_SHA256" \
  patch_sha256

python3 - "$RUNNER_TEMP/seq39.patch" <<'PY'
from pathlib import Path
import re
import sys

expected = {
    '.github/workflows/p0-provider-reconciliation-gate.yml',
    '.github/workflows/p0-release-candidate-gate.yml',
    'docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md',
    'docs/clean-deployment-acceptance-v1.md',
    'docs/development-doc-authority-v1.json',
    'docs/release-evidence/p0-candidate-trigger.json',
    'docs/schemas/cex-release-baseline-manifest-v1.schema.json',
    'docs/templates/cex-release-baseline-manifest-v1.json',
    'docs/traceability/v12-requirements-v1.json',
    'migrations/0088_enforce_provider_terminal_evidence_binding.sql',
    'migrations/readme.md',
    'scripts/check-development-docs.py',
    'scripts/check-hosted-gate-execution-impl.py',
    'scripts/check-p0-release-candidate-hygiene-core.py',
    'scripts/check-p0-wiring.py',
    'scripts/check-provider-reconciliation-postgres.sh',
    'scripts/check-provider-success-evidence.py',
    'scripts/check-release-baseline-manifest-contract-core.py',
    'scripts/check-release-baseline-manifest-core.py',
    'scripts/check-release-baseline-manifest.py',
    'scripts/check-release-evidence-contract.py',
    'scripts/check-strict-release-evidence-wiring.py',
    'scripts/verify-hosted-run-execution.py',
    'services/execution-service/src/provider_dispatch.rs',
}
text = Path(sys.argv[1]).read_text(encoding='utf-8')
observed = {
    right
    for left, right in re.findall(
        r'^diff --git a/(.+?) b/(.+?)$', text, flags=re.MULTILINE
    )
    if left == right
}
if observed != expected:
    raise SystemExit(
        f'sequence39 patch path set drifted: missing={sorted(expected-observed)!r} '
        f'extra={sorted(observed-expected)!r}'
    )
PY

phase "reset to frozen sequence 38 tree"
git read-tree --reset -u "$BASE_SHA"

phase "prove and normalize checkout-only CRLF drift"
python3 - "$BASE_SHA" <<'PY'
from pathlib import Path
import subprocess
import sys

base = sys.argv[1]
expected = {
    'docs/invocation-execution-ledger-flow-v1.md',
    'docs/progress-log-2026-04-07.md',
    'docs/progress-log-2026-04-08.md',
    'migrations/0002_add_account_summary_columns.sql',
    'ops/autopilot/README.md',
    'scripts/apply-migrations.ps1',
    'scripts/legacy/README.md',
    'scripts/legacy/_dev-helpers.ps1',
    'scripts/legacy/start-local-runtime-detached.ps1',
    'scripts/service-host.ps1',
    'scripts/start-rust-service.ps1',
}
raw_status = subprocess.check_output(
    ['git', 'status', '--porcelain=v1', '-z', '--untracked-files=all']
)
records = [record for record in raw_status.split(b'\0') if record]
observed = set()
for record in records:
    if len(record) < 4 or record[:3] != b' M ':
        raise SystemExit(f'unexpected pre-normalization status entry: {record!r}')
    observed.add(record[3:].decode('utf-8'))
if observed != expected:
    raise SystemExit(
        f'checkout normalization set drifted: missing={sorted(expected-observed)!r} '
        f'extra={sorted(observed-expected)!r}'
    )

for relative in sorted(expected):
    blob = subprocess.check_output(['git', 'show', f'{base}:{relative}'])
    normalized = blob.replace(b'\r\n', b'\n')
    if blob == normalized:
        raise SystemExit(f'expected CRLF-bearing blob is already normalized: {relative}')
    if b'\r' in normalized:
        raise SystemExit(f'unsupported lone CR remains after normalization: {relative}')
    if Path(relative).read_bytes() != normalized:
        raise SystemExit(f'checkout changed more than CRLF line endings: {relative}')
PY

NORMALIZED_PATHS=(
  docs/invocation-execution-ledger-flow-v1.md
  docs/progress-log-2026-04-07.md
  docs/progress-log-2026-04-08.md
  migrations/0002_add_account_summary_columns.sql
  ops/autopilot/README.md
  scripts/apply-migrations.ps1
  scripts/legacy/README.md
  scripts/legacy/_dev-helpers.ps1
  scripts/legacy/start-local-runtime-detached.ps1
  scripts/service-host.ps1
  scripts/start-rust-service.ps1
)
git add --renormalize -- "${NORMALIZED_PATHS[@]}"
git diff --quiet || fail "unstaged changes remain after CRLF normalization"

phase "apply hash-bound sequence 39 patch"
git apply --check "$RUNNER_TEMP/seq39.patch" \
  || fail "sequence39 patch does not apply to frozen base"
git apply --index --whitespace=error-all "$RUNNER_TEMP/seq39.patch" \
  || fail "sequence39 patch application failed"

phase "bind clean-checkout discovery into active authority"
python3 <<'PY'
from pathlib import Path
import json

plan_path = Path('docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md')
plan = plan_path.read_text(encoding='utf-8')
heading = 'Before repository qualification:\n\n'
line = (
    '- a clean checkout is byte-clean under `.gitattributes`; '
    'LF-governed tracked blobs are normalized so checkout clean filters '
    'do not mutate candidate bytes;\n'
)
if line not in plan:
    if plan.count(heading) != 1:
        raise SystemExit('cannot bind clean-checkout requirement into active plan')
    plan = plan.replace(heading, heading + line)
    plan_path.write_text(plan, encoding='utf-8')

trigger_path = Path('docs/release-evidence/p0-candidate-trigger.json')
trigger = json.loads(trigger_path.read_text(encoding='utf-8'))
if trigger.get('sequence') != 39:
    raise SystemExit('sequence39 patch did not produce trigger sequence 39')
if trigger.get('production_authorization') != 'not_granted':
    raise SystemExit('sequence39 trigger changed production authorization')
clause = (
    ' It also normalizes the eleven LF-governed tracked blobs that '
    'self-modified on a clean GitHub-hosted checkout, making candidate '
    'hygiene byte-clean before workflow trust and all downstream gates.'
)
purpose = trigger.get('purpose')
if not isinstance(purpose, str) or not purpose.strip():
    raise SystemExit('sequence39 trigger purpose is invalid')
if clause.strip() not in purpose:
    trigger['purpose'] = purpose.rstrip() + clause
trigger_path.write_text(
    json.dumps(trigger, indent=2, ensure_ascii=False) + '\n',
    encoding='utf-8',
)

hygiene_path = Path('scripts/check-p0-release-candidate-hygiene-core.py')
hygiene = hygiene_path.read_text(encoding='utf-8')
anchor = '    f"Candidate migration head: `{EXPECTED_MIGRATION_HEAD}`.",\n'
requirement = '    "clean checkout is byte-clean under `.gitattributes`",\n'
if requirement not in hygiene:
    if hygiene.count(anchor) != 1:
        raise SystemExit('cannot bind clean-checkout marker into hygiene contract')
    hygiene = hygiene.replace(anchor, requirement + anchor)
    hygiene_path.write_text(hygiene, encoding='utf-8')
PY
git add -- \
  docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md \
  docs/release-evidence/p0-candidate-trigger.json \
  scripts/check-p0-release-candidate-hygiene-core.py

phase "validate final candidate tree"
python3 - "$BASE_SHA" <<'PY'
import subprocess
import sys

base = sys.argv[1]
patch_paths = {
    '.github/workflows/p0-provider-reconciliation-gate.yml',
    '.github/workflows/p0-release-candidate-gate.yml',
    'docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md',
    'docs/clean-deployment-acceptance-v1.md',
    'docs/development-doc-authority-v1.json',
    'docs/release-evidence/p0-candidate-trigger.json',
    'docs/schemas/cex-release-baseline-manifest-v1.schema.json',
    'docs/templates/cex-release-baseline-manifest-v1.json',
    'docs/traceability/v12-requirements-v1.json',
    'migrations/0088_enforce_provider_terminal_evidence_binding.sql',
    'migrations/readme.md',
    'scripts/check-development-docs.py',
    'scripts/check-hosted-gate-execution-impl.py',
    'scripts/check-p0-release-candidate-hygiene-core.py',
    'scripts/check-p0-wiring.py',
    'scripts/check-provider-reconciliation-postgres.sh',
    'scripts/check-provider-success-evidence.py',
    'scripts/check-release-baseline-manifest-contract-core.py',
    'scripts/check-release-baseline-manifest-core.py',
    'scripts/check-release-baseline-manifest.py',
    'scripts/check-release-evidence-contract.py',
    'scripts/check-strict-release-evidence-wiring.py',
    'scripts/verify-hosted-run-execution.py',
    'services/execution-service/src/provider_dispatch.rs',
}
normalized_paths = {
    'docs/invocation-execution-ledger-flow-v1.md',
    'docs/progress-log-2026-04-07.md',
    'docs/progress-log-2026-04-08.md',
    'migrations/0002_add_account_summary_columns.sql',
    'ops/autopilot/README.md',
    'scripts/apply-migrations.ps1',
    'scripts/legacy/README.md',
    'scripts/legacy/_dev-helpers.ps1',
    'scripts/legacy/start-local-runtime-detached.ps1',
    'scripts/service-host.ps1',
    'scripts/start-rust-service.ps1',
}
observed = set(
    subprocess.check_output(
        ['git', 'diff', '--cached', '--name-only', base], text=True
    ).splitlines()
)
expected = patch_paths | normalized_paths
if observed != expected:
    raise SystemExit(
        f'final candidate path set drifted: missing={sorted(expected-observed)!r} '
        f'extra={sorted(observed-expected)!r}'
    )
PY
git diff --quiet || fail "unstaged changes remain in final candidate"
git diff --cached --check "$BASE_SHA" || fail "final candidate has whitespace errors"

tree_sha=$(git write-tree)
export GIT_AUTHOR_NAME='github-actions[bot]'
export GIT_AUTHOR_EMAIL='41898282+github-actions[bot]@users.noreply.github.com'
export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME"
export GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
candidate_sha=$(
  printf '%s\n\n%s\n' \
    'fix(v12): bind provider terminal evidence and normalize checkout' \
    'Sequence 39 closes database-bypass, remote-model identity, hosted-step attestation, and clean-checkout normalization gaps while preserving production_authorization=not_granted.' \
    | git commit-tree "$tree_sha" -p "$BASE_SHA"
)
check_eq "$(git rev-parse "$candidate_sha^")" "$BASE_SHA" candidate_parent
git reset --hard "$candidate_sha"
[[ -z "$(git status --porcelain=v1 --untracked-files=all)" ]] \
  || fail "constructed candidate is not byte-clean after checkout"
check_eq "$(git rev-parse 'HEAD^{tree}')" "$tree_sha" candidate_tree
echo "candidate_sha=$candidate_sha"
echo "candidate_tree=$tree_sha"
git show --stat --oneline --summary "$candidate_sha"

phase "static candidate contracts"
python3 -m py_compile \
  scripts/check-development-docs.py \
  scripts/check-hosted-gate-execution-impl.py \
  scripts/check-p0-release-candidate-hygiene-core.py \
  scripts/check-p0-wiring.py \
  scripts/check-provider-success-evidence.py \
  scripts/check-release-baseline-manifest-contract-core.py \
  scripts/check-release-baseline-manifest-core.py \
  scripts/check-release-baseline-manifest.py \
  scripts/check-release-evidence-contract.py \
  scripts/check-strict-release-evidence-wiring.py \
  scripts/verify-hosted-run-execution.py
python3 scripts/check-provider-success-evidence.py
python3 scripts/check-development-docs.py
python3 scripts/check-p0-migrations.py
python3 scripts/check-release-baseline-manifest.py \
  docs/templates/cex-release-baseline-manifest-v1.json --allow-template
python3 scripts/check-p0-wiring.py
python3 scripts/check-strict-release-evidence-wiring.py
python3 scripts/test-p0-release-evidence-strict.py
python3 scripts/check-p0-release-candidate-hygiene.py
python3 scripts/check-repository-integrity.py \
  --expected-sha "$candidate_sha" \
  --expected-tree "$tree_sha"

phase "Rust formatting, tests and lint"
cargo fmt --all --check
cargo test --locked -p execution-service
cargo clippy --locked -p execution-service --all-targets -- -D warnings

phase "fresh PostgreSQL 16 migration chain"
source scripts/_dev-helpers.sh
cex_load_env
cex_sync_postgres_env_from_database_url "$DATABASE_URL"
while IFS= read -r migration; do
  echo "applying $(basename "$migration")"
  cex_psql_stdin -X -v ON_ERROR_STOP=1 -f - < "$migration" >/dev/null
done < <(
  find migrations -maxdepth 1 -type f \
    -name '[0-9][0-9][0-9][0-9]_*.sql' | sort
)

phase "provider terminal authority PostgreSQL matrix"
bash scripts/check-provider-reconciliation-postgres.sh

phase "publish validated staging ref"
remote_target=$(git ls-remote --heads origin "refs/heads/$TARGET_BRANCH" | awk '{print $1}')
remote_staging=$(git ls-remote --heads origin "refs/heads/$STAGING_BRANCH" | awk '{print $1}')
check_eq "$remote_target" "$BASE_SHA" final_target_branch_sha
check_eq "$remote_staging" "$BASE_SHA" final_staging_branch_sha
git push origin "$candidate_sha:refs/heads/$STAGING_BRANCH"
observed=$(git ls-remote --heads origin "refs/heads/$STAGING_BRANCH" | awk '{print $1}')
check_eq "$observed" "$candidate_sha" published_staging_sha

phase "sequence 39 staging candidate validated"
echo "CANDIDATE_SHA=$candidate_sha"
echo "CANDIDATE_TREE=$tree_sha"

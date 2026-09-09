#!/usr/bin/env bash
# Immutable-base allowlist push harness. Candidate bytes are data on the host
# and execute only inside the locked-down container below.
set -euo pipefail
set +x

: "${GITHUB_REPOSITORY:?}" "${GITHUB_EVENT_NAME:?}" "${RUNNER_TEMP:?}" \
  "${RUNNER_NAME:?}" "${TRUSTED_BASE_SHA:?}" "${CANDIDATE_SHA:?}" \
  "${CANDIDATE_REPOSITORY:?}" "${CANDIDATE_REF:?}" "${BASE_REF:?}" \
  "${PR_NUMBER:?}"

[[ "$GITHUB_REPOSITORY" == "TrillionniumFoundation/CEX" ]]
[[ "$GITHUB_EVENT_NAME" == "push" ]]
[[ "$CANDIDATE_REPOSITORY" == "$GITHUB_REPOSITORY" ]]
[[ "$CANDIDATE_REF" == "fix/cex-v12-audit-remediation-20260905" ]]
[[ "$BASE_REF" == "fix/cex-v12-seq53-close-repository-gaps-20260904" ]]
[[ "$TRUSTED_BASE_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ "$CANDIDATE_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ "$PR_NUMBER" =~ ^[1-9][0-9]*$ ]]
[[ "$RUNNER_NAME" == "rog" ]]
[[ "$(git rev-parse HEAD)" == "$TRUSTED_BASE_SHA" ]]
[[ -n "${1:-}" ]]

TRUST_ROOT="$(git rev-parse --show-toplevel)"
CANDIDATE="$(realpath -e -- "$1")"
WORKSPACE="$(realpath -e -- "${GITHUB_WORKSPACE:?}")"
case "$CANDIDATE/" in
  "$WORKSPACE"/*) ;;
  *) echo "candidate checkout escaped GITHUB_WORKSPACE" >&2; exit 1 ;;
esac
[[ ! -L "$CANDIDATE" && -d "$CANDIDATE/.git" ]]
[[ "$(git -C "$CANDIDATE" rev-parse HEAD)" == "$CANDIDATE_SHA" ]]
[[ -z "$(git -C "$CANDIDATE" status --porcelain=v1 --untracked-files=all)" ]]
[[ -z "$(git -C "$CANDIDATE" submodule status 2>/dev/null || true)" ]]
if git -C "$CANDIDATE" ls-tree -r --name-only "$CANDIDATE_SHA" \
    | grep -Eq '(^|/)\.cargo/(config|config\.toml)$'; then
  echo "project Cargo configuration is forbidden in the networked fetch phase" >&2
  exit 1
fi

command -v docker >/dev/null
command -v git >/dev/null
command -v python3 >/dev/null

python3 - "$CANDIDATE/Cargo.lock" <<'PY'
from pathlib import Path
import sys, tomllib
path = Path(sys.argv[1])
data = tomllib.loads(path.read_text(encoding="utf-8"))
allowed = {None, "registry+https://github.com/rust-lang/crates.io-index"}
for package in data.get("package", []):
    source = package.get("source")
    if source not in allowed:
        raise SystemExit(f"forbidden locked dependency source: {source!r}")
PY

WORK="$(mktemp -d "$RUNNER_TEMP/cex-trusted-base-gate.XXXXXXXX")"
chmod 700 "$WORK"
OUT="$TRUST_ROOT/run/trusted-matrix-base-gate"
rm -rf -- "$OUT"
mkdir -p "$OUT" "$WORK/cache" "$WORK/build" "$WORK/output" "$WORK/context"
chmod 700 "$OUT" "$WORK/cache" "$WORK/build" "$WORK/output" "$WORK/context"

printf 'schema=cex.trusted-matrix-base-gate.v1\n' > "$OUT/identity.txt"
printf 'repository=%s\n' "$GITHUB_REPOSITORY" >> "$OUT/identity.txt"
printf 'pull_request=%s\n' "$PR_NUMBER" >> "$OUT/identity.txt"
printf 'trusted_base_sha=%s\n' "$TRUSTED_BASE_SHA" >> "$OUT/identity.txt"
printf 'trusted_base_tree=%s\n' "$(git rev-parse 'HEAD^{tree}')" >> "$OUT/identity.txt"
printf 'candidate_sha=%s\n' "$CANDIDATE_SHA" >> "$OUT/identity.txt"
printf 'candidate_tree=%s\n' "$(git -C "$CANDIDATE" rev-parse 'HEAD^{tree}')" >> "$OUT/identity.txt"
printf 'harness_sha256=sha256:%s\n' "$(sha256sum "$0" | awk '{print $1}')" >> "$OUT/identity.txt"
printf 'production_authorization=not_granted\n' >> "$OUT/identity.txt"

git -C "$CANDIDATE" archive --format=tar "$CANDIDATE_SHA" > "$OUT/source-$CANDIDATE_SHA.tar"
sha256sum "$OUT/source-$CANDIDATE_SHA.tar" > "$OUT/source-$CANDIDATE_SHA.tar.sha256"

SUFFIX="${GITHUB_RUN_ID:?}-${GITHUB_RUN_ATTEMPT:?}-${PR_NUMBER}"
NETWORK="cex-trusted-matrix-$SUFFIX"
DB="cex-trusted-matrix-pg-$SUFFIX"
IMAGE="cex-trusted-matrix:$SUFFIX"

cat > "$WORK/context/Dockerfile" <<'DOCKER'
FROM rust:1.98.1-bookworm
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 postgresql-client ca-certificates git \
    && rm -rf /var/lib/apt/lists/*
RUN rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy \
    && rustup default 1.98.1
ENV RUST_VERSION=1.98.1 \
    CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0 \
    CARGO_NET_GIT_FETCH_WITH_CLI=false \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
DOCKER

docker build --pull -t "$IMAGE" "$WORK/context" \
  > "$OUT/image-build.log" 2>&1
IMAGE_ID="$(docker image inspect "$IMAGE" --format '{{.Id}}')"
printf 'image_id=%s\n' "$IMAGE_ID" >> "$OUT/identity.txt"

UID_VALUE="$(id -u)"
GID_VALUE="$(id -g)"
[[ "$UID_VALUE" != 0 ]]

cleanup() {
  docker rm -f "$DB" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
  docker image rm "$IMAGE" >/dev/null 2>&1 || true
  rm -rf -- "$WORK"
}
trap cleanup EXIT

BASE=(docker run --rm --read-only --user "$UID_VALUE:$GID_VALUE"
  --cap-drop ALL --security-opt no-new-privileges --pids-limit 1024
  --cpus 4 --memory 12g --tmpfs /tmp:rw,nosuid,nodev,size=2g
  --mount "type=bind,src=$CANDIDATE,dst=/candidate,readonly"
  --mount "type=bind,src=$WORK/cache,dst=/cache"
  --mount "type=bind,src=$WORK/build,dst=/build"
  --mount "type=bind,src=$WORK/output,dst=/output"
  -e HOME=/build/home -e CARGO_HOME=/cache -e CARGO_TARGET_DIR=/build/target
  -e CARGO_TERM_COLOR=never -e CARGO_INCREMENTAL=0
  -e CARGO_NET_GIT_FETCH_WITH_CLI=false
  -e CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse)

# Networked phase: dependency acquisition only. Candidate project Cargo config
# is forbidden above; Cargo.lock sources are restricted to crates.io.
set +e
"${BASE[@]}" "$IMAGE" bash -euo pipefail -c '
  mkdir -p /build/home /build/source
  git -C /candidate archive --format=tar "$1" | tar -x -C /build/source
  cp -a /candidate/.git /build/source/.git
  git config --global --add safe.directory /build/source
  cd /build/source
  [[ "$(git rev-parse HEAD)" == "$1" ]]
  cargo fetch --locked --manifest-path Cargo.toml
' _ "$CANDIDATE_SHA" > "$OUT/fetch.log" 2>&1
FETCH_EXIT=$?
set -e
printf '%s\n' "$FETCH_EXIT" > "$OUT/fetch.exit"
[[ "$FETCH_EXIT" == 0 ]]

docker network create --internal "$NETWORK" >/dev/null
docker run -d --name "$DB" --network "$NETWORK" --network-alias postgres \
  --cap-drop ALL --security-opt no-new-privileges --user 999:999 \
  --pids-limit 128 --memory 1g --cpus 1 \
  --tmpfs /var/lib/postgresql/data:rw,nosuid,nodev,uid=999,gid=999,mode=700 \
  --tmpfs /var/run/postgresql:rw,nosuid,nodev,uid=999,gid=999,mode=755 \
  -e POSTGRES_USER=cex -e POSTGRES_PASSWORD=disposable_ci_only \
  -e POSTGRES_DB=matrix_review_ci postgres:16.14-bookworm >/dev/null
READY=0
for _ in $(seq 1 45); do
  if docker exec "$DB" pg_isready -U cex -d matrix_review_ci >/dev/null 2>&1; then
    READY=1
    break
  fi
  sleep 1
done
if [[ "$READY" != 1 ]]; then
  docker logs "$DB" > "$OUT/postgres-startup.log" 2>&1 || true
  exit 1
fi

set +e
"${BASE[@]}" --network "$NETWORK" \
  -e MATRIX_TEST_DATABASE_URL=postgres://cex:disposable_ci_only@postgres:5432/matrix_review_ci \
  -e MATRIX_TEST_ALLOW_SCHEMA_RESET=1 \
  "$IMAGE" bash -euo pipefail -c '
    mkdir -p /build/home
    git config --global --add safe.directory /build/source
    cd /build/source
    [[ "$(git rev-parse HEAD)" == "$1" ]]
    run() {
      name="$1"; shift
      set +e
      "$@" > "/output/$name.log" 2>&1
      code=$?
      set -e
      printf "%s\n" "$code" > "/output/$name.exit"
      return 0
    }
    run versions bash -euo pipefail -c '\''v="$(rustc --version)"; [[ "$v" == "rustc 1.98.1 "* ]]; cargo --version; rustfmt --version; clippy-driver --version; psql --version'\''
    run cargo_metadata cargo metadata --locked --no-deps --format-version 1
    run source_contracts python3 scripts/test-matrix-review-repairs.py
    run matrix_recovery python3 scripts/test-matrix-recovery-contract.py
    run postgres_runner_tests python3 scripts/test-matrix-postgres-runner.py
    run runner_hardening python3 scripts/test-matrix-runner-hardening.py
    run runner_lifecycle python3 scripts/test-matrix-runner-lifecycle.py
    run module_docs python3 scripts/check-module-documentation.py
    run workspace_authority python3 scripts/check-cargo-workspace-authority.py
    run semantic_generate python3 scripts/generate-repository-semantics.py --write
    cp docs/repository-contract-semantics-v1.json /output/repository-contract-semantics-v1.json
    run semantic_check python3 scripts/generate-repository-semantics.py --check
    run consumer_projection python3 scripts/check-consumer-projection-boundary.py
    run development_docs python3 scripts/check-development-docs.py
    run fmt cargo fmt --all -- --check
    run workspace_test cargo test --offline --locked --workspace --all-targets
    run workspace_clippy cargo clippy --offline --locked --workspace --all-targets -- -D warnings
    run matrix_postgres bash scripts/check-matrix-source-observation-postgres.sh
    python3 - <<'\''PY'\''
import json
from pathlib import Path
out = Path("/output")
commands = {p.stem: int(p.read_text()) for p in sorted(out.glob("*.exit"))}
expected = {
    "versions", "cargo_metadata", "source_contracts", "matrix_recovery",
    "postgres_runner_tests", "runner_hardening", "runner_lifecycle",
    "module_docs", "workspace_authority", "semantic_generate",
    "semantic_check", "consumer_projection", "development_docs", "fmt",
    "workspace_test", "workspace_clippy", "matrix_postgres"
}
result = {
    "schema": "cex.trusted-matrix-base-gate-results.v1",
    "commands": commands,
    "expected_commands": sorted(expected),
    "all_pass": set(commands) == expected and all(code == 0 for code in commands.values()),
    "production_authorization": "not_granted",
}
(out / "results.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
raise SystemExit(0 if result["all_pass"] else 1)
PY
  ' _ "$CANDIDATE_SHA" > "$OUT/container.log" 2>&1
RESULT_EXIT=$?
set -e
printf '%s\n' "$RESULT_EXIT" > "$OUT/container.exit"
cp -R "$WORK/output/." "$OUT/"
sha256sum "$OUT"/* > "$OUT/SHA256SUMS"
[[ "$RESULT_EXIT" == 0 ]]

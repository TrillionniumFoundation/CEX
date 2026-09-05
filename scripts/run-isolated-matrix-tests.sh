#!/usr/bin/env bash
# Trusted, push-only supporting validation. Never grant production authority.
set -euo pipefail
set +x
: "${GITHUB_REPOSITORY:?}" "${GITHUB_REF:?}" "${GITHUB_SHA:?}" "${RUNNER_TEMP:?}"
[[ "$GITHUB_REPOSITORY" == TrillionniumFoundation/CEX ]]
[[ "$GITHUB_REF" == refs/heads/fix/cex-v12-audit-remediation-20260905 ]]
[[ "${GITHUB_EVENT_NAME:-}" == push ]]
[[ "${RUNNER_NAME:-}" == rog ]]
[[ "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]]
ROOT="$(git rev-parse --show-toplevel)"
[[ "$(git rev-parse HEAD)" == "$GITHUB_SHA" ]]
WORK="$(mktemp -d "$RUNNER_TEMP/cex-matrix-validation.XXXXXXXX")"
chmod 700 "$WORK"
OUT="$ROOT/run/isolated-matrix"
mkdir -p "$OUT"
printf 'candidate=%s\n' "$GITHUB_SHA" > "$OUT/identity.txt"
printf 'tree=%s\n' "$(git rev-parse 'HEAD^{tree}')" >> "$OUT/identity.txt"
# A source snapshot remains diagnostic input, even when no container can start.
git archive --format=tar HEAD > "$OUT/source-$GITHUB_SHA.tar"
command -v docker >/dev/null
# Only this unique directory, network, database and image are ever removed.
SUFFIX="${GITHUB_RUN_ID:?}-${GITHUB_RUN_ATTEMPT:?}"
NETWORK="cex-matrix-$SUFFIX"
DB="cex-matrix-pg-$SUFFIX"
IMAGE="cex-matrix-validation:$SUFFIX"
cleanup() {
  docker rm -f "$DB" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
  docker image rm "$IMAGE" >/dev/null 2>&1 || true
  # Unprivileged containers create these files under the runner UID.
  rm -rf -- "$WORK"
}
trap cleanup EXIT
mkdir "$WORK/build" "$WORK/output" "$WORK/cache" "$WORK/context"
cat > "$WORK/context/Dockerfile" <<'DOCKER'
FROM rust:1.98.0-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends python3 postgresql-client ca-certificates && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy
DOCKER
# No repository code or credentials are included in the image build context.
docker build --pull -t "$IMAGE" "$WORK/context" > "$OUT/image-build.log" 2>&1
docker image inspect "$IMAGE" --format '{{.Id}}' >> "$OUT/identity.txt"
UID_VALUE="$(id -u)"; GID_VALUE="$(id -g)"
[[ "$UID_VALUE" != 0 ]] || { echo "runner must be unprivileged" >&2; exit 1; }
BASE=(docker run --rm --read-only --user "$UID_VALUE:$GID_VALUE"
  --cap-drop ALL --security-opt no-new-privileges --pids-limit 512
  --cpus 2 --memory 6g --tmpfs /tmp:rw,nosuid,nodev,size=1g
  --mount "type=bind,src=$ROOT,dst=/source,readonly"
  --mount "type=bind,src=$WORK/cache,dst=/cache"
  --mount "type=bind,src=$WORK/build,dst=/build"
  --mount "type=bind,src=$WORK/output,dst=/output"
  -e HOME=/build/home -e CARGO_HOME=/cache -e CARGO_TARGET_DIR=/build/target
  -e CARGO_TERM_COLOR=never -e CARGO_INCREMENTAL=0)
# Cargo fetch does not execute repository build scripts; it runs without host
# home, SSH agent, Git credential store, Docker socket or production secrets.
set +e
"${BASE[@]}" "$IMAGE" cargo fetch --locked --manifest-path /source/Cargo.toml > "$OUT/fetch.log" 2>&1
FETCH=$?
set -e
printf '%s\n' "$FETCH" > "$OUT/fetch.exit"
docker network create --internal "$NETWORK" > /dev/null
# Internal network, no published ports, tmpfs data, no production credential.
docker run -d --name "$DB" --network "$NETWORK" --network-alias postgres \
  --cap-drop ALL --security-opt no-new-privileges --user 999:999 \
  --pids-limit 128 --memory 1g --cpus 1 \
  --tmpfs /var/lib/postgresql/data:rw,nosuid,nodev,uid=999,gid=999,mode=700 \
  --tmpfs /var/run/postgresql:rw,nosuid,nodev,uid=999,gid=999,mode=755 \
  -e POSTGRES_USER=cex -e POSTGRES_PASSWORD=disposable_ci_only \
  -e POSTGRES_DB=matrix_review_ci postgres:16.14-bookworm > /dev/null
READY=0
for _ in $(seq 1 30); do
  if docker exec "$DB" pg_isready -U cex -d matrix_review_ci >/dev/null 2>&1; then READY=1; break; fi
  sleep 1
done
[[ "$READY" == 1 ]] || { docker logs "$DB" > "$OUT/postgres-startup.log" 2>&1; exit 1; }
# Build scripts/tests have no external network. Their only peer is a disposable
# PostgreSQL container; outbound calls to homeservers or real providers fail.
set +e
"${BASE[@]}" --network "$NETWORK" \
  -e MATRIX_TEST_DATABASE_URL=postgres://cex:disposable_ci_only@postgres:5432/matrix_review_ci \
  -e MATRIX_TEST_ALLOW_SCHEMA_RESET=1 "$IMAGE" bash -c '
    set -u
    mkdir -p "$HOME"
    cd /source
    run() { name="$1"; shift; "$@" > "/output/$name.log" 2>&1; code=$?; printf "%s\n" "$code" > "/output/$name.exit"; }
    run versions bash -c "rustc --version; cargo --version; psql --version"
    run source12 python3 scripts/test-matrix-review-repairs.py
    run source29 python3 scripts/test-matrix-recovery-contract.py
    run sql-runner21 python3 scripts/test-matrix-postgres-runner.py
    run module-docs python3 scripts/check-module-documentation.py
    run fmt cargo fmt -p matrix-entry-adapter -p matrix-bot-poller -p matrix-bot-relay -- --check
    run check cargo check --offline --locked -p matrix-entry-adapter -p matrix-bot-poller -p matrix-bot-relay --all-targets
    run test cargo test --offline --locked -p matrix-entry-adapter -p matrix-bot-poller -p matrix-bot-relay --all-targets
    run clippy cargo clippy --offline --locked -p matrix-entry-adapter -p matrix-bot-poller -p matrix-bot-relay --all-targets -- -D warnings
    run postgres bash scripts/check-matrix-source-observation-postgres.sh
    # This tar has diagnostics only. No success is inferred from file presence.
    python3 -c '\''import json,pathlib; p=pathlib.Path("/output"); r={f.stem:int(f.read_text()) for f in p.glob("*.exit")}; (p/"results.json").write_text(json.dumps({"commands":r,"all_pass":bool(r) and all(v==0 for v in r.values()),"production_authorization":"not_granted"},indent=2)); raise SystemExit(0 if len(r)==10 and all(v==0 for v in r.values()) else 1)'\''
  ' > "$OUT/container.log" 2>&1
RESULT=$?
set -e
cp -R "$WORK/output/." "$OUT/"
[[ "$FETCH" == 0 && "$RESULT" == 0 ]]

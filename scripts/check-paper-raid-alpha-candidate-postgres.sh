#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

container_name=hepta-paper-raid-alpha-pg-gate-$$
postgres_image=postgres@sha256:ef257d85f76e48da1c64832459b59fcaba1a4dac97bf5d7450c77753542eee94
cargo_gate=${HEPTA_CARGO_LOCK_FILE:-/tmp/trnm-paper-raid-cargo-gate.lock}

cleanup() {
  sudo -n docker rm -f "$container_name" >/dev/null 2>&1 || true
}
trap cleanup EXIT

for command_name in cargo docker flock nc seq sudo; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'ERROR: PostgreSQL candidate gate requires %s\n' "$command_name" >&2
    exit 1
  }
done

sudo -n docker run --rm --detach \
  --name "$container_name" \
  --publish 127.0.0.1::5432 \
  --env POSTGRES_USER=hepta \
  --env POSTGRES_PASSWORD=hepta-test \
  --env POSTGRES_DB=hepta \
  "$postgres_image" >/dev/null

for _ in $(seq 1 30); do
  if sudo -n docker exec "$container_name" \
    pg_isready --username hepta --dbname hepta >/dev/null 2>&1
  then
    break
  fi
  sleep 1
done
sudo -n docker exec "$container_name" \
  pg_isready --username hepta --dbname hepta >/dev/null
published=$(sudo -n docker port "$container_name" 5432/tcp | head -n 1)
port=${published##*:}
database_url="postgres://hepta:hepta-test@127.0.0.1:${port}/hepta"
for _ in $(seq 1 30); do
  nc -z 127.0.0.1 "$port" && break
  sleep 1
done
nc -z 127.0.0.1 "$port"

# Every PostgreSQL-backed test below shares this one disposable database and
# several tests reset it destructively. Serialize test cases at the libtest
# harness boundary; concurrency exercised inside an individual test remains
# intact.
HEPTA_TEST_DATABASE_URL="$database_url" \
  flock -n "$cargo_gate" cargo test --locked -p hepta-research-league -- --test-threads=1

bash services/paper-raid-bff/scripts/check-postgres.sh

echo "Paper Raid alpha candidate PostgreSQL gate: PASS"

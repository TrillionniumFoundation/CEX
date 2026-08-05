#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
container_name=paper-raid-bff-pg-gate-$$
postgres_image=postgres@sha256:ef257d85f76e48da1c64832459b59fcaba1a4dac97bf5d7450c77753542eee94

cleanup() {
  sudo -n docker rm -f "$container_name" >/dev/null 2>&1 || true
}
trap cleanup EXIT

sudo -n docker run --rm --detach \
  --name "$container_name" \
  --publish 127.0.0.1::5432 \
  --env POSTGRES_USER=paper_raid_bff \
  --env POSTGRES_PASSWORD=paper-raid-test \
  --env POSTGRES_DB=paper_raid_bff \
  "$postgres_image" >/dev/null

for _ in $(seq 1 30); do
  if sudo -n docker exec "$container_name" \
    pg_isready --username paper_raid_bff --dbname paper_raid_bff >/dev/null 2>&1
  then
    break
  fi
  sleep 1
done
sudo -n docker exec "$container_name" \
  pg_isready --username paper_raid_bff --dbname paper_raid_bff >/dev/null
published=$(sudo -n docker port "$container_name" 5432/tcp | head -n 1)
port=${published##*:}
test_database_url="postgres://paper_raid_bff:paper-raid-test@127.0.0.1:${port}/paper_raid_bff"
for _ in $(seq 1 30); do
  if nc -z 127.0.0.1 "$port"; then
    break
  fi
  sleep 1
done
nc -z 127.0.0.1 "$port"

PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n /tmp/trnm-paper-raid-cargo-gate.lock \
  cargo test -p paper-raid-bff auth::tests::real_postgres_lost_rotation_refresh_restart_and_revoke -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n /tmp/trnm-paper-raid-cargo-gate.lock \
  cargo test -p paper-raid-bff hepta::tests::real_postgres_pending_retry_restart_exact_cache_and_assertion_tamper -- --exact

revoked_before=$(sudo -n docker exec "$container_name" \
  psql --username paper_raid_bff --dbname paper_raid_bff --tuples-only --no-align \
  --command 'SELECT count(*) FROM paper_raid_bff_sessions WHERE revoked_at IS NOT NULL')
[[ "$revoked_before" -ge 1 ]]

sudo -n docker restart "$container_name" >/dev/null
for _ in $(seq 1 30); do
  if sudo -n docker exec "$container_name" \
    pg_isready --username paper_raid_bff --dbname paper_raid_bff >/dev/null 2>&1
  then
    break
  fi
  sleep 1
done
sudo -n docker exec "$container_name" \
  pg_isready --username paper_raid_bff --dbname paper_raid_bff >/dev/null
published=$(sudo -n docker port "$container_name" 5432/tcp | head -n 1)
port=${published##*:}
test_database_url="postgres://paper_raid_bff:paper-raid-test@127.0.0.1:${port}/paper_raid_bff"
for _ in $(seq 1 30); do
  if nc -z 127.0.0.1 "$port"; then
    break
  fi
  sleep 1
done
nc -z 127.0.0.1 "$port"

revoked_after=$(sudo -n docker exec "$container_name" \
  psql --username paper_raid_bff --dbname paper_raid_bff --tuples-only --no-align \
  --command 'SELECT count(*) FROM paper_raid_bff_sessions WHERE revoked_at IS NOT NULL')
[[ "$revoked_after" == "$revoked_before" ]]

PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n /tmp/trnm-paper-raid-cargo-gate.lock \
  cargo test -p paper-raid-bff auth::tests::real_postgres_lost_rotation_refresh_restart_and_revoke -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n /tmp/trnm-paper-raid-cargo-gate.lock \
  cargo test -p paper-raid-bff hepta::tests::real_postgres_pending_retry_restart_exact_cache_and_assertion_tamper -- --exact

echo "paper-raid-bff PostgreSQL restart/revoke gate: ok"

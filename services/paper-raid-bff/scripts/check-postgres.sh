#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
container_name=paper-raid-bff-pg-gate-$$
postgres_image=postgres@sha256:ef257d85f76e48da1c64832459b59fcaba1a4dac97bf5d7450c77753542eee94
fixture_container_root=/tmp/paper-raid-bff-migrations
cargo_gate=${HEPTA_CARGO_LOCK_FILE:-/tmp/trnm-paper-raid-cargo-gate.lock}

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

sudo -n docker exec "$container_name" mkdir -p "$fixture_container_root"
sudo -n docker cp "$root/migrations/." \
  "$container_name:$fixture_container_root/"

create_fixture_database() {
  local database=$1
  [[ "$database" =~ ^[a-z0-9_]+$ ]]
  sudo -n docker exec "$container_name" \
    createdb --username paper_raid_bff "$database"
}

run_fixture_success() {
  local database=$1
  local fixture=$2
  sudo -n docker exec "$container_name" \
    psql --username paper_raid_bff --dbname "$database" \
      --file "$fixture_container_root/fixtures/$fixture"
}

run_fixture_failure() {
  local database=$1
  local fixture=$2
  local expected=$3
  local output
  if output=$(sudo -n docker exec "$container_name" \
      psql --username paper_raid_bff --dbname "$database" \
        --file "$fixture_container_root/fixtures/$fixture" 2>&1); then
    echo "PostgreSQL fixture unexpectedly succeeded: $fixture" >&2
    exit 1
  fi
  grep -Fq "$expected" <<<"$output"
}

create_fixture_database paper_raid_bff_0006_upgrade
run_fixture_success \
  paper_raid_bff_0006_upgrade \
  0006_accessctl_operator_audit_upgrade.sql

create_fixture_database paper_raid_bff_0006_guard_tamper
run_fixture_failure \
  paper_raid_bff_0006_guard_tamper \
  0006_accessctl_operator_audit_tampered_guard.sql \
  'access audit append-only function definition is missing or drifted'

create_fixture_database paper_raid_bff_0006_trigger_tamper
run_fixture_failure \
  paper_raid_bff_0006_trigger_tamper \
  0006_accessctl_operator_audit_tampered_trigger.sql \
  'access audit append-only trigger definition is missing or drifted'

create_fixture_database paper_raid_bff_0006_rollback
run_fixture_failure \
  paper_raid_bff_0006_rollback \
  0006_accessctl_operator_audit_rollback_after_drop.sql \
  '0006 fixture deliberate failure after DROP'
# A separate docker exec creates a new psql connection. It must observe the
# complete pre-0006 state after the failed explicit migration transaction.
run_fixture_success \
  paper_raid_bff_0006_rollback \
  0006_accessctl_operator_audit_rollback_verify.sql

create_fixture_database paper_raid_bff_0007_nullable
run_fixture_success \
  paper_raid_bff_0007_nullable \
  0007_invite_activation_nullable_and_json_null.sql

create_fixture_database paper_raid_bff_0007_strict_tamper
run_fixture_success \
  paper_raid_bff_0007_strict_tamper \
  0007_invite_activation_strict_tamper.sql

create_fixture_database paper_raid_bff_0008_review_receipt_context
run_fixture_success \
  paper_raid_bff_0008_review_receipt_context \
  0008_review_receipt_confirmation_context.sql

create_fixture_database paper_raid_bff_0008_true_upgrade
run_fixture_success \
  paper_raid_bff_0008_true_upgrade \
  0008_review_receipt_true_upgrade.sql

create_fixture_database paper_raid_bff_0008_partial_catalog
run_fixture_success \
  paper_raid_bff_0008_partial_catalog \
  0008_review_receipt_partial_catalog.sql

create_fixture_database paper_raid_bff_0008_catalog_tamper
run_fixture_success \
  paper_raid_bff_0008_catalog_tamper \
  0008_review_receipt_catalog_tamper.sql

PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff review_receipts::tests::real_postgres_router_object_surfaces_signed_get_replay_and_audience_separation -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff review_receipts::tests::real_postgres_review_receipt_attempt_concurrency_replay_restart_and_integrity -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff auth::tests::real_postgres_lost_rotation_refresh_restart_and_revoke -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff hepta::tests::real_postgres_pending_retry_restart_exact_cache_and_assertion_tamper -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff db::invite_activation_atomic_tests::real_postgres_v3_atomic_catalog_authority_and_revocation_gate -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff practice::tests::real_postgres_practice_catalog_lifecycle_and_append_only_gate -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff practice_http::tests::real_postgres_abandon_uses_stored_binding_after_external_revocation -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff review_receipts::tests::real_postgres_practice_agent_bridge_owner_replay_restart_and_authority_boundary -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
PAPER_RAID_BFF_EXPECT_PAIRING_STATUS_RESTART=0 \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff agent_bridge::tests::real_postgres_pairing_grant_status_health_binding_and_restart -- --exact

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
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff review_receipts::tests::real_postgres_router_object_surfaces_signed_get_replay_and_audience_separation -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff review_receipts::tests::real_postgres_review_receipt_attempt_concurrency_replay_restart_and_integrity -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff auth::tests::real_postgres_lost_rotation_refresh_restart_and_revoke -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff hepta::tests::real_postgres_pending_retry_restart_exact_cache_and_assertion_tamper -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff db::invite_activation_atomic_tests::real_postgres_v3_atomic_catalog_authority_and_revocation_gate -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff practice::tests::real_postgres_practice_catalog_lifecycle_and_append_only_gate -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff practice_http::tests::real_postgres_abandon_uses_stored_binding_after_external_revocation -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff review_receipts::tests::real_postgres_practice_agent_bridge_owner_replay_restart_and_authority_boundary -- --exact
PAPER_RAID_BFF_TEST_DATABASE_URL="$test_database_url" \
PAPER_RAID_BFF_EXPECT_PAIRING_STATUS_RESTART=1 \
  flock -n "$cargo_gate" \
  cargo test --locked -p paper-raid-bff agent_bridge::tests::real_postgres_pairing_grant_status_health_binding_and_restart -- --exact

PAPER_RAID_DOCKER_USE_SUDO=1 \
  bash "$root/../../scripts/check-paper-raid-fresh-restore.sh" \
    --container "$container_name" \
    --user paper_raid_bff \
    --database paper_raid_bff \
    --label bff

echo "paper-raid-bff PostgreSQL restart/revoke gate: ok"

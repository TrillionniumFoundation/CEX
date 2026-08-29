#!/usr/bin/env bash
set -Eeuo pipefail

# Exercise the supported upgrade boundary for the normalized receipt tables.
#
# The check deliberately builds a database at the 0084 schema, copies that
# database for each case, and then applies 0085/0086/0087 in order.  Keeping the
# cases in cloned databases means a failed migration can be inspected without
# poisoning the next case, while the caller's DATABASE_URL database is never
# reset or otherwise modified.

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
# Capture credentials supplied by the caller before `_dev-helpers.sh` installs
# its convenience default (`postgres`).  A DATABASE_URL commonly carries the
# real password itself; forcing that default into PGPASSWORD would override the
# URI and make the hosted cex/cex_ci gate fail authentication.
RECEIPT_PSQL_PASSWORD="${PGPASSWORD:-${CEX_POSTGRES_PASSWORD:-}}"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"

# Do not let an explicitly supplied URL get replaced by a repository .env file.
# When no URL was supplied, retain the normal local-helper discovery behavior.
if [[ -z "${DATABASE_URL:-}" ]]; then
  cex_load_env
fi
: "${DATABASE_URL:?DATABASE_URL is required}"

# If the optional env file supplied a non-default password, use it; otherwise
# leave PGPASSWORD unset so libpq can read the password embedded in DATABASE_URL.
if [[ -z "$RECEIPT_PSQL_PASSWORD" && "${CEX_POSTGRES_PASSWORD:-postgres}" != "postgres" ]]; then
  RECEIPT_PSQL_PASSWORD="$CEX_POSTGRES_PASSWORD"
fi

BASE_URL="$(cex_effective_database_url)"
MIGRATION_85="$PROJECT_ROOT/migrations/0085_harden_term_exchange_receipt_projections.sql"
MIGRATION_86="$PROJECT_ROOT/migrations/0086_add_trnm_native_receipt_evidence.sql"
MIGRATION_87="$PROJECT_ROOT/migrations/0087_add_term_exchange_receipt_event_history.sql"

for migration in "$MIGRATION_85" "$MIGRATION_86" "$MIGRATION_87"; do
  [[ -f "$migration" ]] || {
    echo "receipt partial-upgrade migration is missing: $migration" >&2
    exit 1
  }
done

if ! cex_has_local_psql && ! cex_can_use_docker_postgres; then
  echo "receipt partial-upgrade check requires psql or a usable Docker Postgres container" >&2
  exit 1
fi

# This is a destructive-looking test only inside databases that this script
# creates.  Refuse a remote URL unless an operator explicitly opts in.
if [[ "$BASE_URL" != *"127.0.0.1"* && "$BASE_URL" != *"localhost"* \
      && "${CEX_ALLOW_NONLOCAL_RECEIPT_PARTIAL_UPGRADE_CHECK:-0}" != "1" ]]; then
  echo "refusing non-local DATABASE_URL for receipt partial-upgrade check" >&2
  echo "set CEX_ALLOW_NONLOCAL_RECEIPT_PARTIAL_UPGRADE_CHECK=1 only for an isolated disposable server" >&2
  exit 1
fi

run_token="${CEX_RECEIPT_PARTIAL_UPGRADE_RUN_ID:-$(date -u +%Y%m%d%H%M%S)_$$}"
if [[ ! "$run_token" =~ ^[A-Za-z0-9_]+$ ]]; then
  echo "unsafe receipt partial-upgrade run id: $run_token" >&2
  exit 2
fi

base_db="cex_receipt_pu_base_${run_token}"
if [[ ! "$base_db" =~ ^[A-Za-z_][A-Za-z0-9_]*$ || ${#base_db} -gt 63 ]]; then
  # The generated default is intentionally short; this branch also catches a
  # user-supplied run id before it can become an SQL identifier.
  echo "receipt partial-upgrade database name is invalid or too long: $base_db" >&2
  exit 2
fi

db_url() {
  local database="$1"
  # DATABASE_URLs used by the CEX helpers are URI-form URLs.  Keep the URI's
  # credentials/host intact while replacing only its database path.
  printf '%s/%s\n' "${BASE_URL%/*}" "$database"
}

run_admin_sql() {
  local sql="$1"
  if cex_has_local_psql; then
    if [[ -n "$RECEIPT_PSQL_PASSWORD" ]]; then
      PGPASSWORD="$RECEIPT_PSQL_PASSWORD" \
        psql "$(db_url postgres)" -X -v ON_ERROR_STOP=1 -c "$sql"
    else
      psql "$(db_url postgres)" -X -v ON_ERROR_STOP=1 -c "$sql"
    fi
    return $?
  fi
  if cex_can_use_docker_postgres; then
    cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" \
      psql -U "$CEX_POSTGRES_USER" -d postgres -X -v ON_ERROR_STOP=1 -c "$sql"
    return $?
  fi
  echo "no usable postgres client found" >&2
  return 1
}

run_db_stdin() {
  local database="$1"
  shift
  if cex_has_local_psql; then
    if [[ -n "$RECEIPT_PSQL_PASSWORD" ]]; then
      PGPASSWORD="$RECEIPT_PSQL_PASSWORD" \
        psql "$(db_url "$database")" -X -v ON_ERROR_STOP=1 "$@"
    else
      psql "$(db_url "$database")" -X -v ON_ERROR_STOP=1 "$@"
    fi
    return $?
  fi
  if cex_can_use_docker_postgres; then
    cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" \
      psql -U "$CEX_POSTGRES_USER" -d "$database" -X -v ON_ERROR_STOP=1 "$@"
    return $?
  fi
  echo "no usable postgres client found" >&2
  return 1
}

run_db_file() {
  local database="$1"
  local file="$2"
  run_db_stdin "$database" -f - < "$file"
}

drop_db() {
  local database="$1"
  # PostgreSQL 13+ supports WITH (force).  Retain a fallback for older local
  # images, where no client can remain connected during this short-lived test.
  run_admin_sql "drop database if exists \"$database\" with (force);" >/dev/null 2>&1 || \
    run_admin_sql "drop database if exists \"$database\";" >/dev/null 2>&1 || true
}

created_databases=()
cleanup() {
  local status=$?
  set +e
  local index
  for ((index=${#created_databases[@]}-1; index>=0; index--)); do
    drop_db "${created_databases[index]}"
  done
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

run_admin_sql 'select 1' >/dev/null
drop_db "$base_db"
run_admin_sql "create database \"$base_db\";" >/dev/null
created_databases+=("$base_db")

echo "==> building pre-0085 receipt schema in $base_db"
while IFS= read -r migration; do
  [[ -f "$migration" ]] || continue
  migration_name="$(basename "$migration")"
  migration_number="${migration_name%%_*}"
  if ((10#$migration_number < 85)); then
    echo "    applying $migration_name"
    run_db_file "$base_db" "$migration" >/dev/null
  fi
done < <(find "$PROJECT_ROOT/migrations" -maxdepth 1 -type f \
  -name '[0-9][0-9][0-9][0-9]_*.sql' | sort)

echo "==> seeding legacy (pre-0085) receipt rows"
run_db_stdin "$base_db" <<'SQL'
-- Native receipts: one amount comes from immutable intent evidence, one is an
-- audit-only intent whose legacy evidence supplies the compatibility amount,
-- and one malformed/negative intent proves the fail-closed zero path.
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values
(
    'pu-intent-native-happy', 'term_exchange_protocol_v1', 'pu-scope',
    'native-happy', repeat('a', 64),
    '{"intent_id":"pu-intent-native-happy","protocol_version":"term_exchange_protocol_v1","amount_credits":37,"kind":"reward"}'::jsonb,
    'accepted'
),
(
    'pu-intent-native-fallback', 'term_exchange_protocol_v1', 'pu-scope',
    'native-fallback', repeat('b', 64),
    '{"intent_id":"pu-intent-native-fallback","protocol_version":"term_exchange_protocol_v1","kind":"audit"}'::jsonb,
    'accepted'
),
(
    'pu-intent-native-invalid', 'term_exchange_protocol_v1', 'pu-scope',
    'native-invalid', repeat('c', 64),
    '{"intent_id":"pu-intent-native-invalid","protocol_version":"term_exchange_protocol_v1","amount_credits":-7,"kind":"audit"}'::jsonb,
    'accepted'
);

insert into public.trnm_economic_receipts (
    receipt_id, intent_id, protocol_version, idempotency_scope,
    idempotency_key, progression_class, status, receipt_json, finalized_at
) values
(
    'pu-receipt-native-happy', 'pu-intent-native-happy',
    'term_exchange_protocol_v1', 'pu-scope', 'native-happy',
    'progression_allowed', 'settled',
    -- The legacy evidence deliberately says 99; 0086 must use the immutable
    -- intent amount (37), rewrite the evidence amount, and rebuild its hash.
    '{"intent_id":"pu-intent-native-happy","receipt_id":"pu-receipt-native-happy","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":99}}'::jsonb,
    '2026-01-01T00:00:00Z'
),
(
    'pu-receipt-native-fallback', 'pu-intent-native-fallback',
    'term_exchange_protocol_v1', 'pu-scope', 'native-fallback',
    'progression_allowed', 'settled',
    '{"intent_id":"pu-intent-native-fallback","receipt_id":"pu-receipt-native-fallback","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":9}}'::jsonb,
    '2026-01-01T00:00:01Z'
),
(
    'pu-receipt-native-invalid', 'pu-intent-native-invalid',
    'term_exchange_protocol_v1', 'pu-scope', 'native-invalid',
    'progression_allowed', 'settled',
    '{"intent_id":"pu-intent-native-invalid","receipt_id":"pu-receipt-native-invalid","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":11}}'::jsonb,
    '2026-01-01T00:00:02Z'
);

-- These rows have the exact 0026 shape: there is intentionally no
-- amount_credits column until 0085 adds it.
insert into public.league_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    finalized_at
) values
(
    'pu-league-known', 'term_exchange_protocol_v1', 'pu-intent-league-known',
    'pu-term', 'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-league-known', 'pu-ledger-league-known', null,
    '2026-01-01T00:01:00Z'
),
(
    'pu-league-unknown', 'term_exchange_protocol_v1', 'pu-intent-league-unknown',
    'pu-term', 'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-league-unknown', 'pu-ledger-league-unknown', null,
    '2026-01-01T00:01:01Z'
);

insert into public.world_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    finalized_at
) values
(
    'pu-world-known', 'term_exchange_protocol_v1', 'pu-intent-world-known',
    'pu-term', 'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-world-known', 'pu-ledger-world-known', null,
    '2026-01-01T00:02:00Z'
),
(
    'pu-world-unknown', 'term_exchange_protocol_v1', 'pu-intent-world-unknown',
    'pu-term', 'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-world-unknown', 'pu-ledger-world-unknown', null,
    '2026-01-01T00:02:01Z'
);
SQL

new_case() {
  local label="$1"
  local database="cex_receipt_pu_${label}_${run_token}"
  if [[ ! "$database" =~ ^[A-Za-z_][A-Za-z0-9_]*$ || ${#database} -gt 63 ]]; then
    echo "generated case database name is invalid or too long: $database" >&2
    return 2
  fi
  drop_db "$database"
  run_admin_sql "create database \"$database\" template \"$base_db\";" >/dev/null
  created_databases+=("$database")
  CASE_DB="$database"
}

expect_migration_failure() {
  local database="$1"
  local label="$2"
  local migration="$3"
  local expected_text="$4"
  local output
  local status
  set +e
  output="$(run_db_file "$database" "$migration" 2>&1)"
  status=$?
  set -e
  if ((status == 0)); then
    echo "ERROR: $label unexpectedly accepted a conflicting upgrade" >&2
    return 1
  fi
  if [[ "$output" != *"$expected_text"* ]]; then
    echo "ERROR: $label failed with an unexpected error (wanted '$expected_text')" >&2
    printf '%s\n' "$output" >&2
    return 1
  fi
  echo "    rejected $label (status=$status)"
}

assert_db() {
  local database="$1"
  run_db_stdin "$database"
}

echo "==> happy partial upgrade (including an interrupted 0086 table)"
new_case happy
happy_db="$CASE_DB"

# Also model an 0085 attempt that committed its expand-only columns before the
# constraint/trigger portion ran.  The real migration must treat these columns
# as already present and finish the hardening idempotently.
run_db_stdin "$happy_db" <<'SQL'
alter table public.league_term_exchange_receipts
    add column amount_credits bigint;
alter table public.world_term_exchange_receipts
    add column amount_credits bigint;
SQL
run_db_file "$happy_db" "$MIGRATION_85" >/dev/null

# Simulate a rollout that created the event table and its default-zero amount
# column, but stopped before the 0086 evidence repair and trigger installation.
run_db_stdin "$happy_db" <<'SQL'
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    intent_id text not null,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
) values (
    'pu-intent-native-happy', 1, repeat('a', 64),
    'pu-receipt-native-happy', 'term_exchange_protocol_v1', 'pu-scope',
    'native-happy', 'progression_allowed', 'settled', 0,
    '{"intent_id":"pu-intent-native-happy","receipt_id":"pu-receipt-native-happy","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed"}'::jsonb,
    'stale-partial-hash', 'initial', '2026-01-01T00:00:00Z'
);
SQL

# A known amount on a projection models an application that wrote the new
# nullable column before the history migration; the second row remains the
# legacy unknown amount and must stay NULL/fail-closed.
run_db_stdin "$happy_db" <<'SQL'
alter table public.league_term_exchange_receipts disable trigger all;
alter table public.world_term_exchange_receipts disable trigger all;
update public.league_term_exchange_receipts
   set amount_credits = 23
 where receipt_id = 'pu-league-known';
update public.world_term_exchange_receipts
   set amount_credits = 31
 where receipt_id = 'pu-world-known';
alter table public.league_term_exchange_receipts
    enable always trigger trg_cex_league_term_exchange_receipt_mutation_v1;
alter table public.league_term_exchange_receipts
    enable always trigger trg_cex_league_term_exchange_receipt_truncate_v1;
alter table public.world_term_exchange_receipts
    enable always trigger trg_cex_world_term_exchange_receipt_mutation_v1;
alter table public.world_term_exchange_receipts
    enable always trigger trg_cex_world_term_exchange_receipt_truncate_v1;
SQL

run_db_file "$happy_db" "$MIGRATION_86" >/dev/null
run_db_file "$happy_db" "$MIGRATION_87" >/dev/null

# Re-apply all three migrations to prove an interrupted deployment can be
# retried without duplicate history or evidence rows.
run_db_file "$happy_db" "$MIGRATION_85" >/dev/null
run_db_file "$happy_db" "$MIGRATION_86" >/dev/null
run_db_file "$happy_db" "$MIGRATION_87" >/dev/null

assert_db "$happy_db" <<'SQL'
do $test$
declare
    amount_value bigint;
    evidence_value jsonb;
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 3 then
        raise exception 'native receipt event backfill count mismatch';
    end if;

    select amount_credits, receipt_json -> 'evidence'
      into amount_value, evidence_value
      from public.trnm_economic_receipt_events_v1
     where intent_id = 'pu-intent-native-happy';
    if amount_value is distinct from 37
       or evidence_value #>> '{amount_credits}' is distinct from '37'
       or evidence_value #>> '{payload_hash}' is distinct from repeat('a', 64) then
        raise exception 'immutable native amount/evidence backfill mismatch';
    end if;

    select amount_credits, receipt_json -> 'evidence'
      into amount_value, evidence_value
      from public.trnm_economic_receipt_events_v1
     where intent_id = 'pu-intent-native-fallback';
    if amount_value is distinct from 9
       or evidence_value #>> '{amount_credits}' is distinct from '9'
       or evidence_value #>> '{payload_hash}' is distinct from repeat('b', 64) then
        raise exception 'legacy evidence amount fallback mismatch';
    end if;

    select amount_credits
      into amount_value
      from public.trnm_economic_receipt_events_v1
     where intent_id = 'pu-intent-native-invalid';
    if amount_value is distinct from 0 then
        raise exception 'invalid immutable amount did not fail closed to zero';
    end if;

    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 2
       or (select count(*) from public.world_term_exchange_receipt_events_v1) <> 2 then
        raise exception 'term receipt history backfill count mismatch';
    end if;
    if (select amount_credits from public.league_term_exchange_receipt_events_v1
         where receipt_id = 'pu-league-known' and event_sequence = 1) is distinct from 23
       or (select amount_credits from public.world_term_exchange_receipt_events_v1
         where receipt_id = 'pu-world-known' and event_sequence = 1) is distinct from 31 then
        raise exception 'known projection amount was not copied to history';
    end if;
    if (select amount_credits from public.league_term_exchange_receipt_events_v1
         where receipt_id = 'pu-league-unknown' and event_sequence = 1) is not null
       or (select amount_credits from public.world_term_exchange_receipt_events_v1
         where receipt_id = 'pu-world-unknown' and event_sequence = 1) is not null then
        raise exception 'legacy unknown projection amount was fabricated';
    end if;
    if exists (
        select 1
          from public.league_term_exchange_receipt_events_v1
         where event_sequence <> 1 or event_kind <> 'backfill'
    ) or exists (
        select 1
          from public.world_term_exchange_receipt_events_v1
         where event_sequence <> 1 or event_kind <> 'backfill'
    ) then
        raise exception 'history backfill sequence/kind mismatch';
    end if;
    if exists (
        select 1
          from public.trnm_economic_receipt_events_v1
         where receipt_hash <> encode(digest(receipt_json::text, 'sha256'), 'hex')
    ) or exists (
        select 1
          from public.league_term_exchange_receipt_events_v1
         where receipt_hash <> 'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
    ) or exists (
        select 1
          from public.world_term_exchange_receipt_events_v1
         where receipt_hash <> 'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
    ) then
        raise exception 'receipt hash backfill mismatch';
    end if;
end
$test$;

do $mutation$
declare
    rejected boolean := false;
begin
    begin
        update public.trnm_economic_receipt_events_v1
           set status = 'tampered'
         where intent_id = 'pu-intent-native-happy';
    exception when others then
        rejected := position('append-only' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'native event mutation guard did not reject update';
    end if;

    rejected := false;
    begin
        delete from public.league_term_exchange_receipt_events_v1
         where receipt_id = 'pu-league-known';
    exception when others then
        rejected := position('append-only' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'term history mutation guard did not reject delete';
    end if;
end
$mutation$;
SQL
echo "    happy partial upgrade passed"

echo "==> conflict: native evidence binding"
new_case native_evidence
evidence_db="$CASE_DB"
run_db_file "$evidence_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$evidence_db" <<'SQL'
update public.trnm_economic_receipts
   set receipt_json = jsonb_set(
       receipt_json,
       '{evidence}',
       jsonb_build_object(
           'payload_hash', repeat('d', 64),
           'amount_credits', 37
       ),
       true
   )
 where intent_id = 'pu-intent-native-happy';
SQL
expect_migration_failure "$evidence_db" "native evidence payload-hash conflict" "$MIGRATION_86" \
  'TRNM receipt event JSON binding mismatch'
assert_db "$evidence_db" <<'SQL'
do $check$
begin
    if to_regclass('public.trnm_economic_receipt_events_v1') is not null then
        raise exception 'rejected 0086 left a native history table behind';
    end if;
end
$check$;
SQL
echo "    native evidence conflict passed"

echo "==> conflict: native receipt_id fork in partial history"
new_case native_id
native_id_db="$CASE_DB"
run_db_file "$native_id_db" "$MIGRATION_85" >/dev/null
run_db_file "$native_id_db" "$MIGRATION_86" >/dev/null
run_db_stdin "$native_id_db" <<'SQL'
alter table public.trnm_economic_receipt_events_v1
    disable trigger trg_cex_validate_trnm_economic_receipt_event_v1;
with payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-happy',
        'receipt_id', 'pu-receipt-native-fork',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'held_review',
        'progression_class', 'recoverable_hold',
        'evidence', jsonb_build_object(
            'payload_hash', repeat('a', 64),
            'amount_credits', 37
        )
    ) as receipt_json
)
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
)
select 'pu-intent-native-happy', 2, repeat('a', 64), 'pu-receipt-native-fork',
       'term_exchange_protocol_v1', 'pu-scope', 'native-happy',
       'recoverable_hold', 'held_review', 37, receipt_json,
       encode(digest(receipt_json::text, 'sha256'), 'hex'),
       'recoverable_hold_retry', '2026-01-01T00:00:03Z'
  from payload;
alter table public.trnm_economic_receipt_events_v1
    enable always trigger trg_cex_validate_trnm_economic_receipt_event_v1;
SQL
expect_migration_failure "$native_id_db" "native receipt_id fork" "$MIGRATION_86" \
  'TRNM native receipt history contains multiple receipt_ids'
assert_db "$native_id_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 4 then
        raise exception 'native receipt_id conflict changed partial history during rollback';
    end if;
end
$check$;
SQL
echo "    native receipt_id conflict passed"

echo "==> conflict: immutable amount versus normalized projection"
new_case amount
amount_db="$CASE_DB"
run_db_stdin "$amount_db" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-amount-conflict', 'term_exchange_protocol_v1', 'pu-scope',
    'amount-conflict', repeat('e', 64),
    '{"intent_id":"pu-intent-amount-conflict","protocol_version":"term_exchange_protocol_v1","amount_credits":41}'::jsonb,
    'accepted'
);
insert into public.league_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    finalized_at
) values (
    'pu-league-amount-conflict', 'term_exchange_protocol_v1',
    'pu-intent-amount-conflict', 'pu-term', 'pu-backend', 'cex', 'settled',
    'progression_allowed', 'pu-settle-amount-conflict', 'pu-ledger-amount-conflict',
    null, '2026-01-01T00:03:00Z'
);
SQL
run_db_file "$amount_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$amount_db" <<'SQL'
alter table public.league_term_exchange_receipts disable trigger all;
update public.league_term_exchange_receipts
   set amount_credits = 99
 where receipt_id = 'pu-league-amount-conflict';
SQL
expect_migration_failure "$amount_db" "immutable amount/projection conflict" "$MIGRATION_87" \
  'normalized receipt amount does not match immutable intent amount'
assert_db "$amount_db" <<'SQL'
do $check$
begin
    if to_regclass('public.league_term_exchange_receipt_events_v1') is not null then
        raise exception 'rejected amount conflict left term history behind';
    end if;
end
$check$;
SQL
echo "    immutable amount conflict passed"

echo "==> conflict: term receipt history receipt_id fork"
new_case history
history_db="$CASE_DB"
run_db_file "$history_db" "$MIGRATION_85" >/dev/null
run_db_file "$history_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$history_db" <<'SQL'
alter table public.world_term_exchange_receipt_events_v1
    disable trigger trg_cex_validate_world_term_exchange_receipt_event_v1;
with predecessor as (
    select receipt_hash
      from public.world_term_exchange_receipt_events_v1
     where receipt_id = 'pu-world-unknown'
       and event_sequence = 1
), payload as (
    select jsonb_build_object(
        'protocol_version', 'term_exchange_protocol_v1',
        'receipt_id', 'pu-world-history-fork',
        'intent_id', 'pu-intent-world-unknown',
        'term_id', 'pu-term',
        'backend_id', 'pu-backend',
        'backend_kind', 'cex',
        'status', 'held_review',
        'progression_class', 'recoverable_hold',
        'settlement_reference', null,
        'ledger_entry_id', null,
        'reason', null,
        'amount_credits', null,
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:03:01Z')::bigint
    ) as receipt_json,
    predecessor.receipt_hash
      from predecessor
)
insert into public.world_term_exchange_receipt_events_v1 (
    receipt_id, event_sequence, event_kind, previous_receipt_hash,
    protocol_version, intent_id, term_id, backend_id, backend_kind, status,
    progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at, receipt_json, receipt_hash
)
select 'pu-world-history-fork', 2, 'transition', receipt_hash,
       'term_exchange_protocol_v1', 'pu-intent-world-unknown', 'pu-term',
       'pu-backend', 'cex', 'held_review', 'recoverable_hold', null, null,
       null, null, '2026-01-01T00:03:01Z', receipt_json,
       'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
  from payload;
alter table public.world_term_exchange_receipt_events_v1
    enable always trigger trg_cex_validate_world_term_exchange_receipt_event_v1;
SQL
expect_migration_failure "$history_db" "world receipt history receipt_id fork" "$MIGRATION_87" \
  'normalized world receipt history contains multiple receipt_ids'
assert_db "$history_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.world_term_exchange_receipt_events_v1) <> 3 then
        raise exception 'world history receipt_id conflict changed partial history during rollback';
    end if;
end
$check$;
SQL
echo "    term history conflict passed"

echo "==> conflict: projection/history identity and amount"
new_case projection
projection_db="$CASE_DB"
run_db_file "$projection_db" "$MIGRATION_85" >/dev/null
run_db_file "$projection_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$projection_db" <<'SQL'
alter table public.league_term_exchange_receipts disable trigger all;
update public.league_term_exchange_receipts
   set term_id = 'pu-tampered-term', amount_credits = 999
 where receipt_id = 'pu-league-known';
SQL
expect_migration_failure "$projection_db" "league projection/history conflict" "$MIGRATION_87" \
  'normalized league receipt history/projection identity or amount conflict'
assert_db "$projection_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 2 then
        raise exception 'projection conflict changed existing history during rollback';
    end if;
    if (select term_id from public.league_term_exchange_receipts
         where receipt_id = 'pu-league-known') is distinct from 'pu-tampered-term'
       or (select amount_credits from public.league_term_exchange_receipts
         where receipt_id = 'pu-league-known') is distinct from 999 then
        raise exception 'projection conflict fixture was unexpectedly rewritten';
    end if;
end
$check$;
SQL
echo "    projection conflict passed"

echo "receipt partial-upgrade regression passed"

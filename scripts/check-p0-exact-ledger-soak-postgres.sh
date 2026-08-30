#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
source "$root/scripts/_dev-helpers.sh"
: "${DATABASE_URL:?DATABASE_URL is required}"
cex_load_env
cex_sync_postgres_env_from_database_url "$DATABASE_URL"
evidence_dir="${CEX_P0_EVIDENCE_DIR:-$root/run/p0-release-evidence}"
# The standalone producer must satisfy the strict release-evidence contract
# even when the aggregate workflow does not inject an override.
iterations="${CEX_P0_SOAK_ITERATIONS:-250}"

if ! [[ "$iterations" =~ ^[1-9][0-9]*$ ]] || (( iterations > 5000 )); then
  echo "CEX_P0_SOAK_ITERATIONS must be an integer in 1..5000" >&2
  exit 64
fi

started_at_epoch=$(date +%s)

cex_psql_stdin -X -v iterations="$iterations" <<'SQL'
begin;
select set_config('cex.p0_soak_iterations', :'iterations', true);

insert into public.organizations (org_id, name)
values ('90000000-0000-4000-8000-000000000001', 'P0 exact Ledger soak org')
on conflict (org_id) do nothing;

select public.cex_open_account_v2(
    '90000000-0000-4000-8000-000000000101'::uuid,
    '90000000-0000-4000-8000-000000000001'::uuid,
    '90000000-0000-4000-8000-000000000201'::uuid,
    'p0-soak',
    'credit',
    6::smallint,
    1000000000::bigint,
    'p0-soak:account',
    'open-v1',
    'p0-release-candidate'
);

do $soak$
declare
    iteration integer;
    first_result jsonb;
    replay_result jsonb;
    account_id_value constant uuid := '90000000-0000-4000-8000-000000000101';
    expected_iterations integer := current_setting('cex.p0_soak_iterations')::integer;
    balance_value bigint;
    reserved_value bigint;
    entry_count bigint;
    operation_count bigint;
    compatibility_count bigint;
    audit_count bigint;
begin
    for iteration in 1..expected_iterations loop
        first_result := public.cex_apply_ledger_effect_v1(
            account_id_value,
            public.cex_deterministic_uuid_v1('p0-soak:trace:grant:' || iteration::text),
            public.cex_deterministic_uuid_v1('p0-soak:operation:grant:' || iteration::text),
            'grant',
            1000::bigint,
            6::smallint,
            'p0_soak',
            public.cex_deterministic_uuid_v1('p0-soak:reference:grant:' || iteration::text),
            'p0-soak:grant:' || iteration::text,
            'apply',
            'ledger-service',
            'p0-release-candidate',
            'explicit'
        );
        replay_result := public.cex_apply_ledger_effect_v1(
            account_id_value,
            public.cex_deterministic_uuid_v1('p0-soak:trace:grant:' || iteration::text),
            public.cex_deterministic_uuid_v1('p0-soak:operation:grant:' || iteration::text),
            'grant',
            1000::bigint,
            6::smallint,
            'p0_soak',
            public.cex_deterministic_uuid_v1('p0-soak:reference:grant:' || iteration::text),
            'p0-soak:grant:' || iteration::text,
            'apply',
            'ledger-service',
            'p0-release-candidate',
            'explicit'
        );
        if first_result ->> 'replayed' <> 'false'
           or replay_result ->> 'replayed' <> 'true'
           or first_result #>> '{effect,entry_id}'
              is distinct from replay_result #>> '{effect,entry_id}' then
            raise exception 'grant exact replay failed at iteration %', iteration;
        end if;

        perform public.cex_apply_ledger_effect_v1(
            account_id_value,
            public.cex_deterministic_uuid_v1('p0-soak:trace:reserve:' || iteration::text),
            public.cex_deterministic_uuid_v1('p0-soak:operation:reserve:' || iteration::text),
            'reserve',
            100::bigint,
            6::smallint,
            'p0_soak',
            public.cex_deterministic_uuid_v1('p0-soak:reference:reserve:' || iteration::text),
            'p0-soak:reserve:' || iteration::text,
            'apply',
            'ledger-service',
            'p0-release-candidate',
            'explicit'
        );
        perform public.cex_apply_ledger_effect_v1(
            account_id_value,
            public.cex_deterministic_uuid_v1('p0-soak:trace:refund:' || iteration::text),
            public.cex_deterministic_uuid_v1('p0-soak:operation:refund:' || iteration::text),
            'refund',
            100::bigint,
            6::smallint,
            'p0_soak',
            public.cex_deterministic_uuid_v1('p0-soak:reference:refund:' || iteration::text),
            'p0-soak:refund:' || iteration::text,
            'apply',
            'ledger-service',
            'p0-release-candidate',
            'explicit'
        );
    end loop;

    select balance_minor, reserved_minor
      into balance_value, reserved_value
      from public.accounts
     where account_id=account_id_value;

    select count(*)::bigint,
           count(distinct operation_id)::bigint,
           count(*) filter (where provenance_mode <> 'explicit')::bigint
      into entry_count, operation_count, compatibility_count
      from public.ledger_entries
     where account_id=account_id_value;

    select count(*)::bigint
      into audit_count
      from public.cex_audit_outbox_v1
     where source_service='ledger-service'
       and envelope ->> 'event_type'='ledger.effect.persisted'
       and envelope #>> '{payload,account_id}'=account_id_value::text;

    if balance_value <> 1000000000 + expected_iterations::bigint * 1000
       or reserved_value <> 0
       or entry_count <> 1 + expected_iterations::bigint * 3
       or operation_count <> entry_count
       or compatibility_count <> 0
       or audit_count <> entry_count then
        raise exception using message=format(
            'P0 exact soak invariant failed balance=%s reserved=%s entries=%s operations=%s compatibility=%s audit=%s',
            balance_value,reserved_value,entry_count,operation_count,compatibility_count,audit_count
        );
    end if;
end
$soak$;

commit;
SQL

ended_at_epoch=$(date +%s)
tree_sha=$(git -C "$root" rev-parse 'HEAD^{tree}')
raw_json=$(cex_psql_stdin -X -Atc "
select json_build_object(
  'schema','cex.p0-exact-ledger-soak.v1',
  'ok',true,
  'iterations',$iterations,
  'account_id','90000000-0000-4000-8000-000000000101',
  'balance_minor',balance_minor,
  'reserved_minor',reserved_minor,
  'ledger_entry_count',(select count(*) from public.ledger_entries where account_id=accounts.account_id),
  'distinct_operation_count',(select count(distinct operation_id) from public.ledger_entries where account_id=accounts.account_id),
  'compatibility_entry_count',(select count(*) from public.ledger_entries where account_id=accounts.account_id and provenance_mode <> 'explicit'),
  'audit_effect_count',(select count(*) from public.cex_audit_outbox_v1 where source_service='ledger-service' and envelope ->> 'event_type'='ledger.effect.persisted' and envelope #>> '{payload,account_id}'=accounts.account_id::text)
) from public.accounts where account_id='90000000-0000-4000-8000-000000000101';")

python3 - "$evidence_dir/exact-ledger-soak.json" "$raw_json" "$started_at_epoch" "$ended_at_epoch" "${GITHUB_SHA:-unknown}" "$tree_sha" "$root" <<'PY'
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(sys.argv[7]) / "scripts"))
from evidence_safe_io import write_json_nofollow

path = Path(sys.argv[1])
data = json.loads(sys.argv[2])
data.update({
    "started_at_epoch": int(sys.argv[3]),
    "ended_at_epoch": int(sys.argv[4]),
    "duration_seconds": int(sys.argv[4]) - int(sys.argv[3]),
    "commit_sha": sys.argv[5],
    "tree_sha": sys.argv[6],
})
write_json_nofollow(path, data)
PY

echo "P0 exact Ledger soak passed: $evidence_dir/exact-ledger-soak.json"

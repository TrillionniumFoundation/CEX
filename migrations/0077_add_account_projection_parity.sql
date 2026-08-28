begin;

-- P0-N3: exact projection rebuild, shadow comparison, authoritative-read gate and race-safe repair evidence.

create table if not exists public.cex_account_projection_runs_v1 (
    run_id uuid primary key,
    org_id uuid not null references public.organizations(org_id),
    status text not null default 'completed',
    account_count bigint not null,
    match_count bigint not null,
    mismatch_count bigint not null,
    unknown_provenance_count bigint not null,
    corrupt_account_count bigint not null,
    captured_by text not null,
    captured_at timestamptz not null default now(),
    constraint cex_account_projection_run_status_v1 check (status = 'completed'),
    constraint cex_account_projection_counts_v1 check (
        account_count >= 0 and match_count >= 0 and mismatch_count >= 0
        and unknown_provenance_count >= 0 and corrupt_account_count >= 0
    )
);

create table if not exists public.cex_account_projection_items_v1 (
    run_id uuid not null references public.cex_account_projection_runs_v1(run_id),
    account_id uuid not null references public.accounts(account_id),
    org_id uuid not null,
    summary_balance_minor bigint not null,
    summary_reserved_minor bigint not null,
    rebuilt_balance_minor bigint not null,
    rebuilt_reserved_minor bigint not null,
    balance_delta_minor bigint not null,
    reserved_delta_minor bigint not null,
    exact_entry_count bigint not null,
    unknown_entry_count bigint not null,
    parity_status text not null,
    captured_at timestamptz not null default now(),
    primary key (run_id, account_id),
    constraint cex_account_projection_item_counts_v1 check (
        exact_entry_count >= 0 and unknown_entry_count >= 0
    ),
    constraint cex_account_projection_item_status_v1 check (
        parity_status in ('match', 'mismatch', 'unknown_provenance', 'corrupt_projection')
    )
);

create table if not exists public.cex_money_read_policies_v1 (
    org_id uuid primary key references public.organizations(org_id),
    mode text not null default 'shadow',
    min_coverage_ratio numeric(8,6) not null default 1.000000,
    max_capture_age_seconds integer not null default 300,
    updated_by text not null,
    updated_at timestamptz not null default now(),
    constraint cex_money_read_policy_mode_v1 check (mode in ('legacy_v1','shadow','require_v2')),
    constraint cex_money_read_policy_slo_v1 check (
        min_coverage_ratio between 0 and 1
        and max_capture_age_seconds between 1 and 86400
    )
);

create table if not exists public.cex_account_projection_repairs_v1 (
    repair_id uuid primary key,
    run_id uuid not null references public.cex_account_projection_runs_v1(run_id),
    account_id uuid not null references public.accounts(account_id),
    org_id uuid not null,
    old_balance_minor bigint not null,
    old_reserved_minor bigint not null,
    new_balance_minor bigint not null,
    new_reserved_minor bigint not null,
    repaired_by text not null,
    reason text not null,
    evidence jsonb not null,
    repaired_at timestamptz not null default now(),
    constraint cex_account_projection_repair_text_v1 check (
        length(btrim(repaired_by)) between 1 and 256
        and length(btrim(reason)) between 1 and 1000
        and jsonb_typeof(evidence) = 'object'
    )
);

create or replace function public.cex_capture_account_projection_v1(
    p_org_id uuid,
    p_captured_by text
)
returns public.cex_account_projection_runs_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    run_id_value uuid := gen_random_uuid();
    run_row public.cex_account_projection_runs_v1%rowtype;
begin
    if p_org_id is null or p_org_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'projection org_id must be non-nil';
    end if;
    if p_captured_by is null or length(btrim(p_captured_by)) not between 1 and 256 then
        raise exception 'projection capturer must contain 1..256 characters';
    end if;

    with entry_projection as (
        select
            a.account_id,
            a.org_id,
            a.balance_minor as summary_balance_minor,
            a.reserved_minor as summary_reserved_minor,
            coalesce(sum(case
                when e.provenance_mode = 'explicit' and e.operation_kind in ('genesis','grant')
                    then e.amount_minor
                when e.provenance_mode = 'explicit' and e.operation_kind = 'consume'
                    then -e.amount_minor
                else 0 end), 0)::bigint as rebuilt_balance_minor,
            coalesce(sum(case
                when e.provenance_mode = 'explicit' and e.operation_kind = 'reserve'
                    then e.amount_minor
                when e.provenance_mode = 'explicit' and e.operation_kind in ('consume','refund')
                    then -e.amount_minor
                else 0 end), 0)::bigint as rebuilt_reserved_minor,
            count(e.entry_id) filter (where e.provenance_mode = 'explicit')::bigint as exact_count,
            count(e.entry_id) filter (
                where e.provenance_mode <> 'explicit' or e.operation_kind = 'legacy_entry'
            )::bigint as unknown_count
        from public.accounts a
        left join public.ledger_entries e using (account_id)
        where a.org_id = p_org_id
        group by a.account_id, a.org_id, a.balance_minor, a.reserved_minor
    ), classified as (
        select *,
            rebuilt_balance_minor - summary_balance_minor as balance_delta,
            rebuilt_reserved_minor - summary_reserved_minor as reserved_delta,
            case
                when rebuilt_balance_minor < 0 or rebuilt_reserved_minor < 0
                  or rebuilt_reserved_minor > rebuilt_balance_minor then 'corrupt_projection'
                when unknown_count > 0 then 'unknown_provenance'
                when rebuilt_balance_minor = summary_balance_minor
                 and rebuilt_reserved_minor = summary_reserved_minor then 'match'
                else 'mismatch'
            end as parity_status
        from entry_projection
    )
    insert into public.cex_account_projection_items_v1 (
        run_id, account_id, org_id, summary_balance_minor, summary_reserved_minor,
        rebuilt_balance_minor, rebuilt_reserved_minor, balance_delta_minor,
        reserved_delta_minor, exact_entry_count, unknown_entry_count, parity_status
    )
    select run_id_value, account_id, org_id, summary_balance_minor, summary_reserved_minor,
           rebuilt_balance_minor, rebuilt_reserved_minor, balance_delta, reserved_delta,
           exact_count, unknown_count, parity_status
      from classified;

    insert into public.cex_account_projection_runs_v1 (
        run_id, org_id, account_count, match_count, mismatch_count,
        unknown_provenance_count, corrupt_account_count, captured_by
    )
    select
        run_id_value,
        p_org_id,
        count(*)::bigint,
        count(*) filter (where parity_status = 'match')::bigint,
        count(*) filter (where parity_status = 'mismatch')::bigint,
        coalesce(sum(unknown_entry_count), 0)::bigint,
        count(*) filter (where parity_status = 'corrupt_projection')::bigint,
        btrim(p_captured_by)
    from public.cex_account_projection_items_v1
    where run_id = run_id_value
    returning * into run_row;

    return run_row;
end
$$;

create or replace function public.cex_set_money_read_policy_v1(
    p_org_id uuid,
    p_mode text,
    p_min_coverage_ratio numeric,
    p_max_capture_age_seconds integer,
    p_updated_by text
)
returns public.cex_money_read_policies_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare policy_row public.cex_money_read_policies_v1%rowtype;
begin
    if p_mode not in ('legacy_v1','shadow','require_v2') then
        raise exception 'unsupported money read mode';
    end if;
    if p_min_coverage_ratio not between 0 and 1 then
        raise exception 'coverage ratio must be between 0 and 1';
    end if;
    if p_max_capture_age_seconds not between 1 and 86400 then
        raise exception 'capture age must be between 1 and 86400 seconds';
    end if;
    if p_updated_by is null or length(btrim(p_updated_by)) not between 1 and 256 then
        raise exception 'policy actor is invalid';
    end if;
    insert into public.cex_money_read_policies_v1 (
        org_id, mode, min_coverage_ratio, max_capture_age_seconds, updated_by
    ) values (
        p_org_id, p_mode, p_min_coverage_ratio, p_max_capture_age_seconds, btrim(p_updated_by)
    ) on conflict (org_id) do update set
        mode = excluded.mode,
        min_coverage_ratio = excluded.min_coverage_ratio,
        max_capture_age_seconds = excluded.max_capture_age_seconds,
        updated_by = excluded.updated_by,
        updated_at = now()
    returning * into policy_row;
    return policy_row;
end
$$;

create or replace view public.cex_money_v2_org_status_v1 as
select
    org.org_id,
    coalesce(policy.mode, 'shadow') as mode,
    inventory.run_id as inventory_run_id,
    inventory.sealed_at as inventory_sealed_at,
    inventory.unknown_provenance_count as inventory_unknown_provenance_count,
    inventory.corrupt_account_count as inventory_corrupt_account_count,
    projection.run_id as projection_run_id,
    projection.captured_at,
    projection.account_count,
    projection.match_count,
    projection.mismatch_count,
    projection.unknown_provenance_count,
    projection.corrupt_account_count,
    case when projection.account_count > 0
        then projection.match_count::numeric / projection.account_count::numeric
        else 1::numeric end as coverage_ratio,
    case
        when inventory.run_id is null then 'signed_inventory_missing'
        when inventory.fabricated_history then 'fabricated_history_forbidden'
        when inventory.unknown_provenance_count > 0 then 'inventory_unknown_provenance'
        when inventory.corrupt_account_count > 0 then 'inventory_corrupt_accounts'
        when projection.run_id is null then 'projection_capture_missing'
        when projection.captured_at < now() - make_interval(
            secs => coalesce(policy.max_capture_age_seconds, 300)
        ) then 'projection_capture_stale'
        when projection.unknown_provenance_count > 0 then 'projection_unknown_provenance'
        when projection.corrupt_account_count > 0 then 'projection_corrupt_accounts'
        when projection.mismatch_count > 0 then 'projection_mismatch'
        when projection.account_count > 0 and
             projection.match_count::numeric / projection.account_count::numeric
             < coalesce(policy.min_coverage_ratio, 1) then 'projection_coverage_below_slo'
        else null
    end as stop_reason,
    case
        when inventory.run_id is not null
         and not inventory.fabricated_history
         and inventory.unknown_provenance_count = 0
         and inventory.corrupt_account_count = 0
         and projection.run_id is not null
         and projection.captured_at >= now() - make_interval(
             secs => coalesce(policy.max_capture_age_seconds, 300)
         )
         and projection.unknown_provenance_count = 0
         and projection.corrupt_account_count = 0
         and projection.mismatch_count = 0
         and (projection.account_count = 0 or
             projection.match_count::numeric / projection.account_count::numeric
             >= coalesce(policy.min_coverage_ratio, 1))
        then true else false end as eligible
from public.organizations org
left join public.cex_money_read_policies_v1 policy using (org_id)
left join lateral (
    select * from public.cex_account_opening_inventory_runs_v1 r
    where r.org_id = org.org_id and r.status = 'sealed'
    order by r.sealed_at desc, r.run_id desc limit 1
) inventory on true
left join lateral (
    select * from public.cex_account_projection_runs_v1 r
    where r.org_id = org.org_id and r.status = 'completed'
    order by r.captured_at desc, r.run_id desc limit 1
) projection on true;

create or replace function public.cex_read_account_money_v2(
    p_account_id uuid
)
returns jsonb
language plpgsql
stable
set search_path = pg_catalog, public
as $$
declare
    account_row public.accounts%rowtype;
    policy_row public.cex_money_read_policies_v1%rowtype;
    status_row record;
    item_row public.cex_account_projection_items_v1%rowtype;
    mode_value text;
begin
    select * into account_row from public.accounts where account_id = p_account_id;
    if not found then
        raise exception using errcode = 'P0002', message = 'account not found';
    end if;
    select * into policy_row from public.cex_money_read_policies_v1
     where org_id = account_row.org_id;
    mode_value := coalesce(policy_row.mode, 'shadow');
    select * into status_row from public.cex_money_v2_org_status_v1
     where org_id = account_row.org_id;
    if status_row.projection_run_id is not null then
        select * into item_row from public.cex_account_projection_items_v1
         where run_id = status_row.projection_run_id and account_id = p_account_id;
    end if;
    if mode_value = 'require_v2' and not coalesce(status_row.eligible, false) then
        raise exception 'Money v2 authoritative read is blocked: %', coalesce(status_row.stop_reason, 'unknown');
    end if;
    if mode_value = 'require_v2' and item_row.account_id is null then
        raise exception 'Money v2 authoritative read lacks account projection evidence';
    end if;
    return jsonb_build_object(
        'schema_version', 'cex.account.money.v2',
        'mode', mode_value,
        'eligible', coalesce(status_row.eligible, false),
        'stop_reason', status_row.stop_reason,
        'account_id', account_row.account_id,
        'org_id', account_row.org_id,
        'currency_unit', account_row.currency_unit,
        'currency_scale', account_row.currency_scale,
        'balance_minor', case when mode_value = 'require_v2'
            then item_row.rebuilt_balance_minor::text else account_row.balance_minor::text end,
        'reserved_minor', case when mode_value = 'require_v2'
            then item_row.rebuilt_reserved_minor::text else account_row.reserved_minor::text end,
        'shadow_projection', case when item_row.account_id is null then null else jsonb_build_object(
            'run_id', item_row.run_id,
            'rebuilt_balance_minor', item_row.rebuilt_balance_minor::text,
            'rebuilt_reserved_minor', item_row.rebuilt_reserved_minor::text,
            'balance_delta_minor', item_row.balance_delta_minor::text,
            'reserved_delta_minor', item_row.reserved_delta_minor::text,
            'unknown_entry_count', item_row.unknown_entry_count,
            'parity_status', item_row.parity_status
        ) end
    );
end
$$;

create or replace function public.cex_repair_account_projection_v1(
    p_run_id uuid,
    p_account_id uuid,
    p_repaired_by text,
    p_reason text
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    account_row public.accounts%rowtype;
    item_row public.cex_account_projection_items_v1%rowtype;
    repair_id_value uuid;
    event_id_value uuid;
    envelope_value jsonb;
begin
    if p_repaired_by is null or length(btrim(p_repaired_by)) not between 1 and 256 then
        raise exception 'repair actor is invalid';
    end if;
    if p_reason is null or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'repair reason is invalid';
    end if;
    select * into item_row from public.cex_account_projection_items_v1
     where run_id = p_run_id and account_id = p_account_id;
    if not found then raise exception using errcode='P0002', message='projection item not found'; end if;
    if item_row.unknown_entry_count <> 0 or item_row.parity_status <> 'mismatch' then
        raise exception 'only deterministic mismatch items with zero unknown provenance can be repaired';
    end if;
    select * into account_row from public.accounts where account_id = p_account_id for update;
    if account_row.balance_minor is distinct from item_row.summary_balance_minor
       or account_row.reserved_minor is distinct from item_row.summary_reserved_minor then
        raise exception 'account summary changed after projection capture';
    end if;
    if item_row.rebuilt_balance_minor < 0 or item_row.rebuilt_reserved_minor < 0
       or item_row.rebuilt_reserved_minor > item_row.rebuilt_balance_minor then
        raise exception 'rebuilt projection is unsafe';
    end if;
    repair_id_value := public.cex_deterministic_uuid_v1(
        'account-projection-repair:' || p_run_id::text || ':' || p_account_id::text
    );
    if exists (select 1 from public.cex_account_projection_repairs_v1 where repair_id=repair_id_value) then
        return (select evidence from public.cex_account_projection_repairs_v1 where repair_id=repair_id_value);
    end if;
    update public.accounts set
        balance_minor = item_row.rebuilt_balance_minor,
        reserved_minor = item_row.rebuilt_reserved_minor
    where account_id = p_account_id;
    insert into public.cex_account_projection_repairs_v1 (
        repair_id, run_id, account_id, org_id, old_balance_minor, old_reserved_minor,
        new_balance_minor, new_reserved_minor, repaired_by, reason, evidence
    ) values (
        repair_id_value, p_run_id, p_account_id, account_row.org_id,
        account_row.balance_minor, account_row.reserved_minor,
        item_row.rebuilt_balance_minor, item_row.rebuilt_reserved_minor,
        btrim(p_repaired_by), btrim(p_reason), jsonb_build_object(
            'repair_id', repair_id_value,
            'run_id', p_run_id,
            'account_id', p_account_id,
            'unknown_entry_count', 0,
            'captured_summary_unchanged', true,
            'ledger_history_mutated', false
        )
    );
    event_id_value := public.cex_deterministic_uuid_v1('account-projection-repaired:' || repair_id_value::text);
    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', repair_id_value,
        'org_id', account_row.org_id,
        'actor_type', 'operator',
        'actor_id', btrim(p_repaired_by),
        'event_type', 'ledger.account_projection.repaired',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', now(),
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'ledger-service',
            'repair_id', repair_id_value,
            'run_id', p_run_id,
            'account_id', p_account_id,
            'old_balance_minor', account_row.balance_minor::text,
            'old_reserved_minor', account_row.reserved_minor::text,
            'new_balance_minor', item_row.rebuilt_balance_minor::text,
            'new_reserved_minor', item_row.rebuilt_reserved_minor::text,
            'reason', btrim(p_reason)
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'ledger-service', event_id_value, repair_id_value, account_row.org_id, envelope_value, 10
    );
    return jsonb_build_object('repair_id', repair_id_value, 'replayed', false);
end
$$;

create or replace function public.cex_reject_account_projection_evidence_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'account projection evidence is append-only';
end
$$;

drop trigger if exists trg_cex_reject_projection_run_mutation_v1 on public.cex_account_projection_runs_v1;
create trigger trg_cex_reject_projection_run_mutation_v1
before update or delete on public.cex_account_projection_runs_v1
for each row execute function public.cex_reject_account_projection_evidence_mutation_v1();
drop trigger if exists trg_cex_reject_projection_item_mutation_v1 on public.cex_account_projection_items_v1;
create trigger trg_cex_reject_projection_item_mutation_v1
before update or delete on public.cex_account_projection_items_v1
for each row execute function public.cex_reject_account_projection_evidence_mutation_v1();
drop trigger if exists trg_cex_reject_projection_repair_mutation_v1 on public.cex_account_projection_repairs_v1;
create trigger trg_cex_reject_projection_repair_mutation_v1
before update or delete on public.cex_account_projection_repairs_v1
for each row execute function public.cex_reject_account_projection_evidence_mutation_v1();

create index if not exists idx_cex_projection_org_v1
    on public.cex_account_projection_runs_v1 (org_id, captured_at desc, run_id);

commit;

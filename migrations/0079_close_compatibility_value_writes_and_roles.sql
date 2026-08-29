begin;

-- P0 production posture: new value writes are explicit-only and database privileges follow command boundaries.

create or replace function public.cex_reject_new_compatibility_ledger_writes_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.provenance_mode <> 'explicit'
       and coalesce(current_setting('cex.compatibility_value_breakglass', true), '') <> 'enabled' then
        raise exception 'new compatibility/legacy value writes are disabled; use exact Ledger v2';
    end if;
    return new;
end
$$;

drop trigger if exists trg_cex_reject_new_compatibility_ledger_writes_v1 on public.ledger_entries;
create trigger trg_cex_reject_new_compatibility_ledger_writes_v1
before insert on public.ledger_entries
for each row execute function public.cex_reject_new_compatibility_ledger_writes_v1();

create table if not exists public.cex_compatibility_breakglass_evidence_v1 (
    evidence_id uuid primary key,
    actor text not null,
    reason text not null,
    expires_at timestamptz not null,
    created_at timestamptz not null default now(),
    constraint cex_compatibility_breakglass_text_v1 check (
        length(btrim(actor)) between 1 and 256 and length(btrim(reason)) between 1 and 1000
    ),
    constraint cex_compatibility_breakglass_expiry_v1 check (expires_at > created_at)
);

create or replace function public.cex_record_compatibility_breakglass_v1(
    p_actor text, p_reason text, p_lifetime_seconds integer default 300
)
returns public.cex_compatibility_breakglass_evidence_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare evidence_row public.cex_compatibility_breakglass_evidence_v1%rowtype;
begin
    if p_lifetime_seconds not between 1 and 900 then raise exception 'breakglass lifetime invalid'; end if;
    if length(btrim(p_actor)) not between 1 and 256 or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'breakglass actor/reason invalid';
    end if;
    insert into public.cex_compatibility_breakglass_evidence_v1 (
        evidence_id, actor, reason, expires_at
    ) values (gen_random_uuid(),btrim(p_actor),btrim(p_reason),now()+make_interval(secs=>p_lifetime_seconds))
    returning * into evidence_row;
    return evidence_row;
end
$$;

do $$
begin
    if not exists (select 1 from pg_roles where rolname='cex_ledger_opening_source') then
        create role cex_ledger_opening_source nologin;
    end if;
    if not exists (select 1 from pg_roles where rolname='cex_projection_operator') then
        create role cex_projection_operator nologin;
    end if;
    if not exists (select 1 from pg_roles where rolname='cex_provider_dispatch_source') then
        create role cex_provider_dispatch_source nologin;
    end if;
    if not exists (select 1 from pg_roles where rolname='cex_provider_dispatch_worker') then
        create role cex_provider_dispatch_worker nologin;
    end if;
    if not exists (select 1 from pg_roles where rolname='cex_release_evidence_admitter') then
        create role cex_release_evidence_admitter nologin;
    end if;
end
$$;

revoke all on public.accounts, public.ledger_entries,
    public.cex_account_opening_contracts_v1,
    public.cex_account_opening_inventory_runs_v1,
    public.cex_account_opening_inventory_items_v1,
    public.cex_account_projection_runs_v1,
    public.cex_account_projection_items_v1,
    public.cex_account_projection_repairs_v1,
    public.cex_provider_dispatch_commands_v1,
    public.cex_provider_dispatch_transitions_v1
from public;

revoke execute on function public.cex_open_account_v2(uuid,uuid,uuid,text,text,smallint,bigint,text,text,text) from public;
revoke execute on function public.cex_build_account_opening_inventory_v1(uuid,text) from public;
revoke execute on function public.cex_seal_account_opening_inventory_v1(uuid,text,text,text,jsonb) from public;
revoke execute on function public.cex_capture_account_projection_v1(uuid,text) from public;
revoke execute on function public.cex_set_money_read_policy_v1(uuid,text,numeric,integer,text) from public;
revoke execute on function public.cex_repair_account_projection_v1(uuid,uuid,text,text) from public;
revoke execute on function public.cex_enqueue_provider_dispatch_v1(uuid,text,text,integer) from public;
revoke execute on function public.cex_claim_provider_dispatch_v1(text,integer,integer) from public;
revoke execute on function public.cex_finish_provider_dispatch_v1(uuid,text,text,jsonb,integer,text,text,integer) from public;
revoke execute on function public.cex_acknowledge_provider_dispatch_v1(uuid,text,text) from public;
revoke execute on function public.cex_requeue_provider_dispatch_v1(uuid,text,text,integer) from public;

grant execute on function public.cex_open_account_v2(uuid,uuid,uuid,text,text,smallint,bigint,text,text,text)
    to cex_ledger_opening_source;
grant execute on function public.cex_read_account_money_v2(uuid)
    to cex_ledger_opening_source, cex_projection_operator;
grant execute on function public.cex_build_account_opening_inventory_v1(uuid,text),
    public.cex_seal_account_opening_inventory_v1(uuid,text,text,text,jsonb),
    public.cex_capture_account_projection_v1(uuid,text),
    public.cex_set_money_read_policy_v1(uuid,text,numeric,integer,text),
    public.cex_repair_account_projection_v1(uuid,uuid,text,text)
    to cex_projection_operator;
grant execute on function public.cex_enqueue_provider_dispatch_v1(uuid,text,text,integer),
    public.cex_transition_provider_execution_terminal_v1(uuid,text,text,text)
    to cex_provider_dispatch_source;
grant execute on function public.cex_claim_provider_dispatch_v1(text,integer,integer),
    public.cex_finish_provider_dispatch_v1(uuid,text,text,jsonb,integer,text,text,integer)
    to cex_provider_dispatch_worker;
grant execute on function public.cex_acknowledge_provider_dispatch_v1(uuid,text,text),
    public.cex_requeue_provider_dispatch_v1(uuid,text,text,integer)
    to cex_projection_operator;

create or replace view public.cex_p0_database_role_matrix_v1 as
select * from (values
    ('cex_ledger_opening_source','exact account opening and exact reads'),
    ('cex_projection_operator','signed inventory, projection capture, policy and repair evidence'),
    ('cex_provider_dispatch_source','provider command enqueue and explicit terminal decision'),
    ('cex_provider_dispatch_worker','provider claim and durable outcome'),
    ('cex_release_evidence_admitter','external release evidence admission')
) role_matrix(role_name, responsibility);

commit;

begin;

-- migration-check: allow-destructive

-- Normalized Term Exchange receipts are projections of immutable protocol
-- receipts.  Keep the exact whole-credit amount when it is present, while
-- leaving the column nullable so rows written before this migration remain
-- readable (and continue to fail closed at value-authority gates).
alter table if exists public.league_term_exchange_receipts
    add column if not exists amount_credits bigint;
alter table if exists public.world_term_exchange_receipts
    add column if not exists amount_credits bigint;

do $function$
begin
    if to_regclass('public.league_term_exchange_receipts') is not null
       and not exists (
           select 1
             from pg_catalog.pg_constraint constraint_row
             join pg_catalog.pg_class table_row
               on table_row.oid = constraint_row.conrelid
             join pg_catalog.pg_namespace schema_row
               on schema_row.oid = table_row.relnamespace
            where schema_row.nspname = 'public'
              and table_row.relname = 'league_term_exchange_receipts'
              and constraint_row.conname = 'league_term_exchange_receipts_amount_credits_nonnegative_v1'
       ) then
        alter table public.league_term_exchange_receipts
            add constraint league_term_exchange_receipts_amount_credits_nonnegative_v1
            check (amount_credits is null or amount_credits >= 0);
    end if;
    if to_regclass('public.world_term_exchange_receipts') is not null
       and not exists (
           select 1
             from pg_catalog.pg_constraint constraint_row
             join pg_catalog.pg_class table_row
               on table_row.oid = constraint_row.conrelid
             join pg_catalog.pg_namespace schema_row
               on schema_row.oid = table_row.relnamespace
            where schema_row.nspname = 'public'
              and table_row.relname = 'world_term_exchange_receipts'
              and constraint_row.conname = 'world_term_exchange_receipts_amount_credits_nonnegative_v1'
       ) then
        alter table public.world_term_exchange_receipts
            add constraint world_term_exchange_receipts_amount_credits_nonnegative_v1
            check (amount_credits is null or amount_credits >= 0);
    end if;
end
$function$;

-- Mutation is represented by a new receipt row.  In particular, a replay or
-- a snapshot projection must use INSERT ... ON CONFLICT DO NOTHING; UPDATE,
-- DELETE and TRUNCATE are never valid ways to revise receipt evidence.
create or replace function public.cex_reject_term_exchange_receipt_mutation_v1()
returns trigger
language plpgsql
security invoker
set search_path = pg_catalog, public
as $function$
begin
    raise exception using
        errcode = '55000',
        message = 'normalized term exchange receipts are append-only';
end;
$function$;

drop trigger if exists trg_cex_league_term_exchange_receipt_mutation_v1
    on public.league_term_exchange_receipts;
create trigger trg_cex_league_term_exchange_receipt_mutation_v1
before update or delete on public.league_term_exchange_receipts
for each statement
execute function public.cex_reject_term_exchange_receipt_mutation_v1();

drop trigger if exists trg_cex_league_term_exchange_receipt_truncate_v1
    on public.league_term_exchange_receipts;
create trigger trg_cex_league_term_exchange_receipt_truncate_v1
before truncate on public.league_term_exchange_receipts
for each statement
execute function public.cex_reject_term_exchange_receipt_mutation_v1();

drop trigger if exists trg_cex_world_term_exchange_receipt_mutation_v1
    on public.world_term_exchange_receipts;
create trigger trg_cex_world_term_exchange_receipt_mutation_v1
before update or delete on public.world_term_exchange_receipts
for each statement
execute function public.cex_reject_term_exchange_receipt_mutation_v1();

drop trigger if exists trg_cex_world_term_exchange_receipt_truncate_v1
    on public.world_term_exchange_receipts;
create trigger trg_cex_world_term_exchange_receipt_truncate_v1
before truncate on public.world_term_exchange_receipts
for each statement
execute function public.cex_reject_term_exchange_receipt_mutation_v1();

alter table public.league_term_exchange_receipts
    enable always trigger trg_cex_league_term_exchange_receipt_mutation_v1;
alter table public.league_term_exchange_receipts
    enable always trigger trg_cex_league_term_exchange_receipt_truncate_v1;
alter table public.world_term_exchange_receipts
    enable always trigger trg_cex_world_term_exchange_receipt_mutation_v1;
alter table public.world_term_exchange_receipts
    enable always trigger trg_cex_world_term_exchange_receipt_truncate_v1;

-- Ordinary projection/read roles may insert and read, but never mutate or
-- remove normalized receipt evidence.  The trigger remains the owner-level
-- backstop for service roles that own the tables.
revoke update, delete, truncate
    on table public.league_term_exchange_receipts,
              public.world_term_exchange_receipts
    from public;

do $privileges$
begin
    if exists (select 1 from pg_catalog.pg_roles where rolname = 'cex_projection_operator') then
        execute 'revoke update, delete, truncate on table public.league_term_exchange_receipts, public.world_term_exchange_receipts from cex_projection_operator';
    end if;
end
$privileges$;

comment on column public.league_term_exchange_receipts.amount_credits is
    'Exact whole-credit amount from immutable EconomicIntent evidence; NULL is legacy and fails closed';
comment on column public.world_term_exchange_receipts.amount_credits is
    'Exact whole-credit amount from immutable EconomicIntent evidence; NULL is legacy and fails closed';
comment on table public.league_term_exchange_receipts is
    'Append-only normalized Term Exchange receipt projection; replay uses insert-on-conflict-do-nothing';
comment on table public.world_term_exchange_receipts is
    'Append-only normalized Term Exchange receipt projection; replay uses insert-on-conflict-do-nothing';

commit;

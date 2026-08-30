begin;

-- Serialize the catalog-aware repair with the adjacent receipt migrations.
-- The guards intentionally use check-then-create/repair paths; one
-- transaction-scoped lock prevents concurrent migration runners from both
-- observing a missing column/constraint and racing to install the authority
-- objects.
select pg_catalog.pg_advisory_xact_lock(
    pg_catalog.hashtextextended('cex:migration:term-exchange-receipt-schema', 0)
);

-- migration-check: allow-destructive

-- Normalized Term Exchange receipts are projections of immutable protocol
-- receipts.  Keep the exact whole-credit amount when it is present, while
-- leaving the column nullable so rows written before this migration remain
-- readable (and continue to fail closed at value-authority gates).
-- `ADD COLUMN IF NOT EXISTS` compares only the column name.  Validate the
-- exact integer authority and existing values before accepting a column left
-- by an interrupted or hand-authored rollout; a numeric/float column could
-- otherwise admit fractional credits while reporting a successful migration.
do $amount_schema_guard$
declare
    table_spec record;
    table_oid oid;
    relation_kind "char";
    relation_persistence "char";
    relation_is_partition boolean;
    amount_attnum smallint;
    amount_type oid;
    amount_notnull boolean;
    amount_identity text;
    amount_generated text;
    amount_default_oid oid;
    has_invalid_value boolean;
    constraint_type "char";
    constraint_validated boolean;
    constraint_expression text;
    expected_constraint_expression constant text := '((amount_creditsisnull)or(amount_credits>=0))';
begin
    for table_spec in
        select *
          from (values
              (
                  'league_term_exchange_receipts'::text,
                  'league'::text,
                  'league_term_exchange_receipts_amount_credits_nonnegative_v1'::text
              ),
              (
                  'world_term_exchange_receipts'::text,
                  'world'::text,
                  'world_term_exchange_receipts_amount_credits_nonnegative_v1'::text
              )
          ) as allowed_tables(table_name, label, constraint_name)
    loop
        table_oid := to_regclass('public.' || table_spec.table_name);
        if table_oid is null then
            raise exception 'normalized % receipt projection table is missing', table_spec.label;
        end if;
        select c.relkind, c.relpersistence, c.relispartition
          into relation_kind, relation_persistence, relation_is_partition
          from pg_catalog.pg_class c
         where c.oid = table_oid;
        if relation_kind is distinct from 'r'
           or relation_persistence is distinct from 'p'
           or relation_is_partition then
            raise exception
                'normalized % receipt projection relation is not a permanent ordinary table',
                table_spec.label;
        end if;

        select a.attnum,
               a.atttypid,
               a.attnotnull,
               a.attidentity::text,
               a.attgenerated::text,
               ad.oid
          into amount_attnum, amount_type, amount_notnull,
               amount_identity, amount_generated, amount_default_oid
          from pg_catalog.pg_attribute a
          left join pg_catalog.pg_attrdef ad
            on ad.adrelid = a.attrelid
           and ad.adnum = a.attnum
         where a.attrelid = table_oid
           and a.attname = 'amount_credits'
           and not a.attisdropped;
        if amount_attnum is null then
            execute format(
                'alter table public.%I add column amount_credits bigint',
                table_spec.table_name
            );
            select a.attnum,
                   a.atttypid,
                   a.attnotnull,
                   a.attidentity::text,
                   a.attgenerated::text,
                   ad.oid
              into amount_attnum, amount_type, amount_notnull,
                   amount_identity, amount_generated, amount_default_oid
              from pg_catalog.pg_attribute a
              left join pg_catalog.pg_attrdef ad
                on ad.adrelid = a.attrelid
               and ad.adnum = a.attnum
             where a.attrelid = table_oid
               and a.attname = 'amount_credits'
               and not a.attisdropped;
        end if;
        if amount_type is distinct from 'int8'::regtype then
            raise exception
                'normalized % receipt amount_credits must be bigint', table_spec.label;
        end if;
        if amount_notnull
           or coalesce(amount_identity, '') <> ''
           or coalesce(amount_generated, '') <> ''
           or amount_default_oid is not null then
            raise exception
                'normalized % receipt amount_credits column has incompatible nullability/default/generated shape',
                table_spec.label;
        end if;

        execute format(
            'select exists (
                 select 1
                   from public.%I
                  where amount_credits is not null
                    and amount_credits < 0
             )',
            table_spec.table_name
        ) into has_invalid_value;
        if has_invalid_value then
            raise exception
                'normalized % receipt amount_credits contains a negative value',
                table_spec.label;
        end if;

        select c.contype,
               c.convalidated,
               regexp_replace(
                   lower(coalesce(pg_get_expr(c.conbin, c.conrelid), '')),
                   '[[:space:]]+',
                   '',
                   'g'
               )
          into constraint_type, constraint_validated, constraint_expression
          from pg_catalog.pg_constraint c
         where c.conrelid = table_oid
           and c.conname = table_spec.constraint_name;
        if found then
            if constraint_type is distinct from 'c'
               or constraint_validated is distinct from true
               or constraint_expression is distinct from expected_constraint_expression then
                raise exception
                    'normalized % receipt amount constraint is not canonical',
                    table_spec.label;
            end if;
        else
            execute format(
                'alter table public.%I add constraint %I ' ||
                'check (amount_credits is null or amount_credits >= 0)',
                table_spec.table_name,
                table_spec.constraint_name
            );
        end if;
    end loop;
end
$amount_schema_guard$;

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

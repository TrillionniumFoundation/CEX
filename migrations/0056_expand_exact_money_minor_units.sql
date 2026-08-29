begin;

-- Money v2 expand phase. Existing numeric(20,6) columns remain available while
-- exact signed 64-bit minor-unit columns are introduced and kept in sync.

create or replace function public.cex_money_scale_factor(p_scale smallint)
returns numeric
language plpgsql
immutable
strict
set search_path = pg_catalog, public
as $$
begin
    if p_scale < 0 or p_scale > 6 then
        raise exception 'money scale % is outside supported range 0..6', p_scale;
    end if;
    return power(10::numeric, p_scale::numeric);
end
$$;

create or replace function public.cex_numeric_to_minor(
    p_value numeric,
    p_scale smallint
)
returns bigint
language plpgsql
immutable
strict
set search_path = pg_catalog, public
as $$
declare
    scaled numeric;
begin
    scaled := p_value * public.cex_money_scale_factor(p_scale);
    if scaled <> trunc(scaled) then
        raise exception 'money value % exceeds configured scale %', p_value, p_scale;
    end if;
    if scaled > 9223372036854775807::numeric
       or scaled < -9223372036854775808::numeric then
        raise exception 'money value % exceeds bigint minor-unit range', p_value;
    end if;
    return scaled::bigint;
end
$$;

create or replace function public.cex_minor_to_numeric(
    p_minor bigint,
    p_scale smallint
)
returns numeric
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select p_minor::numeric / public.cex_money_scale_factor(p_scale)
$$;

-- Abort before schema mutation if current values cannot be represented exactly.
do $$
begin
    perform public.cex_numeric_to_minor(balance, 6::smallint)
      from public.accounts;
    perform public.cex_numeric_to_minor(reserved, 6::smallint)
      from public.accounts;
    perform public.cex_numeric_to_minor(amount, 6::smallint)
      from public.ledger_entries;
end
$$;

alter table public.accounts
    add column if not exists currency_scale smallint,
    add column if not exists balance_minor bigint,
    add column if not exists reserved_minor bigint;

update public.accounts
   set currency_scale = coalesce(currency_scale, 6),
       balance_minor = coalesce(
           balance_minor,
           public.cex_numeric_to_minor(balance, coalesce(currency_scale, 6))
       ),
       reserved_minor = coalesce(
           reserved_minor,
           public.cex_numeric_to_minor(reserved, coalesce(currency_scale, 6))
       )
 where currency_scale is null
    or balance_minor is null
    or reserved_minor is null;

alter table public.accounts
    alter column currency_scale set default 6,
    alter column currency_scale set not null,
    alter column balance_minor set not null,
    alter column reserved_minor set not null;

alter table public.ledger_entries
    add column if not exists currency_scale smallint,
    add column if not exists amount_minor bigint;

update public.ledger_entries
   set currency_scale = coalesce(currency_scale, 6),
       amount_minor = coalesce(
           amount_minor,
           public.cex_numeric_to_minor(amount, coalesce(currency_scale, 6))
       )
 where currency_scale is null
    or amount_minor is null;

alter table public.ledger_entries
    alter column currency_scale set default 6,
    alter column currency_scale set not null,
    alter column amount_minor set not null;

do $$
begin
    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.accounts'::regclass
           and conname = 'accounts_currency_scale_v2'
    ) then
        alter table public.accounts
            add constraint accounts_currency_scale_v2
            check (currency_scale between 0 and 6);
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.accounts'::regclass
           and conname = 'accounts_minor_nonnegative_v2'
    ) then
        alter table public.accounts
            add constraint accounts_minor_nonnegative_v2
            check (balance_minor >= 0 and reserved_minor >= 0);
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.accounts'::regclass
           and conname = 'accounts_minor_reservation_v2'
    ) then
        alter table public.accounts
            add constraint accounts_minor_reservation_v2
            check (reserved_minor <= balance_minor);
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_currency_scale_v2'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_currency_scale_v2
            check (currency_scale between 0 and 6);
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_minor_positive_v2'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_minor_positive_v2
            check (amount_minor > 0);
    end if;
end
$$;

create or replace function public.cex_sync_account_money_v2()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    numeric_changed boolean;
    minor_changed boolean;
    scale_changed boolean;
begin
    if new.currency_scale is null then
        new.currency_scale := 6;
    end if;
    perform public.cex_money_scale_factor(new.currency_scale);

    if tg_op = 'INSERT' then
        if new.balance_minor is null then
            new.balance_minor := public.cex_numeric_to_minor(new.balance, new.currency_scale);
        elsif new.balance is distinct from public.cex_minor_to_numeric(new.balance_minor, new.currency_scale) then
            raise exception 'account balance numeric/minor representations disagree';
        end if;

        if new.reserved_minor is null then
            new.reserved_minor := public.cex_numeric_to_minor(new.reserved, new.currency_scale);
        elsif new.reserved is distinct from public.cex_minor_to_numeric(new.reserved_minor, new.currency_scale) then
            raise exception 'account reserved numeric/minor representations disagree';
        end if;
        return new;
    end if;

    scale_changed := new.currency_scale is distinct from old.currency_scale;

    numeric_changed := new.balance is distinct from old.balance;
    minor_changed := new.balance_minor is distinct from old.balance_minor;
    if numeric_changed and minor_changed then
        if new.balance is distinct from public.cex_minor_to_numeric(new.balance_minor, new.currency_scale) then
            raise exception 'account balance numeric/minor representations disagree';
        end if;
    elsif numeric_changed or scale_changed then
        new.balance_minor := public.cex_numeric_to_minor(new.balance, new.currency_scale);
    elsif minor_changed then
        new.balance := public.cex_minor_to_numeric(new.balance_minor, new.currency_scale);
    end if;

    numeric_changed := new.reserved is distinct from old.reserved;
    minor_changed := new.reserved_minor is distinct from old.reserved_minor;
    if numeric_changed and minor_changed then
        if new.reserved is distinct from public.cex_minor_to_numeric(new.reserved_minor, new.currency_scale) then
            raise exception 'account reserved numeric/minor representations disagree';
        end if;
    elsif numeric_changed or scale_changed then
        new.reserved_minor := public.cex_numeric_to_minor(new.reserved, new.currency_scale);
    elsif minor_changed then
        new.reserved := public.cex_minor_to_numeric(new.reserved_minor, new.currency_scale);
    end if;

    return new;
end
$$;

create or replace function public.cex_sync_ledger_entry_money_v2()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    numeric_changed boolean;
    minor_changed boolean;
    scale_changed boolean;
begin
    if new.currency_scale is null then
        new.currency_scale := 6;
    end if;
    perform public.cex_money_scale_factor(new.currency_scale);

    if tg_op = 'INSERT' then
        if new.amount_minor is null then
            new.amount_minor := public.cex_numeric_to_minor(new.amount, new.currency_scale);
        elsif new.amount is distinct from public.cex_minor_to_numeric(new.amount_minor, new.currency_scale) then
            raise exception 'ledger amount numeric/minor representations disagree';
        end if;
        return new;
    end if;

    scale_changed := new.currency_scale is distinct from old.currency_scale;
    numeric_changed := new.amount is distinct from old.amount;
    minor_changed := new.amount_minor is distinct from old.amount_minor;

    if numeric_changed and minor_changed then
        if new.amount is distinct from public.cex_minor_to_numeric(new.amount_minor, new.currency_scale) then
            raise exception 'ledger amount numeric/minor representations disagree';
        end if;
    elsif numeric_changed or scale_changed then
        new.amount_minor := public.cex_numeric_to_minor(new.amount, new.currency_scale);
    elsif minor_changed then
        new.amount := public.cex_minor_to_numeric(new.amount_minor, new.currency_scale);
    end if;

    return new;
end
$$;

drop trigger if exists trg_cex_sync_account_money_v2 on public.accounts;
create trigger trg_cex_sync_account_money_v2
before insert or update of balance, reserved, balance_minor, reserved_minor, currency_scale
on public.accounts
for each row execute function public.cex_sync_account_money_v2();

drop trigger if exists trg_cex_sync_ledger_entry_money_v2 on public.ledger_entries;
create trigger trg_cex_sync_ledger_entry_money_v2
before insert or update of amount, amount_minor, currency_scale
on public.ledger_entries
for each row execute function public.cex_sync_ledger_entry_money_v2();

create index if not exists idx_accounts_money_v2_reconciliation
    on public.accounts (currency_unit, currency_scale, account_id);

commit;

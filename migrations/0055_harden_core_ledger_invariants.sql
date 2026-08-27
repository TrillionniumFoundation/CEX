begin;

-- Fail before adding constraints so operators receive a precise remediation signal.
do $$
begin
    if exists (
        select 1
          from public.accounts
         where balance < 0
            or reserved < 0
            or reserved > balance
    ) then
        raise exception
            '0055 blocked: accounts contain negative or over-reserved summaries; run reconciliation before retrying';
    end if;

    if exists (
        select 1
          from public.ledger_entries
         where amount <= 0
            or btrim(reason) = ''
    ) then
        raise exception
            '0055 blocked: ledger_entries contain non-positive amounts or empty reasons';
    end if;
end
$$;

do $$
begin
    if not exists (
        select 1
          from pg_constraint
         where conrelid = 'public.accounts'::regclass
           and conname = 'accounts_balance_nonnegative'
    ) then
        alter table public.accounts
            add constraint accounts_balance_nonnegative
            check (balance >= 0);
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conrelid = 'public.accounts'::regclass
           and conname = 'accounts_reserved_nonnegative'
    ) then
        alter table public.accounts
            add constraint accounts_reserved_nonnegative
            check (reserved >= 0);
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conrelid = 'public.accounts'::regclass
           and conname = 'accounts_reserved_not_above_balance'
    ) then
        alter table public.accounts
            add constraint accounts_reserved_not_above_balance
            check (reserved <= balance);
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_amount_positive'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_amount_positive
            check (amount > 0);
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_reason_nonempty'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_reason_nonempty
            check (btrim(reason) <> '');
    end if;
end
$$;

create index if not exists idx_ledger_entries_account_timeline
    on public.ledger_entries (account_id, created_at, entry_id);

commit;

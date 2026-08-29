-- Reversible seller payout holds and native player identity recovery.
-- Seller proceeds remain reserved until the dispute window matures, so a
-- committed trade can be reversed without depending on spendable seller cash.

alter table trnm_escrow_trades
    add column if not exists seller_hold_amount numeric(20, 6) not null default 0,
    add column if not exists seller_hold_released boolean not null default true,
    add column if not exists reversible_until timestamptz;

do $$
begin
    if not exists (
        select 1 from pg_constraint
        where conname = 'trnm_escrow_seller_hold_non_negative'
    ) then
        alter table trnm_escrow_trades
            add constraint trnm_escrow_seller_hold_non_negative
            check (seller_hold_amount >= 0);
    end if;
end $$;

create index if not exists idx_trnm_escrow_seller_hold_release
    on trnm_escrow_trades(seller_account_id, reversible_until)
    where status = 'committed' and seller_hold_released = false;

create table if not exists trnm_player_identities (
    player_id text primary key,
    account_id uuid not null unique references accounts(account_id),
    recovery_key_hash text not null,
    recovery_generation bigint not null default 1 check (recovery_generation > 0),
    status text not null default 'active' check (status in ('active', 'suspended', 'closed')),
    created_at timestamptz not null default now(),
    recovered_at timestamptz,
    updated_at timestamptz not null default now()
);

create unique index if not exists idx_trnm_player_identity_recovery_hash
    on trnm_player_identities(recovery_key_hash);

create table if not exists trnm_identity_recovery_audit (
    audit_id uuid primary key,
    player_id text not null references trnm_player_identities(player_id),
    recovery_generation bigint not null check (recovery_generation > 0),
    event_kind text not null check (event_kind in ('registered', 'recovered', 'suspended', 'closed')),
    event_hash text not null unique,
    created_at timestamptz not null default now()
);

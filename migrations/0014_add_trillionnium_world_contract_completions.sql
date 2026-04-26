-- Trillionnium World contract completions: judged delivery, ledger settlement, and asset/reputation growth.

create table if not exists world_contract_completions (
    completion_id text primary key,
    contract_id text not null references world_contracts(contract_id),
    matrix_user_id text not null,
    body text not null,
    score numeric(10,2) not null default 0,
    grade text not null,
    reward_amount numeric(18,6) not null default 0,
    judge_status text not null,
    payout_status text not null,
    anti_cheat_flags jsonb not null default '[]'::jsonb,
    score_events jsonb not null default '[]'::jsonb,
    ledger_status text,
    ledger_account_id text,
    ledger_entry_id text,
    ledger_balance_after numeric(18,6),
    ledger_error text,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_contract_completions_contract on world_contract_completions(contract_id, created_at desc);
create index if not exists idx_world_contract_completions_player on world_contract_completions(matrix_user_id, created_at desc);
create index if not exists idx_world_contract_completions_ledger on world_contract_completions(ledger_status, created_at desc);

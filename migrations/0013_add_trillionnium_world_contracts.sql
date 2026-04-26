-- Trillionnium World contracts connect free-form world actions to real CEX execution.

alter table world_events
    add column if not exists cex_task_id text,
    add column if not exists cex_status text;

create table if not exists world_contracts (
    contract_id text primary key,
    event_id text not null references world_events(event_id),
    actor_matrix_user_id text not null,
    location_id text not null references world_locations(location_id),
    task_id text not null,
    title text not null,
    body text not null,
    status text not null,
    cex_status text,
    value_score integer not null default 0,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_events_cex_task on world_events(cex_task_id) where cex_task_id is not null;
create index if not exists idx_world_contracts_actor on world_contracts(actor_matrix_user_id, created_at desc);
create index if not exists idx_world_contracts_task on world_contracts(task_id);

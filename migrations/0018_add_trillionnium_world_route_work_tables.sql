-- Trillionnium World normalized route/work tables.
-- This closes the remaining JSON-only world seams before the repository cutover:
-- map nodes/player positions, delivery review lanes, and buyer-side reserve/consume ledger fields.

create table if not exists world_map_nodes (
    node_id text primary key,
    location_id text not null references world_locations(location_id),
    zone_id text not null references world_zones(zone_id),
    name text not null,
    node_kind text not null,
    description text not null,
    x integer not null default 0,
    y integer not null default 0,
    exits jsonb not null default '{}'::jsonb,
    interaction_tags jsonb not null default '[]'::jsonb,
    freedom_hooks jsonb not null default '[]'::jsonb,
    status text not null
);

create index if not exists idx_world_map_nodes_location on world_map_nodes(location_id, status);
create index if not exists idx_world_map_nodes_zone on world_map_nodes(zone_id, status);

create table if not exists world_player_positions (
    matrix_user_id text primary key,
    node_id text not null references world_map_nodes(node_id),
    location_id text not null references world_locations(location_id),
    updated_at timestamptz not null default now()
);

create index if not exists idx_world_player_positions_node on world_player_positions(node_id, updated_at desc);

alter table world_purchases
    add column if not exists buyer_ledger_status text,
    add column if not exists buyer_ledger_account_id text,
    add column if not exists buyer_ledger_entry_id text,
    add column if not exists buyer_ledger_balance_after double precision,
    add column if not exists buyer_ledger_error text,
    add column if not exists buyer_consume_status text,
    add column if not exists buyer_consume_entry_id text,
    add column if not exists buyer_consume_balance_after double precision,
    add column if not exists buyer_consume_error text;

create index if not exists idx_world_purchases_buyer_ledger on world_purchases(buyer_ledger_status, created_at desc);
create index if not exists idx_world_purchases_buyer_consume on world_purchases(buyer_consume_status, created_at desc);

create table if not exists world_work_deliveries (
    delivery_id text primary key,
    work_order_id text not null references world_work_orders(work_order_id),
    matrix_user_id text not null,
    body text not null,
    score double precision not null default 0,
    judge_status text not null,
    status text not null,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_work_deliveries_order on world_work_deliveries(work_order_id, created_at desc);
create index if not exists idx_world_work_deliveries_user on world_work_deliveries(matrix_user_id, created_at desc);

create table if not exists world_work_acceptances (
    acceptance_id text primary key,
    work_order_id text not null references world_work_orders(work_order_id),
    matrix_user_id text not null,
    body text not null,
    status text not null,
    reputation_delta integer not null default 0,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_work_acceptances_order on world_work_acceptances(work_order_id, created_at desc);
create index if not exists idx_world_work_acceptances_user on world_work_acceptances(matrix_user_id, created_at desc);

create table if not exists world_work_rejections (
    rejection_id text primary key,
    work_order_id text not null references world_work_orders(work_order_id),
    matrix_user_id text not null,
    body text not null,
    status text not null,
    refund_status text not null,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_work_rejections_order on world_work_rejections(work_order_id, created_at desc);
create index if not exists idx_world_work_rejections_refund on world_work_rejections(refund_status, created_at desc);

create table if not exists world_work_reopens (
    reopen_id text primary key,
    work_order_id text not null references world_work_orders(work_order_id),
    matrix_user_id text not null,
    body text not null,
    status text not null,
    reserve_status text not null,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_work_reopens_order on world_work_reopens(work_order_id, created_at desc);
create index if not exists idx_world_work_reopens_reserve on world_work_reopens(reserve_status, created_at desc);

create table if not exists world_work_cancellations (
    cancellation_id text primary key,
    work_order_id text not null references world_work_orders(work_order_id),
    matrix_user_id text not null,
    body text not null,
    status text not null,
    refund_status text not null,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_work_cancellations_order on world_work_cancellations(work_order_id, created_at desc);
create index if not exists idx_world_work_cancellations_refund on world_work_cancellations(refund_status, created_at desc);

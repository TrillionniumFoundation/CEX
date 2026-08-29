-- Trillionnium World asset upgrade tree: assets can grow through judged work.

alter table world_assets
    add column if not exists upgrade_level integer not null default 1,
    add column if not exists upgrade_points integer not null default 0,
    add column if not exists last_upgrade_kind text;

create table if not exists world_asset_upgrades (
    upgrade_id text primary key,
    asset_id text not null references world_assets(asset_id),
    matrix_user_id text not null,
    body text not null,
    upgrade_kind text not null,
    score numeric(10,2) not null default 0,
    grade text not null,
    judge_status text not null,
    status text not null,
    value_delta integer not null default 0,
    level_before integer not null default 1,
    level_after integer not null default 1,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_assets_owner_level on world_assets(owner_matrix_user_id, upgrade_level desc, value_score desc);
create index if not exists idx_world_asset_upgrades_asset on world_asset_upgrades(asset_id, created_at desc);
create index if not exists idx_world_asset_upgrades_player on world_asset_upgrades(matrix_user_id, created_at desc);

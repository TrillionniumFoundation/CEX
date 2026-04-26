-- Trillionnium World economy layer: companies, shops, listings, and economy events.

create table if not exists world_companies (
    company_id text primary key,
    owner_matrix_user_id text not null,
    asset_id text not null references world_assets(asset_id),
    location_id text not null references world_locations(location_id),
    name text not null,
    company_kind text not null,
    status text not null,
    revenue_score integer not null default 0,
    reputation_score integer not null default 0,
    level integer not null default 1,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_companies_owner on world_companies(owner_matrix_user_id, level desc, revenue_score desc);
create index if not exists idx_world_companies_asset on world_companies(asset_id);
create index if not exists idx_world_companies_location on world_companies(location_id, status);

create table if not exists world_shops (
    shop_id text primary key,
    company_id text not null references world_companies(company_id),
    owner_matrix_user_id text not null,
    location_id text not null references world_locations(location_id),
    name text not null,
    shop_kind text not null,
    status text not null,
    listing_count integer not null default 0,
    gross_merchandise_score integer not null default 0,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_shops_company on world_shops(company_id);
create index if not exists idx_world_shops_owner on world_shops(owner_matrix_user_id, gross_merchandise_score desc);
create index if not exists idx_world_shops_location on world_shops(location_id, status);

create table if not exists world_listings (
    listing_id text primary key,
    shop_id text not null references world_shops(shop_id),
    company_id text not null references world_companies(company_id),
    owner_matrix_user_id text not null,
    asset_id text not null references world_assets(asset_id),
    title text not null,
    listing_kind text not null,
    status text not null,
    price_credits integer not null default 0,
    quality_score integer not null default 0,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_listings_shop on world_listings(shop_id, status);
create index if not exists idx_world_listings_company on world_listings(company_id, price_credits desc);
create index if not exists idx_world_listings_owner on world_listings(owner_matrix_user_id, quality_score desc);

create table if not exists world_economy_events (
    economy_event_id text primary key,
    matrix_user_id text not null,
    event_kind text not null,
    subject_id text not null,
    credits_delta integer not null default 0,
    reputation_delta integer not null default 0,
    created_at timestamptz not null default now()
);

create index if not exists idx_world_economy_events_user on world_economy_events(matrix_user_id, created_at desc);
create index if not exists idx_world_economy_events_subject on world_economy_events(subject_id, created_at desc);

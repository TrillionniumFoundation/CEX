-- Trillionnium World commerce/work/faction tables.

create table if not exists world_purchases (
  purchase_id text primary key,
  listing_id text not null references world_listings(listing_id),
  shop_id text not null references world_shops(shop_id),
  company_id text not null references world_companies(company_id),
  buyer_matrix_user_id text not null,
  seller_matrix_user_id text not null,
  price_credits integer not null default 0,
  status text not null,
  ledger_status text,
  ledger_account_id text,
  ledger_entry_id text,
  ledger_balance_after double precision,
  ledger_error text,
  created_at timestamptz not null default now()
);

create index if not exists idx_world_purchases_listing on world_purchases(listing_id, status);
create index if not exists idx_world_purchases_buyer on world_purchases(buyer_matrix_user_id, created_at desc);
create index if not exists idx_world_purchases_seller on world_purchases(seller_matrix_user_id, created_at desc);

create table if not exists world_work_orders (
  work_order_id text primary key,
  purchase_id text not null references world_purchases(purchase_id),
  listing_id text not null references world_listings(listing_id),
  buyer_matrix_user_id text not null,
  seller_matrix_user_id text not null,
  company_id text not null references world_companies(company_id),
  status text not null,
  brief text not null,
  value_score integer not null default 0,
  created_at timestamptz not null default now()
);

create index if not exists idx_world_work_orders_purchase on world_work_orders(purchase_id);
create index if not exists idx_world_work_orders_status on world_work_orders(status, created_at desc);
create index if not exists idx_world_work_orders_seller on world_work_orders(seller_matrix_user_id, status);

create table if not exists world_factions (
  faction_id text primary key,
  zone_id text not null references world_zones(zone_id),
  name text not null,
  faction_kind text not null,
  status text not null,
  reputation_score integer not null default 0
);

create table if not exists world_faction_standings (
  standing_id text primary key,
  matrix_user_id text not null,
  faction_id text not null references world_factions(faction_id),
  reputation_score integer not null default 0,
  rank text not null,
  updated_at timestamptz not null default now(),
  unique(matrix_user_id, faction_id)
);

create index if not exists idx_world_faction_standings_user on world_faction_standings(matrix_user_id, reputation_score desc);
create index if not exists idx_world_faction_standings_faction on world_faction_standings(faction_id, reputation_score desc);

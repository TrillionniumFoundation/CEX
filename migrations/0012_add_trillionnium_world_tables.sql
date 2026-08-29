-- Trillionnium World open-world simulation tables.
-- These tables extend League into a reality-mirror sandbox: zones,
-- locations, Agent/NPC residents, player assets, world events, and relationships.

create table if not exists world_zones (
    zone_id text primary key,
    name text not null,
    status text not null,
    theme text not null,
    mirror_kind text not null,
    created_at timestamptz not null default now()
);

create table if not exists world_locations (
    location_id text primary key,
    zone_id text not null references world_zones(zone_id),
    name text not null,
    location_kind text not null,
    description text not null,
    status text not null,
    created_at timestamptz not null default now()
);

create table if not exists world_entities (
    entity_id text primary key,
    location_id text not null references world_locations(location_id),
    name text not null,
    entity_kind text not null,
    role text not null,
    status text not null,
    created_at timestamptz not null default now()
);

create table if not exists world_assets (
    asset_id text primary key,
    owner_matrix_user_id text not null,
    location_id text not null references world_locations(location_id),
    asset_kind text not null,
    name text not null,
    status text not null,
    value_score integer not null default 0,
    created_at timestamptz not null default now()
);

create table if not exists world_events (
    event_id text primary key,
    actor_matrix_user_id text not null,
    room_id text,
    location_id text not null references world_locations(location_id),
    event_kind text not null,
    body text not null,
    result text not null,
    impact_score integer not null default 0,
    created_at timestamptz not null default now()
);

create table if not exists world_relationships (
    relationship_id text primary key,
    from_id text not null,
    to_id text not null,
    relation_kind text not null,
    strength integer not null default 0,
    updated_at timestamptz not null default now()
);

create index if not exists idx_world_locations_zone on world_locations(zone_id, status);
create index if not exists idx_world_assets_owner on world_assets(owner_matrix_user_id, created_at desc);
create index if not exists idx_world_events_actor on world_events(actor_matrix_user_id, created_at desc);
create index if not exists idx_world_events_location on world_events(location_id, created_at desc);
create index if not exists idx_world_relationships_from on world_relationships(from_id, relation_kind);

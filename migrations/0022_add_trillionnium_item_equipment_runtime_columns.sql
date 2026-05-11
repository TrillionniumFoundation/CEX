-- Trillionnium item/equipment runtime mirror.
-- Rust WorldState remains the command source of truth; these JSONB columns
-- mirror character inventory/equipment slots for normalized parity audits and
-- direct-write read-model evolution.

alter table world_trillionnium_characters
    add column if not exists inventory_items jsonb not null default '[]'::jsonb,
    add column if not exists equipment_slots jsonb not null default '{}'::jsonb;

create index if not exists idx_world_trillionnium_characters_inventory_items
    on world_trillionnium_characters using gin (inventory_items);

create index if not exists idx_world_trillionnium_characters_equipment_slots
    on world_trillionnium_characters using gin (equipment_slots);

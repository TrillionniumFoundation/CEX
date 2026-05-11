-- Add Rust-owned Trillionnium combat numerics runtime state.

alter table world_trillionnium_characters
    add column if not exists combat_numerics_state jsonb not null default '{
        "hp_current": 176,
        "hp_max": 176,
        "inner_energy_current": 126,
        "inner_energy_max": 126,
        "guard_current": 24,
        "guard_max": 24,
        "focus_current": 103,
        "focus_max": 103,
        "injury_level": 0,
        "stance": "balanced_guard",
        "tempo": "steady",
        "hit_chance": 73,
        "critical_chance": 11,
        "mitigation_rating": 18,
        "mutation_count": 0,
        "last_mutation_command": null,
        "last_mutation_event": null,
        "last_mutation_result": null,
        "updated_at_epoch": 0,
        "recent_exchanges": []
    }'::jsonb;

create index if not exists idx_world_trillionnium_characters_combat_numerics_state
    on world_trillionnium_characters using gin (combat_numerics_state);

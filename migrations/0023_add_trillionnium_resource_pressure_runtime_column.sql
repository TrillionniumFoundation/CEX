-- Trillionnium time/stamina/injury/evidence-integrity runtime mirror.
-- Rust WorldState remains the command source of truth; this JSONB column mirrors
-- resource pressure mutation state for normalized parity audits and read-model evolution.

alter table world_trillionnium_characters
    add column if not exists resource_pressure_state jsonb not null default '{
        "day_index": 1,
        "minute_of_day": 480,
        "stamina_current": 100,
        "stamina_max": 100,
        "injury_level": 0,
        "evidence_integrity": 72,
        "evidence_fragments": 0,
        "mutation_count": 0,
        "last_mutation_command": null,
        "last_mutation_event": null,
        "last_mutation_result": null,
        "updated_at_epoch": 0,
        "recent_mutations": []
    }'::jsonb;

create index if not exists idx_world_trillionnium_characters_resource_pressure_state
    on world_trillionnium_characters using gin (resource_pressure_state);

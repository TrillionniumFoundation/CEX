-- Add Rust-owned Trillionnium region graph / story-arc unlock runtime state.

alter table world_trillionnium_characters
    add column if not exists region_story_unlock_state jsonb not null default '{
        "unlocked_region_ids": ["reality-mirror-city"],
        "unlocked_story_arc_ids": ["mirror_city_arrival"],
        "visited_node_ids": ["mirror-city-square"],
        "visited_zone_ids": ["reality-mirror-city"],
        "mutation_count": 0,
        "last_mutation_command": null,
        "last_mutation_event": null,
        "last_mutation_result": null,
        "updated_at_epoch": 0,
        "recent_unlock_events": []
    }'::jsonb;

create index if not exists idx_world_trillionnium_characters_region_story_unlock_state
    on world_trillionnium_characters using gin (region_story_unlock_state);

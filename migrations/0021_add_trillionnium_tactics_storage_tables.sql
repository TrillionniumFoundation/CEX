-- Trillionnium tactics normalized storage boundary.
-- TW-4.8 decision: keep Rust WorldState as command source of truth, but mirror
-- character/session/tick state into typed normalized tables for parity audits,
-- direct-write final-cutover coverage, and read-model evolution.

create table if not exists world_trillionnium_characters (
    matrix_user_id text primary key,
    character_id text not null,
    display_name text not null,
    attributes jsonb not null default '{}'::jsonb,
    sect_id text,
    title text not null,
    skill_ids jsonb not null default '[]'::jsonb,
    updated_at timestamptz not null default now()
);

create index if not exists idx_world_trillionnium_characters_character on world_trillionnium_characters(character_id);
create index if not exists idx_world_trillionnium_characters_updated on world_trillionnium_characters(updated_at desc);

create table if not exists world_tactics_sessions (
    session_id text primary key,
    matrix_user_id text not null,
    room_id text,
    board_id text not null,
    active_node_id text not null references world_map_nodes(node_id),
    active_overlay_id text not null,
    active_unit_id text not null,
    active_side text not null,
    status text not null,
    round integer not null default 1,
    action_points_remaining integer not null default 2,
    current_tick integer not null default 0,
    objective_id text not null default 'defeat_market_bandit',
    objective_progress integer not null default 0,
    objective_goal integer not null default 1,
    victory_state text not null default 'active',
    reward_status text not null default 'not_eligible',
    reward_event_id text,
    reward_credits_awarded integer not null default 0,
    reward_xp_awarded integer not null default 0,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    source_of_truth text not null,
    persistence_owner text not null
);

create index if not exists idx_world_tactics_sessions_user on world_tactics_sessions(matrix_user_id, updated_at desc);
create index if not exists idx_world_tactics_sessions_node on world_tactics_sessions(active_node_id, status);
create index if not exists idx_world_tactics_sessions_victory on world_tactics_sessions(victory_state, reward_status, updated_at desc);

create table if not exists world_tactics_simulation_ticks (
    tick_id text primary key,
    session_id text not null references world_tactics_sessions(session_id),
    matrix_user_id text not null,
    room_id text,
    tick_index integer not null,
    command text not null,
    unit_id text not null,
    target_tile text,
    outcome_result text not null,
    outcome_accepted boolean not null default false,
    simulation_effect text not null,
    round_before integer not null default 1,
    round_after integer not null default 1,
    action_points_before integer not null default 0,
    action_points_after integer not null default 0,
    objective_id text not null default 'defeat_market_bandit',
    objective_progress_before integer not null default 0,
    objective_progress_after integer not null default 0,
    objective_delta integer not null default 0,
    victory_state_before text not null default 'active',
    victory_state_after text not null default 'active',
    reward_status_after text not null default 'not_eligible',
    active_unit_after text not null,
    generated_encounter_id text,
    osm_game_overlay_id text,
    created_at timestamptz not null default now(),
    source_of_truth text not null
);

create unique index if not exists idx_world_tactics_simulation_ticks_session_tick on world_tactics_simulation_ticks(session_id, tick_index);
create index if not exists idx_world_tactics_simulation_ticks_user on world_tactics_simulation_ticks(matrix_user_id, created_at desc);
create index if not exists idx_world_tactics_simulation_ticks_objective on world_tactics_simulation_ticks(objective_id, victory_state_after, created_at desc);

-- Trillionnium League durable game-domain tables.
-- The current MVP can run on the consumer-entry JSON state store; these tables
-- define the Postgres cutover shape for durable seasons, matches, player state,
-- battle history, submissions, scoring, and rewards.

create table if not exists league_seasons (
    season_id uuid primary key default gen_random_uuid(),
    code text not null unique,
    name text not null,
    status text not null,
    starts_at timestamptz,
    ends_at timestamptz,
    created_at timestamptz not null default now()
);

create table if not exists league_players (
    player_id uuid primary key default gen_random_uuid(),
    org_id uuid,
    matrix_user_id text unique,
    display_name text,
    class_tag text not null default 'summoner',
    rank_tier text not null default 'Bronze I',
    rating integer not null default 1000,
    xp integer not null default 0,
    reputation integer not null default 0,
    battles integer not null default 0,
    submissions integer not null default 0,
    wins integer not null default 0,
    earned_credits numeric not null default 0,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists league_matches (
    match_id uuid primary key default gen_random_uuid(),
    season_id uuid references league_seasons(season_id),
    code text not null unique,
    title text not null,
    mode text not null,
    status text not null,
    objective text not null,
    reward text,
    recommended_roles jsonb not null default '[]'::jsonb,
    scoring_rules jsonb not null default '{}'::jsonb,
    reward_rules jsonb not null default '{}'::jsonb,
    starts_at timestamptz,
    ends_at timestamptz,
    created_at timestamptz not null default now()
);

create table if not exists league_match_entries (
    entry_id uuid primary key default gen_random_uuid(),
    match_id uuid not null references league_matches(match_id),
    player_id uuid not null references league_players(player_id),
    status text not null,
    loadout jsonb not null default '{}'::jsonb,
    battles_started integer not null default 0,
    submissions integer not null default 0,
    best_score numeric not null default 0,
    rewards_earned numeric not null default 0,
    joined_at timestamptz not null default now(),
    unique (match_id, player_id)
);

create table if not exists league_battles (
    battle_id uuid primary key default gen_random_uuid(),
    match_id uuid not null references league_matches(match_id),
    entry_id uuid not null references league_match_entries(entry_id),
    player_id uuid not null references league_players(player_id),
    invocation_id uuid,
    execution_id uuid,
    prompt text not null,
    status text not null,
    created_at timestamptz not null default now()
);

create table if not exists league_submissions (
    submission_id uuid primary key default gen_random_uuid(),
    match_id uuid not null references league_matches(match_id),
    entry_id uuid not null references league_match_entries(entry_id),
    player_id uuid not null references league_players(player_id),
    invocation_id uuid,
    task_id uuid,
    body jsonb not null default '{}'::jsonb,
    score numeric,
    grade text,
    reward_amount numeric not null default 0,
    status text not null default 'scored',
    submitted_at timestamptz not null default now(),
    scored_at timestamptz
);

create table if not exists league_score_events (
    score_event_id uuid primary key default gen_random_uuid(),
    submission_id uuid not null references league_submissions(submission_id),
    dimension text not null,
    score numeric not null,
    weight numeric not null,
    judge_kind text not null,
    evidence jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now()
);

create table if not exists league_reward_events (
    reward_event_id uuid primary key default gen_random_uuid(),
    match_id uuid not null references league_matches(match_id),
    entry_id uuid references league_match_entries(entry_id),
    player_id uuid not null references league_players(player_id),
    account_id uuid,
    ledger_entry_id uuid,
    reward_kind text not null,
    amount numeric not null,
    currency_unit text not null default 'credit',
    reason text not null,
    created_at timestamptz not null default now()
);

create index if not exists idx_league_players_matrix_user on league_players(matrix_user_id);
create index if not exists idx_league_matches_season on league_matches(season_id, status);
create index if not exists idx_league_entries_match on league_match_entries(match_id, status);
create index if not exists idx_league_battles_player on league_battles(player_id, created_at desc);
create index if not exists idx_league_submissions_match_score on league_submissions(match_id, score desc nulls last);
create index if not exists idx_league_rewards_player on league_reward_events(player_id, created_at desc);

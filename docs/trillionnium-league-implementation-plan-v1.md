# Trillionnium League / World Implementation Plan v1

## Goal

Make CEX's frontend feel like a real game: first a Dota/WoW-inspired AI-agent esports league, then **Trillionnium World** — a high-freedom reality-mirror sandbox where players build companies, assets, shops, Agent teams, guilds, and real-task economies.

## Current Baseline

Already available:

- Matrix/Element real-room loop.
- `/task` task creation.
- `/status` task status card.
- `/wallet` / `/balance` wallet card.
- `/plans` package card.
- Ledger-backed account balance.
- Gateway invocation and execution status.
- Matrix `formatted_body` and `cex_card` projection.
- First League backend MVP endpoints are now available in `consumer-entry-api` with an in-memory preseason state: home, matches, join, battle, rankings, profile, loadout.
- First real-room League E2E passed through Matrix/Element with `/league`, `/arena`, `/join`, `/battle`, `/rank`, `/loadout`.
- The League state now persists to `CONSUMER_ENTRY_LEAGUE_STATE_PATH` in the Linux runtime entry config, so player entries, battles, submissions, rewards, XP, rating, and earned credits survive process restarts.
- SQL cutover bridge is active: each persisted League state can also write a SQL-ready JSONB snapshot to `CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH`, backed by `migrations/0011_add_trillionnium_league_state_snapshots.sql` and verified by `scripts/check-trillionnium-league-sql-snapshot.sh`.
- The Matrix E2E now also covers `/submit`, `/profile`, `/rewards`, and `/history`.
- The Matrix E2E now also covers `/world`, `/season`, `/guild`, guild join, and `/draft`, adding MMO map/guild and Dota-like draft feel.
- League submission rewards now have a ledger settlement path: `consumer-entry-api` resolves the Matrix player account and calls ledger `/v1/ledger/grant`, while the Matrix submission card exposes `ledger_status` and `ledger_entry_id`; real-room E2E checks `ledger_status=settled` and the post-submit wallet balance.
- Submission scoring now uses Judge Pipeline v2: rubric dimensions remain the base score, hidden tests add objective pass/fail events, and an optional provider-agnostic LLM judge adapter can append a zero-weight disagreement/audit event without breaking local-dev E2E.
- Anti-cheat payout hold is active: submissions flagged by rubric, hidden tests, repetition checks, or judge-disagreement gates are marked `payout_status=review_hold`, ledger settlement is skipped with `ledger_status=held_review`, and normal eligible submissions continue to settle.
- Review/admin release flow is active: held rewards can be listed and approved/rejected through `/v1/league/reviews/*`; approval marks the submission `approved_release` and performs the ledger grant through the same idempotent settlement path.
- Player inventory/loot is active: eligible submissions mint cosmetic League items, `/inventory` returns the player's bag, and the web shell shows item count/top loot.
- Guild raid contribution is active: `/raid` lists guild raids, `/raid <raid-id> <action>` records team progress, and the web shell can contribute to `guild-raid-001`.
- Raid team roster is active: `/team` shows the `guild-raid-001` roster, `/team <raid-id> <role>` claims a Scout/Builder/Auditor/Closer-style team role, and the backend records active raid slots with hero IDs.
- A server-rendered web game shell is available at `GET /league` on `consumer-entry-api`, showing the Trillionnium League lobby, match cards, live stats, leaderboard, rewards, playable commands, guild halls, current loadout, replay timeline, and a local-dev Web Battle Console.
- The web shell now has local-dev playable forms via `POST /league/web/action` for join/guild/team/raid/draft/submit, plus a battle timeline/replay panel that shows ledger settlement state.
- Production Web session gate is active: `/league/web/session` can mint an HttpOnly SameSite web session cookie from signed upstream auth, and `/league/web/action` uses signed session + CSRF outside local-dev while preserving the local-dev playable shell.
- Trillionnium World first slice is active: `GET /v1/world/home` returns zones/locations/Agent residents/assets/events, `POST /v1/world/action` records free-form reality-mirror actions, Matrix `/world action <自由行动>` mutates world state, and `GET /world` exposes a playable web World shell with a CSRF-protected `/world/web/action` console.

This is enough to build the first League MVP inside Matrix before creating a custom web game shell.

## Brand Architecture

- **Trillionnium World**: top-level open world and reality mirror.
- **Trillionnium League**: competitive quests, raids, judging, ranks, and rewards inside World.
- **Trillionnium Craft**: building/workshop/company/asset creation inside World.
- **Trillionnium Ledger**: credit, asset, payout, and audit settlement.
- **Trillionnium Agents**: Agent residents, hirelings, heroes, NPCs, and guild members.

## Architecture

```text
Element / Web Game Shell
  -> matrix-bot-poller / matrix-bot-relay
  -> matrix-entry-adapter
  -> consumer-entry-api
  -> world + league domain read/write endpoints
  -> gateway / execution / ledger / audit
```

## Phase 1: Matrix Playable League MVP

Status: first command slice implemented.

### Commands

Add to `matrix-entry-adapter`:

- `/league` / `/tl`
  - returns Trillionnium League home card.
- `/arena`
  - returns active match list.
- `/join <match_id>`
  - joins a match.
- `/battle <match_id> <prompt>`
  - creates a CEX task linked to match entry.
- `/rank`
  - returns leaderboard.
- `/loadout`
  - returns current agent hero lineup.
- `/world`
  - returns Trillionnium World open-world map, locations, residents, assets, and recent events.
- `/world action <free text>`
  - records a free-form world action such as opening a company, building a shop, hiring an Agent, exploring a market, or mapping a real-world task into the world.
- `/craft <build text>`
  - shortcut into Trillionnium Craft; records a `craft` world action in `starter-studio` and mints a craft-style asset seed.
- `/season`
  - returns current season stats.
- `/guild` / `/guild <guild_id>`
  - lists guilds or joins a guild.
- `/raid` / `/raid <raid_id> <body>`
  - lists guild raids or contributes team progress to a raid.
- `/team` / `/team <raid_id> <role>`
  - lists the raid roster or claims a raid role with an Agent hero mapping.
- `/draft <hero...>`
  - locks an Agent hero lineup.
- `/submit <match_id> <body>`
  - creates a scored submission and reward event.
- `/profile`
  - returns player rank, XP, reputation, and earned credits.
- `/rewards`
  - returns accumulated League rewards.
- `/inventory`
  - returns earned cosmetic/loot items.
- `/history`
  - returns battles and submissions.

Implemented behavior:

- `/league`, `/arena`, `/quest`, `/rank`, `/loadout` call `consumer-entry-api` League endpoints and project game cards.
- `/join <match_id>` calls the backend join endpoint and creates/returns a player entry.
- `/battle <match_id> <prompt>` calls the backend battle endpoint, which creates a CEX task with League metadata and returns a League battle card.
- `/submit <match_id> <body>` calls the backend submit endpoint, which calculates a first deterministic score, updates player XP/rating/earned credits, persists the event, grants the reward through ledger when account resolution/admin token are available, and returns a reward card with settlement status.
- `/world`, `/season`, `/guild`, `/raid`, `/team`, and `/draft` call backend game-state endpoints and make the Matrix MVP feel closer to an MMO/MOBA loop.

### Consumer endpoints

Add to `consumer-entry-api`:

- `GET /v1/league/home`
- `GET /v1/league/matches`
- `GET /v1/league/world`
- `GET /v1/league/season`
- `GET /v1/league/raids`
- `POST /v1/league/raids/:id/contribute`
- `GET /v1/league/raids/:id/roster`
- `POST /v1/league/raids/:id/roster`
- `GET /v1/league/reviews/held`
- `POST /v1/league/reviews/:reward_id/approve`
- `POST /v1/league/reviews/:reward_id/reject`
- `GET /v1/league/guilds`
- `POST /v1/league/guilds/:id/join`
- `POST /v1/league/matches/:id/join`
- `POST /v1/league/matches/:id/battle`
- `GET /v1/league/rankings`
- `GET /v1/league/players/:matrix_user_id/profile`

Also added:

- `GET /v1/league/players/:matrix_user_id/loadout`
- `POST /v1/league/players/:matrix_user_id/draft`
- `GET /v1/league/players/:matrix_user_id/rewards`
- `GET /v1/league/players/:matrix_user_id/inventory`
- `GET /v1/league/players/:matrix_user_id/history`
- `POST /v1/league/matches/:id/submit`
- `POST /league/web/action` local-dev-only server-side Web Battle Console action bridge for join/guild/team/raid/draft/submit
- `POST /v1/ledger/grant` in `ledger-service` for verified League reward settlement
- `GET /league`
- `POST /league/web/action` local-dev-only form action endpoint for the web game shell.

### MVP data source

For the first cut, use an in-memory/static starter league read model to move fast:

- `daily-dungeon-001`: 5-minute solo quest.
- `bounty-arena-001`: small bounty match.
- `guild-raid-001`: playable guild raid with contribution progress and raid team roster slots.

Then migrate to database tables. Normalized migration skeleton exists at `migrations/0010_add_trillionnium_league_tables.sql`; the low-risk cutover bridge exists at `migrations/0011_add_trillionnium_league_state_snapshots.sql`.

### SQL Snapshot Bridge

Current runtime defaults:

- `CONSUMER_ENTRY_LEAGUE_STATE_PATH=$ENTRY_CONFIG_DIR/league-state.json`
- `CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH=$ENTRY_CONFIG_DIR/league-state-snapshot.sql`

The snapshot file is a replayable SQL insert into `league_state_snapshots(snapshot_kind, state_hash, state)`. This keeps local-dev and Matrix/Web E2E stable while giving the SQL cutover a durable checkpoint, state hash, and DB-loadable JSONB artifact. Once production Postgres is wired, the same state can be migrated into the normalized 0010 tables or consumed directly as a SQL repository snapshot.

- `GET /v1/league/state/snapshot` returns repository status, state hash, and object counts.
- `scripts/check-trillionnium-league-sql-snapshot.sh` validates the generated SQL snapshot and the status endpoint without exposing ingress tokens.

## Phase 1.5: Trillionnium World Open Sandbox

Implemented first-slice concepts:

- world zones: `reality-mirror-city`, `craft-district`, `market-bazaar`, `league-arena`
- locations: city square, starter studio, ZBJ market gate, League coliseum
- entities: Agent residents and NPC-style helpers
- assets: player-created ventures/builds from free-form actions
- events: durable world action log with impact score
- relationships: player-to-location/entity/asset relationship changes

Endpoints:

- `GET /v1/world/home`
- `POST /v1/world/action`
- `GET /world`
- `POST /world/web/action`

Matrix:

- `/world` returns the Trillionnium World card.
- `/world action <free text>` records a sandbox action such as opening an AI design company, building a shop, hiring an Agent, exploring a market, or mapping a real-world task into the world.
- `/craft <build text>` records a Trillionnium Craft build action and creates a reusable asset seed.

Web shell:

- `GET /world` renders World zones, locations, Agent residents/NPCs, player assets, and the world event timeline.
- `POST /world/web/action` uses the same signed web session + CSRF model as League web actions outside local-dev, while local-dev remains playable.

SQL shape:

- `migrations/0012_add_trillionnium_world_tables.sql`

## Phase 2: Durable League Domain

### Migration: league tables

Recommended tables:

```sql
create table league_seasons (
  season_id uuid primary key default gen_random_uuid(),
  code text not null unique,
  name text not null,
  status text not null,
  starts_at timestamptz,
  ends_at timestamptz,
  created_at timestamptz not null default now()
);

create table league_players (
  player_id uuid primary key default gen_random_uuid(),
  org_id uuid,
  matrix_user_id text unique,
  display_name text,
  class_tag text,
  rank_tier text not null default 'bronze',
  rating integer not null default 1000,
  xp integer not null default 0,
  reputation integer not null default 0,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table league_matches (
  match_id uuid primary key default gen_random_uuid(),
  season_id uuid references league_seasons(season_id),
  code text not null unique,
  title text not null,
  mode text not null,
  status text not null,
  objective text not null,
  scoring_rules jsonb not null default '{}'::jsonb,
  reward_rules jsonb not null default '{}'::jsonb,
  starts_at timestamptz,
  ends_at timestamptz,
  created_at timestamptz not null default now()
);

create table league_match_entries (
  entry_id uuid primary key default gen_random_uuid(),
  match_id uuid not null references league_matches(match_id),
  player_id uuid not null references league_players(player_id),
  status text not null,
  loadout jsonb not null default '{}'::jsonb,
  joined_at timestamptz not null default now(),
  unique (match_id, player_id)
);

create table league_submissions (
  submission_id uuid primary key default gen_random_uuid(),
  match_id uuid not null references league_matches(match_id),
  entry_id uuid references league_match_entries(entry_id),
  invocation_id uuid,
  execution_id uuid,
  status text not null,
  body jsonb not null default '{}'::jsonb,
  score numeric,
  submitted_at timestamptz not null default now(),
  scored_at timestamptz
);

create table league_score_events (
  score_event_id uuid primary key default gen_random_uuid(),
  submission_id uuid not null references league_submissions(submission_id),
  dimension text not null,
  score numeric not null,
  weight numeric not null,
  judge_kind text not null,
  evidence jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now()
);

create table league_reward_events (
  reward_event_id uuid primary key default gen_random_uuid(),
  match_id uuid not null references league_matches(match_id),
  player_id uuid not null references league_players(player_id),
  account_id uuid,
  ledger_entry_id uuid,
  reward_kind text not null,
  amount numeric not null,
  currency_unit text not null,
  reason text not null,
  created_at timestamptz not null default now()
);
```

## Phase 3: Scoring and Rewards

### Score dimensions

- quality
- speed
- cost_efficiency
- evidence
- customer_fit
- sportsmanship

### Reward rules

- Daily dungeon: small credits/XP.
- Bounty arena: prize pool split by placement.
- Guild raid: contribution-based split.
- Ranked ladder: rating/XP, periodic prize events.

### Ledger integration

Use `LedgerEntry.reference_type` values:

- `league_match_entry_fee`
- `league_reward`
  - settled through ledger `/v1/ledger/grant` with idempotency key `league_reward:<reward_id>`.
- `league_prize_pool`
- `league_refund`

## Phase 4: Web Game Shell

Build after Matrix loop proves gameplay.

Status: first server-rendered shell implemented at `/league`.

Recommended stack can be decided later, but screens should be stable now:

- `/league` lobby
- `/league/world` world map
- `/league/matches/:id` match room
- `/league/loadout` agent heroes
- `/league/rankings` leaderboard
- `/league/guilds/:id` guild hall
- `/league/wallet` wallet/rewards

The current `/league` shell is intentionally same-origin and server-rendered, so it can show live local state without exposing ingress tokens to browser JavaScript. Later web work can split this into a richer app shell once product auth/session issuance is ready for browser clients.

## Production Web Session Gate

- `POST /league/web/session` issues a signed League web session cookie.
- In local-dev, the session endpoint may be used directly for E2E; in beta/production it requires the existing signed user-session headers and request fingerprint `league-web-session:<matrix_user_id>:<room_id>:<session_id>` with source kind `league_web_session`.
- The cookie is HttpOnly, SameSite=Lax, and Secure outside local-dev.
- `/league/web/action` binds the acting player to the signed session and checks the submitted CSRF token before mutating League state.
- Runtime knobs:
  - `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_REQUIRED`
  - `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET` / `CONSUMER_ENTRY_WEB_SESSION_SECRET`
  - `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_COOKIE`
  - `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_TTL_SECS`

## Judge Pipeline v2

Runtime knobs:

- `CONSUMER_ENTRY_LEAGUE_HIDDEN_TESTS=true|false` enables objective hidden-test score events.
- `CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_URL` optionally points to a provider-agnostic HTTP judge adapter.
- `CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_TOKEN` is sent as bearer auth when configured.
- `CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_REQUIRED=true` turns adapter failures into payout review holds; local-dev defaults to optional/fallback.
- `CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_TIMEOUT_MS` bounds adapter latency.

The local green path stays deterministic: rubric + hidden tests + `llm_judge_adapter: not_configured` audit event. When a provider is configured, its score/verdict is appended as a zero-weight event; large disagreement raises `judge_disagreement` and routes payout to review instead of silently changing the base score.

## Anti-cheat / Safety

- No direct betting/wagering MVP.
- Entry fee must be compute/platform cost or clearly governed prize pool contribution.
- Store full audit/replay trail.
- Add duplicate output detection.
- Add judge disagreement flag.
- Add payout hold for suspicious wins.

## Acceptance Criteria for MVP 1

A user in the real Matrix room can:

1. Send `/league` and see Trillionnium League home.
2. Send `/arena` and see active game modes/matches.
3. Send `/join daily-dungeon-001` and get joined confirmation.
4. Send `/battle daily-dungeon-001 <prompt>` and create a CEX task linked to the match.
5. Send `/rank` and see at least local starter leaderboard.
6. Send `/wallet` and see existing wallet card.

The E2E script should validate these commands in the same real Matrix room.

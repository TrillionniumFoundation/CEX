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
- Player progression systems are active: `/progression|/level`, `/skills`, `/tools`, and `/skins` expose 门派/guild-school alignment, skills, equipment/tools, multi-agent skins, experience data points, and level computed from successful task count.
- Guild raid contribution is active: `/raid` lists guild raids, `/raid <raid-id> <action>` records team progress, and the web shell can contribute to `guild-raid-001`.
- Raid team roster is active: `/team` shows the `guild-raid-001` roster, `/team <raid-id> <role>` claims a Scout/Builder/Auditor/Closer-style team role, and the backend records active raid slots with hero IDs.
- A server-rendered web game shell is available at `GET /league` on `consumer-entry-api`, showing the Trillionnium League lobby, match cards, live stats, leaderboard, rewards, playable commands, guild halls, current loadout, replay timeline, and a local-dev Web Battle Console.
- The web shell now has local-dev playable forms via `POST /league/web/action` for join/guild/team/raid/draft/submit, plus a battle timeline/replay panel that shows ledger settlement state.
- Production Web session gate is active: `/league/web/session` can mint an HttpOnly SameSite web session cookie from signed upstream auth, and `/league/web/action` uses signed session + CSRF outside local-dev while preserving the local-dev playable shell.
- First game account client is active at `GET /account` and `GET /game/account`: it exposes player register/sign-in/password-change/session-refresh/session-revoke forms and actions, optional self-serve Argon2id password registration/login through `POST /account/register` and `POST /account/login`, signed-session password changes through `POST /account/password/change`, signed-session refresh through `POST /account/session/refresh`, all-device game-account session revocation through `POST /account/session/revoke`, server-verifiable status through `GET /account/session`, logout through `POST /account/logout`, and signed game-session cookies compatible with `/league/web/session`. It still does not count as public-launch account readiness without real external account/security review evidence.
- Trillionnium Client App first slice is active: `GET /app` and `GET /v1/client/app/:matrix_user_id` expose a mobile-style shell that integrates World Map (global real-world Leaflet/OpenStreetMap mirror with lightweight Hero's Tale + Gather LOD overlays), Face Duel (Pokémon-like nearby battle via `face-duel-001`), Social (WeChat/Telegram-like room/contact layer), Wallet (Alipay-like credit wallet), and Progression (门派/skills/tools/skins/XP/level). Matrix `/app`, `/duel nearby <出招>`, `/social`, `/pay`, `/progression`, `/skills`, `/tools`, and `/skins` project these modules back into the room.
- Trillionnium World first slice is active: `GET /v1/world/home` returns zones/locations/detailed map nodes/player positions/Agent residents/assets/upgrades/companies/shops/listings/purchases/work orders/work deliveries/work acceptances/work rejections/work reopens/work cancellations/factions/economy events/contracts/completions, `GET /v1/world/map/:matrix_user_id` and `POST /v1/world/map/move` power a Hero's Tale + Gather style text map, `GET /v1/world/map/:matrix_user_id/viewport` now exposes the map-engine viewport contract for active region shards / Web Mercator tile shards / LOD / nearby POIs, `/world/web/map-viewport` mirrors that viewport stream into the signed web shell for live Leaflet hydration, `POST /v1/world/action` records free-form reality-mirror actions, Matrix `/world action <自由行动>` mutates world state, `GET /world` exposes a playable web World shell with a real-world Leaflet/OpenStreetMap map panel plus live tile/region/POI viewport hydration and CSRF-protected map-move/action/asset/company/listing/buy/work-deliver/work-accept/work-reject/work-reopen/work-cancel/contract consoles, `/contract <委托内容>` creates a real CEX task-backed World Contract, `/complete <contract-id> <交付内容>` scores/settles/upgrades World state, `/upgrade <asset-id|latest> <升级内容>` grows persistent World assets, `/company <asset-id|latest> <公司方案>` launches an operating company/shop from an asset, `/sell <company-id|latest> <服务/商品>` publishes a priced listing, `/buy <listing-id|latest> <需求>` creates a purchase + work order with buyer ledger reserve plus seller ledger settlement/faction standing, `/work deliver <work-id|latest> <交付内容>` records fulfillment proof, `/work accept <work-id|latest> <验收内容>` closes the service loop while attempting buyer ledger consume, `/work reject <work-id|latest> <拒收原因>` rejects delivered work while attempting buyer ledger refund, `/work reopen <work-id|latest> <返工要求>` re-reserves buyer funds so a rejected order can be redelivered, and `/work cancel <work-id|latest> <取消原因>` cancels open work before delivery while attempting buyer ledger refund; `/map` and `/go <direction|node-id>` expose fine-grained map exploration and position persistence.

This is enough to build the first League MVP inside Matrix before creating a custom web game shell.

Companion architecture notes for the current World evolution path:

- `docs/trillionnium-real-world-map-engine-evaluation-v1.md`
- `docs/trillionnium-open-source-stack-reference-v1.md`
- `docs/trillionnium-world-ecs-refactor-v1.md`

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
- `/progression` / `/level`
  - returns 门派、successful-task-count level, experience data points, and unlock counts.
- `/skills`
  - returns the player's skill tree unlocks.
- `/tools`
  - returns equipment/tools plus earned item power.
- `/skins`
  - returns multi-agent skin/capability unlocks.
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
- `/progression`, `/skills`, `/tools`, and `/skins` call the player progression endpoint and turn guild/faction, data accumulation, successful task counts, tools, and multi-agent capacity into Matrix cards.

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

- `GET /v1/league/players/:matrix_user_id/progression`
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
- asset upgrades: judged work that increases asset value/level and records an upgrade tree
- companies: judged operating entities launched from owned assets with level/revenue/reputation scores
- shops/listings: storefronts and priced service offers that form the first persistent commerce loop
- purchases/work orders: buying or hiring a listing creates durable commerce/work state and credits seller revenue through Ledger when configured
- factions/standings: Market Guild, Craft Union, City Clerks, and League Order reputation grows from commerce/work activity
- economy events: durable revenue/reputation deltas for launches, listings, and purchases
- contracts: task-backed real-world commissions linked to CEX invocation IDs
- completions: judged delivery records with hidden-test scoring, ledger settlement status, payout hold gates, and asset/reputation growth
- events: durable world action log with impact score and optional CEX task link
- relationships: player-to-location/entity/asset relationship changes

Endpoints:

- `GET /v1/world/home`
- `POST /v1/world/action`
- `GET /world`
- `POST /world/web/action`

Matrix:

- `/world` returns the Trillionnium World card.
- `/world action <free text>` records a sandbox action such as opening an AI design company, building a shop, hiring an Agent, exploring a market, or mapping a real-world task into the world.
- `/assets` lists World assets and upgrade counts.
- `/upgrade <asset-id|latest> <upgrade text>` judges and applies a World asset upgrade.
- `/companies` lists player companies; `/company <asset-id|latest> <company text>` turns an owned asset into an operating company, shop, starter listing, owner relationship, and economy event.
- `/shops` lists storefronts/listings; `/sell <company-id|latest> <listing text>` publishes a priced service listing and updates company/shop economy scores.
- `/buy <listing-id|latest> <brief>` buys/hires a listing, creates a purchase + work order, attempts buyer ledger reserve, settles seller ledger revenue when configured, and updates faction standings.
- `/work deliver <work-id|latest> <brief>` records seller delivery evidence and Judge Pipeline scoring.
- `/work accept <work-id|latest> <brief>` records buyer acceptance, attempts buyer ledger consume, completes the work order, and increases company/player/faction reputation.
- `/work reject <work-id|latest> <brief>` records buyer rejection, attempts buyer ledger refund, marks purchase/work as rejected/refunded, and keeps an economy-event audit trail.
- `/work reopen <work-id|latest> <brief>` records buyer reopen/revision requirements, attempts a fresh buyer ledger reserve, returns the work order to `open`, and allows seller redelivery plus later buyer consume.
- `/work cancel <work-id|latest> <brief>` records buyer cancellation before delivery, attempts buyer ledger refund, and closes the purchase/work order as cancelled/refunded.
- `/work` lists purchases/work orders/deliveries/acceptances; `/factions` lists factions and player standings.
- `/craft <build text>` records a Trillionnium Craft build action and creates a reusable asset seed.
- `/contract <commission text>` records a World Contract and creates a real CEX task/invocation through the same signed Matrix identity path used by League battles.
- `/complete <contract-id> <delivery text>` submits the delivery, runs Judge Pipeline v2, grants ledger rewards when eligible, updates contract status, and upgrades the player's World assets/reputation.

Web shell:

- `GET /world` renders World zones, locations, Agent residents/NPCs, player assets, asset upgrade form, company launch form, shop/listing publish form, buy/work-order form, delivery/acceptance forms, faction reputation map, World Contracts, completion form, and the world event timeline.
- `POST /world/web/action` uses the same signed web session + CSRF model as League web actions outside local-dev, while local-dev remains playable.

SQL shape:

- `migrations/0012_add_trillionnium_world_tables.sql`
- `migrations/0013_add_trillionnium_world_contracts.sql`
- `migrations/0014_add_trillionnium_world_contract_completions.sql`
- `migrations/0015_add_trillionnium_world_asset_upgrades.sql`
- `migrations/0016_add_trillionnium_world_companies.sql`
- `migrations/0017_add_trillionnium_world_commerce.sql`

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

Status: first server-rendered shell implemented at `/league`, with a same-origin account/session client at `/account` and `/game/account`.

Recommended stack can be decided later, but screens should be stable now:

- `/league` lobby
- `/league/world` world map
- `/league/matches/:id` match room
- `/league/loadout` agent heroes
- `/league/rankings` leaderboard
- `/league/guilds/:id` guild hall
- `/league/wallet` wallet/rewards

The current `/league` shell is intentionally same-origin and server-rendered, so it can show live local state without exposing ingress tokens to browser JavaScript. `/account` and `/game/account` now provide a browser account surface for register/sign-in/password-change/session-refresh/session-revoke, optional server-side Argon2id password auth, signed game-session creation, current session status, and logout. Password auth is disabled unless explicitly configured with a registry path and session secret.

## Production Web Session Gate

- `POST /league/web/session` issues a signed League web session cookie.
- `GET /account` and `GET /game/account` expose the game account client for register/sign-in, local profile hint storage, and current session status.
- `POST /account/register` creates a game account with Argon2id password hash when `CONSUMER_ENTRY_GAME_ACCOUNT_PASSWORD_AUTH_ENABLED=true`, persists it to the configured registry path, and mints the same HttpOnly signed game-session cookie.
- `POST /account/login` verifies the Argon2id password hash and mints a fresh signed game-session cookie.
- Account-issued cookies carry a server-side game-account session generation. The auth layer rejects account cookies whose generation no longer matches the persisted account record.
- `POST /account/password/change` requires the active signed game-session cookie plus CSRF and the current password, then replaces only the Argon2id password hash, bumps the session generation, and returns a refreshed cookie so old account cookies are revoked while the current browser remains signed in.
- `POST /account/session/refresh` requires the active signed game-session cookie plus CSRF and mints a refreshed HttpOnly session cookie with a rotated CSRF value.
- `POST /account/session/revoke` requires the active signed game-session cookie plus CSRF, bumps the session generation, and clears the current cookie so all previously issued game-account cookies become invalid.
- Register/login attempts are rate-limited by normalized account id and request source before password verification.
- Account auth observability uses aggregate counters only: register successes, login successes, login failures, password-change successes/failures, session-refresh successes, session-revoke successes, logout successes, and account-auth rate-limit hits. The observability contract explicitly forbids logging passwords, tokens, or cookie values.
- `GET /account/session` reports the current signed game-session status; `POST /account/logout` expires the server cookie.
- `scripts/check-trillionnium-game-account-auth.sh` gates the account client contract, endpoint advertisement, no-cookie session status, password-auth posture, rate-limit configuration, aggregate auth metrics exposure, password-change/session-refresh/session-revoke contracts, and public-launch boundary. Its default mode is read-only; `--mutating` is opt-in for local/password-auth smoke and verifies register/session/password-change/session-refresh/session-revoke/logout, Argon2id registry storage, plaintext absence, old-password rejection, new-password login, CSRF rotation, all-device revocation, and bad-login rate limiting.
- In local-dev, the session endpoint may be used directly for E2E; in beta/production it requires the existing signed user-session headers and request fingerprint `league-web-session:<matrix_user_id>:<room_id>:<session_id>` with source kind `league_web_session`.
- The cookie is HttpOnly, SameSite=Lax, and Secure outside local-dev.
- `/league/web/action` binds the acting player to the signed session and checks the submitted CSRF token before mutating League state.
- Runtime knobs:
  - `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_REQUIRED`
  - `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET` / `CONSUMER_ENTRY_WEB_SESSION_SECRET`
  - `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_COOKIE`
  - `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_TTL_SECS`
  - `CONSUMER_ENTRY_GAME_ACCOUNT_PASSWORD_AUTH_ENABLED`
  - `CONSUMER_ENTRY_GAME_ACCOUNT_REGISTRY_PATH`
  - `CONSUMER_ENTRY_GAME_ACCOUNT_PASSWORD_MIN_CHARS`
  - `CONSUMER_ENTRY_GAME_ACCOUNT_AUTH_RATE_LIMIT_MAX_REQUESTS`
  - `CONSUMER_ENTRY_GAME_ACCOUNT_LOCAL_DOMAIN`
  - `CEX_GAME_ACCOUNT_AUTH_MUTATING_SMOKE` for explicitly enabling the mutating gate smoke outside production signoff.

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

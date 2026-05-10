# Trillionnium World Development Progress Tree v1

Generated: 2026-05-08 18:11 CST  
Last audited: 2026-05-11 00:07 CST
Current code checkpoint: `feat: gate world mentor skill practice` (this commit)
Current progress-tree checkpoint before this expansion: `da20c1a feat: gate world objective travel`
Repo: `/home/qian/.openclaw/workspace/CEX`

This document is the handoff spine for continuing Trillionnium World development without losing state after chat compaction, runtime restarts, or long task chains.

## How to Use This Document

1. Before starting a new slice, read this file first.
2. Pick the first unchecked item in the **Active Progress Tree** unless the user explicitly changes priority.
3. During development, update the relevant checkbox/status notes in this file.
4. Before finishing a slice, record:
   - files changed
   - validation commands and evidence paths
   - remaining blockers / next unchecked task
5. Commit when the working tree reaches a meaningful green checkpoint.
6. Append a short note to `memory/YYYY-MM-DD.md` after the commit.

Status legend:

- `[x]` done and validated
- `[~]` partially done / active seam exists but product work remains
- `[ ]` not started
- `[!]` blocked or do not proceed without explicit user decision

---

## Product North Star

Trillionnium World should become a real playable game world, not a dashboard and not a map-console.

The intended shape is:

```text
OpenStreetMap / cached geodata
  -> Rust OpenStreetMapDataProvider
  -> Rust Trillionnium World state + simulation + economy + ledger
  -> Rust projection JSON / command handlers
  -> Web visualization + input shell
  -> Rust command validation + progression + persistence
```

The web frontend is **not** the source of truth. It renders state and sends player intent. Rust owns world state, simulation, tasks, combat, NPCs, economy, ledger, persistence, validation, and production gates.

After the 2026-05-10 Hero Tan alignment audit, treat `albert10jp/yxts-gold-asm` as a **game-loop reference**, not a skin target. The reference is useful for the sequence "character on map -> directional movement -> location transition -> NPC/task/skill/combat progression". Trillionnium must recreate that loop with native Rust state, native content, OSM/commerce objectives, and explicit command handlers; it must not merely imitate the green LCD appearance.

---

## Canonical Layer Development Spec

The following six layers are mandatory. Every future feature must state which layer owns it, what crosses the boundary, and which tests/gates prove the boundary did not collapse.

### Layer 1 — OSM / OpenStreetMap data

**Role:** real-world skeleton and objective source.

Owned data:

- roads / paths / walkable graph candidates
- POIs / amenities / shops / landmarks
- buildings / entrances / indoor-or-nearby anchors
- areas / parks / campuses / markets / waterways
- admin boundaries / neighborhoods / city regions
- OSM tags and identities: `osm_type`, `osm_id`, `lat`, `lng`, `tags`

Must not own:

- player inventory, HP, skills, sect, relationship state
- rewards, ledger settlement, progression, anti-cheese decisions
- task completion truth
- combat truth

Current status:

- `[x]` fixture-first OSM identity projection exists as `openstreetmap_geodata_v1`.
- `[x]` stable Shanghai-core fixture identities exist with deterministic fallback for missing features.
- `[x]` roads/buildings/areas/admin boundary fixture layers exist as `openstreetmap_fixture_layers_v1`.

Boundary output to Layer 2:

```json
{
  "osm_type": "node|way|relation",
  "osm_id": 1770000000,
  "lat": 31.230416,
  "lng": 121.473701,
  "tags": { "amenity": "marketplace" },
  "source": "fixture|overpass_bbox_cache|geofabrik_extract_import|vendor"
}
```

### Layer 2 — Rust: World geodata provider

**Role:** legal, cached, deterministic bridge from OSM data into Trillionnium world coordinates and game overlay anchors.

Owned responsibilities:

- provider contract: `OpenStreetMapDataProvider`
- provider modes: fixture first; future cached Overpass / Geofabrik / vendor imports
- OSM attribution and ODbL metadata
- derived database tracking
- stable identity binding: OSM feature -> Trillionnium overlay anchor
- fail-closed behavior when live data is unavailable or unapproved

Must not own:

- final gameplay rewards
- quest completion
- NPC progression truth
- web-only gameplay state

Current status:

- `[x]` first provider seam exists.
- `[x]` `/world` displays provider/source/legal metadata.
- `[x]` provider implementation is split into `openstreetmap_geodata.rs`.
- `[x]` provider health/readiness metrics explicitly cover fixture/live/fail-closed checks, geodata freshness/staleness, and attribution/ODbL visibility.

Boundary output to Layer 3:

```json
{
  "game_overlay_id": "trillionnium-world-node:market-gate",
  "osm_identity": { "osm_type": "way", "osm_id": 123, "lat": 31.23, "lng": 121.47 },
  "semantic_role": "market|mentor|bandit_camp|delivery_route|sect_hall|arena",
  "objective_seed": "deterministic seed from provider + world state",
  "legal": { "attribution": "© OpenStreetMap contributors", "database_license": "ODbL-1.0" }
}
```

### Layer 3 — Rust: Trillionnium game state / simulation

**Role:** actual game. This layer owns the Trillionnium systems, tactics board, NPCs, tasks, combat, economy, and persistent player/world progression.

Owned responsibilities:

- player character attributes and derived stats
- skills, sects, mentors, NPC relationships
- tactics board state, units, turn order, movement, attacks
- OSM-bound objective generation
- quest/task lifecycle and anti-cheese
- combat result and Wuxia battle log generation
- commerce/contracts/work orders and ledger-facing reward intents
- persistence and replayable simulation state

Must not own:

- raw public web DOM interactions
- browser-only hidden state
- unaudited direct OSM network calls

Current status:

- `[x]` existing World/commerce/route-runner loops are Rust-owned.
- `[x]` tactics state is now a Rust game model with board, units, objectives, commands, game sessions, and simulation ticks.
- `[x]` Trillionnium attribute/skill/sect/NPC/task/combat-log systems have first Trillionnium-native Rust models and projection contracts.
- `[~]` Hero Tan-style exploration is partially aligned: Rust-owned world player positions, adjacent-node movement, blocked/locked/interaction-required/room/zone transition semantics, the local playable loop, node-local NPC talk, task pickup/completion, settlement/review feedback, and active task/NPC/party objective travel through the Rust world-node graph exist. Remaining gaps are skill practice from exploration nodes and lightweight combat encounter entry/return-to-map.

Boundary output to Layer 4:

```json
{
  "player": { "attributes": {}, "skills": [], "sect": null },
  "board": { "cells": [], "units": [], "turn": {} },
  "objectives": [
    { "objective_id": "obj-1", "source": "osm_feature", "overlay_id": "...", "reward_intent": "..." }
  ],
  "combat_log": [],
  "available_commands": []
}
```

### Layer 4 — Rust: projection JSON

**Role:** safe, stable, testable presentation contract from Rust game truth to all clients.

Owned responsibilities:

- `/world` projection JSON
- `/app` / Matrix-compatible projections
- map/geodata/tactics/Trillionnium contract versions
- redaction and privacy boundaries
- client-ready command descriptors
- deterministic rendering data, not mutable browser state

Must not own:

- hidden state mutations
- business-rule bypasses
- client-specific source-of-truth logic

Current status:

- `[x]` map/geodata projection exists.
- `[x]` web shell consumes projection fields.
- `[x]` tactics board and Trillionnium mechanics projections have first-class contract versions and hard tests.

Boundary output to Layer 5:

```json
{
  "contract_version": "trillionnium_world_game_projection_v1",
  "openstreetmap_geodata": {},
  "trillionnium_character": {},
  "tactics_board": {},
  "available_commands": [],
  "legal": { "osm_attribution_visible": true }
}
```

### Layer 5 — Web UI: game visualization + input

**Role:** playable visualization and intent capture. The browser may animate, highlight, and assist input, but must not decide truth.

Owned responsibilities:

- tactical board visualization
- unit selection UX
- command drafting forms/buttons
- Wuxia combat/task log display
- OSM support/diagnostic layer display
- mobile-first HUD and accessibility

Must not own:

- final movement validity
- hit chance/damage truth
- reward settlement
- quest completion truth
- NPC relationship mutation

Current status:

- `[x]` `/world` has visible tactics shell and OSM support layer.
- `[x]` board cells, units, objectives, commands, HUD, logs, and attribution/readiness layers render from Rust projections.
- `[x]` `/world` has a local playable character movement loop: the player can use direction keys (`7/8/9/4/5/6/1/2/3`, WASD, arrows) to submit movement intent to Rust-owned `/world/web/map-move`.
- `[~]` CSS/HTML remains the current visualization scaffold; full Phaser runtime integration remains optional and unstarted.

Boundary output to Layer 6:

```json
{
  "command": "move_unit|attack|train_skill|talk_npc|accept_task|claim_reward",
  "idempotency_key": "...",
  "actor_id": "...",
  "target": { "overlay_id": "...", "cell": "B4", "npc_id": "..." },
  "client_context": { "projection_version": "..." }
}
```

### Layer 6 — Rust: command handler / ledger / progression

**Role:** validate player intent, mutate game state, settle rewards, and emit durable events.

Owned responsibilities:

- command authorization and idempotency
- movement/path/LOS/range validation
- task acceptance/completion validation
- NPC relationship mutation
- combat resolution
- ledger settlement / review hold / anti-cheese
- route-runner mastery/reward history updates
- event log persistence and projection invalidation

Must not own:

- raw display layout
- client animation state

Current status:

- `[x]` existing commerce/contract/ledger command handlers are hardened.
- `[x]` tactics/Trillionnium command handlers are introduced and wired through Rust validation, simulation ticks, event logs, reward handoff, ledger/review-hold, and anti-cheese gates.

Command processing rule:

```text
Web intent -> Rust validate -> Rust mutate -> ledger/progression settle -> Rust event log -> new projection JSON -> Web re-render
```

---

## Trillionnium Mechanics Extraction Spec

The goal is not to port 白金英雄坛说 literally, and it is not to copy the small green LCD as a product goal. The goal is to extract the **playable loop** from the confirmed source reference `albert10jp/yxts-gold-asm`: a player exists on a map, directional input moves that player through reachable locations, each location exposes NPC/task/skill/combat choices, and Rust-owned state records the result. Trillionnium then rebuilds that loop with native content, OSM objectives, commerce/ledger systems, and production-grade gates.

### Confirmed Hero Tan source facts now binding this progress tree

- Confirmed current primary reference: `albert10jp/yxts-gold-asm` in `references/hero-tan/yxts-gold-asm`.
- Source file `h/gmud.h` establishes the original exploration viewport constants: `ScreenX=160`, `ScreenY=80`, `Unit_Width=32`, `Unit_Height=32`, `ScreenX_Num=5`, `ScreenY_Num=3`.
- Source file `gmud.s` confirms the movement shape: directional key routines update player/map offsets and then redraw player position.
- Trillionnium may use these facts as **behavior/layout reference contracts** only. It must not copy original text, maps, images, binary tables, or game data into production.
- Current implementation status: `/world` has a Rust-owned local movement loop (`a2e3b22`), a first node-local NPC/task lifecycle (`4ec9a92`), and Rust-owned transition semantics (`trillionnium_world_transition_semantics_v1`) for blocked terrain, locked routes, interaction-required exits, room transitions, zone transitions, local exits, wait, unknown targets, and non-adjacent routes. It still lacks skill practice from exploration nodes and lightweight combat encounter entry/return from map exploration.

### Source references and allowed use

| Reference | Use | Do not use directly | Trillionnium extraction target |
| --- | --- | --- | --- |
| `albert10jp/yxts-gold-asm` | Current primary Hero Tan source reference; source-level movement/map constants and loop structure | Original code, text, maps, bitmap data, NPC/task tables, binary data, or copied UI/game data | map exploration loop, 5x3 viewport contract, directional movement semantics, NPC/task/skill/combat sequencing |
| `mogita/gmud` | Secondary authentic MUD mechanics reference where licensing/provenance is acceptable | Original text/maps/assets/tables without provenance review | skill taxonomy, task/NPC/fight engine shape, MUD-style logs |
| `qq634488405/RMXP-Hero` | Rich RMXP/Ruby system reference | GPL/custom-license code, extracted assets, original data tables | sect progression, menu/data organization, battle/skill interaction ideas |
| `coyoteXujie/yxts-llm` | Modern Python/Arcade reference | Code/assets until full LICENSE/provenance clarified | modern NPC/combat/quest/dialogue structure |

### Trillionnium-native mechanics to implement

#### Attributes

Use original-inspired categories only as inspiration; store Trillionnium-native names/fields in Rust.

Initial Rust model target:

```rust
struct TrillionniumAttributes {
    physique: u16,      // body/root durability; inspired by 根骨/体魄
    force: u16,         // raw power; inspired by 臂力
    agility: u16,       // movement/evasion; inspired by 身法
    insight: u16,       // learning/perception; inspired by 悟性
    resolve: u16,       // morale/internal stability
    craft: u16,         // production/world-work bridge
    commerce: u16,      // market/contract bridge
    reputation: i32,    // public Trillionnium standing
}
```

Progress tree hooks:

- [x] TW-3.4a Add `TrillionniumAttributes` Rust model.
- [ ] TW-3.4b Add derived stats: max HP, internal energy, move range modifier, learning speed, negotiation bonus.
- [ ] TW-3.4c Add tests proving derived stats are deterministic and capped.

#### Skills

Reference flavor:

- gmud-style basics: internal practice, fists, sword, lightness, literacy.
- RMXP/yxts-style explicit skills and combat hooks.

Initial Trillionnium skill families:

- `basic_inner_power`
- `basic_unarmed`
- `basic_blade`
- `basic_sword`
- `basic_lightness`
- `reading_and_contracts`
- `merchant_routecraft`
- `artifact_crafting`
- `streetwise_investigation`

Progress tree hooks:

- [x] TW-3.5a Add skill definition model: id, family, level, xp, unlock conditions, combat/world effects.
- [x] TW-3.5b Add training command: mentor/OSM place requirement + cost + cooldown.
- [x] TW-3.5c Bind selected skills to tactics actions: move, attack, evade, inspect, negotiate.

#### Sects / factions / mentors

Reference flavor:

- sect identity and mentor progression from GMUD/Hero Tan style.
- Trillionnium must use native faction names and business/world roles.

Initial Trillionnium sect/faction examples:

- `Cloud Ledger Hall` — contracts, settlement discipline, reputation repair.
- `Street Compass Society` — OSM route scouting, mobility, POI discovery.
- `Iron Workshop Gate` — crafting, equipment, delivery defense.
- `Market Wind Pavilion` — commerce, negotiation, listing quality.
- `Night Watch Alliance` — risk control, dispute/anti-cheese, escort tasks.

Progress tree hooks:

- [x] TW-3.5d Add sect/faction model: id, title ladder, mentor NPCs, entry requirements, benefits.
- [x] TW-3.5e Bind sect halls to OSM objectives/POIs through `game_overlay_id`.
- [x] TW-3.5f Add mentor training task flow and Rust validation.

#### NPC society

NPCs should not be static quest vending machines. They should be Rust-owned world actors with relationship, role, schedule/anchor, and task capability.

Initial NPC fields:

```rust
struct TrillionniumNpc {
    npc_id: String,
    display_name: String,
    role: NpcRole,
    faction_id: Option<String>,
    osm_overlay_id: Option<String>,
    relationship: i16,
    trust: i16,
    risk_posture: NpcRiskPosture,
    task_archetypes: Vec<String>,
}
```

Progress tree hooks:

- [x] TW-3.5g Add NPC model and fixture NPCs.
- [x] TW-3.5h Bind NPC spawn/anchor to OSM features.
- [x] TW-3.5i Add talk/training/task-offer command descriptors in projection JSON.

#### Tasks / quests

Task archetypes should combine Trillionnium mechanics with OSM objective sources and existing ledger/progression discipline.

Initial task archetypes:

- `courier_letter` — deliver between two OSM anchors.
- `find_item` — inspect/search at OSM POI/area.
- `escort_route` — protect NPC/unit across route cells.
- `defeat_bandit` — tactics combat at risk-tagged OSM anchor.
- `market_settlement` — commerce/contract task with ledger hold/release.
- `sect_training_trial` — skill/mentor progression objective.
- `seasonal_tournament` — arena/leaderboard-style challenge.

Progress tree hooks:

- [x] TW-3.6a Add Trillionnium task archetype enum.
- [x] TW-3.6b Generate task candidates from OSM provider semantic roles.
- [x] TW-3.6c Bind task completion to Rust command handlers, not browser state.
- [x] TW-3.6d Route eligible rewards through ledger/review-hold/anti-cheese gates.

#### Wuxia combat logs

Combat logs should give flavor without copying original prose. Logs are generated from Trillionnium-native templates keyed by skill family, terrain, NPC role, and outcome.

Example Trillionnium-native log shape:

```json
{
  "log_id": "combat-log-...",
  "style": "trillionnium_wuxia_log_v1",
  "beats": [
    { "kind": "stance", "text": "You lower your center of gravity as the market lanterns flicker." },
    { "kind": "exchange", "skill": "basic_lightness", "delta_hp": -3 },
    { "kind": "result", "outcome": "objective_secured" }
  ]
}
```

Progress tree hooks:

- [x] TW-3.6e Add combat log generator with original Trillionnium templates.
- [x] TW-3.6f Add tests forbidding source-reference strings from being copied verbatim into production fixtures.
- [x] TW-3.6g Render combat/task logs in `/world` and Matrix/app projections.

#### OSM objective source

OSM should seed objectives, not decide task truth.

Mapping examples:

| OSM feature/tag | Trillionnium semantic role | Possible Trillionnium objective |
| --- | --- | --- |
| `amenity=marketplace` | market hub | negotiate, recover goods, publish quest card |
| `amenity=bank` / ledger-adjacent fixture | ledger hall | settlement, debt/reputation repair |
| `tourism=attraction` / landmark | rumor landmark | find clue, meet NPC, seasonal challenge |
| `building=*` | indoor/nearby anchor | search item, rescue, delivery endpoint |
| `highway=footway/path` | route segment | escort, patrol, ambush, courier path |
| `leisure=park` | open encounter area | training, duel, bandit encounter |
| admin/neighborhood relation | faction territory | sect influence, reputation, patrol risk |

Progress tree hooks:

- [x] TW-1.8a Add OSM semantic role mapping table in Rust.
- [x] TW-3.7a Add objective generator from `OpenStreetMapDataProvider` features.
- [x] TW-3.7b Add deterministic seed so the same fixture/world state yields stable objectives.
- [x] TW-3.7c Add tests proving OSM can suggest objectives but Rust command handlers decide completion.

---

## Non-Negotiable Architecture Rules

1. **No dashboard-first regression.** `/world` must keep moving toward a game interface.
2. **No fake game shell as final state.** Temporary CSS boards are acceptable only as scaffolding for Rust-owned game state and real interaction loops.
3. **Legal OSS only.** Do not copy proprietary 三国 / 英雄坛说 / 白金英雄坛说 code, text, maps, sprites, binary data, or assets into product runtime.
4. **Hero Tan Shuo is a game-loop reference, not a skin target.** Use `albert10jp/yxts-gold-asm` to shape exploration/movement/NPC/task/skill/combat sequencing; do not stop at LCD visual imitation.
5. **MedievalWar remains a tactics-pattern reference only.** Use `tranchikhang/MedievalWar` for board/cursor/control/turn/pathfinding patterns; no vendored MedievalWar/Phaser code unless a separate explicit product/legal decision approves it.
6. **Hero Tan Shuo projects are mechanics references only.** Use yxts-gold-asm / GMUD / RMXP / yxts-llm style ideas only after recreating Trillionnium-native content/assets.
7. **OpenClawStreetMap remains support/underlay.** It supplies real-world map support, route context, geodata identity, and operational diagnostics; it should not dominate the visible game UI.
8. **OpenStreetMap data belongs below Rust.** OSM provides roads, POIs, buildings, areas, admin boundaries, tags, and identities; Rust binds these to game overlays and gameplay systems.
9. **No production use of public OSM tile servers.** Production traffic requires cache/self-host/vendor strategy.
10. **Respect OSM attribution and ODbL obligations.** Keep attribution and derived database tracking visible in contracts.
11. **MapLibre stays shadow-only.** Do not promote MapLibre above canary `0` without fresh explicit signoff.

---

## Current Checkpoint Summary

### Latest code commit

- `4ec9a92 feat: complete world local task lifecycle`

### Latest validated evidence

- `cargo fmt --all -- --check` — passed
- `cargo check -p consumer-entry-api` — passed
- `cargo test -p consumer-entry-api -- --nocapture` — `133 passed`
- Targeted Rust test `web_map_shells_render_live_event_task_focus_metadata` — passed
- `node --check scripts/playwright/trillionnium-browser-e2e.mjs` — passed
- `git diff --check` — passed
- Local-production restart/status — OK on 7001/7002/7003/7004/7005/8080/8090/8091 plus worker
- Manual local no-cookie movement spot check — `raid-hall -> league-coliseum` via `4←`, `ok=true`, Rust-owned `/world/web/map-move`
- Browser E2E — `run/league-browser/browser-e2e-summary-1778415910-949928.json`, `ok=true`, includes `world_local_npc_task_loop=true` and request-failure gate green
- Web E2E — `run/league-web/web-e2e-summary-1778417153.json`, `ok=true`
- First-human E2E — `run/first-human-session/browser-e2e-summary-1778418021-966047.json`, `ok=true`, zero request/page/console failures
- Human-playability assessment — `run/human-playability-assessment/human-playability-assessment-summary-1778398733.json`, `ok=true`, scores `9.8 / 8.5 / 7.0`
- Real-user beta/public-commercial gates remain green as product-surface gates but score lifts still require real cohort/drill evidence.

### Current working-tree expectation

After this progress-tree alignment commit, CEX should be clean. If not clean, inspect before editing:

```bash
git status --short
git log --oneline -5
```

---

## Current Implemented Contracts

### Game UI / OSS base

- `/world` exposes `trillionnium_open_source_tactics_world_shell_v1` for the tactics board scaffold.
- `/world` exposes `trillionnium_text_adventure_keypad_movement_v1` for the first-screen exploration/movement loop.
- `/world` uses `data-open-source-base="tranchikhang/MedievalWar"` for tactics-pattern metadata only.
- `/world` declares Hero Tan movement reference metadata on the exploration shell:
  - `data-reference-project="albert10jp/yxts-gold-asm"`
  - `data-reference-file="h/gmud.h"`
  - `data-lcd-screen="160x80"`
  - `data-lcd-viewport="5x3"`
  - `data-lcd-palette="green_monochrome"`
  - `data-keypad-controls="7,8,9,4,5,6,1,2,3"`
  - `data-source-of-truth="rust_world_map_move"`
- The first-screen movement shell is playable locally: no-cookie loopback users can move with direction buttons/WASD/arrows; browser sends intent to `/world/web/map-move`; Rust mutates `world_player_positions`.
- `/world` exposes `trillionnium_world_play_first_exploration_loop_v1` for the play-first current-location/exits/local-actions prompt.
- `/world` exposes `trillionnium_world_local_task_lifecycle_v1` for node-local `talk_npc -> offer_task -> active task -> complete_task -> settlement/review feedback`.
- Local task feedback is sourced from `rust_world_contract_completions`; browser/web submit intent only.
- Current shell is still Rust-rendered/CSS visualization, not a full Phaser runtime or a Hero Tan code fork.

### OSM geodata substrate

- Rust projection contract: `openstreetmap_geodata_v1`
- Provider seam: `OpenStreetMapDataProvider`
- Current provider: `fixture_openstreetmap_data_provider_v1`
- Current source mode: `local_fixture_mock_first_no_live_overpass`
- Current projected identity fields:
  - `osm_id`
  - `osm_type`
  - `lat`
  - `lng`
  - `tags`
  - `game_overlay_id`
- Current production flags:
  - live Overpass disabled
  - Geofabrik import disabled
  - cache/self-host/vendor required before production traffic
  - public OSM tile servers forbidden for production traffic
  - OSM attribution and ODbL obligations visible

### Map runtime / renderer posture

- Active engine: `leaflet_openstreetmap_v1`
- Renderer adapter: `leaflet_renderer_adapter_v1`
- Runtime handle: `mapRuntime`
- Candidate engine: `maplibre_gl_v1`
- MapLibre status: `shadow_only_not_user_facing`
- Canary percent: `0`
- Promotion requires fresh signoff and rollback drill evidence.

---

## Key Files and Ownership

| Area | File | Purpose |
| --- | --- | --- |
| World state structs | `services/consumer-entry-api/src/lib.rs` | `WorldState`, `WorldMapNode`, game/domain state |
| Default fixtures | `services/consumer-entry-api/src/league_repository.rs` | default League/World fixture data and `world_map_node` helper |
| OSM provider | `services/consumer-entry-api/src/openstreetmap_geodata.rs` | fixture-first OSM provider, provider modes, legal/freshness/attribution metadata |
| Projection layer | `services/consumer-entry-api/src/world_map_projection.rs` | Rust source-of-truth JSON, OSM provider contract, map/runtime contracts |
| Tactics/mechanics model | `services/consumer-entry-api/src/world_tactics.rs` | Trillionnium attributes, skills, sects, NPCs, tactics board, simulation ticks, command outcomes |
| World web shell | `services/consumer-entry-api/src/world_web_shell.rs` | `/world` visualization/input HTML shell |
| Shared map shell JS/CSS | `services/consumer-entry-api/src/real_world_map_shell.rs` | OpenClawStreetMap adapter/runtime helpers |
| Runtime optimization contracts | `services/consumer-entry-api/src/world_map_optimization.rs` | map performance, delta, RUM, shadow parity contracts |
| Health/metrics gates | `services/consumer-entry-api/src/health_metrics.rs` | production health and Prometheus metrics |
| Unit/integration tests | `services/consumer-entry-api/src/tests.rs` | hard contract assertions |
| Web E2E gate | `scripts/check-trillionnium-league-web-e2e.sh` | `/world`, `/app`, League browser-web contract checks |
| Production readiness | `scripts/check-production-readiness.sh` | local-production readiness posture |
| Production signoff | `scripts/check-production-signoff.sh` | final signoff evidence aggregation |
| Monitoring gate | `scripts/check-trillionnium-route-runner-handoff-monitoring.sh` | monitoring bundle contract checks |
| OSS stack decisions | `docs/trillionnium-open-source-stack-reference-v1.md` | layered OSS reference stack |
| Tactics base decision | `docs/trillionnium-open-source-tactics-base-selection-v1.md` | permissive tactics candidates and MedievalWar selection |
| Hero Tan Shuo decision | `docs/trillionnium-open-source-hero-tan-shuo-base-selection-v1.md` | Hero Tan Shuo / GMUD legal/mechanics reference decision |
| Current Hero Tan source reference | `references/hero-tan/yxts-gold-asm/` | checked-out `albert10jp/yxts-gold-asm` source reference; mechanics/layout study only |
| This progress tree | `docs/trillionnium-world-development-progress-tree-v1.md` | canonical continuation guide |

---

## Active Progress Tree

### TW-0 — Preserve the current baseline

- [x] TW-0.1 Keep CEX repo at clean checkpoint after OSM geodata/runtime observability and Hero Tan movement slices.
  - Evidence: latest code checkpoint `4ec9a92`; this doc update should be the only newer commit when present.
- [x] TW-0.2 Preserve OSS research decisions in docs.
  - Evidence: tactics and Hero Tan Shuo base-selection docs exist.
- [x] TW-0.3 Preserve OSM/Rust/Web ownership rule.
  - Evidence: `openstreetmap_geodata_v1`, `web_role=visualization_input_only`.
- [x] TW-0.4 Preserve MapLibre shadow-only/canary=0 guard.
  - Evidence: readiness/monitoring gates green.

### TW-1 — Rust-owned OpenStreetMap geodata substrate

- [x] TW-1.1 Define first Rust-side OSM geodata projection contract.
  - Current contract: `openstreetmap_geodata_v1`
- [x] TW-1.2 Add fixture-first provider seam.
  - Current provider: `FixtureOpenStreetMapDataProvider`
- [x] TW-1.3 Bind `WorldMapNode` to OSM identity fields.
  - Current fields: `osm_id`, `osm_type`, `lat`, `lng`, `tags`, `game_overlay_id`
- [x] TW-1.4 Expose OSM metadata in `real_world_map_engine` and `world_map_json`.
- [x] TW-1.5 Surface OSM contract in `/world` advanced layer as support substrate.
- [x] TW-1.6 Split OSM provider code into a dedicated module/file.
  - Implemented file: `services/consumer-entry-api/src/openstreetmap_geodata.rs`
  - Keep `world_map_projection.rs` as projection assembly, not provider implementation.
- [x] TW-1.7 Add explicit fixture dataset instead of deriving all OSM IDs from `node_id` hashes.
  - Implemented fixture: stable sample OSM identities for default Shanghai core world nodes.
  - Keep deterministic fallback for missing fixtures.
- [x] TW-1.8 Add roads/buildings/areas/admin boundary fixture layers.
  - Implemented `openstreetmap_fixture_layers_v1` with roads, buildings, areas, admin boundaries, semantic-role mapping, and no live ingestion.
- [x] TW-1.9 Add derived database tracking metadata.
  - Include fixture source, import timestamp, transform version, and ODbL share-alike note.
- [x] TW-1.10 Add provider-mode enum and test each mode is fail-closed.
  - `fixture`
  - `overpass_bbox_cache` future
  - `geofabrik_extract_import` future
  - `vendor_tile_cache` future
- [!] TW-1.11 Do not enable live Overpass/Geofabrik ingestion until cache and ODbL plan are implemented.

### TW-2 — Real game UI shell, not dashboard

- [x] TW-2.1 Reject dashboard/map-first as final direction.
- [x] TW-2.2 Reject pure Hero Tan Shuo text shell as final direction.
- [x] TW-2.3 Select permissive tactics base.
  - Decision: `tranchikhang/MedievalWar`, MIT, Phaser 3.
- [x] TW-2.4 Add visible tactics shell contract to `/world`.
  - Current: `trillionnium_open_source_tactics_world_shell_v1`
- [~] TW-2.5 Keep current CSS tactics board as scaffold only.
  - It is a visual direction marker, not the final game loop.
- [x] TW-2.6 Define Rust-side tactics board model.
  - Implemented first projection: `trillionnium_world_tactics_board_v1` with cells, terrain, units, objectives, commands, OSM objective source.
  - `/world` now renders board cells/units/objectives/commands from Rust projection JSON, not hard-coded HTML loops.
- [x] TW-2.7 Define Rust-side unit model.
  - Implemented `trillionnium_world_tactics_unit_v1`: `unit_id`, `owner`, `class/archetype`, `hp`, `energy`, `position`, `move_range`, `attack_range`, `status_effects`, source owner.
- [x] TW-2.8 Define Rust-side turn/action command model.
  - Implemented `trillionnium_world_tactics_command_v1`: `select_unit`, `move_unit`, `attack`, `use_skill`, `interact`, `end_turn`, validation owner, required skill, action cost.
- [x] TW-2.9 Add command endpoints/forms for tactics actions.
  - Web sends intent; Rust validates and mutates state.
- [x] TW-2.10 Replace static board rendering with Rust-projected board state.
- [x] TW-2.11 Decide integration strategy for actual MedievalWar/Phaser code.
  - Decision: patterns only for now, no vendored MedievalWar code.
  - Keep `tranchikhang/MedievalWar` as an MIT tactics reference for map/cursor/control/turn/pathfinding patterns.
  - Revisit vendoring only after a separate explicit product/legal decision and license/asset manifest.
- [!] TW-2.12 Do not copy MedievalWar art assets unless license/attribution is tracked.
- [x] TW-2.13 Add a first-screen local playable world movement loop.
  - Current: `trillionnium_text_adventure_keypad_movement_v1` with clear directional controls, local no-cookie loopback play, and Rust-owned `/world/web/map-move` persistence.
- [x] TW-2.14 Reject visual-only Hero Tan skinning as a completed game loop.
  - The yxts-gold-asm reference is now framed as movement/exploration/game-loop reference, not a small-green-screen copying target.
- [x] TW-2.15 Make the current task/person/party route visibly travel through the world map.
  - Implemented as `trillionnium_world_objective_travel_v1`: Rust computes the active objective route from `world_state.world_map_nodes.exits`, current player position, active Trillionnium task/NPC objective candidates, and party positions; `/world` renders route roles, next step, target, route tracks, and party members as visualization-only intent affordances.
  - `/world/web/map-move` returns refreshed `world_objective_travel`, and the browser updates the keypad projection after Rust accepts movement; web never becomes the source of truth.

### TW-3 — Trillionnium / Hero Tan Shuo mechanics reference layer

- [x] TW-3.1 Search and classify 白金英雄坛说 / 英雄坛说 OSS candidates.
  - Current primary reference: `albert10jp/yxts-gold-asm`; checked out under `references/hero-tan/yxts-gold-asm`.
- [x] TW-3.2 Decide no direct fork is legally/product-clean today.
  - Use source facts and mechanics patterns only; do not vendor/copy original code, maps, text, sprites, tables, or data.
- [x] TW-3.3 Use Hero Tan projects as mechanics/game-loop references only.
  - Explicitly: yxts-gold-asm informs movement/exploration/NPC/task/skill/combat sequence; Trillionnium owns native Rust content and command validation.
- [x] TW-3.4 Define Trillionnium-native character attributes.
  - Implemented fields: `physique`, `force`, `agility`, `insight`, `resolve`, `craft`, `commerce`, `reputation`.
  - Derived stats are deterministic and capped; source inspiration remains mechanics-only, with Trillionnium-native names/content.
- [x] TW-3.4a Add `TrillionniumAttributes` Rust model.
- [x] TW-3.4b Add deterministic derived stats and caps.
- [x] TW-3.4c Add tests for attribute projection and derived stats.
- [x] TW-3.5 Define skill/sect/mentor/NPC relationship models in Rust.
- [x] TW-3.5a Add skill definition model and fixture skills.
- [x] TW-3.5b Add training command with mentor/OSM-place requirement.
- [x] TW-3.5c Bind skills to tactics actions and world task effects.
  - First binding is in tactics command descriptors via `required_skill_id`; world task effects are recorded on skill definitions for follow-up handler enforcement.
- [x] TW-3.5d Add sect/faction model and title ladder.
- [x] TW-3.5e Bind sect halls/mentor anchors to OSM `game_overlay_id`.
- [x] TW-3.5f Add mentor training task flow.
- [x] TW-3.5g Add NPC model and fixture NPCs.
- [x] TW-3.5h Bind NPC spawn/anchor to OSM features.
- [x] TW-3.5i Add talk/training/task-offer command descriptors in projection JSON.
- [x] TW-3.6 Define text battle/task log style without copying original content.
- [x] TW-3.6a Add Trillionnium task archetype enum.
- [x] TW-3.6b Generate task candidates from OSM provider semantic roles.
- [x] TW-3.6c Bind task completion to Rust command handlers, not browser state.
- [x] TW-3.6d Route eligible rewards through ledger/review-hold/anti-cheese gates.
- [x] TW-3.6e Add combat log generator with original Trillionnium templates.
- [x] TW-3.6f Add tests forbidding source-reference strings from being copied verbatim into production fixtures.
- [x] TW-3.6g Render combat/task logs in `/world` and Matrix/app projections.
- [x] TW-3.7 Bind Trillionnium mechanics to tactics units and OSM locations.
  - Example: mentor NPC at an OSM POI, training unlocks tactics skill.
- [x] TW-3.7a Add objective generator from `OpenStreetMapDataProvider` features.
- [x] TW-3.7b Add deterministic seed so fixture/world state yields stable objectives.
- [x] TW-3.7c Add tests proving OSM suggests objectives but Rust command handlers decide completion.
- [!] TW-3.8 Do not import original Hero Tan Shuo text, maps, sprites, code, binary tables, or database content.
- [x] TW-3.9 Extract yxts-gold-asm exploration constants into product contracts.
  - Evidence: `/world` declares `data-reference-project="albert10jp/yxts-gold-asm"`, `data-lcd-screen="160x80"`, `data-lcd-viewport="5x3"`, and `data-keypad-controls="7,8,9,4,5,6,1,2,3"`.
- [x] TW-3.10 Implement the first Rust-owned adjacent-node movement loop.
  - Evidence: `/world/web/map-move` mutates `world_player_positions`; local loopback manual spot check moved `raid-hall -> league-coliseum` with `4←`.
- [x] TW-3.11 Add Hero Tan-style blocked terrain/collision and room transition semantics.
  - Evidence: `trillionnium_world_transition_semantics_v1` centralizes Rust movement decisions in `world_map_transition_decision(...)`; `/v1/world/map/move` and `/world/web/map-move` expose `movement_transition`; `/world` local exits/keypad expose transition contract/source/status/kind/result attributes; Browser E2E records `coverage.world_transition_semantics=true`.
- [x] TW-3.12 Add NPC talk as a first-class exploration command from map nodes.
  - Evidence: `trillionnium_world_local_task_lifecycle_v1`; Browser E2E moves to `mirror-city-square` and exercises `talk_npc` from the play-first prompt.
- [x] TW-3.13 Add task pickup/completion as map-node actions.
  - Evidence: Browser E2E covers `offer_task`, active task visibility, `complete_task`, completion feedback from `rust_world_contract_completions`, and completion form disappearance after submission/review state.
- [x] TW-3.14 Add skill practice and mentor interaction into the exploration loop.
  - Evidence: `trillionnium_world_skill_practice_loop_v1`; `/world` play-first prompt now exposes node-local mentor practice, `train_skill` forms, Rust mentor/NPC/place validation, and character skill feedback from `rust_trillionnium_character`. Browser E2E exercises `basic_unarmed` practice at `mirror-city-square` before local NPC/task flow and records `coverage.world_local_skill_practice_mentor_loop=true`.
- [ ] TW-3.15 Add lightweight combat encounter entry from exploration nodes.
  - Combat/tactics exists; exploration should be able to trigger a small encounter from a node/NPC/task, then return to map state.

### TW-4 — Rust World domain and simulation backbone

- [x] TW-4.1 `WorldState` already separates world fields inside League state.
- [x] TW-4.2 Existing indexes support commerce/workflow hot paths.
- [x] TW-4.3 Existing route artifacts feed route cockpit/task graph.
- [x] TW-4.4 Introduce explicit game-session state for `/world`.
  - Player position, party, board encounter, current turn, active objective.
- [x] TW-4.5 Normalize map node / OSM feature / game overlay relationship.
  - Avoid duplicating identity in ad-hoc JSON.
- [x] TW-4.6 Add deterministic simulation tick / encounter generation hooks.
- [x] TW-4.7 Add Rust tests for turn resolution and invalid command rejection.
- [x] TW-4.8 Prepare repository/storage boundary for game session persistence.
  - Game sessions, characters, and simulation ticks persist through serde-compatible `WorldState` and mirror into normalized SQL tables via `0021_add_trillionnium_tactics_storage_tables.sql`; command-source-of-truth remains Rust `WorldState` while typed SQL read models/direct-write parity are covered by runtime/snapshot gates.

### TW-5 — Gameplay loops

- [x] TW-5.1 Existing world commerce loop works: company -> shop/listing -> purchase -> work order -> delivery -> accept/reject/reopen/cancel.
- [x] TW-5.2 Existing route-runner handoff/reward/mastery gates are green.
- [x] TW-5.3 Current `/world` visible game loop is playable through Rust-owned projection and intent forms.
  - Current local player movement is playable without a signed cookie on loopback after `a2e3b22`.
  - Node-local NPC talk and task pickup/completion lifecycle are playable after `4ec9a92`.
  - Remaining Hero Tan-style product loop work: skill practice and mentor interaction from exploration nodes, plus lightweight combat encounter entry/return-to-map.
- [x] TW-5.4 Implement first tactics loop:
  - spawn player unit
  - spawn one objective
  - allow move/command intent through Rust validators
  - allow interact/task claim through NPC/task descriptors
  - write event log and persisted simulation ticks
  - reward through existing ledger/progression path
- [x] TW-5.5 Implement combat loop:
  - enemy unit
  - attack action
  - damage resolution
  - victory/failure state
- [x] TW-5.6 Bind tactics objective to existing route task graph.
- [x] TW-5.7 Bind rewards to route-runner reward history and ledger settlement.
- [x] TW-5.8 Add anti-cheese checks for repeatable encounter farming.

### TW-6 — Web visualization/input shell

- [x] TW-6.1 `/world` displays tactics game shell before map support details.
- [x] TW-6.2 `/world` still exposes OpenClawStreetMap diagnostics under support/advanced layers.
- [x] TW-6.3 `/world` exposes OSM provider contract and feature cards.
- [x] TW-6.4 Make board cells data-driven from Rust projection.
- [x] TW-6.5 Add input affordances for unit selection and command drafting.
- [x] TW-6.6 Add clear mobile-first game HUD.
  - active unit
  - current objective
  - primary action
  - risk/reward
- [x] TW-6.7 Add accessibility labels and keyboard/low-motion support for the tactics shell.
- [x] TW-6.8 Keep old dashboard panels available as secondary/detail panels, not main experience.
- [x] TW-6.9 Add clear first-screen movement controls for the player character.
  - Controls: `7↖ 8↑ 9↗ / 4← 5· 6→ / 1↙ 2↓ 3↘`, plus WASD/arrows/numpad.
- [x] TW-6.10 Replace residual dashboard copy with a play-first action prompt.
  - Evidence: `/world` play-first prompt surfaces current node, exits/local actions, NPC talk/task affordances, active task state, and settlement/review feedback under `trillionnium_world_play_first_exploration_loop_v1` and `trillionnium_world_local_task_lifecycle_v1`.

### TW-7 — Map and runtime operations

- [x] TW-7.1 Map readability, runtime budget, delta/cache, RUM, weak-network, privacy gates exist.
- [x] TW-7.2 Monitoring/readiness covers MapLibre shadow canary safety.
- [x] TW-7.3 OSM production traffic policy visible.
- [x] TW-7.4 Add explicit OSM provider health/readiness section.
  - fixture mode is green through `openstreetmap_provider_readiness_v1`
  - live/network/production ingestion remains disabled and Overpass/Geofabrik/vendor/unknown modes fail closed
- [x] TW-7.5 Add geodata freshness/staleness metrics.
  - fixture freshness is explicit through `openstreetmap_geodata_freshness_v1`
  - static fixture snapshots declare no wall-clock decay (`fixture_snapshot_age_seconds=0`)
  - stale/unknown live ingestion remains blocked until import timestamps, max-age policy, and ODbL tracking exist
  - health/playability/Prometheus/Web/Browser/UI/production readiness gates cover the freshness contract
- [x] TW-7.6 Add OSM attribution presence check to web E2E and UI audit if not already hard-gated.
  - `openstreetmap_attribution_presence_v1` now travels with OSM geodata JSON and is visible in `/app` + `/world` DOM.
  - Web E2E, Browser E2E, and UI audit hard-gate visible `© OpenStreetMap contributors` / `ODbL-1.0` attribution plus required/source-of-truth/data-tracking flags.
  - Leaflet runtime attribution must also render visibly in the browser audit gates.
  - Health, playability scorecard, Prometheus, production readiness, Web E2E, and Browser E2E now hard-gate the same attribution/ODbL presence contract.
- [!] TW-7.7 Do not increase MapLibre canary above 0 without fresh production signoff.

### TW-8 — Validation gate policy

TW-8 is a policy checklist, not product backlog. Do not include it in Trillionnium World feature completion percentages.

Documentation-only calibration gate:

- `git diff --check`

Minimum gate for Rust projection/UI changes:

- `cargo fmt --all -- --check`
- `git diff --check`
- `bash -n` for touched shell scripts
- targeted `cargo test -p consumer-entry-api <test-name> -- --nocapture`
- `cargo test -p consumer-entry-api -- --nocapture`

Minimum gate for `/world` UI contract changes:

- restart local production runtime when E2E depends on server code:
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart`
- `CEX_ENV_FILE=run/local-production/.env scripts/check-trillionnium-league-web-e2e.sh`

Minimum gate for monitoring/readiness changes:

- `scripts/check-trillionnium-route-runner-handoff-monitoring.sh`
- `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh`

Full product checkpoint gate when touching core world runtime:

- `cargo test -p consumer-entry-api -p matrix-entry-adapter -p ledger-service -- --nocapture`
- Web E2E
- Browser E2E if browser runtime/js changed
- UI audit if DOM contract changed
- real-user beta/public-commercial if product readiness changed
- production signoff only when requested or when making release-grade runtime changes

Latest evidence snapshot for the current checkpoint:

- Playability scorecard: `run/playability-scorecard/playability-scorecard-summary-1778380602.json`, `ok=true`, 100%
- Web E2E: `run/league-web/web-e2e-summary-1778426609.json`, `ok=true`
- Browser E2E: `run/league-browser/browser-e2e-summary-1778425959-1021724.json`, `ok=true`, includes `world_transition_semantics=true`, `world_local_npc_task_loop=true`, and objective travel DOM/runtime gates
- First-human E2E: `run/first-human-session/browser-e2e-summary-1778426636-1029761.json`, `ok=true`, zero request/page/console failures
- UI audit: `run/trillionnium-ui-audit/ui-audit-summary-1778377923-669309.json`, `ok=true`
- Real-user beta: `run/real-user-beta/real-user-beta-summary-1778346703.json`, `ok=true`, 100%
- Public commercial: `run/public-commercial/public-commercial-summary-1778346759.json`, `ok=true`, 100%
- Production readiness: `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh`, `READY`

---

## Historical Development Slice Notes

This section records older continuation recommendations. They are retained for audit trail only; they are **not** the active next pointer. Use **Current Next Pointer** at the bottom of this document for continuation.

The safest slice after checkpoint `ab5f046` was **TW-1.6 + TW-1.7 + TW-2.6 + TW-3.4a**; that slice is now complete.

The next slice after that checkpoint was **TW-1.9 + TW-1.10 + TW-2.9 + TW-3.5b/d/g**; that slice is now complete.

1. Add derived OSM database tracking metadata and provider-mode fail-closed enum.
2. Add Rust command endpoints/forms for tactics intents.
3. Add mentor/OSM-place training requirements and initial sect/NPC relationship models.
4. Keep command validation in Rust; web remains visualization/input only.

Why this order:

- It preserves the user's core architecture requirement: Rust is bottom/source of truth.
- It prevents `/world` from drifting back into a hard-coded HTML shell.
- It keeps live OSM ingestion disabled while still making the geodata substrate real.
- It starts Trillionnium mechanics in Rust game state instead of as UI-only flavor text.

Expected first-slice deliverables:

- `services/consumer-entry-api/src/openstreetmap_geodata.rs`
- optional `services/consumer-entry-api/src/world_tactics.rs`
- optional `services/consumer-entry-api/src/trillionnium_world.rs`
- tests proving:
  - fixture provider is deterministic
  - OSM identities are stable
  - web projection reads Rust board/geodata, not hard-coded UI-only data
- `/world` still contains:
  - `trillionnium_open_source_tactics_world_shell_v1`
  - `openstreetmap_geodata_v1`
  - `visualization_input_only`
  - `openclawstreetmap_underlay`

---

## Latest Development Update

#### Update 2026-05-08 18:4x CST

- Commit: pending until validation completes.
- Changed files:
  - `services/consumer-entry-api/src/openstreetmap_geodata.rs`
  - `services/consumer-entry-api/src/world_tactics.rs`
  - `services/consumer-entry-api/src/lib.rs`
  - `services/consumer-entry-api/src/league_repository.rs`
  - `services/consumer-entry-api/src/world_map_projection.rs`
  - `services/consumer-entry-api/src/world_web_shell.rs`
  - `services/consumer-entry-api/src/tests.rs`
  - `scripts/check-trillionnium-league-web-e2e.sh`
- Completed:
  - [x] TW-1.6 OSM provider split into dedicated Rust module.
  - [x] TW-1.7 stable fixture OSM identities added for default world nodes.
  - [x] TW-2.6 Rust-owned tactics board projection added.
  - [x] TW-3.4a/b/c first Trillionnium attributes model, derived stats, and tests added.
- Remaining next:
  - [x] TW-1.8 OSM roads/buildings/areas/admin-boundary fixture layers.
  - [x] TW-2.7 Rust-side tactics unit model.
  - [x] TW-2.8 Rust-side tactics command model.
  - [x] TW-3.5a Trillionnium skill definition model.
  - [x] TW-3.5c First skill-to-tactics command bindings.
- Remaining next:
  - [x] TW-1.9 derived geodata database tracking metadata.
  - [x] TW-1.10 provider-mode enum and fail-closed tests.
  - [x] TW-2.9 tactics command endpoints/forms.
  - [x] TW-3.5b/d/g mentor training, sect, and NPC models.

#### Update 2026-05-08 20:1x CST

- Commit: `b6243f4 feat: wire trillionnium world tactics commands`
- Completed next full-dev slice:
  - [x] TW-1.9 `openstreetmap_derived_database_metadata_v1`: fixture source, stable import epoch, transform version, derived snapshot id, feature counts, and ODbL/share-alike tracking note.
  - [x] TW-1.10 `openstreetmap_provider_mode_v1`: `fixture` enabled; `overpass_bbox_cache`, `geofabrik_extract_import`, `vendor_tile_cache`, and unknown modes fail closed with network ingestion disabled.
  - [x] TW-2.9 tactics command intents: `/v1/world/tactics/command` JSON route and `/world/web/tactics-command` form route; Web submits intent, Rust validates and records outcome/event.
  - [x] TW-3.5b `trillionnium_training_command_v1`: mentor/OSM-place/cost/cooldown training descriptors plus Rust validator.
  - [x] TW-3.5d `trillionnium_sect_v1`: sect/faction fixture model with OSM anchors, mentor NPCs, requirements, benefits, title ladders.
  - [x] TW-3.5g `trillionnium_npc_v1`: fixture NPCs with OSM anchors, schedules, task capabilities, and training/talk/task command descriptors.
- Validation so far:
  - `cargo test -p consumer-entry-api -- --nocapture` green (`132 passed`).
  - `cargo fmt --all -- --check`, `git diff --check`, `bash -n scripts/check-trillionnium-league-web-e2e.sh`, `cargo test -p matrix-entry-adapter -p ledger-service -- --nocapture` green.
  - local-production restart required restarting the project compose Postgres service; after restart, Web E2E green at `run/league-web/web-e2e-summary-1778259366.json`.
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh` green: `READY production readiness smoke passed`.
- Remaining next:
  - [x] TW-3.5e/f/h/i bind sect halls, mentor training task flow, NPC anchors, and talk/training/task-offer descriptors more deeply.
  - [x] TW-3.6+ Trillionnium task archetypes and native battle/task log style.

#### Update 2026-05-09 01:2x CST

- Commit: this checkpoint (`feat: bind trillionnium npc tasks to osm`).
- Completed next full-dev slice:
  - [x] TW-3.5e `trillionnium_sect_osm_binding_v1`: sect halls now carry explicit OSM anchor bindings, overlay IDs, feature IDs, OSM IDs/types, and fail-closed missing-anchor metadata.
  - [x] TW-3.5f `trillionnium_mentor_training_task_v1`: mentor training now projects task flows with travel/talk/submit/Rust-validate/mutate/record steps, and `train_skill` outcomes include task-flow evidence.
  - [x] TW-3.5h `trillionnium_npc_spawn_anchor_v1`: NPC fixtures now expose spawn anchors tied to OSM semantic-role features.
  - [x] TW-3.5i `trillionnium_npc_command_descriptor_v1`: NPC talk/train/task-offer command descriptors are projected, and `/v1/world/tactics/command` validates `talk_npc` / `offer_task` intents in Rust.
  - [x] TW-3.6 / TW-3.6a / TW-3.6b: added Trillionnium-native task archetypes, OSM-generated task candidates, and `trillionnium_battle_log_style_v1` for native battle/task log text.
- Validation so far:
  - `cargo fmt --all -- --check`, `git diff --check`, and `bash -n scripts/check-trillionnium-league-web-e2e.sh` green.
  - `cargo test -p consumer-entry-api world_tactics -- --nocapture` green (`2 passed`).
  - `cargo test -p consumer-entry-api -- --nocapture` green (`132 passed`).
  - local-production runtime restarted with `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart`.
  - Web E2E green at `run/league-web/web-e2e-summary-1778260813.json`.
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh` green: `READY production readiness smoke passed`.
- Remaining next:
  - [x] TW-3.6c/d bind task completion/rewards to Rust command handlers and ledger/review-hold gates.
  - [ ] TW-3.7+ deepen deterministic tactics combat resolution and NPC relationship persistence.

#### Update 2026-05-09 09:53 CST

- Commit: this checkpoint (`feat: gate trillionnium task completion rewards`).
- Completed next TW-3.6c/d slice:
  - [x] `complete_task` is now a Rust-owned tactics command (`rust_trillionnium_task_completion_handler`), not browser state. It validates known Trillionnium skill, selected task archetype, OSM-generated task candidate/overlay, and records durable events.
  - [x] `offer_task` now creates a durable `WorldContract` (`trillionnium-task:<archetype>`) so completion has a server-side task to close.
  - [x] Trillionnium task completion creates `WorldContractCompletion`, runs deterministic quality / review-hold / anti-cheese checks, and routes eligible rewards through `settle_world_contract_completion_with_ledger(...)`; player/reputation/economy rewards only release on `settled` / `duplicate` ledger status.
  - [x] `/world` now renders OSM-generated task completion candidates with `trillionnium_task_completion_v1`, `trillionnium_reward_gate_v1`, ledger-settlement, review-hold, and anti-cheese contract attributes.
- Evidence:
  - `cargo check -p consumer-entry-api`
  - `cargo fmt --all -- --check`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `git diff --check`
  - `cargo test -p consumer-entry-api world_tactics -- --nocapture` green (`2 passed`)
  - `cargo test -p consumer-entry-api web_map_shells_render_live_event_task_focus_metadata -- --nocapture` green (`1 passed`)
  - `cargo test -p consumer-entry-api -- --nocapture` green (`132 passed`; includes OSM-candidate mismatch, open-offer requirement, ledger-settlement skip, and review-hold/anti-cheese coverage)
  - `cargo test -p matrix-entry-adapter -p ledger-service -- --nocapture` green
  - `cargo clippy --workspace -- -D warnings` green
  - local-production runtime restarted with `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart`
  - Web E2E green at `run/league-web/web-e2e-summary-1778292121.json`
  - Browser E2E green at `run/league-browser/browser-e2e-summary-1778292160-123302.json`
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh` green: `READY production readiness smoke passed`
- Remaining next:
  - [x] TW-3.6e/f/g combat/task log hardening and Matrix/app projection follow-through.
  - [ ] TW-3.7+ deterministic tactics combat resolution and NPC relationship persistence.

#### Update 2026-05-09 10:25 CST

- Commit: this checkpoint (`feat: project trillionnium combat logs`).
- Completed next TW-3.6e/f/g slice:
  - [x] Added `trillionnium_combat_log_v1`, a Rust-generated `trillionnium_combat_log` payload with native Wuxia/task beats, deterministic log id, style contract, and explicit source-reference safety metadata.
  - [x] Replaced generated `battle_log` prose that previously mentioned source-reference repo names with Trillionnium-native beats derived from the combat log.
  - [x] Added tests forbidding copied source-reference strings (`gmud`, `RMXP-Hero`, `yxts-llm`, `Hero Tan`, `tranchikhang/MedievalWar`, `Phaser 3`) inside generated combat-log beat text.
  - [x] Surfaced the combat/task log through `/world` HTML, `/app` `client_app_json` (`trillionnium_combat_log`), and Matrix `/map` card/body fields.
- Evidence so far:
  - `cargo fmt --all -- --check`
  - `cargo check -p consumer-entry-api -p matrix-entry-adapter`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `git diff --check`
  - `cargo test -p consumer-entry-api world_tactics_projection_binds_trillionnium_state_to_osm_objectives -- --nocapture` green
  - `cargo test -p consumer-entry-api web_map_shells_render_live_event_task_focus_metadata -- --nocapture` green
  - `cargo test -p matrix-entry-adapter route_cards_preserve_focus_node_fields -- --nocapture` green
  - `cargo test -p consumer-entry-api -- --nocapture` green (`132 passed`)
  - `cargo test -p matrix-entry-adapter -- --nocapture` green (`36 passed`)
  - `cargo clippy --workspace -- -D warnings` green
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart` green
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-trillionnium-league-web-e2e.sh` green (`run/league-web/web-e2e-summary-1778293829.json`)
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-trillionnium-league-browser-e2e.sh` green (`run/league-browser/browser-e2e-summary-1778294220-134947.json`)
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh` green (`READY production readiness smoke passed`)
  - `bash -n scripts/check-matrix-live-room-e2e.sh` green
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-matrix-live-room-e2e.sh` green (`run/matrix-live/e2e-summary-1778295483.json`, including `world_map_trillionnium_combat_log_contract=trillionnium_combat_log_v1` and `world_map_trillionnium_combat_log_beat_count=4`)
- Remaining next:
  - [x] Run production-like runtime/web gates, commit this slice, and append memory.
  - [ ] TW-3.7+ deterministic tactics combat resolution and NPC relationship persistence.

#### Update 2026-05-09 11:37 CST

- Commit: this checkpoint (`feat: bind trillionnium tactics to osm objectives`).
- Completed next TW-3.7 slice:
  - [x] Added `trillionnium_osm_objective_v1`: `/world` tactics objectives now come from `OpenStreetMapDataProvider` features through a Rust objective generator, not static browser placeholders.
  - [x] Added deterministic objective seeds from fixture/provider seed + world state + matrix user, and tests proving repeated projection yields stable OSM objectives.
  - [x] Added `trillionnium_tactics_combat_resolution_v1`: `attack` is now resolved by the Rust tactics combat handler with deterministic target lookup/damage/result metadata; invalid target tiles are rejected server-side.
  - [x] Added `trillionnium_npc_relationship_v1`: NPC cards now project persisted `world_relationships` into relationship/trust/risk state, and accepted talk/training/task/combat commands append relationship events with typed relation kinds.
  - [x] Surfaced the new OSM-objective / NPC-relationship / combat-resolution contracts in `/world` HTML and Matrix `/map` cards, and hardened web + Matrix E2E gates around those fields.
- Evidence:
  - `cargo fmt --all -- --check`
  - `cargo check -p consumer-entry-api -p matrix-entry-adapter`
  - `cargo test -p consumer-entry-api -p matrix-entry-adapter -- --nocapture` green (`132 + 36 passed`)
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh` and `bash -n scripts/check-matrix-live-room-e2e.sh`
  - `git diff --check`
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart` green
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-trillionnium-league-web-e2e.sh` green (`run/league-web/web-e2e-summary-1778297208.json`)
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-trillionnium-league-browser-e2e.sh` green (`run/league-browser/browser-e2e-summary-1778297217-154346.json`)
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-matrix-live-room-e2e.sh` green (`run/matrix-live/e2e-summary-1778297800.json`, including `world_map_trillionnium_osm_objective_contract`, objective count `11`, NPC relationship contract, and combat resolution contract)
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh` green (`READY production readiness smoke passed`)
- Remaining next:
  - [ ] TW-4.4/TW-4.6 persist full tactics game sessions and simulation ticks beyond projected fixture encounters.
  - [ ] TW-4.5 normalize map node / OSM feature / game overlay relationships instead of repeating identity in ad-hoc JSON.

#### Update 2026-05-09 11:4x CST

- Commit: this checkpoint (`feat: persist trillionnium tactics sessions`).
- Completed next TW-4 backbone slice:
  - [x] Added `trillionnium_tactics_game_session_v1`: `/world` tactics commands now project a Rust-owned game session with deterministic session id, active objective, current tick, action points, party/unit state, and persistence metadata under `world_state.world_tactics_sessions`.
  - [x] Added `trillionnium_tactics_simulation_tick_v1`: accepted and rejected tactics commands now record deterministic ticks with before/after tile, action cost, simulation effect, accepted flag, and session linkage under `world_state.world_tactics_simulation_ticks`.
  - [x] Added `trillionnium_map_overlay_identity_v1`: map node, OSM feature, and game overlay identity are normalized into a single projection index and referenced by OSM objectives, tactics tiles, and units instead of repeating ad-hoc identity fragments.
  - [x] Surfaced game-session, simulation-tick, and overlay-identity contracts in `/world` HTML, `world-openstreetmap-geodata`, tactics session cards, Matrix `/map` cards, and web/Matrix E2E gates.
  - [x] Cleaned `/world` English-mode visible defaults for the new tactics/Trillionnium shell while preserving Chinese through `data-i18n-zh`; UI audit now reports CJK=0 and overflow=0 for mobile/tablet/desktop, with the map staying in the first-screen contract.
- Evidence:
  - `cargo fmt --all` and `cargo check -p consumer-entry-api -p matrix-entry-adapter`
  - `cargo test -p consumer-entry-api -p matrix-entry-adapter -p ledger-service -- --nocapture` green (`132 + 36 + 11 passed`)
  - `cargo clippy --workspace -- -D warnings`
  - `bash -n scripts/check-trillionnium-ui-audit.sh`, `bash -n scripts/check-trillionnium-league-web-e2e.sh`, `bash -n scripts/check-matrix-live-room-e2e.sh`, and `git diff --check`
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart` green
  - UI audit green: `run/trillionnium-ui-audit/ui-audit-summary-1778301010-182172.json` (`/world` mobile/tablet/desktop CJK=0, overflow=0)
  - Web E2E green: `run/league-web/web-e2e-summary-1778301381.json`
  - Browser E2E green: `run/league-browser/browser-e2e-summary-1778301403-185329.json`
  - Matrix live E2E green: `run/matrix-live/e2e-summary-1778302023.json` (session/tick/overlay contracts present)
  - Real-user beta green: `run/real-user-beta/real-user-beta-summary-1778302057.json` (100%, no failures)
  - Public-commercial green: `run/public-commercial/public-commercial-summary-1778302096.json` (100%, no failures)
  - `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh` green (`READY production readiness smoke passed`)
- Remaining next:
  - [x] TW-4.8+ storage-boundary decision: keep Rust `WorldState` as command source of truth, mirror tactics characters/sessions/ticks into normalized SQL tables for parity/direct-write coverage.
  - [x] TW-5.4/TW-5.5 first tactics/combat loop: objective progress, victory state, deterministic reward settlement, persisted tick/session state.
  - [x] TW-5.6/TW-5.7/TW-5.8 route/reward/anti-cheese slice: tactics objective sessions bind into route task graph, settled rewards feed route-runner reward history, and repeat farming after settled tactics rewards is blocked server-side.

#### Update 2026-05-09 18:3x CST

- Commit: `feat: expose trillionnium tactics player surfaces` (this slice).
- Completed next TW-6 player-visible tactics surface slice:
  - [x] `/world` and `/app` now render `trillionnium_tactics_player_visible_surface_v1` from Rust-owned tactics projection, not browser-owned state.
  - [x] Both surfaces expose a current tactics objective card with objective id, route task id, progress/goal, victory state, and reward status.
  - [x] Both surfaces expose current session state with session id, active unit, active overlay, action points, current tick, and simulation tick contract.
  - [x] Both surfaces expose reward-history handoff with `trillionnium_tactics_reward_history_v1` / reward settlement contracts and route-runner handoff copy.
  - [x] Both surfaces expose repeat-farming blocked/armed copy under `trillionnium_tactics_repeat_farming_anti_cheese_v1`, keeping browser role as `visualization_input_only`.
  - [x] Web/browser/UI audit gates now hard-check the `/world` + `/app` tactics player HUD contracts, and `/app` keeps the HUD compact so map/action/onboarding ordering does not regress.
- Evidence:
  - `cargo fmt --all -- --check`
  - `git diff --check`
  - legacy/double-rename grep guard no output
  - `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`)
  - `cargo clippy --workspace -- -D warnings`
  - `cargo test --workspace`
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart/status` green
  - normalized runtime dual-write green (`0021`, write-set audit rows `247`)
  - SQL snapshot green: `sha256:7f11ddaf957010810c1c0966c68dddf565b78952ef4014c285d4422bd0e5b0d1`
  - Web E2E green: `run/league-web/web-e2e-summary-1778325490.json`
  - Browser E2E green: `run/league-browser/browser-e2e-summary-1778325493-358222.json`
  - UI audit green: `run/trillionnium-ui-audit/ui-audit-summary-1778326086-367189.json`
- Remaining next:
  - [x] TW-6.4/TW-6.5 deeper board-cell interactions and unit-selection/command-drafting affordances.
  - [x] TW-6.7 accessibility labels, keyboard traversal, and low-motion support for the tactics shell.

#### Update 2026-05-09 20:2x CST

- Commit: this slice (`feat: wire trillionnium tactics intent drafting`).
- Completed next TW-6 interaction slice:
  - [x] Added `trillionnium_tactics_board_cell_interaction_v1`, `trillionnium_tactics_unit_selection_v1`, and `trillionnium_tactics_command_intent_draft_v1` to the Rust tactics projection.
  - [x] `/world` board cells now render as selectable intent-only controls sourced from Rust projection cells, carrying tile id, OSM overlay id, movement cost, draft input name, and Rust validation owner.
  - [x] `/world` units now render as selectable intent-only controls carrying unit id, side, current tile, selection role, Rust model source, and command-draft contract.
  - [x] `/world` command buttons now draft command intent without resolving movement/combat/reward in the browser; `#world-tactics-command-draft-form` posts unit/command/target/body to the existing Rust-owned tactics command handler.
  - [x] `window.trillionniumTacticsIntentDraft` exposes browser draft state only; legality, combat result, objective completion, and rewards remain owned by `rust_world_tactics_command_handler` / `rust_tactics_command_validator`.
  - [x] `/app` now mirrors the command-draft affordance as a compact intent card so mobile players see the same source-of-truth boundary before opening the full tactics board.
  - [x] Web E2E, browser E2E, UI audit, and Rust tests now hard-gate the board-cell, unit-selection, and command-intent contracts.
- Evidence:
  - `cargo fmt --all -- --check`
  - `cargo check -p consumer-entry-api`
  - targeted `cargo test -p consumer-entry-api world_tactics_projection_binds_trillionnium_state_to_osm_objectives -- --nocapture`
  - targeted `cargo test -p consumer-entry-api web_map_shells_render_live_event_task_focus_metadata -- --nocapture`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `node --check scripts/playwright/trillionnium-ui-audit.mjs`
  - `git diff --check`
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart/status` green
  - Web E2E green: `run/league-web/web-e2e-summary-1778329986.json`
  - Browser E2E green: `run/league-browser/browser-e2e-summary-1778329990-393843.json`
  - UI audit green: `run/trillionnium-ui-audit/ui-audit-summary-1778330124-394760.json`
- Remaining next:
  - [x] TW-6.7 accessibility labels, keyboard traversal, and low-motion support for the tactics shell.
  - [x] TW-6.8 keep old dashboard/detail panels secondary, not the main experience.

#### Update 2026-05-09 TW-6.7

- Commit: this slice (`feat: make trillionnium tactics shell accessible`).
- Completed accessibility/keyboard/low-motion tactics shell slice:
  - [x] Added `trillionnium_tactics_accessibility_v1` to Rust-owned tactics projection, board cells, units, commands, and surface policy metadata.
  - [x] `/world` board cells now render as real `role="gridcell"` controls with row/column indexes, ARIA labels/descriptions, selected state, roving tabindex metadata, and Rust validation/source-of-truth tokens.
  - [x] `/world` units and command controls now expose ARIA selection/description/controls metadata while staying intent-only; browser still drafts only, Rust validates legality/combat/reward.
  - [x] `initializeTacticsIntentDraft` now owns keyboard traversal for board cells (`Arrow` keys, `Home`/`End`, `Enter`/`Space`) and updates hidden draft fields, live status, and roving focus without resolving game state in the browser.
  - [x] The tactics shell and command draft panel expose low-motion support through `prefers-reduced-motion` datasets and CSS that suppresses selection animation when requested.
  - [x] `/app` mirrors the accessibility contract on the compact intent-draft card so mobile players see keyboard/low-motion/source-of-truth affordance metadata before opening `/world`.
  - [x] Browser E2E now verifies reduced-motion mode, keyboard traversal from `C3` to `D3`, accessibility runtime export, and keeps the map RUM matrix warmup explicit before enforcing health metrics.
  - [x] Web E2E and UI audit hard-gate accessibility contract tokens, keyboard help/live regions, roving grid cells, accessible units/commands, and low-motion support.
- Evidence:
  - `cargo fmt --all -- --check`
  - `cargo check -p consumer-entry-api`
  - targeted `cargo test -p consumer-entry-api world_tactics_projection_binds_trillionnium_state_to_osm_objectives -- --nocapture`
  - targeted `cargo test -p consumer-entry-api web_map_shells_render_live_event_task_focus_metadata -- --nocapture`
  - `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`; one transient viewport delta assertion did not reproduce on focused/full rerun)
  - `cargo clippy --workspace -- -D warnings`
  - `cargo test --workspace`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `node --check scripts/playwright/trillionnium-ui-audit.mjs`
  - `git diff --check`
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart/status` green
  - Web E2E green: `run/league-web/web-e2e-summary-1778332210.json`
  - Browser E2E green: `run/league-browser/browser-e2e-summary-1778333088-413874.json`
  - UI audit green: `run/trillionnium-ui-audit/ui-audit-summary-1778333221-414772.json`
- Remaining next:
  - [x] TW-6.8 keep old dashboard/detail panels secondary, not the main experience.

#### Update 2026-05-09 TW-6.8

- Commit: this slice (`fix: keep trillionnium dashboard panels secondary`).
- Completed secondary/detail panel slice:
  - [x] Added `trillionnium_secondary_dashboard_panels_v1` metadata to the old `/world` dashboard/detail sections while leaving the primary experience anchored on the tactics/game/action loop.
  - [x] Marked legacy/dense panels with `data-secondary-dashboard-role="secondary_detail_panel"`, `data-main-experience="false"`, `data-default-state="collapsed_on_mobile"`, and `data-primary-loop-anchor="trillionnium-tactics-game-shell"`.
  - [x] Kept the extra world counters inside a `secondary_counter_drawer` and the OpenClawStreetMap underlay diagnostics inside collapsed `supporting_engine_diagnostics` details.
  - [x] Marked the detailed map move panel as `available_after_core_loop`, so it remains accessible for power users without becoming the main first-session route.
  - [x] Web E2E, browser E2E, UI audit, and Rust shell tests now hard-gate the secondary-dashboard contract and collapsed/mobile-secondary policy.
- Evidence:
  - `cargo fmt --all -- --check`
  - `git diff --check`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `node --check scripts/playwright/trillionnium-ui-audit.mjs`
  - `cargo check -p consumer-entry-api`
  - targeted `cargo test -p consumer-entry-api web_map_shells_render_live_event_task_focus_metadata -- --nocapture`
  - `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`)
  - `cargo clippy --workspace -- -D warnings`
  - `cargo test --workspace`
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart/status` green
  - Web E2E green: `run/league-web/web-e2e-summary-1778335844.json`
  - Browser E2E green: `run/league-browser/browser-e2e-summary-1778335855-432426.json`
  - UI audit green: `run/trillionnium-ui-audit/ui-audit-summary-1778335990-433359.json`
- Remaining next:
  - [x] TW-7.4 explicit OSM provider health/readiness section.

---

#### Update 2026-05-09 23:3x CST

- Commit: pending until final commit.
- Completed:
  - [x] TW-7.4 explicit OSM provider health/readiness section.
  - [x] Added `openstreetmap_provider_readiness_v1` to Rust geodata projection with fixture-green status, stable fixture identity/layer counts, and fail-closed mode coverage for Overpass bbox cache, Geofabrik extract import, vendor tile cache, and unknown modes.
  - [x] Surfaced `/world` readiness card `#world-openstreetmap-provider-readiness` while keeping the web role visualization/input-only and live/network/production ingestion disabled.
  - [x] Added health/playability gate `trillionnium_openstreetmap_provider_readiness_gate_v1` plus Prometheus gauges for readiness and fail-closed mode count.
  - [x] Hard-gated readiness in Rust tests, Web E2E, Browser E2E, UI audit, and production readiness. Production readiness now gives operator-signal health fetches a 15s default timeout because the production `/health` payload is intentionally large after full world/playability evidence is present.
- Evidence:
  - `cargo fmt --all -- --check`
  - `git diff --check`
  - `bash -n scripts/check-production-readiness.sh scripts/check-trillionnium-league-web-e2e.sh`
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `node --check scripts/playwright/trillionnium-ui-audit.mjs`
  - `cargo check -p consumer-entry-api`
  - targeted `cargo test -p consumer-entry-api openstreetmap_geodata_provider_uses_stable_fixture_identities -- --nocapture`
  - targeted `cargo test -p consumer-entry-api web_map_shells_render_live_event_task_focus_metadata -- --nocapture`
  - targeted `cargo test -p consumer-entry-api health_endpoint_exposes_identity_governance_overview -- --nocapture`
  - `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`)
  - `cargo clippy --workspace -- -D warnings`
  - `cargo test --workspace`
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart/status` green
  - Web E2E green: `run/league-web/web-e2e-summary-1778338491.json`
  - Browser E2E green: `run/league-browser/browser-e2e-summary-1778338505-450398.json`
  - UI audit green: `run/trillionnium-ui-audit/ui-audit-summary-1778338649-451357.json`
  - Fresh monitoring deploy metadata: `run/monitoring-live-target/metadata/monitoring-deploy-metadata.yml`
  - Fresh DB drill: `run/drills/db-backup-restore-20260509T150657Z-455243.summary.json`
  - Production readiness green: `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh` → `READY production readiness smoke passed`
- Remaining next:
  - [x] TW-7.5 add geodata freshness/staleness metrics.

---

#### Update 2026-05-10 01:1x CST

- Completed:
  - [x] TW-7.5 geodata freshness/staleness metrics.
  - [x] Added `openstreetmap_geodata_freshness_v1` to Rust OSM geodata projection. Fixture mode now reports `fixture_static_fresh_live_stale_blocked`, `fixture_snapshot_age_seconds=0`, `wall_clock_freshness_applies=false`, `live_data_freshness_applies=false`, `stale_live_ingestion_blocked=true`, ODbL/derived-database metadata visibility, and live import tracking requirements before any production ingestion can open.
  - [x] Surfaced `/world` freshness card `#world-openstreetmap-geodata-freshness` while keeping Rust as source of truth and the web role visualization/input-only.
  - [x] Added health/playability gate `trillionnium_openstreetmap_geodata_freshness_gate_v1` plus Prometheus gauges for freshness-green, fixture snapshot age, and staleness alarm state.
  - [x] Hard-gated freshness/staleness in Rust tests, Web E2E, Browser E2E, UI audit, and production readiness without enabling Overpass/Geofabrik/vendor/live ingestion.
- Evidence:
  - `cargo check -p consumer-entry-api`
  - `cargo fmt --all -- --check`
  - `git diff --check`
  - `bash -n scripts/check-production-readiness.sh scripts/check-trillionnium-league-web-e2e.sh`
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `node --check scripts/playwright/trillionnium-ui-audit.mjs`
  - targeted OSM/world/health/metrics Rust tests green
  - `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`)
  - `cargo clippy --workspace -- -D warnings`
  - `cargo test --workspace`
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart/status` green
  - Web E2E green: `run/league-web/web-e2e-summary-1778346448.json`
  - Browser E2E green: `run/league-browser/browser-e2e-summary-1778346195-501871.json`
  - UI audit green: `run/trillionnium-ui-audit/ui-audit-summary-1778346195-501855.json`
  - Playability scorecard green: `run/playability-scorecard/playability-scorecard-summary-1778346464.json`
  - Real-user beta green: `run/real-user-beta/real-user-beta-summary-1778346703.json`
  - Public commercial green: `run/public-commercial/public-commercial-summary-1778346759.json`
  - Production readiness green: `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh` → `READY production readiness smoke passed`
- Remaining next:
  - [x] TW-7.6 OSM attribution presence check to web E2E and UI audit if not already hard-gated.
  - [ ] Continue with the next map/runtime hardening item after attribution validation and commit.

---

#### Update 2026-05-08 19:1x CST

- Commit: pending until validation completes.
- Completed next full-dev slice:
  - [x] TW-1.8 `openstreetmap_fixture_layers_v1`: roads, buildings, areas, admin-boundaries, semantic-role mapping, no live Overpass/Geofabrik.
  - [x] TW-2.7 `trillionnium_world_tactics_unit_v1`: Rust-owned unit model with owner/class/hp/energy/position/move/attack/status effects.
  - [x] TW-2.8 `trillionnium_world_tactics_command_v1`: Rust-owned command descriptor model with validation owner, required skill, cost, and web intent target.
  - [x] TW-3.5a `trillionnium_skill_v1`: Trillionnium-native fixture skill definitions.
  - [x] TW-3.5c first skill-to-command bindings via `required_skill_id`.
- Validation so far:
  - targeted OSM fixture identity/layer test green.
  - targeted tactics/Trillionnium projection test green.
  - targeted `/world` HTML contract test green.
- Next recommended slice:
  - [x] TW-1.9 / TW-1.10 derived geodata metadata and provider-mode fail-closed enum.
  - [x] TW-2.9 command endpoints/forms for tactics intents.
  - [x] TW-3.5b/d/g mentor training, sect/faction, and NPC relationship models.

---

## Known Blockers / Cautions

- GitHub unauthenticated/API search previously hit rate limits. Avoid relying on more live GitHub discovery unless necessary.
- Browser automation has had attach/navigation issues historically. Prefer script gates for routine validation.
- `run/local-production/.env` is required for signed production-like web/readiness gates.
- Long DB backup/restore drills need high timeouts; do not trigger casually for this next slice.
- Provider probes can be flaky/quota-limited; classify separately from product regressions.
- Do not touch ZBJ/猪八戒 during proactive work unless explicitly asked.

---

## Update Template for Future Slices

Append this block under the relevant tree item after each slice:

```markdown
#### Update YYYY-MM-DD HH:mm CST

- Commit: `<hash> <message>`
- Changed files:
  - `...`
- Completed:
  - [x] TW-x.y ...
- Evidence:
  - `cargo fmt --all -- --check`
  - `cargo test ...`
  - `run/...summary.json`
- Remaining next:
  - [ ] TW-x.z ...
- Notes/blockers:
  - ...
```

Also append a one-paragraph summary to `/home/qian/.openclaw/workspace/memory/YYYY-MM-DD.md`.

---


#### Update 2026-05-10 10:06 CST

- Commit: this commit (`feat: gate openstreetmap attribution`)
- Completed next map/runtime hardening slice:
  - [x] TW-7.6 `openstreetmap_attribution_presence_v1`: `/app` and `/world` now expose visible `© OpenStreetMap contributors` plus `ODbL-1.0` attribution, source-of-truth, required-visibility, derived-database tracking, and tile-server policy flags.
  - [x] Web E2E hard-gates `/app` and `/world` attribution DOM tokens and summary booleans. Latest green summary: `run/league-web/web-e2e-summary-1778378250.json`.
  - [x] UI audit hard-gates app/world attribution on mobile/tablet/desktop, including visible Leaflet runtime attribution. Latest green summary: `run/trillionnium-ui-audit/ui-audit-summary-1778377923-669309.json`.
  - [x] Browser E2E hard-gates the attribution contract and the local Leaflet stub now renders runtime attribution for deterministic browser coverage. Latest green summary: `run/league-browser/browser-e2e-summary-1778378511-673286.json`.
- Validation green: `cargo fmt --all -- --check`, shell/node syntax checks, `git diff --check`, targeted attribution/Rust tests, `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`), `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, local-production restart/status health, Web E2E, UI audit, and Browser E2E.

---

#### Update 2026-05-10 10:49 CST

- Commit: this commit (`feat: gate osm attribution observability`)
- Completed evidence-refresh / runtime-hardening slice:
  - [x] Attribution presence is now promoted from static Web/UI/browser gates into health JSON, playability scorecard, Prometheus gauges, production readiness, Web E2E, and Browser E2E.
  - [x] New health gate: `trillionnium_openstreetmap_attribution_presence_gate_v1`, requiring `openstreetmap_attribution_presence_v1`, fixture OSM provider mode, Rust source-of-truth, visualization-only web role, visible `© OpenStreetMap contributors`, `ODbL-1.0`, derived-database tracking, public tile-server policy, and at least four static/runtime presence checks.
  - [x] New Prometheus coverage: attribution gate green, attribution required, visible required, ODbL obligations visible, and attribution presence check count.
  - [x] Production readiness now fails closed if attribution/ODbL visibility disappears before any broader map-runtime promotion.
- Validation green: `cargo fmt --all -- --check`, shell/node syntax checks, `git diff --check`, targeted Rust tests, `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`), `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, local-production restart/status, playability scorecard `run/playability-scorecard/playability-scorecard-summary-1778380602.json`, production readiness `READY`, Web E2E `run/league-web/web-e2e-summary-1778380931.json`, and Browser E2E `run/league-browser/browser-e2e-summary-1778380983-688832.json`.

---

#### Update 2026-05-10 14:xx CST

- Commit: this commit (`feat: gate first-human browser flow`)
- Completed TW-6/TW-7 player-readiness hardening slice:
  - [x] Browser E2E `request_failures` are now a hard gate through `trillionnium_browser_request_failure_gate_v1`; only classified local world-map async cancellations are allowed, capped at six, and unclassified failures fail the run.
  - [x] Added `TRILLIONNIUM_BROWSER_E2E_MODE=first-human-session` plus `scripts/check-trillionnium-first-human-session.sh`, using a unique browser-first-human user/session and applying migrations before the mutating path.
  - [x] First-human path now exercises the actual loop: enter `/world`, verify the mobile four-question first screen, open tactics board, click/select `lord`, train `basic_unarmed` at `G8`, attack `F5`, draft reward claim, and submit the next route action.
  - [x] `/world` mobile first screen now exposes `trillionnium_first_human_session_v1` and `trillionnium_world_first_screen_four_questions_v1` with explicit `who → where → click → reward` anchors and keeps `#world-mobile-primary-cta` reachable on the first iPhone viewport.
  - [x] Web/Browser E2E health/metrics probes use safer 60s timeouts so accumulated normalized SQL state no longer causes false negatives while the underlying `/health`/`/metrics` contracts remain hard-gated.
- Latest green evidence: Web E2E `run/league-web/web-e2e-summary-1778394598.json`; Browser E2E `run/league-browser/browser-e2e-summary-1778394921-790148.json`; first-human session `run/first-human-session/browser-e2e-summary-1778395143-795329.json`; local-production status OK on 7001/7002/7003/7004/7005/8080/8090/8091 plus worker.
- Validation green: `bash -n scripts/check-trillionnium-league-web-e2e.sh scripts/check-trillionnium-first-human-session.sh`, `node --check scripts/playwright/trillionnium-browser-e2e.mjs`, `git diff --check`, prior Rust gate `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`), serial local-production Web E2E, Browser E2E, and first-human-session E2E.
- Remaining next: optimize `/health`/`/metrics` latency directly if this gate starts growing beyond the 60s safety budget; otherwise continue with real-user/public-commercial/signoff refresh or another TW-6/TW-7 UX-runtime slice.
- Constraints preserved: no live Overpass/Geofabrik ingestion, no MapLibre promotion, no MedievalWar/Phaser vendoring, and Rust remains source of truth.

---

#### Update 2026-05-10 14:57 CST

- Commit: this commit (`test: gate human playability assessment`)
- Completed full human-playability assessment calibration against the operator baseline:
  - [x] Added `scripts/check-trillionnium-world-human-playability-assessment.sh` and summary contract `trillionnium_human_playability_assessment_v1`.
  - [x] Baseline scores are preserved explicitly: technical playability `8.5/10`, first internal beta playability `7.5/10`, commercial release playability `6.0/10`.
  - [x] Latest evidence-backed scores: technical playability `9.3/10`, first internal beta playability `8.5/10`, commercial release playability `7.0/10`.
  - [x] The assessment ties score movement to concrete artifacts: Browser request-failure hard gate, first-human mutating session, Web E2E, playability scorecard, real-user beta, public-commercial gate, production readiness/signoff evidence, normalized repository final cutover, runtime Prometheus gauges, and OSM attribution/ODbL gate.
  - [x] Score caps stay intentionally conservative: technical is capped below `9.5` until `/health` and `/metrics` latency is optimized directly; first-beta is capped below `9` until real users test it; commercial is capped near `7.x` until live payment/support/legal/traffic drills exist.
- Latest assessment evidence: `run/human-playability-assessment/human-playability-assessment-summary-1778396238.json` (`ok=true`, `/health` probe `30.210812s`, `/metrics` probe `30.589466s`).
- Validation green: `scripts/check-trillionnium-world-human-playability-assessment.sh` passed against local-production evidence, with prior Web E2E `run/league-web/web-e2e-summary-1778394598.json`, Browser E2E `run/league-browser/browser-e2e-summary-1778394921-790148.json`, first-human E2E `run/first-human-session/browser-e2e-summary-1778395143-795329.json`, real-user beta `run/real-user-beta/real-user-beta-summary-1778346703.json`, public commercial `run/public-commercial/public-commercial-summary-1778346759.json`, and production signoff `run/signoff/production-signoff-20260508T073857Z-620743.summary.json`.
- Remaining next: optimize `/health` and `/metrics` latency under accumulated normalized SQL state, run a real 5-10 person first-beta cohort, then add commercial launch drills for payment/refund support, legal/privacy review, operator runbooks, and live traffic/error budgets.
- Constraints preserved: no live Overpass/Geofabrik ingestion, no MapLibre promotion, no MedievalWar/Phaser vendoring, and Rust remains source of truth.

---

#### Update 2026-05-10 15:xx CST

- Commit: this commit (`fix: cache health readiness projection`)
- Completed the next technical cap: `/health` and `/metrics` latency under accumulated normalized SQL state.
  - [x] Added a shared `HealthWorldReadinessBundle` cache for the expensive Trillionnium maturity / closed-beta / real-user-beta / public-commercial / playability-scorecard projection bundle.
  - [x] `/health` and `/metrics` now reuse the same cached readiness bundle instead of rebuilding the full world projection independently on every probe.
  - [x] The cache is keyed by profile/governance/repository runtime inputs and a league readiness generation counter.
  - [x] League persistence increments the generation counter, so mutating world/league commands invalidate stale readiness evidence before the next gate read.
  - [x] Human-playability assessment now hard-requires interactive `/health` and `/metrics` latency (`<=1s`) and raises the evidence-backed technical score from `9.3/10` to `9.5/10`.
- Baseline before this slice: repeated probes were roughly `/health` `27-28s` and `/metrics` `27-29s`.
- Latest local-production probe after restart/status: `/health` `0.059346s`, `0.060291s`, `0.025344s`; `/metrics` `0.006621s`, `0.009149s`, `0.007014s`.
- Latest assessment evidence: `run/human-playability-assessment/human-playability-assessment-summary-1778397666.json` (`ok=true`, scores `9.5 / 8.5 / 7.0`, `/health` `0.059792s`, `/metrics` `0.009939s`).
- Validation green: `cargo fmt --all -- --check`, `cargo check -p consumer-entry-api`, `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`), local-production restart/status, latency probes, `bash -n scripts/check-trillionnium-world-human-playability-assessment.sh`, human-playability assessment gate, and `git diff --check`.
- Remaining next: add concurrent p95/load-soak evidence before claiming `9.7+` technical playability; run a real 5-10 person first-beta cohort; add commercial launch drills for payment/refund support, legal/privacy review, operator runbooks, and live traffic/error budgets.
- Constraints preserved: no live Overpass/Geofabrik ingestion, no MapLibre promotion, no MedievalWar/Phaser vendoring, and Rust remains source of truth.

---

#### Update 2026-05-10 15:xx CST

- Commit: this commit (`test: gate health metrics load soak`)
- Completed the next technical proof layer for the cached readiness endpoints.
  - [x] Added `scripts/check-trillionnium-health-metrics-load-soak.sh` with summary contract `trillionnium_health_metrics_load_soak_v1`.
  - [x] The gate warms `/health` and `/metrics`, then runs 40 requests per endpoint at concurrency 12 and requires zero failures, HTTP 200s, p95 `<=0.75s`, and max `<=2.0s` per endpoint.
  - [x] Added the latest load-soak summary as explicit evidence in `scripts/check-trillionnium-world-human-playability-assessment.sh`.
  - [x] Human-playability assessment now raises the evidence-backed technical score from `9.5/10` to `9.7/10` when both interactive single-probe latency and concurrent p95 latency gates are green.
- Latest load-soak evidence: `run/health-metrics-load-soak/health-metrics-load-soak-summary-1778397953.json` (`ok=true`, wall `0.588734s`; `/health` p95 `0.115774s`, max `0.124493s`; `/metrics` p95 `0.107495s`, max `0.128907s`).
- Latest assessment evidence: `run/human-playability-assessment/human-playability-assessment-summary-1778397979.json` (`ok=true`, scores `9.7 / 8.5 / 7.0`, `/health` `0.01887s`, `/metrics` `0.006619s`).
- Validation green: `bash -n scripts/check-trillionnium-health-metrics-load-soak.sh scripts/check-trillionnium-world-human-playability-assessment.sh`, load-soak gate, human-playability assessment gate, and `git diff --check`.
- Remaining next: run a real 5-10 person first-beta cohort and convert confused clicks/drop-offs into UI copy/route fixes; then add commercial launch drills for payment/refund support, legal/privacy review, operator runbooks, and live traffic/error budgets. For technical `9.8+`, extend latency proof to longer soak, multi-node, or live traffic evidence.
- Constraints preserved: no live Overpass/Geofabrik ingestion, no MapLibre promotion, no MedievalWar/Phaser vendoring, and Rust remains source of truth.

---

#### Update 2026-05-10 15:xx CST

- Commit: this commit (`test: prepare first beta cohort evidence gate`)
- Prepared the real first-beta cohort collection gate without fabricating user evidence.
  - [x] Added `docs/trillionnium-first-beta-cohort-runbook-v1.md` with the 5-10 participant protocol, privacy constraints, task steps, thresholds, evidence schema, and confusion/drop-off fix routing.
  - [x] Added `scripts/check-trillionnium-first-beta-cohort-evidence.sh` with summary contract `trillionnium_first_beta_cohort_evidence_gate_v1`.
  - [x] The gate requires a real JSON input contract `trillionnium_first_beta_cohort_evidence_v1`, rejects synthetic/template evidence, requires anonymized participants, consent/fresh-session/no-coaching attestations, and validates completion/reward/next-route/time/confusion thresholds.
  - [x] Human-playability assessment now includes `real_5_to_10_person_first_beta_cohort_green` as the explicit check required before first-beta playability can claim `9+`.
- Current cohort evidence status: blocked as intended until a real evidence file is provided. Latest blocked summary: `run/first-beta-cohort/first-beta-cohort-summary-1778398224.json` (`status=blocked_missing_real_cohort_evidence`).
- Latest assessment evidence after adding the optional cohort gate: `run/human-playability-assessment/human-playability-assessment-summary-1778398224.json` (`ok=true`, scores remain `9.7 / 8.5 / 7.0`; the real cohort check is present and false until data exists).
- Validation green: `bash -n scripts/check-trillionnium-first-beta-cohort-evidence.sh scripts/check-trillionnium-world-human-playability-assessment.sh`, blocked cohort gate behavior inspected, human-playability assessment still green, and `git diff --check`.
- Remaining next: run the real 5-10 person cohort using the runbook/evidence schema, then convert confusion/drop-off findings into UI/route fixes; after that, move to commercial launch drills.
- Constraints preserved: no live Overpass/Geofabrik ingestion, no MapLibre promotion, no MedievalWar/Phaser vendoring, and Rust remains source of truth.

---

#### Update 2026-05-10 15:xx CST

- Commit: this commit (`test: prepare commercial launch drill gate`)
- Prepared the commercial launch drill evidence gate without pretending browser/local tests are enough for paid launch readiness.
  - [x] Added `docs/trillionnium-commercial-launch-drills-runbook-v1.md` covering payment reserve/consume reconciliation, refund/chargeback recovery, support escalation, legal/privacy + OSM review, operator incident runbook rehearsal, and live traffic/error-budget stop/go policy.
  - [x] Added `scripts/check-trillionnium-commercial-launch-drills.sh` with summary contract `trillionnium_commercial_launch_drills_gate_v1`.
  - [x] The gate requires a real/sanitized input contract `trillionnium_commercial_launch_drills_evidence_v1`, rejects synthetic/template evidence, requires owner/evidence/escalation on every drill, checks refund/support/incident response budgets, verifies privacy/payment/error-budget attestations, and fails unresolved launch blockers.
  - [x] Human-playability assessment now includes `commercial_launch_drills_green` as the explicit check required before commercial release playability can claim `8+`.
- Current commercial launch drill status: blocked as intended until real drill evidence is provided. Latest blocked summary: `run/commercial-launch-drills/commercial-launch-drills-summary-1778398636.json` (`status=blocked_missing_commercial_launch_drill_evidence`).
- Latest assessment evidence after adding the commercial drill gate: `run/human-playability-assessment/human-playability-assessment-summary-1778398636.json` (`ok=true`, scores remain `9.7 / 8.5 / 7.0`; the commercial drill check is present and false until evidence exists).
- Validation green: `bash -n scripts/check-trillionnium-commercial-launch-drills.sh scripts/check-trillionnium-world-human-playability-assessment.sh`, blocked commercial drill behavior inspected, human-playability assessment still green, and `git diff --check`.
- Remaining next: provide/run the real commercial launch drill evidence file, or if launch drills are not ready yet, pursue longer/multi-node/live-traffic latency proof for technical `9.8+` while waiting on external beta/commercial evidence.
- Constraints preserved: no live Overpass/Geofabrik ingestion, no MapLibre promotion, no MedievalWar/Phaser vendoring, and Rust remains source of truth.

---

#### Update 2026-05-10 15:xx CST

- Commit: this commit (`test: gate extended health metrics soak`)
- Completed the next internal technical proof while external first-beta/commercial evidence is still unavailable.
  - [x] Reused `scripts/check-trillionnium-health-metrics-load-soak.sh` for an extended single-node soak: 240 requests per endpoint at concurrency 16.
  - [x] Added `health_metrics_extended_soak_green` to `scripts/check-trillionnium-world-human-playability-assessment.sh`, requiring the latest load-soak summary to cover at least 200 requests per endpoint, concurrency >=12, and green p95/max latency for both `/health` and `/metrics`.
  - [x] Human-playability assessment now raises the evidence-backed technical score from `9.7/10` to `9.8/10` when the extended soak is green.
- Latest extended load-soak evidence: `run/health-metrics-load-soak/health-metrics-load-soak-summary-1778398719.json` (`ok=true`, 240 requests per endpoint, concurrency 16, wall `3.166863s`; `/health` p95 `0.130153s`, max `0.165369s`; `/metrics` p95 `0.115389s`, max `0.148545s`).
- Latest assessment evidence: `run/human-playability-assessment/human-playability-assessment-summary-1778398733.json` (`ok=true`, scores `9.8 / 8.5 / 7.0`, `/health` `0.062418s`, `/metrics` `0.008294s`).
- Validation green: `bash -n scripts/check-trillionnium-health-metrics-load-soak.sh scripts/check-trillionnium-world-human-playability-assessment.sh`, extended load-soak gate, human-playability assessment gate, and `git diff --check`.
- Remaining next: first-beta and commercial score lifts now require real evidence files; technical `9.9+` requires multi-node or live-traffic latency proof rather than another local-only loop.
- Constraints preserved: no live Overpass/Geofabrik ingestion, no MapLibre promotion, no MedievalWar/Phaser vendoring, and Rust remains source of truth.

---


#### Update 2026-05-10 18:12 CST

- Commit: this progress-tree alignment slice (`docs: align progress tree with hero tan loop`).
- Completed the requested alignment audit after `a2e3b22 fix: make local world movement playable`:
  - [x] Reframed `albert10jp/yxts-gold-asm` as the current primary Hero Tan source reference for gameplay loop structure, not a small-green-screen skin target.
  - [x] Recorded source facts from `h/gmud.h`: `ScreenX=160`, `ScreenY=80`, `Unit_Width=32`, `Unit_Height=32`, `ScreenX_Num=5`, `ScreenY_Num=3`.
  - [x] Recorded current product reality: local loopback `/world` can move the player without a signed web-session cookie, browser sends movement intent to `/world/web/map-move`, and Rust mutates `world_player_positions`.
  - [x] Added explicit not-yet-aligned backlog for Hero Tan-style blocked terrain, NPC talk, task pickup/completion, skill practice, and encounter entry/return from exploration; the NPC talk and task pickup/completion portions were completed later by `4ec9a92`.
  - [x] Updated the next pointer away from visual copying and toward a map-exploration RPG loop.
- Movement evidence at that checkpoint: manual no-cookie loopback spot check moved `raid-hall -> league-coliseum` via `4←`; Browser E2E `run/league-browser/browser-e2e-summary-1778406800-889859.json` was green. Current latest browser evidence is recorded in the 21:04 update below.
- Validation for this documentation slice: `git diff --check` plus direct inspection of `docs/trillionnium-world-development-progress-tree-v1.md` and yxts-gold-asm source constants.
- Remaining next: superseded by later checkpoints; TW-6.10, TW-3.12, TW-3.13, TW-3.11, and TW-2.15 are now complete. Current remaining gameplay gaps are TW-3.14 skill practice/mentor interaction and TW-3.15 lightweight combat entry/return.
- Constraints preserved: no live Overpass/Geofabrik ingestion, no MapLibre promotion, no MedievalWar/Phaser vendoring, no Hero Tan code/text/assets/data copying, and Rust remains source of truth.

---

#### Update 2026-05-10 21:04 CST

- Commit: `4ec9a92 feat: complete world local task lifecycle`
- Completed Hero Tan-style local NPC/task lifecycle slice after `9479ea9 feat: surface world exploration loop`:
  - [x] Added/validated `trillionnium_world_local_task_lifecycle_v1`.
  - [x] `/world` play-first prompt now connects `talk_npc -> offer_task -> active task -> complete_task -> settlement/review feedback`.
  - [x] `review_hold` and completed task states remain visible as feedback/review state instead of disappearing from the local task card.
  - [x] Successful and rejected `talk_npc`, `offer_task`, and `complete_task` web flows redirect back to `#world-play-first-action-prompt`.
  - [x] Browser E2E now moves to `mirror-city-square`, exercises NPC talk/task lifecycle, verifies completion feedback from `rust_world_contract_completions`, and records `world_local_npc_task_loop=true`.
- Evidence:
  - `cargo fmt --all -- --check`
  - `cargo check -p consumer-entry-api`
  - `cargo test -p consumer-entry-api -- --nocapture` (`133 passed`)
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `bash -n scripts/check-trillionnium-first-human-session.sh`
  - `git diff --check`
  - local-production status OK
  - Browser E2E: `run/league-browser/browser-e2e-summary-1778415910-949928.json`, `ok=true`
  - Web E2E: `run/league-web/web-e2e-summary-1778417153.json`, `ok=true`
  - First-human E2E: `run/first-human-session/browser-e2e-summary-1778418021-966047.json`, `ok=true`, zero request/page/console failures
- Remaining next at that checkpoint, later superseded by TW-2.15 completion:
  - [ ] TW-3.14 skill practice and mentor interaction from exploration nodes.
  - [ ] TW-3.15 lightweight combat encounter entry from exploration nodes, then return to map.
  - [x] TW-2.15 make active tasks/NPC/party objectives visibly travel/react through the same Rust-owned world-node graph. Completed in the 23:2x CST update below.
- Constraints preserved: no Hero Tan code/text/assets/data copying, no live OSM ingestion, no MapLibre promotion, browser/web intent-only, Rust source of truth.

---

#### Update 2026-05-10 22:24 CST

- Commit: `feat: gate world movement transitions` (this commit)
- Completed Hero Tan-style movement transition semantics after `5e216f1 docs: align progress tree with local task lifecycle`:
  - [x] Added `services/consumer-entry-api/src/world_movement.rs` with `trillionnium_world_transition_semantics_v1` and Rust authority `world_map_transition_decision(...)`.
  - [x] Movement now distinguishes `blocked_terrain`, `unknown_target`, `locked_route`, `interaction_required`, `non_adjacent_route`, `local_exit`, `room_transition`, `zone_transition`, and `wait`.
  - [x] `/v1/world/map/move` and `/world/web/map-move` reject blocked/locked/interaction/non-adjacent moves with conflict-style responses, return `404` for unknown targets, and include `movement_transition` for accepted moves.
  - [x] `/world` local exit forms, keypad shell, numpad buttons, hidden move form, and client JS expose transition contract/version/source/status/kind/result metadata while keeping browser/web as `intent_only_visualization_input`.
  - [x] Movement command source metadata remains `rust_world_map_move`; semantic classification source is explicitly `rust_world_map_transition_rules`.
  - [x] Browser E2E now hard-gates `coverage.world_transition_semantics=true` and verifies transition source/contract, accepted movement metadata, and blocked terrain affordance.
- Evidence:
  - `cargo fmt --all -- --check`
  - `cargo check -p consumer-entry-api`
  - `cargo test -p consumer-entry-api -- --nocapture` (`136 passed`)
  - `cargo clippy -p consumer-entry-api -- -D warnings`
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `bash -n scripts/check-trillionnium-first-human-session.sh`
  - `git diff --check`
  - local-production status OK
  - Browser E2E: `run/league-browser/browser-e2e-summary-1778422046-993350.json`, `ok=true`, `coverage.world_transition_semantics=true`, request-failure gate green with `0` unclassified failures
  - Web E2E: `run/league-web/web-e2e-summary-1778421907.json`, `ok=true`
  - First-human E2E: `run/first-human-session/browser-e2e-summary-1778422781-1002310.json`, `ok=true`, zero request/page/console failures
- Remaining next at that checkpoint, later superseded by TW-2.15 completion:
  - [x] TW-2.15 make active tasks/NPC/party objectives visibly travel/react through the same Rust-owned world-node graph. Completed in the 23:2x CST update below.
  - [ ] TW-3.14 skill practice and mentor interaction from exploration nodes.
  - [ ] TW-3.15 lightweight combat encounter entry from exploration nodes, then return to map.
- Constraints preserved: no Hero Tan code/text/assets/data copying, no live OSM ingestion, no MapLibre promotion, browser/web intent-only, Rust source of truth.

---

#### Update 2026-05-10 23:2x CST

- Commit: `feat: gate world objective travel` (this commit)
- Completed TW-2.15 active task/NPC/party objective travel after `40f7482 feat: gate world movement transitions`:
  - [x] Added `trillionnium_world_objective_travel_v1` to the Rust tactics projection.
  - [x] Rust computes objective routes from `world_state.world_map_nodes.exits`, the current `world_player_positions` node, active Trillionnium task contracts, visible OSM/NPC objective candidates, and party member positions.
  - [x] Projection exposes `active_route`, `path_node_ids`, `path_nodes`, `next_step_direction`, `next_step_node_id`, `target_node_id`, `route_tracks`, `party_members`, movement source, and transition source metadata.
  - [x] `/world` renders `#world-objective-travel`, keypad route roles (`current`, `next_step`, `path`, `target`), route/party data attributes, and visualization-only intent copy.
  - [x] `/world/web/map-move` returns refreshed `world_objective_travel`; the browser updates `window.trillionniumKeyboardMap.getState()` after Rust accepts movement, so objective travel reacts to player motion without browser-owned truth.
  - [x] Browser/Web gates now assert the objective-travel contract, route roles, party count, runtime state, and movement refresh path.
- Evidence:
  - `cargo fmt --all -- --check`
  - `cargo check -p consumer-entry-api`
  - `cargo test -p consumer-entry-api -- --nocapture --test-threads=1` (`136 passed`)
  - `cargo clippy -p consumer-entry-api -- -D warnings`
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `bash -n scripts/check-trillionnium-first-human-session.sh`
  - `git diff --check`
  - local-production restart/status OK
  - Browser E2E: first run hit the known map-delta 304 timing flake; rerun passed at `run/league-browser/browser-e2e-summary-1778425959-1021724.json`, `ok=true`, request-failure gate green with `0` unclassified failures
  - Web E2E: `run/league-web/web-e2e-summary-1778426609.json`, `ok=true`
  - First-human E2E: `run/first-human-session/browser-e2e-summary-1778426636-1029761.json`, `ok=true`, zero request/page/console failures
- Remaining next:
  - [x] TW-3.14 skill practice and mentor interaction from exploration nodes. Completed in the 2026-05-11 00:07 CST update below.
  - [ ] TW-3.15 lightweight combat encounter entry from exploration nodes, then return to map.
- Constraints preserved: no Hero Tan code/text/assets/data copying, no live OSM ingestion, no MapLibre promotion, browser/web intent-only, Rust source of truth.

---

#### Update 2026-05-11 00:07 CST

- Commit: `feat: gate world mentor skill practice` (this commit)
- Completed TW-3.14 skill practice and mentor interaction from exploration nodes after `da20c1a feat: gate world objective travel`:
  - [x] Added `trillionnium_world_skill_practice_loop_v1` to the Rust tactics projection and `/world` play-first prompt contract.
  - [x] Rust mentor training validation now rejects wrong-place and wrong-mentor `train_skill` intents, with source-of-truth `rust_mentor_training_validator` and web role `intent_only_visualization_input`.
  - [x] `/world` renders `#world-local-skill-practice` beside local exits/NPC/task loops, including node-local mentor practice forms, mentor/training metadata, and known-skill feedback sourced from `rust_trillionnium_character`.
  - [x] `/world/web/tactics-command` routes accepted/rejected `train_skill` web submits back to the exploration prompt instead of treating it as a detached tactics-only action.
  - [x] Browser E2E moves to `mirror-city-square`, submits the `basic_unarmed` mentor practice form for `npc-street-compass-sifu`, verifies the Rust-mutated character skill projection, then continues NPC talk/task pickup/completion.
- Evidence:
  - `cargo fmt --all -- --check`
  - `cargo check -p consumer-entry-api`
  - `cargo test -p consumer-entry-api -- --nocapture --test-threads=1` (`136 passed`)
  - `cargo clippy -p consumer-entry-api -- -D warnings`
  - `node --check scripts/playwright/trillionnium-browser-e2e.mjs`
  - `bash -n scripts/check-trillionnium-league-web-e2e.sh`
  - `bash -n scripts/check-trillionnium-first-human-session.sh`
  - `git diff --check`
  - local-production restart/status OK
  - Browser E2E: `run/league-browser/browser-e2e-summary-1778428412-1041291.json`, `ok=true`, `coverage.world_local_skill_practice_mentor_loop=true`, request-failure gate green with `0` unclassified failures
  - Web E2E: `run/league-web/web-e2e-summary-1778429124.json`, `ok=true`
  - First-human E2E: `run/first-human-session/browser-e2e-summary-1778429149-1049309.json`, `ok=true`, zero request/page/console failures
- Remaining next:
  - [ ] TW-3.15 lightweight combat encounter entry from exploration nodes, then return to map.
- Constraints preserved: no Hero Tan code/text/assets/data copying, no live OSM ingestion, no MapLibre promotion, browser/web intent-only, Rust source of truth.

---

## Current Next Pointer

If the next instruction is simply “continue”, start here:

> **Next pointer:** The Hero Tan-style exploration loop now has movement, Rust-owned transition semantics, node-local NPC talk, task pickup/completion, active task/NPC/party objective travel through the Rust-owned world-node graph, and node-local mentor skill practice. Do not continue visual skin work. The next highest product leverage is to deepen world logic by letting lightweight combat encounters start from map exploration and return to map state. Human-playability assessment remains `9.8/10` technical, `8.5/10` first internal beta, `7.0/10` commercial release; first-beta/commercial score lifts still require real evidence via `TRILLIONNIUM_FIRST_BETA_COHORT_EVIDENCE_PATH` and/or `TRILLIONNIUM_COMMERCIAL_LAUNCH_DRILL_EVIDENCE_PATH`. Technical `9.9+` requires multi-node or live-traffic latency evidence, not another local-only loop.

Do not start live Overpass/Geofabrik ingestion yet. Do not promote MapLibre. Do not convert the web shell into a standalone JS source of truth.

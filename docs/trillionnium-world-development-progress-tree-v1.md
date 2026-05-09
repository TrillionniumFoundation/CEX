# Trillionnium World Development Progress Tree v1

Generated: 2026-05-08 18:11 CST  
Current code checkpoint: `ea13ae6 feat: anchor trillionnium world osm geodata`
Current progress-tree checkpoint before this expansion: `8fc65f1 docs: add trillionnium world progress tree`
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
- `[ ]` stable fixture dataset still needs to replace mostly hash-derived identities.
- `[ ]` roads/buildings/areas/admin boundary fixture layers are not yet implemented.

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
- `[ ]` provider implementation should be split into `openstreetmap_geodata.rs`.
- `[ ]` provider health/readiness metrics still need explicit fixture/live/fail-closed checks.

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
- `[~]` tactics shell exists visually, but tactics state is not yet a Rust game model.
- `[ ]` Trillionnium attribute/skill/sect/NPC/task/combat-log systems need Trillionnium-native Rust models.

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
- `[ ]` tactics board and Trillionnium mechanics projections need first-class contract versions.

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
- `[~]` board is still mostly static/CSS scaffold.
- `[ ]` web must be rewired to render Rust `tactics_board` and `trillionnium_character` projections.

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
- `[ ]` tactics/Trillionnium command handlers need to be introduced and wired to the same ledger/progression discipline.

Command processing rule:

```text
Web intent -> Rust validate -> Rust mutate -> ledger/progression settle -> Rust event log -> new projection JSON -> Web re-render
```

---

## Trillionnium Mechanics Extraction Spec

The goal is not to port 白金英雄坛说 literally. The goal is to extract proven Trillionnium/MUD mechanics from `gmud`, `RMXP-Hero`, and `yxts-llm`, then rebuild them as Trillionnium-native Rust systems bound to OSM objectives and the tactics board.

### Source references and allowed use

| Reference | Use | Do not use directly | Trillionnium extraction target |
| --- | --- | --- | --- |
| `mogita/gmud` | Best authentic mechanics reference; MIT repo | Original text/maps/assets/tables without provenance review | skill taxonomy, task/NPC/fight engine shape, MUD-style logs |
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
3. **Legal OSS only.** Do not copy proprietary 三国 / 英雄坛说 / 白金英雄坛说 code, text, or assets.
4. **MedievalWar is the current direct tactics UI base.** Use `tranchikhang/MedievalWar` as the permissive MIT tactics reference/base for browser-side board patterns.
5. **Hero Tan Shuo projects are mechanics references only.** Use GMUD / RMXP / yxts-llm style ideas only after recreating Trillionnium-native content/assets.
6. **OpenClawStreetMap remains support/underlay.** It supplies real-world map support, route context, geodata identity, and operational diagnostics; it should not dominate the visible game UI.
7. **OpenStreetMap data belongs below Rust.** OSM provides roads, POIs, buildings, areas, admin boundaries, tags, and identities; Rust binds these to game overlays and gameplay systems.
8. **No production use of public OSM tile servers.** Production traffic requires cache/self-host/vendor strategy.
9. **Respect OSM attribution and ODbL obligations.** Keep attribution and derived database tracking visible in contracts.
10. **MapLibre stays shadow-only.** Do not promote MapLibre above canary `0` without fresh explicit signoff.

---

## Current Checkpoint Summary

### Latest local commit

- `ea13ae6 feat: anchor trillionnium world osm geodata`

### Latest validated evidence

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `bash -n` touched scripts — passed
- `cargo test -p consumer-entry-api -- --nocapture` — `129 passed`
- Web E2E — `run/league-web/web-e2e-summary-1778234500.json`, `ok=true`
- Route-runner handoff monitoring contract — passed
- Production readiness — `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh`, `READY`

### Current working-tree expectation

After `ea13ae6`, CEX should be clean. If not clean, inspect before editing:

```bash
git status --short
git log --oneline -5
```

---

## Current Implemented Contracts

### Game UI / OSS base

- `/world` exposes `trillionnium_open_source_tactics_world_shell_v1`.
- `/world` uses `data-open-source-base="tranchikhang/MedievalWar"`.
- `/world` declares:
  - `data-base-license="MIT"`
  - `data-base-engine="Phaser 3"`
  - `data-base-patterns="map,cursor,control,turn_system,pathfinding,context_menu,objectives,ai"`
  - `data-map-engine-role="openclawstreetmap_underlay"`
- Current shell is still a Rust-rendered/CSS tactics scaffold, not a full Phaser integration.

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
| Projection layer | `services/consumer-entry-api/src/world_map_projection.rs` | Rust source-of-truth JSON, OSM provider contract, map/runtime contracts |
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
| This progress tree | `docs/trillionnium-world-development-progress-tree-v1.md` | canonical continuation guide |

---

## Active Progress Tree

### TW-0 — Preserve the current baseline

- [x] TW-0.1 Keep CEX repo at clean checkpoint after OSM geodata slice.
  - Evidence: `ea13ae6`
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
- [ ] TW-2.11 Decide integration strategy for actual MedievalWar/Phaser code.
  - Option A: port patterns only, no vendored code.
  - Option B: vendor MIT code under `third_party/` with license notice.
  - Recommendation: start with patterns only; vendor only when the Rust game-state contract stabilizes.
- [!] TW-2.12 Do not copy MedievalWar art assets unless license/attribution is tracked.

### TW-3 — Trillionnium / Hero Tan Shuo mechanics reference layer

- [x] TW-3.1 Search and classify 白金英雄坛说 / 英雄坛说 OSS candidates.
- [x] TW-3.2 Decide no direct fork is legally/product-clean today.
- [x] TW-3.3 Use GMUD/Hero Tan projects as mechanics references only.
- [ ] TW-3.4 Define Trillionnium-native character attributes.
  - Required fields: `physique`, `force`, `agility`, `insight`, `resolve`, `craft`, `commerce`, `reputation`.
  - Source inspiration: gmud/RMXP-Hero/yxts-llm attribute loops; names/content must be Trillionnium-native.
- [x] TW-3.4a Add `TrillionniumAttributes` Rust model.
- [x] TW-3.4b Add deterministic derived stats and caps.
- [x] TW-3.4c Add tests for attribute projection and derived stats.
- [ ] TW-3.5 Define skill/sect/mentor/NPC relationship models in Rust.
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
- [!] TW-3.8 Do not import original Hero Tan Shuo text, maps, sprites, or database content.

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
- [~] TW-4.8 Prepare repository/storage boundary for game session persistence.
  - Game sessions and simulation ticks now persist through serde-compatible `WorldState` fields; a dedicated normalized repository table remains a later storage cutover.

### TW-5 — Gameplay loops

- [x] TW-5.1 Existing world commerce loop works: company -> shop/listing -> purchase -> work order -> delivery -> accept/reject/reopen/cancel.
- [x] TW-5.2 Existing route-runner handoff/reward/mastery gates are green.
- [~] TW-5.3 Current `/world` visible game loop is still mostly presentation + forms.
- [ ] TW-5.4 Implement first tactics loop:
  - spawn player unit
  - spawn one objective
  - allow move
  - allow interact/claim
  - write event log
  - reward through existing ledger/progression path
- [ ] TW-5.5 Implement combat loop:
  - enemy unit
  - attack action
  - damage resolution
  - victory/failure state
- [ ] TW-5.6 Bind tactics objective to existing route task graph.
- [ ] TW-5.7 Bind rewards to route-runner reward history and ledger settlement.
- [ ] TW-5.8 Add anti-cheese checks for repeatable encounter farming.

### TW-6 — Web visualization/input shell

- [x] TW-6.1 `/world` displays tactics game shell before map support details.
- [x] TW-6.2 `/world` still exposes OpenClawStreetMap diagnostics under support/advanced layers.
- [x] TW-6.3 `/world` exposes OSM provider contract and feature cards.
- [ ] TW-6.4 Make board cells data-driven from Rust projection.
- [ ] TW-6.5 Add input affordances for unit selection and command drafting.
- [ ] TW-6.6 Add clear mobile-first game HUD.
  - active unit
  - current objective
  - primary action
  - risk/reward
- [ ] TW-6.7 Add accessibility labels and keyboard/low-motion support for the tactics shell.
- [ ] TW-6.8 Keep old dashboard panels available as secondary/detail panels, not main experience.

### TW-7 — Map and runtime operations

- [x] TW-7.1 Map readability, runtime budget, delta/cache, RUM, weak-network, privacy gates exist.
- [x] TW-7.2 Monitoring/readiness covers MapLibre shadow canary safety.
- [x] TW-7.3 OSM production traffic policy visible.
- [ ] TW-7.4 Add explicit OSM provider health/readiness section.
  - fixture mode should be green
  - live mode should be disabled/fail-closed
- [ ] TW-7.5 Add geodata freshness/staleness metrics.
- [ ] TW-7.6 Add OSM attribution presence check to web E2E and UI audit if not already hard-gated.
- [!] TW-7.7 Do not increase MapLibre canary above 0 without fresh production signoff.

### TW-8 — Validation gates

Minimum gate for documentation-only changes:

- [ ] `git diff --check`

Minimum gate for Rust projection/UI changes:

- [ ] `cargo fmt --all -- --check`
- [ ] `git diff --check`
- [ ] `bash -n` for touched shell scripts
- [ ] targeted `cargo test -p consumer-entry-api <test-name> -- --nocapture`
- [ ] `cargo test -p consumer-entry-api -- --nocapture`

Minimum gate for `/world` UI contract changes:

- [ ] restart local production runtime when E2E depends on server code:
  - `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart`
- [ ] `CEX_ENV_FILE=run/local-production/.env scripts/check-trillionnium-league-web-e2e.sh`

Minimum gate for monitoring/readiness changes:

- [ ] `scripts/check-trillionnium-route-runner-handoff-monitoring.sh`
- [ ] `CEX_ENV_FILE=run/local-production/.env scripts/check-production-readiness.sh`

Full product checkpoint gate when touching core world runtime:

- [ ] `cargo test -p consumer-entry-api -p matrix-entry-adapter -p ledger-service -- --nocapture`
- [ ] Web E2E
- [ ] Browser E2E if browser runtime/js changed
- [ ] UI audit if DOM contract changed
- [ ] real-user beta/public-commercial if product readiness changed
- [ ] production signoff only when requested or when making release-grade runtime changes

---

## Recommended Next Development Slice

The safest next slice after checkpoint `ab5f046` was **TW-1.6 + TW-1.7 + TW-2.6 + TW-3.4a**; that slice is now implemented in the active working tree and should be committed after validation.

The next development slice after the current checkpoint should be **TW-1.9 + TW-1.10 + TW-2.9 + TW-3.5b/d/g**:

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
  - [ ] TW-4.8+ decide whether game sessions/ticks need a dedicated normalized repository table or whether the current serde-compatible `WorldState` persistence is enough for first playable.
  - [ ] TW-5.4/TW-5.5 continue from persistent sessions into a tighter first tactics loop and combat loop: objective progress, victory/failure state, and reward settlement.

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

## Current Next Pointer

If the next instruction is simply “continue”, start here:

> **TW-5+:** build on the persisted tactics sessions/ticks and normalized overlay identity by tightening the first tactics loop and combat loop: objective progress, victory/failure state, reward settlement, and the TW-4.8 storage-boundary decision.

Do not start live Overpass/Geofabrik ingestion yet. Do not promote MapLibre. Do not convert the web shell into a standalone JS source of truth.

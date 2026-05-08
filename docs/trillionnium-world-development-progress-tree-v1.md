# Trillionnium World Development Progress Tree v1

Generated: 2026-05-08 18:11 CST  
Current checkpoint: `ea13ae6 feat: anchor trillionnium world osm geodata`  
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
- [ ] TW-1.6 Split OSM provider code into a dedicated module/file.
  - Suggested file: `services/consumer-entry-api/src/openstreetmap_geodata.rs`
  - Keep `world_map_projection.rs` as projection assembly, not provider implementation.
- [ ] TW-1.7 Add explicit fixture dataset instead of deriving all OSM IDs from `node_id` hashes.
  - Suggested fixture: stable sample OSM identities for Shanghai core nodes.
  - Keep deterministic fallback for missing fixtures.
- [ ] TW-1.8 Add roads/buildings/areas/admin boundary fixture layers.
  - Do not add live ingestion yet.
- [ ] TW-1.9 Add derived database tracking metadata.
  - Include fixture source, import timestamp, transform version, and ODbL share-alike note.
- [ ] TW-1.10 Add provider-mode enum and test each mode is fail-closed.
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
- [ ] TW-2.6 Define Rust-side tactics board model.
  - Board size / terrain / occupied cells / objectives / turn state / active unit.
  - Projection should come from Rust JSON, not hard-coded HTML loops.
- [ ] TW-2.7 Define Rust-side unit model.
  - `unit_id`, `owner`, `class/archetype`, `hp`, `energy`, `position`, `move_range`, `attack_range`, `status_effects`.
- [ ] TW-2.8 Define Rust-side turn/action command model.
  - `select_unit`, `move_unit`, `attack`, `use_skill`, `interact`, `end_turn`.
- [ ] TW-2.9 Add command endpoints/forms for tactics actions.
  - Web sends intent; Rust validates and mutates state.
- [ ] TW-2.10 Replace static board rendering with Rust-projected board state.
- [ ] TW-2.11 Decide integration strategy for actual MedievalWar/Phaser code.
  - Option A: port patterns only, no vendored code.
  - Option B: vendor MIT code under `third_party/` with license notice.
  - Recommendation: start with patterns only; vendor only when the Rust game-state contract stabilizes.
- [!] TW-2.12 Do not copy MedievalWar art assets unless license/attribution is tracked.

### TW-3 — Jianghu / Hero Tan Shuo mechanics reference layer

- [x] TW-3.1 Search and classify 白金英雄坛说 / 英雄坛说 OSS candidates.
- [x] TW-3.2 Decide no direct fork is legally/product-clean today.
- [x] TW-3.3 Use GMUD/Hero Tan projects as mechanics references only.
- [ ] TW-3.4 Define Trillionnium-native character attributes.
  - Suggested: physique, agility, insight, reputation, craft, commerce, resolve.
- [ ] TW-3.5 Define skill/sect/mentor/NPC relationship models in Rust.
- [ ] TW-3.6 Define text battle/task log style without copying original content.
- [ ] TW-3.7 Bind Jianghu mechanics to tactics units and OSM locations.
  - Example: mentor NPC at an OSM POI, training unlocks tactics skill.
- [!] TW-3.8 Do not import original Hero Tan Shuo text, maps, sprites, or database content.

### TW-4 — Rust World domain and simulation backbone

- [x] TW-4.1 `WorldState` already separates world fields inside League state.
- [x] TW-4.2 Existing indexes support commerce/workflow hot paths.
- [x] TW-4.3 Existing route artifacts feed route cockpit/task graph.
- [ ] TW-4.4 Introduce explicit game-session state for `/world`.
  - Player position, party, board encounter, current turn, active objective.
- [ ] TW-4.5 Normalize map node / OSM feature / game overlay relationship.
  - Avoid duplicating identity in ad-hoc JSON.
- [ ] TW-4.6 Add deterministic simulation tick / encounter generation hooks.
- [ ] TW-4.7 Add Rust tests for turn resolution and invalid command rejection.
- [ ] TW-4.8 Prepare repository/storage boundary for game session persistence.

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

The safest next slice is **TW-1.6 + TW-1.7 + TW-2.6**:

1. Move OSM provider implementation out of `world_map_projection.rs` into a dedicated Rust module.
2. Add an explicit OSM fixture dataset with stable feature identities.
3. Start a Rust-side tactics board projection model, but do not yet add full combat.

Why this order:

- It preserves the user's core architecture requirement: Rust is bottom/source of truth.
- It prevents `/world` from drifting back into a hard-coded HTML shell.
- It keeps live OSM ingestion disabled while still making the geodata substrate real.

Expected first-slice deliverables:

- `services/consumer-entry-api/src/openstreetmap_geodata.rs`
- optional `services/consumer-entry-api/src/world_tactics.rs`
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

> **TW-1.6 / TW-1.7 / TW-2.6:** split the OSM provider into a dedicated Rust module, add stable fixture identities, then introduce the first Rust-owned tactics board projection.

Do not start live Overpass/Geofabrik ingestion yet. Do not promote MapLibre. Do not convert the web shell into a standalone JS source of truth.

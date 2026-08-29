# Trillionnium World Open-source Stack Reference v1

## Goal

给 `Trillionnium World` 做一份**按层拆分**的开源参考栈，而不是去找一个“全包式游戏引擎”硬套。

核心判断：

- 我们当前是 **map-first / world-first / route-first**，不是传统 3D 游戏先行。
- 当前最成熟的产品能力在：**地图镜像 + overlay + route cockpit + event focus + commerce / contract / work loop**。
- 所以应该按层借鉴：**渲染层 / 世界建模层 / 社交实时层 / 3D 地理层**，而不是试图把整套产品迁到单一引擎。

---

## Executive Summary

如果只保留一句建议：

1. **现在继续用 Leaflet** 做 live engine。
2. **接口和 adapter 设计向 MapLibre 靠拢**，但先不切默认引擎。
3. **世界状态建模借 Bevy 的 ECS 思路**。
4. **玩法/场景组织可借 Godot**，但不要现在整机迁移。
5. **社交 / presence / matchmaking / realtime 可借 Nakama**。
6. **只有在明确需要 3D 地理世界时再引入 CesiumJS**。
7. **新的可玩前台界面以 `tranchikhang/MedievalWar` 为直接战棋魔改底座**；详见 `docs/trillionnium-open-source-tactics-base-selection-v1.md`。
8. **白金英雄坛说 / 英雄坛说 系项目不作为直接 UI fork**，但可作为 Trillionnium 机制参考层；详见 `docs/trillionnium-open-source-hero-tan-shuo-base-selection-v1.md`。
9. **后续开发按进度树推进**，不要靠聊天记忆续命；详见 `docs/trillionnium-world-development-progress-tree-v1.md`。

这与当前仓库里已经落下的路线一致：

- active engine = `leaflet_openstreetmap_v1`
- planned candidate = `maplibre_gl_v1`
- shared runtime handle = `mapRuntime`
- browser-side map logic already sits behind a renderer adapter seam

---

## Layered Reference Table

| Layer | Trillionnium need | Best open-source references | What to borrow | Recommendation |
| --- | --- | --- | --- | --- |
| 2D live map renderer | POI / route / event / density / overlay-heavy world map | **Leaflet** | Lightweight raster map, simple overlay layering, fast iteration | **Keep as current live engine** |
| Planned vector/WebGL renderer | Future vector tiles, zoom styling, heavier object counts | **MapLibre GL JS** | GPU map rendering, style pipeline, camera model, vector tile workflow | **Prepare via adapter, not default yet** |
| 3D geospatial layer | Globe / city-scale digital twin / pitch-bearing-heavy world view | **CesiumJS** | 3D globe, terrain, 3D geospatial rendering patterns | **Only adopt when 3D becomes product-critical** |
| World state / simulation model | Entities like nodes/events/contracts/companies/listings/work orders | **Bevy** | ECS-style entity/component/system thinking, data-driven world modeling | **Borrow architecture ideas now** |
| Gameplay scene / interaction organization | Rich scene/state/UI composition for future immersive shells | **Godot** | Scene tree, gameplay/state flow, interaction packaging | **Borrow interaction patterns, avoid full migration now** |
| Visible tactics game shell | Turn-based board, cursor, units, movement, objectives, battle log | **tranchikhang/MedievalWar** | MIT Phaser 3 tactics structure: map/cursor/control/turn/pathfinding/menu/objectives/AI | **Use as direct modding base for `/world` shell** |
| Trillionnium RPG mechanics layer | Attributes, skills, sects, mentors, NPC society, Wuxia text battle/task flavor | **mogita/gmud**, `RMXP-Hero`, `yxts-llm` | 白金英雄坛说-like progression and society/survival loops | **Reference mechanics only; recreate Trillionnium-native content/assets** |
| Social / realtime / multiplayer backend | Presence, chat, guilds, matchmaking, live coordination | **Nakama** | Realtime game backend patterns, social/presence/matchmaking APIs | **Reference for backend structure later** |

---

## Project-by-project Notes

## 1) Leaflet

### Why it fits now

Leaflet still matches the current product reality:

- base map is still raster OpenStreetMap
- current value comes from overlays and route/event focus, not shaders
- `/app` and `/world` still change quickly
- DOM/SVG overlay primitives are cheap to iterate on while the UX is still moving

### What to borrow / keep

- simple overlay grouping
- fast marker/line/pulse iteration
- lightweight event and POI visualization
- operational familiarity while the world product surface is still changing fast

### Trillionnium decision

**Keep Leaflet as the active engine now.**

That is already expressed in the repo as:

- `leaflet_openstreetmap_v1`
- shared adapter contract
- engine-neutral `mapRuntime`

---

## 2) MapLibre GL JS

### Why it matters

MapLibre is the most natural next renderer once the world starts needing:

- vector tiles
- GPU-first overlay scaling
- style-by-zoom / style-by-world-state behavior
- smoother dense object rendering on mobile
- more advanced camera semantics

### What to borrow now

Even before migration, we should borrow its *shape*:

- engine-neutral camera model
- style/layer separation
- explicit renderer capability contracts
- vector/WebGL-ready adapter seam

### Trillionnium decision

**Do not switch yet.**

Instead:

- keep `maplibre_gl_v1` as the planned engine candidate
- make all new map behavior pass through the adapter seam first
- only promote it when Leaflet becomes a measurable bottleneck

---

## 3) CesiumJS

### Why it is interesting

Cesium is not the next step for the current web shell, but it is the clearest reference if Trillionnium World evolves into:

- world-scale real geography
- 3D globe / terrain / city twins
- spatial simulation with altitude / orbital / infrastructure views

### What to borrow

- 3D geospatial mental model
- world-scale coordinate/camera semantics
- separation between geospatial scene and domain overlays

### Trillionnium decision

**Not for the current product slice.**

Treat it as a later-stage reference for a future 3D “world mode”, not as the next renderer swap.

---

## 4) Bevy

### Why it is highly relevant

Bevy is valuable less as a renderer choice and more as a **world-modeling reference**.

Trillionnium already has domain objects that map naturally onto ECS-style thinking:

- map nodes
- locations
- entities / agents
- world events
- route tasks
- contracts
- companies
- listings
- purchases
- work orders
- faction standing

### What to borrow

- entity/component/system decomposition
- data-first world state updates
- explicit system boundaries for route, commerce, social, progression, reputation
- easier reasoning about “same world object, different surface projections”

### Trillionnium decision

**Borrow ECS ideas now, without rewriting the product into Bevy.**

This is especially relevant for future cleanup of the world/commerce/progression model.

Companion repo-facing refactor note:

- `docs/trillionnium-world-ecs-refactor-v1.md`

---

## 5) Godot

### Why it is useful

Godot is useful as a reference for:

- scene/state composition
- interaction packaging
- UI-state + world-state coordination
- event-driven gameplay shells

### What to borrow

- scene graph thinking for future immersive world shells
- packaging of interaction flows as reusable scene/state units
- clean separation between visual shell and gameplay logic

### Trillionnium decision

**Borrow patterns, not platform.**

A full migration into Godot would be architectural overreach for the current map-first web product.

---

## 6) Nakama

### Why it matters

Nakama does not solve the map renderer, but it is a strong reference for later layers such as:

- presence
- realtime coordination
- guild/social graph behavior
- chat-adjacent multiplayer backend patterns
- matchmaking and activity loops

### What to borrow

- service boundaries for social/realtime game backend
- event and presence patterns
- game-centric backend API shapes

### Trillionnium decision

**Reference it for future social/realtime backend design, not for the current renderer decision.**

---

## What Not To Do

## 1. Do not replace everything with a single “game engine” now

That would collapse several distinct needs into one tool choice:

- geospatial renderer
- world domain model
- social backend
- commerce/contract pipeline
- mobile/web shell

Those should remain separate decisions.

## 2. Do not migrate to MapLibre before finishing the adapter seam

If the swap happens before renderer-neutral structure is stable, we pay rewrite cost twice.

## 3. Do not import game-backend assumptions into ledger/contract truth

Realtime/social backends can inspire presence and session behavior, but CEX settlement / ledger / contract truth should stay in the current domain services.

---

## Recommended Borrow Order For Trillionnium

### Now

- **Leaflet** for live rendering
- **MapLibre** as target interface pressure
- **Bevy** as world-model inspiration

### Next

- **Nakama** for future social/realtime design references
- **Godot** for richer world-shell / scene-flow inspiration

### Later

- **CesiumJS** only if 3D geospatial world mode becomes a real product need

---

## Concrete Repo-facing Recommendation

For the current repo, the best next-order borrow strategy is:

1. keep `leaflet_openstreetmap_v1` active
2. keep expanding the renderer adapter contract rather than leaking renderer-specific APIs
3. treat `maplibre_gl_v1` as the first serious future-engine target
4. shape world/route/event/commerce state with more ECS-like boundaries inspired by Bevy
5. keep social/realtime concerns separate from renderer work, with Nakama as a future reference rather than an immediate dependency

That gives us the upside of open-source precedent **without** forcing the product into the wrong abstraction too early.

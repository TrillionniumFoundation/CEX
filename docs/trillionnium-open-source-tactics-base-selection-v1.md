# Trillionnium Open-Source Tactics Base Selection v1

## Decision

Use **`tranchikhang/MedievalWar`** as the direct modding base for the next `/world` shell.

- Repository: <https://github.com/tranchikhang/MedievalWar>
- License: MIT (`LICENSE`, copyright Khang Tran, 2019)
- Stack: browser JavaScript + Phaser 3
- Gameplay shape: Fire Emblem-inspired turn-based strategy/tactics
- Directly useful systems: map data/loading, cursor, controls, movement, context menu, camera, unit turn system, pathfinding, battle system, enemy AI, map objectives
- Asset note: README credits Toen's Medieval Strategy sprite pack as CC-BY 3.0. Trillionnium should not import those assets unless attribution is carried through. The first integration uses CSS/token units to avoid asset-license coupling.

## Why this base

The user's target is not another dashboard or map console. The new target is a real game interface closer to 三国群英传 / 三国策 / 三国志 / 三国霸业 / 白金英雄坛说 as *style references*, while staying legally open-source.

`MedievalWar` is the best immediate base because it is:

1. **Actually a browser tactics game** — unlike skeletal Three Kingdoms data-model projects, it has a playable map/cursor/unit/turn loop.
2. **Permissive** — MIT source code, much easier to embed or fork than GPL/AGPL projects.
3. **Moddable into Trillionnium** — map objectives can become OpenClawStreetMap capture points; units can become Trillionnium Agents / factions; battle logs can become route evidence; rewards can stay ledger-gated.
4. **Light enough for `/world`** — no heavy server stack required.

## Candidate comparison

| Candidate | License / rights | Stack | Fit | Decision |
|---|---:|---|---|---|
| `tranchikhang/MedievalWar` | MIT code; art pack CC-BY 3.0 if reused | JS + Phaser 3 | Real browser turn-based tactics with map/cursor/turn/pathfinding/objectives | **Chosen direct base** |
| `noiron/battle-chess` | **No license file / package license found** | TypeScript + React/Vite | Best Three Kingdoms visual/theme match: “三国背景战棋类策略游戏”, with map editor, battle cells, city/cursor assets | **Do not fork unless author adds license; reference only for UX** |
| `yiyuezhuo/Hex-Wargame-JavaScript` | MIT | JavaScript + HTML/CSS | Classic hex wargame, scenario editor, AI and CSV scenario data; stronger wargame system than RPG flavor | Reference / possible later hex engine |
| `byn9826/Warring-States-Epic` | BSD-3-Clause | Vue + Electron/static web | Chinese historical board-war flavor and browser build, but more board game than tactics RPG | Reference only |
| `chessmasterhong/WaterEmblem` | **No license file found** | JavaScript | Fire Emblem-like game with many level/entity/battle animation files, but legal rights unclear and naming/assets are risky | Do not fork |
| `semibran/tactics` | MIT | JS/canvas | Clean deterministic tactical RPG, but very minimal and old build chain | Reference / fallback |
| `excaliburjs/sample-tactics` | BSD-2-Clause | TypeScript + Excalibur | Modern sample, but sample-size rather than full game | Reference |
| `lzxb/sanguozhi` | LICENSE says MIT; package says ISC | TypeScript | Three Kingdoms naming/model fields, but mostly random city/person model and console output | Reference only |
| `freeors/War-Of-Kingdom` | **No license file found in repo** | C/C++/Rose SDK + bundled binaries/assets | Strong Three Kingdoms/theme-adjacent turn-based tactics, but license unclear, binary-heavy, and not browser-native | Do not fork until license is clarified |
| `nkzw-tech/athena-crisis` | MIT source only; branding/art/content/campaign/multiplayer/music not open source | TypeScript/React | High-quality modern tactics engine, but open-core rights are too constrained for direct content/asset mod | Reference only |
| `wesnoth/wesnoth` | GPL-2 | C++ desktop | Mature tactics RPG, but copyleft + desktop integration heavy | Reference only |

## Integration stance

- Keep `OpenClawStreetMap` as the hidden real-world map engine.
- The visible `/world` first screen should be a **turn-based tactics board**, not a map dashboard.
- Use MedievalWar's structural concepts in the product contract:
  - `map`
  - `cursor`
  - `control`
  - `turn_system`
  - `pathfinding`
  - `context_menu`
  - `objectives`
  - `ai`
- Do not copy proprietary Three Kingdoms/Fire Emblem assets, code, names, or UI. Those games remain style references only.
- MapLibre remains shadow-only/candidate; no canary promotion without fresh signoff.

## First implementation contract

DOM contract added to `/world`:

- `data-contract-version="trillionnium_open_source_tactics_world_shell_v1"`
- `data-open-source-base="tranchikhang/MedievalWar"`
- `data-base-license="MIT"`
- `data-base-engine="Phaser 3"`
- `data-base-patterns="map,cursor,control,turn_system,pathfinding,context_menu,objectives,ai"`
- `data-map-engine-role="openclawstreetmap_underlay"`

The first shell is intentionally CSS-token based while we prepare a clean import path. This proves the product direction and gate contract without introducing Phaser bundle weight or third-party art attribution risks in the same slice.

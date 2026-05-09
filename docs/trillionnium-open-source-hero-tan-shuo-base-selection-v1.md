# Trillionnium Open-Source Hero Tan Shuo Base Selection v1

## Decision

No legally clean, browser-ready, permissive **白金英雄坛说 / 英雄坛说** direct base was found.

Use the Hero Tan Shuo search as a **mechanics / Trillionnium-system reference layer**, not as the immediate `/world` UI shell fork. The safest near-term plan remains:

1. Keep `tranchikhang/MedievalWar` as the browser tactics/game-shell base.
2. Use `OpenClawStreetMap` as the real-world underlay and objective/event source.
3. Use the 白金英雄坛说-like projects below to inform Trillionnium mechanics: attributes, skills, sects, NPC dialogue, training, tasks, survival loops, and text-battle flavor.
4. Do not copy original proprietary names/assets/text wholesale unless the rights are explicit.

## Search scope

Searched GitHub/web/Gitee-style queries for:

- `白金英雄坛说 开源 GitHub`
- `英雄坛说 源码`
- `白金英雄坛说 Python`
- `白金英雄坛说 Godot`
- `BaiJinHero`
- `yingxiongtanshuo`
- `GMUD 英雄坛说`
- `新英雄坛说`
- `文字武侠 MUD Python/MIT`

Local inspection clones live under `/tmp/cex-hero-tan-bases`.

## Candidate summary

| Candidate | License / rights | Stack | Fit | Decision |
|---|---:|---|---|---|
| `mogita/gmud` | GitHub license: MIT; README says “Lee 开源版 GMUD 源码” | WQX/GMUD assembly + data | Best authentic mechanics/source reference for 英雄坛说; has skills, maps, text/data, original engine structure | **Best mechanics reference; not a browser UI base** |
| `qq634488405/RMXP-Hero` | GitHub license: GPL-3.0, but script header includes restrictive/noncommercial/custom terms; README says assets extracted from WQX/GP1288 versions | RPG Maker XP / Ruby | Rich 白金/黄金融合 remake, many data tables and battle/skill/menu systems | **Reference only unless GPL + asset + custom-license conflicts are resolved** |
| `coyoteXujie/yxts-llm` | GitHub license metadata: none; README says “MIT License” but no full `LICENSE` file found | Python + Arcade | Modern Python remake with 平安镇, NPC, combat, quests, LLM dialogue; strong system reference | **Ask author/add LICENSE before direct reuse; reference only for now** |
| `Toxicccxz/BaiJinHero` | No license | Godot 4.5 mobile | Title/menu/bootstrap scaffold named 白金英雄坛说; no real world scene present in repo | **Do not fork; too incomplete and unlicensed** |
| `lw0717/GmudEX` | No license | Android Java | 白金英雄坛说EX2014 Android implementation with map/assets/game classes | **Do not fork; unlicensed Android/reference only** |
| `sbhhbs/lava_collection` | No license | WQX LAVA archive | Contains `新英雄坛说(公测最终版)` docs and archived LAVA programs; useful design notes | **Archive/design reference only** |
| `GeorgeChen-666/sgmud` | No license | RPG Maker MV / JS plus archived GMUD-simple Android docs/assets | Contains GMUD/simple docs and RPG Maker web runtime shell; huge asset-heavy repo | **Do not fork; reference only** |
| `gengjian1203/FreedomLegend` | No license; README says assets are from internet and not for commercial use | Cocos Creator + WeChat mini-game/cloud | “醉梦坛说” homage with idle/card RPG systems | **Do not use directly** |
| `0920mzy/yingxiongtanshuo` | No license; empty repo checkout | Empty/minimal | Name match only | Ignore |
| `lhing17/hero_altar` | GPL-3.0 | WAR3 map project | Search false-positive: “英雄传说”, not 英雄坛说 | Ignore |

## Detailed notes

## 1) `mogita/gmud`

- Repository: <https://github.com/mogita/gmud>
- Local clone: `/tmp/cex-hero-tan-bases/gmud`
- License: MIT (`LICENSE`)
- README: “文曲星经典之作《英雄坛说》研究资料整理” and “Lee 开源版 GMUD 源码”
- Useful files:
  - `src/skill.dat` — basic skills such as `基本内功`, `读书识字`
  - `src/serve.s`, `src/task.s`, `src/fight.s`, `src/gmud.s` — NPC/dialogue/task/fight/engine structure
  - `src/data/map/*` — map/NPC data

### Fit

This is the strongest authentic source/mechanics reference. It is not a frontend/game UI base: the code is old WQX/assembly-oriented and should be treated as a source-of-truth reference for mechanics, not as code we embed into the web app.

### Legal stance

MIT is promising, but because the repo is an organized copy of an older game source, we should still keep attribution and avoid blindly importing exact text/assets into commercial UI until provenance is reviewed.

## 2) `qq634488405/RMXP-Hero`

- Repository: <https://github.com/qq634488405/RMXP-Hero>
- Local clone: `/tmp/cex-hero-tan-bases/RMXP-Hero`
- GitHub license metadata: GPL-3.0
- Stack: RPG Maker XP / Ruby
- README says it is based on WQX 黄金英雄坛说 and 白金英雄坛说, thanks Lee for opening NC2000 白金英雄坛说, and uses GP1288/extracted gray-scale assets.
- Rich data in `Script/106 - New_Data.rb`: skills, sects, maps, config copy, credits.

### Fit

Mechanically rich and close to the desired Trillionnium feeling. It is also not browser-native and is tightly bound to RMXP/RGSS conventions.

### Legal stance

Not suitable as a direct product fork right now:

- GPL-3.0 is copyleft and not permissive.
- `Script/000 - 开源协议.rb` includes additional noncommercial / limited-distribution / author-approval style restrictions that conflict with a clean permissive commercial reuse posture.
- README explicitly mentions extracted original tiles/assets.

Use for high-level systems comparison only unless legal posture is intentionally changed.

## 3) `coyoteXujie/yxts-llm`

- Repository: <https://github.com/coyoteXujie/yxts-llm>
- Local clone: `/tmp/cex-hero-tan-bases/yxts-llm`
- GitHub license metadata: none
- README has a “MIT License” line but no full `LICENSE` file was found.
- Stack: Python 3.9+ + Arcade 3.x
- Systems: 2D map, 平安镇, attributes, skills, NPCs, combat, quest system, LLM dialogue.
- Asset folder has 323 tile PNGs; provenance is not documented in the inspected README.

### Fit

Best modern-code reference. It is much easier to understand than the assembly/RMXP versions and has game-system shapes that map well to Trillionnium.

### Legal stance

Do not directly copy until a real license file / author confirmation is available. If clarified as MIT, it could become a good non-browser gameplay logic reference, but still not a browser UI base.

## 4) `Toxicccxz/BaiJinHero`

- Repository: <https://github.com/Toxicccxz/BaiJinHero>
- Local clone: `/tmp/cex-hero-tan-bases/BaiJinHero`
- License: none
- Stack: Godot 4.5 mobile
- Repo has bootstrap/title/menu/autoload scaffolding and title BGM.
- `TitleScreen.gd` points to `res://features/exploration/scenes/World_01.tscn`, but no such gameplay scene was present in the inspected file tree.

### Fit

Name match and useful Godot shell idea only. Not enough game code to serve as a base.

## 5) `lw0717/GmudEX`

- Repository: <https://github.com/lw0717/GmudEX>
- Local clone: `/tmp/cex-hero-tan-bases/GmudEX`
- License: none
- Stack: Android Java
- App string: `白金英雄坛说EX2014`
- Contains Android game classes/assets/doc output.

### Fit

Closer to original than generic Wuxia projects, but old Android-native, unlicensed, and asset-heavy.

## 6) `sbhhbs/lava_collection`

- Repository: <https://github.com/sbhhbs/lava_collection>
- Local clone: `/tmp/cex-hero-tan-bases/lava_collection`
- License: none
- Relevant path: `sandbox/新英雄坛说(公测最终版)/Doc/readme.txt`

### Fit

Useful historical/design reference. The `新英雄坛说` document explicitly describes the goal as a new GMUD-like society/survival game with new characters, plot, engine, settings, tasks, sects, items, and survival/progression loops.

Do not use code/assets directly.

## 7) `GeorgeChen-666/sgmud`

- Repository: <https://github.com/GeorgeChen-666/sgmud>
- Local sparse clone: `/tmp/cex-hero-tan-bases/sgmud`
- License: none
- Stack: RPG Maker MV browser runtime plus archived GMUD-simple Android materials.
- `doc/xtulnx-gmudsimple-04cabd28a77e/README`: “Gmud (simple) 英雄坛说 / 基于‘文曲星’英雄坛说PC版本移植。”

### Fit

Interesting as an archive/reference, but too large, asset-heavy, and unlicensed for direct use.

## Recommendation for Trillionnium

### Product direction

Build a **hybrid game shell**:

- **Outer UI / playfield:** keep the open-source browser tactics direction from `MedievalWar` so `/world` feels like a real game rather than a dashboard.
- **Inner Trillionnium loop:** borrow system concepts from `gmud`, `RMXP-Hero`, and `yxts-llm`:
  - attributes: 臂力 / 身法 / 悟性 / 根骨
  - skills: 基本内功 / 基本拳脚 / 基本剑法 / 基本轻功 / 读书识字
  - sect / mentor / title progression
  - NPC social society rather than single linear questline
  - survival/time/resource pressure
  - text battle log flavor
  - task types: 送信 / 找物 / 杀恶人 / 送镖 / 武林大会-like seasonal challenge
- **Real-world bridge:** OpenClawStreetMap provides terrain, events, route objectives, and local POIs beneath the Trillionnium shell.

### Legal rule

For implementation, do **not** copy original Hero Tan Shuo text, tiles, sprites, music, or full tables into Trillionnium unless rights are explicit.

Safe implementation posture:

1. Use `mogita/gmud` / `RMXP-Hero` / `yxts-llm` to understand mechanics.
2. Recreate Trillionnium-native equivalents with new names/content/assets.
3. Keep `MedievalWar` MIT tactics structure as the visible browser/game UI base unless a clean permissive Hero Tan Shuo browser base appears.
4. If we want to use `yxts-llm` code, first ask the author to add a standard `LICENSE` file and asset provenance notes.

## Immediate next implementation target

Do not pivot the `/world` shell into a direct 白金英雄坛说 clone. Instead, layer a **Trillionnium RPG panel** onto the current tactics shell:

- `data-trillionnium-reference="gmud_mit_mechanics_reference"`
- player attributes panel
- skill/training panel
- sect/NPC/task panel
- turn log with Wuxia text flavor
- OpenClawStreetMap objective source chip

That gives the user the 白金英雄坛说 feel while staying legally safer and browser-native.

# Trillionnium World 统一开发文档 v1

**项目**：Trillionnium World
**目标仓库**：<https://github.com/TrillionniumFoundation/Trillionnium>
**当前孵化实现**：`/home/qian/.openclaw/workspace/CEX`
**合并来源**：Dropbox `rust_geo_mmo_development_doc_v0_4_execution_ready.md`、CEX 已落地代码、CEX 现有 Trillionnium/World 文档与运行证据
**当前代码基线**：CEX `111af46 feat: gate rust-owned map focus ui fragments`
**文档状态**：统一执行版；用于后续拆票、验收、迁移到 Trillionnium 主仓、Alpha/公开测试准入评审
**修订日期**：2026-05-12

---

## 0. 一句话结论

Trillionnium World 的正确路线不是从零重写一个“概念 MMO”，而是以 CEX 中已经跑通并有证据的 Rust 世界状态、Ledger 结算、真实地图投影、Matrix/Web 双入口、Rust-owned UI fragment、运行门禁与 E2E 证据为当前事实基线，再吸收 Dropbox v0.4 文档里的 MMO 阶段化、地图合规、证据包、ADR、Bevy/移动端、Cell/AOI/QUIC/H3 等长线设计，形成一条可执行迁移路线：

```text
CEX 已验证垂直切片
  -> Trillionnium World 独立模块边界
  -> Trillionnium 主仓文档/接口/证据迁移
  -> Rust-owned Web/Matrix 全量安全等价
  -> 独立 World Server / API / 数据 schema
  -> 合规真实地图包与生产地图管线
  -> Native/Bevy 移动客户端实验门
  -> Cell/AOI/实时多人 MMO 硬化
  -> Alpha / Beta / 商业化准入
```

当前最重要的原则：**已经落地并跑绿的 CEX 代码是真实进度，Dropbox v0.4 是目标工程制度和长线 MMO 蓝图；本文档把二者合并，而不是用愿景覆盖事实。**

---

## 1. 当前事实基线

### 1.1 已落地代码与运行状态

CEX 当前已经不是纯文档阶段。可作为 Trillionnium World 的孵化实现，具备以下已验证能力：

- Rust workspace 多服务架构：`identity-service`、`ledger-service`、`gateway-service`、`execution-service`、`audit-service`、`capability-service`、`consumer-entry-api`、`matrix-entry-adapter`、Matrix bot poller/relay。
- `consumer-entry-api` 已承载 Trillionnium League / World 的核心产品垂直切片。
- `WorldState` 已从 `LeagueState` 中拆出第一层世界边界，并保留旧 JSON/持久化兼容。
- World 数据已覆盖：地图节点、玩家位置、角色、战术会话、公司、商店、listing、purchase、work order、delivery/accept/reject/reopen/cancel、faction、economy event、route runner、tactics、task/NPC loop。
- Ledger-backed commerce 已具备 reserve / seller settlement / buyer consume / refund / seller chargeback / recovery / fail-closed 的回归测试与 runtime gate。
- `/world`、`/app`、Matrix commands、web session/CSRF、本地 production runtime、SQL snapshot、health/metrics、browser/web/first-human E2E 均已存在。
- 当前 Rust-owned UI 工作已推进到 `/world` shell、route UI、route-runner UI、live-event/task-route cards、map popup、map support cards、map focus/action rail。

### 1.2 最新已验证证据

最新本地证据来自 CEX 本地 production runtime：

- Head：`111af46 feat: gate rust-owned map focus ui fragments`
- SQL snapshot：`run/linux-runtime/entry-config/league-state-snapshot.sql`
- 最新 SQL state hash：`sha256:56a08eb92d1f50c1e9402801193b3d24ca52b1efe7d5a96daac323677c808569`
- Web E2E：`run/league-web/web-e2e-summary-1778568027.json`，`ok=true`
- Browser E2E：`run/league-browser/browser-e2e-summary-1778568045-1918845.json`，`ok=true`，request failure gate green，unclassified=0
- First-human E2E：`run/first-human-session/browser-e2e-summary-1778568310-1924693.json`，`ok=true`，request/page errors=0
- Full Rust tests before latest UI slice：`cargo test -p consumer-entry-api -- --nocapture --test-threads=1`，136 passed
- Clippy：`cargo clippy -p consumer-entry-api -- -D warnings` passed
- Local production status：8 个 HTTP service + worker healthy
- 最新 honest assessment 基线：technical 约 `9.8/10`，first internal beta `8.5/10`，commercial release `7.0/10`

重要限制：first-beta 9+ 与 commercial 8+ 仍需要真实 cohort / commercial drill / multi-node 或 live-traffic 证据，不能靠 localhost 继续刷分。

---

## 2. 产品定义

### 2.1 Trillionnium World 是什么

Trillionnium World 是一个基于真实地理骨架的开放世界 AI/Agent 江湖 MMO。真实地图提供道路、区域、POI、地理分区与位置语义；游戏系统在其上生成任务、NPC、门派/阵营、资产、公司、商店、路线、事件、战斗、协作和经济结算。

产品由五个互相连接的品牌域组成：

| 域 | 当前 CEX 状态 | 长线形态 |
| --- | --- | --- |
| Trillionnium World | `/world` 已可玩，Rust WorldState 为权威 | 独立 World server、真实地图区域、多人 Cell/AOI |
| Trillionnium League | 已有比赛、提交、评分、奖励、review hold、route runner | 世界内赛事、赛季、任务联盟、竞技经济 |
| Trillionnium Craft | 资产/公司/商店/listing/work order 已有 | 建造、制造、公司经营、Agent 产业链 |
| Trillionnium Ledger | CEX ledger 已接入 World commerce | Trillionnium 链/账本/审计/结算一体化 |
| Trillionnium Agents | Agent residents、party、handoff 已有 | 可雇佣 Agent、NPC 社会、自动任务与队伍 |

### 2.2 MVP 定义

MVP 不是“完整开放世界上线”，而是证明下面八件事：

1. Rust 权威世界状态可以稳定承载移动、任务、经济、战斗和 UI projection。
2. 客户端只提交意图，服务端决定结果。
3. 真实地图作为游戏骨架是合规、可署名、可缓存、可降级的。
4. 玩家能完成 10 分钟内的新手闭环：进入世界、移动、练功/交互、战斗/任务、获得奖励、进入下一条路线。
5. 经济系统不会复制货币/物品，不会在 ledger 失败时推进世界状态。
6. 浏览器/Web/Matrix 只是输入、focus、event bridge，不是世界状态来源。
7. 每个 Gate 都有可复跑证据，不靠主观“看起来可以”。
8. 运维有 health/metrics、E2E、runbook、备份恢复和失败动作。

---

## 3. 不可妥协架构原则

这些规则同时来自 Dropbox v0.4 和 CEX 已经落地的技术路线：

1. **Rust source of truth**：`WorldState`、Rust projection、Rust command handler 是权威；浏览器、Matrix、未来 Bevy 客户端只提交 intent。
2. **UI Rust-owned**：`/world` 的玩家可见状态、action rail、route card、task card、popup、support card 逐步由 Rust server-rendered fragment / projection 生成；JS 只做事件绑定、focus 选择、可视化 fallback。
3. **Ledger fail-closed**：经济结算、refund、chargeback、review release、completion reward 都必须以 ledger 成功或可恢复状态为推进条件。
4. **地图逻辑和渲染分离**：真实地图/OSM 是 muted context layer；游戏语义和位置 mutation 由 Rust world graph 决定。
5. **OSM 合规先行**：当前只允许 fixture 模式。live Overpass / Geofabrik ingestion、生产公网 OSM tile 直接使用、MapLibre promotion 都必须另有合规/缓存/回滚证据。
6. **不能复制 Hero Tan / 白金英雄坛说**：只能作为公开参考做 mechanics/layout study；不得复制代码、文本、NPC、任务表、资产、数据。
7. **不能复制 MedievalWar/Phaser 资产**：当前只借鉴 permissive tactics patterns；不 vendoring 源码/资产，除非单独法律/产品决策。
8. **分数不能靠本地假证据抬升**：first-beta、commercial、technical 9.9+ 必须要真实 cohort、commercial drill、multi-node/live traffic。
9. **证据包优先**：没有证据包，不算 Gate 通过。
10. **兼容迁移**：CEX 已跑通路径先保留，再抽象/迁移；不做 full clone-then-replace。

---

## 4. 当前 CEX 架构

### 4.1 Runtime 拓扑

```text
Telegram / Matrix / Web
  -> matrix-bot-poller / matrix-bot-relay
  -> matrix-entry-adapter
  -> consumer-entry-api
       - League / World API
       - Web shell `/league`, `/app`, `/world`
       - Rust WorldState / projection / command handlers
       - health / metrics / production readiness gates
  -> ledger-service
  -> gateway-service / execution-service / audit-service / capability-service / identity-service
  -> local-production runtime + SQL snapshot / normalized repository gates
```

### 4.2 关键文件归属

| 领域 | CEX 文件 |
| --- | --- |
| World state / domain structs | `services/consumer-entry-api/src/lib.rs` |
| Default fixtures / repository | `services/consumer-entry-api/src/league_repository.rs` |
| OSM provider seam | `services/consumer-entry-api/src/openstreetmap_geodata.rs` |
| Map projection | `services/consumer-entry-api/src/world_map_projection.rs` |
| Route projection | `services/consumer-entry-api/src/world_route_projection.rs`、`world_routes.rs` |
| World web shell | `services/consumer-entry-api/src/world_web_shell.rs` |
| Shared map adapter/runtime JS/CSS | `services/consumer-entry-api/src/real_world_map_shell.rs` |
| Tactics/mechanics | `services/consumer-entry-api/src/world_tactics.rs` |
| Runtime gates | `services/consumer-entry-api/src/health_metrics.rs`、`world_map_optimization.rs` |
| Tests | `services/consumer-entry-api/src/tests.rs` |
| Browser E2E | `scripts/playwright/trillionnium-browser-e2e.mjs` |
| Web E2E | `scripts/check-trillionnium-league-web-e2e.sh` |
| First-human E2E | `scripts/check-trillionnium-first-human-session.sh` |
| Production readiness/signoff | `scripts/check-production-readiness.sh`、`scripts/check-production-signoff.sh` |

### 4.3 当前已落地 World contract 家族

| Contract | 状态 |
| --- | --- |
| `trillionnium_world_rust_owned_ui_shell_v1` | `/world` first shell / keypad / core UI Rust-owned |
| `trillionnium_text_adventure_keypad_movement_v1` | 方向键探索循环已可玩 |
| `trillionnium_world_play_first_exploration_loop_v1` | 首屏四问/探索 loop 已进 E2E |
| `trillionnium_world_objective_travel_v1` | Rust graph objective travel 已落地 |
| `trillionnium_world_skill_practice_loop_v1` | mentor skill practice 已落地 |
| `trillionnium_world_rust_route_ui_fragments_v1` | route task graph / route-flow action Rust-owned |
| `trillionnium_world_rust_route_runner_ui_fragments_v1` | route-runner handoff cards/actions Rust-owned |
| `trillionnium_world_rust_live_task_ui_fragments_v1` | live-event / avatar-task-route cards Rust-owned |
| `trillionnium_world_rust_map_popup_ui_fragments_v1` | marker/runner/avatar popups Rust-owned |
| `trillionnium_world_rust_map_support_ui_fragments_v1` | density/stream/region/tile/POI/prefetch/cluster cards Rust-owned |
| `trillionnium_world_rust_map_focus_ui_fragments_v1` | focus summary/detail/action rail Rust-owned |
| `openstreetmap_geodata_v1` | fixture OSM geodata source-of-truth |
| `openstreetmap_provider_readiness_v1` | fixture-ready/live-fail-closed gate |
| `openstreetmap_geodata_freshness_v1` | fixture freshness / live staleness gate |
| `openstreetmap_attribution_presence_v1` | OSM/ODbL attribution presence gate |
| `trillionnium_browser_request_failure_gate_v1` | Browser E2E request-failure hard gate |
| `trillionnium_route_runner_handoff_v1` | handoff/feed/world loop gate |
| `trillionnium_world_public_commercial_product_v1` | commercial readiness product gate |

---

## 5. Dropbox v0.4 与 CEX 当前实现的合并关系

Dropbox v0.4 是完整 MMO 工程蓝图，很多内容是未来阶段，不应误标为当前已实现。合并后的解释如下：

| Dropbox v0.4 主题 | 当前 CEX 状态 | 本文档处理方式 |
| --- | --- | --- |
| Rust 全栈 MMO | 已有 Rust CEX 多服务 + World vertical slice | 作为当前孵化实现 |
| Bevy Android client | 未落地；当前是 Web/Matrix shell | 作为 M5 实验 Gate，不阻塞当前 Web Rust-owned 收口 |
| QUIC + WebSocket fallback | 当前 World 不是实时 socket MMO | 作为 standalone server / Cell 阶段 ADR |
| H3 / region-local / AOI | 当前有 map viewport/tile/region projection，不是 full H3 Cell server | 作为 M4/M5 迁移目标 |
| map_pack 签名 | 当前是 fixture OSM + attribution gate | 纳入 M1.5/M4 地图包 Gate |
| economy idempotency/recovery | CEX 已有大量 ledger/world commerce 回归 | 作为已落地基线继续强化 |
| acceptance evidence package | CEX `run/*` 已有证据；Dropbox `acceptance/*` 更规范 | 合并为统一证据制度 |
| chat/UGC 风控 | CEX 当前重点不是公开聊天 | 外部测试前硬 Gate |
| privacy/data retention | CEX 有审计/运行基础，需文档化 | 纳入 Alpha 准入 |
| public test readiness | CEX 有 beta/commercial gate，但真实 cohort/drill 未补 | 保留为 blocker |

---

## 6. 目标系统架构

### 6.1 当前阶段：CEX Incubation Architecture

```text
CEX consumer-entry-api
  ├─ WorldState / LeagueState / Ledger integration
  ├─ /world web shell
  ├─ /app mobile-style shell
  ├─ Matrix command projection
  ├─ Rust-owned UI fragments
  ├─ OSM fixture provider
  ├─ Health / metrics / playability gates
  └─ SQL snapshot / normalized repository path
```

该阶段目标是把 World 的产品闭环、Rust source-of-truth、经济安全、真实地图合规姿态和 UI 所有权做实。

### 6.2 中期阶段：Trillionnium World Server

从 CEX 中抽出独立模块，但保留接口兼容：

```text
trillionnium-world-server
  ├─ world-domain
  ├─ world-projection
  ├─ world-command
  ├─ world-ledger-adapter
  ├─ world-map-provider
  ├─ world-ui-fragments
  ├─ world-api
  └─ world-evidence-gates
```

抽离原则：

- 先复制 contract / tests / evidence gate，再搬 implementation。
- 外部行为、JSON contract、E2E 先不变。
- CEX 仍可作为 adapter/host，直到 Trillionnium 主仓 runtime 可替代。

### 6.3 长期阶段：Standalone MMO Architecture

```text
Native/Web Client
  -> Gateway / Transport adapter
  -> Router
  -> Cell Server(s)
  -> WorldState shards / AOI / route systems
  -> Economy / Ledger / Audit
  -> Map service / map_pack CDN / attribution service
  -> Ops / metrics / replay / recovery
  -> Trillionnium chain settlement bridge
```

Dropbox v0.4 的 Bevy、QUIC、H3、Cell、map_pack、AOI、mobile release 进入这一阶段，而不是覆盖当前 CEX Web-first 已验证路线。

---

## 7. 阶段路线图

### S0 — 保住 CEX 事实基线（当前）

**目标**：不破坏当前 green runtime。

验收：

- `git status` 仅允许明确 untracked 临时目录。
- `cargo fmt --all -- --check`
- `cargo check -p consumer-entry-api`
- `cargo test -p consumer-entry-api -- --nocapture --test-threads=1`
- `cargo clippy -p consumer-entry-api -- -D warnings`
- `CEX_ENV_FILE=run/local-production/.env scripts/runtime-manager-linux.sh restart && status`
- Web / Browser / First-human E2E green。

### S1 — 完成 `/world` Rust-owned UI 全量安全等价

已完成：shell、route、runner、live/task、popup、support、focus/action rail。

剩余：

- 审计并迁移仍由浏览器拼接的次级 dashboard / commerce / timeline / support 面板。
- 将 heavy secondary panels 改为 Rust projection + lazy hydration。
- 明确 `/app` 的 shared fallback 边界，避免 `/world` 回退成 JS-owned。

验收：

- `/world` 玩家可见核心 UI 均有 `data-render-owner="rust_world_ui_renderer"` 或明确 fallback 标记。
- Browser JS 无权威状态 mutation，只做 input/focus/event bridge。
- E2E 覆盖 post-move、delta、304、weak-network cache、request-failure gate。

### S2 — World domain 模块边界

目标：把 `WorldState` 从 CEX 的 League/CEX 混合语境中进一步模块化。

任务：

- 拆分 `WorldTopologyState`、`WorldPresenceState`、`WorldCommerceState`、`WorldContractState`、`WorldReputationState`、`WorldRoutingState`、`WorldEventState`。
- 让 systems own mutation：navigation、task、combat、commerce、contract、reputation、route projection。
- 把 indexes 从 helper 推进到显式 derived read model。
- 保留 serde compatibility 和 SQL snapshot/normalized repository gates。

### S3 — Trillionnium 主仓文档/API 对齐

目标：让 <https://github.com/TrillionniumFoundation/Trillionnium> 有一份能承接 CEX World 的开发文档、接口清单和迁移计划。

任务：

- 将本文档同步到 Trillionnium 主仓 `docs/development/`。
- 新建 World API contract index。
- 新建 World evidence/runbook index。
- 标注 CEX 当前为 incubator/source-of-evidence。
- 不在主仓宣称 CEX 中尚未迁移的代码已存在。

### S4 — 地图合规与 map_pack Gate

目标：从 fixture OSM 走向可测试真实地图包，但不直接开 live ingestion。

任务：

- ADR：地图数据源、授权、缓存、离线、署名、敏感 POI、地理围栏、下架流程。
- `map_pack_manifest_signed.json`：canonical manifest + Ed25519 signature + key_id + key rotation + revocation。
- Attribution screenshots：Web/Native/Matrix 关键界面都要有。
- 敏感 POI filter report。
- 不满足时继续 fixture-only。

### S5 — Native/Bevy Mobile Gate（可选实验）

目标：验证 Bevy Android 是否适合 Trillionnium World，不把它当默认前提。

Go 条件：

- 中端 Android 30 FPS。
- 中文显示/输入、前后台、弱网、资源包、崩溃诊断通过。
- 客户端仅提交 intent；Rust server 仍权威。
- 能复用 Rust projection/API，不复制 Web 逻辑。

No-Go：继续 Web/PWA 或评估 Godot/Unity/轻量客户端。

### S6 — First Beta / Commercial Evidence

当前 blocker：

- `TRILLIONNIUM_FIRST_BETA_COHORT_EVIDENCE_PATH` 真实 5-10 人 cohort。
- `TRILLIONNIUM_COMMERCIAL_LAUNCH_DRILL_EVIDENCE_PATH` payment/refund/support/legal/operator/traffic drill。
- `TRILLIONNIUM_MULTI_NODE_LATENCY_EVIDENCE_PATH` 或 live traffic latency。

没有这些证据，不提升 first-beta/commercial/technical 最高档评分。

### S7 — Chain/Ledger/Settlement Integration

目标：将 CEX ledger/world economy 与 Trillionnium 主链/结算能力对齐。

任务：

- World economy command 与 chain task/settlement interface 的桥接 ADR。
- 幂等键范围：`(command_kind, actor_scope, idempotency_key)` 或服务端签发。
- Review hold、refund、chargeback、recovery worker 进入链上/链下一致性模型。
- 保留 CEX ledger fail-closed 语义。

---

## 8. 统一验收证据制度

CEX 当前使用 `run/*`，Dropbox v0.4 使用 `acceptance/*`。统一后：

```text
acceptance/
  S0_cex_baseline/
    git_head.txt
    cargo_test.log
    clippy.log
    runtime_status.json
    sql_snapshot.json
    web_e2e_summary.json
    browser_e2e_summary.json
    first_human_e2e_summary.json
    owner_signoff.md
  S1_rust_owned_world_ui/
    fragment_contract_matrix.md
    browser_bridge_audit.md
    e2e_post_move_delta_304_report.md
    owner_signoff.md
  S4_map_pack_gate/
    map_data_decision_record.md
    license_review.md
    attribution_screenshots/
    sensitive_poi_filter_report.md
    geofence_test_report.md
    map_pack_manifest_signed.json
    owner_signoff.md
  S6_beta_commercial/
    first_beta_cohort_evidence.json
    commercial_launch_drill_evidence.json
    multi_node_latency_report.json
    owner_signoff.md
```

最低字段：

| 字段 | 要求 |
| --- | --- |
| `gate_name` | S0/S1/S2/S4/S6 等 |
| `git_head` | commit hash |
| `build_id` | Rust binary / frontend / config version |
| `protocol_contracts` | contract version list |
| `test_commands` | 可复跑命令 |
| `evidence_paths` | `run/*` 或 `acceptance/*` 路径 |
| `known_failures` | 已知失败与分类 |
| `rollback_plan` | 回滚/停损动作 |
| `owner` / `reviewer` | 责任人与验收人 |
| `signoff_time` | 签收时间 |

规则：没有证据包，不进入下一阶段；豁免项必须有过期时间。

---

## 9. Go / No-Go 闸门

| Gate | 当前状态 | Go 条件 | No-Go 动作 |
| --- | --- | --- | --- |
| CEX green baseline | Green | S0 命令全绿 | 停止迁移，先修 baseline |
| Rust-owned `/world` UI | In progress / strong green slices | 核心 `/world` 无 JS-owned state/UI | 继续 fragment migration，不做新功能膨胀 |
| Ledger economy | Green but must preserve | 所有 commerce/recovery fail-closed | 关闭市场/交易/GM 发放 |
| Map provider | Fixture green | 合规数据源、缓存、署名、下架、POI filter | 继续 fixture-only，不开 live ingestion |
| Map renderer | Leaflet live, MapLibre shadow only | fresh signoff + rollback drill | 不提升 MapLibre canary |
| First beta | Blocked on real cohort | 真实 5-10 人 evidence | 不宣称 beta 9+ |
| Commercial | Blocked on drill evidence | payment/refund/support/legal/operator/traffic drill | 不宣称 commercial 8+ |
| Native/Bevy | Not started | mobile gate 通过 | 不做大规模 Bevy 内容开发 |
| MMO Cell/AOI | Future | multi-node/load/chaos 证据 | 不宣称实时 MMO readiness |

---

## 10. UI / 客户端策略

### 10.1 当前 Web UI 策略

- `/world` 是 game-first shell，不是 dashboard-first。
- Rust 生成 player-visible fragments。
- Browser JS 只做：
  - click/keyboard handler
  - focus selection
  - fetch map viewport/delta
  - bind map marker/popups
  - apply Rust-projected HTML
  - fallback for `/app` or degraded payloads
- 每次 UI 迁移都要加：contract constant、bootstrap payload、viewport/delta payload、DOM ownership attributes、unit test、Web/Browser E2E assertion。

### 10.2 Future Native/Bevy 策略

Bevy 是实验 Gate，不是当前事实。若启动：

- 不复刻 Web 的 JS 逻辑。
- 直接消费 Rust World API/projection。
- Native client 只提交 intent。
- 地图包和配置包必须签名。
- Android release chain 要单独 ADR。

---

## 11. 地图与合规策略

当前：

- Active engine：`leaflet_openstreetmap_v1`
- Renderer adapter：`leaflet_renderer_adapter_v1`
- Runtime handle：`mapRuntime`
- Future candidate：`maplibre_gl_v1`
- Status：`shadow_only_not_user_facing`，canary=0
- OSM provider：fixture only
- Live Overpass / Geofabrik：disabled
- Public production OSM tile：forbidden until cache/vendor/self-host policy

未来真实地图 Gate 必须覆盖：

- 数据授权矩阵。
- ODbL attribution 和 derived database obligation。
- 缓存/离线授权。
- 敏感 POI filter。
- 地理围栏与地区政策。
- map_pack signing / revocation / rollback。
- Attribution screenshots。

---

## 12. 经济、结算与安全

CEX 已落地的强原则必须继承：

- 经济命令必须 idempotent。
- Ledger DB unavailable 时 fail-closed。
- Buyer reserve / consume / refund 与 seller settlement / chargeback 必须一致。
- Review hold 不允许刷 progression / standing。
- Completion reward / League reward 必须 ledger release 后推进。
- Failed chargeback/refund 必须可恢复，不得把世界状态误标为完成。
- In-memory fallback 只能用于明确 local-dev；production/local-production 必须 fail-fast。

未来增强：

- Command ledger table 明确 `pending/applying/applied/failed/recoverable/dead_letter`。
- Recovery worker 有 owner、lock、retry、dead letter。
- 多资产锁顺序固定，deadlock retry 有测试。
- GM 发放、价格、商店、掉落、配置包都需要审批和审计。

---

## 13. 测试与运行门禁

### 13.1 当前最小开发门禁

```bash
cargo fmt --all -- --check
cargo check -p consumer-entry-api
cargo test -p consumer-entry-api -- --nocapture --test-threads=1
cargo clippy -p consumer-entry-api -- -D warnings
node --check scripts/playwright/trillionnium-browser-e2e.mjs
bash -n scripts/check-trillionnium-league-web-e2e.sh \
  scripts/check-trillionnium-league-browser-e2e.sh \
  scripts/check-trillionnium-first-human-session.sh
git diff --check
```

### 13.2 Runtime/E2E 门禁

```bash
export CEX_ENV_FILE=run/local-production/.env
scripts/runtime-manager-linux.sh restart
scripts/runtime-manager-linux.sh status
scripts/check-trillionnium-league-sql-snapshot.sh
scripts/check-trillionnium-league-web-e2e.sh
scripts/check-trillionnium-league-browser-e2e.sh
scripts/check-trillionnium-first-human-session.sh
```

### 13.3 Signoff / maturity 门禁

根据改动范围追加：

- `scripts/check-trillionnium-ui-audit.sh`
- `scripts/check-trillionnium-playability-scorecard.sh`
- `scripts/check-trillionnium-world-real-user-beta.sh`
- `scripts/check-trillionnium-world-public-commercial-product.sh`
- `scripts/check-production-readiness.sh`
- `scripts/check-production-signoff.sh`
- Health/metrics load soak。

---

## 14. Backlog 映射

### Epic A — Rust-owned `/world` UI 完结

- A1 审计残留 `innerHTML` / browser card builders。
- A2 迁移 commerce panels。
- A3 迁移 timeline / secondary dashboard。
- A4 lazy hydrate heavy payload。
- A5 Browser E2E post-delta / weak-network / 304 coverage。

### Epic B — WorldState 模块化

- B1 分 sub-state。
- B2 系统化 command handlers。
- B3 indexes/read model 标准化。
- B4 JSON/SQL compatibility gates。

### Epic C — Trillionnium 主仓承接

- C1 同步本文档。
- C2 建 World API index。
- C3 建 World evidence index。
- C4 写 CEX incubator handoff ADR。

### Epic D — Map compliance / map_pack

- D1 地图数据 ADR。
- D2 map_pack manifest/signature。
- D3 attribution screenshots。
- D4 sensitive POI / geofence / takedown runbook。

### Epic E — Real beta evidence

- E1 5-10 人 cohort evidence。
- E2 commercial launch drill evidence。
- E3 multi-node/live latency evidence。
- E4 update human-playability assessment。

### Epic F — Native/mobile experiment

- F1 Bevy/Godot/PWA comparison ADR。
- F2 Android device matrix。
- F3 intent protocol prototype。
- F4 package/resource/signature gate。

---

## 15. 当前下一步

如果下一条指令是“继续”，优先做：

1. 继续审计 `/world` 残留 browser-built secondary/dashboard/commerce/timeline UI。
2. 把剩余面板改成 Rust-owned fragment + lazy hydration。
3. 不新增 live OSM、不提升 MapLibre、不引入 Hero Tan copy workflow。
4. 若目标是评分提升，而不是 UI/player-value，则先补真实 evidence path：
   - `TRILLIONNIUM_MULTI_NODE_LATENCY_EVIDENCE_PATH`
   - `TRILLIONNIUM_FIRST_BETA_COHORT_EVIDENCE_PATH`
   - `TRILLIONNIUM_COMMERCIAL_LAUNCH_DRILL_EVIDENCE_PATH`
5. 同步本文档到 Trillionnium 主仓 `docs/development/`，并在主仓 README/docs index 中标注 CEX incubator source。

---

## 16. Source Documents

本文档合并自：

- Dropbox：`/home/qian/Dropbox/rust_geo_mmo_development_doc_v0_4_execution_ready.md`
- CEX：`docs/trillionnium-world-development-progress-tree-v1.md`
- CEX：`docs/trillionnium-world-ecs-refactor-v1.md`
- CEX：`docs/trillionnium-world-map-optimization-execution-v1.md`
- CEX：`docs/trillionnium-real-world-map-engine-evaluation-v1.md`
- CEX：`docs/trillionnium-open-source-stack-reference-v1.md`
- CEX：`docs/trillionnium-open-source-hero-tan-shuo-base-selection-v1.md`
- CEX：`docs/trillionnium-open-source-tactics-base-selection-v1.md`
- CEX：`docs/trillionnium-league-implementation-plan-v1.md`
- CEX runtime evidence under `run/*`

---

## 17. 最终判断

Trillionnium World 当前已经有一个强于普通 MVP 文档的基础：CEX 里存在可运行、可测试、可审计、可恢复的 Rust-first 世界垂直切片。接下来不要推倒重来，也不要只写愿景。正确路径是：

1. 以 CEX green baseline 为事实。
2. 把 `/world` UI ownership 收口到 Rust。
3. 把 World domain 从 CEX 中模块化。
4. 把文档、API、证据迁移到 Trillionnium 主仓。
5. 再进入真实地图包、移动客户端、Cell/AOI、Alpha/商业化证据阶段。

这条路线同时保留 Dropbox v0.4 的工程纪律，也尊重 CEX 已经落地的代码现实。

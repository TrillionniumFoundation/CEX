# Trillionnium World ECS-style Refactor Outline v1

## Goal

把 `Trillionnium World` 从当前 `consumer-entry-api` 里的**单体 world state 聚合**，逐步整理成更接近 Bevy/ECS 思想的结构：

- **实体（entity）清晰**
- **组件/属性（component-like data）分层**
- **系统（system）按行为拆分**
- **读模型/投影（projection）与写模型/状态变更分离**
- **索引（indexes）显式化**

注意：这不是说要把 CEX 改写成 Bevy，也不是要把 repo 迁到真正的 ECS runtime。

这里借的是 **ECS 的建模方法**，不是它的渲染器或运行时。

---

## Current Repo Reality

当前 `services/consumer-entry-api/src/lib.rs` 里的 `LeagueState` 同时装着：

- League PvP / raid / reward / inventory / progression state
- World spatial state
- World economy state
- World contract/workflow state
- World reputation/social state
- World event log

其中 world 部分当前大致长这样：

- keyed maps:
  - `world_zones`
  - `world_locations`
  - `world_entities`
  - `world_map_nodes`
  - `world_player_positions`
  - `world_factions`
- append/list-oriented collections:
  - `world_assets`
  - `world_asset_upgrades`
  - `world_companies`
  - `world_shops`
  - `world_listings`
  - `world_economy_events`
  - `world_purchases`
  - `world_work_orders`
  - `world_work_deliveries`
  - `world_work_acceptances`
  - `world_work_rejections`
  - `world_work_reopens`
  - `world_work_cancellations`
  - `world_faction_standings`
  - `world_events`
  - `world_contracts`
  - `world_contract_completions`
  - `world_relationships`

这已经足够把产品做起来，但长期会遇到三个明显问题：

1. **同一类对象的访问模式不一致**
   - 有些是 `HashMap`
   - 有些是 `Vec`
   - 查询时常常要全表扫描 + 手工 filter

2. **行为边界不够清楚**
   - map / route / commerce / contract / faction / progression 都堆在一个大状态体里

3. **投影逻辑越来越重**
   - `/world`
   - `/map`
   - `/app`
   - Matrix fallback cards
   都在从同一个大对象里抓不同切片，后续复杂度会继续上升

---

## What “ECS-style” Means Here

这里的 ECS-style，不是字面上的 `Entity + Component + System` 框架照搬，而是 5 条约束：

### 1. Entity-first identity

每个重要 world object 都应该先被当作“有稳定身份的实体”对待。

比如：

- node
- location
- player-presence
- asset
- company
- shop
- listing
- purchase
- work-order
- contract
- faction-standing
- world-event

而不是混在各个 endpoint 的“业务临时对象”里。

### 2. Components are grouped by concern

同一个实体的不同维度属性，不要被迫绑死在一个超胖 struct 里；至少在逻辑上要分层：

- identity
- spatial
- ownership
- commerce
- reputation
- workflow/status
- projection hints

### 3. Systems own mutations

状态变更必须按“系统”归属，而不是让 endpoint handler 到处直接改集合：

- navigation system
- world action system
- commerce system
- work-order lifecycle system
- contract system
- faction/reputation system
- route derivation system
- projection system

### 4. Read models are explicit projections

`/world`、`/map`、`/app`、Matrix cards 都应该是**投影**，而不是原始状态本体。

### 5. Indexes are first-class

如果一个集合经常按某个维度查，就应该有显式 index，而不是不停扫 `Vec`。

---

## Recommended Target Shape

## 1) Split `LeagueState` into `LeagueState` + `WorldState`

第一步不是全 ECS 化，而是先把边界拉清：

```rust
struct LeagueState {
    league: LeagueDomainState,
    world: WorldState,
}
```

或者在过渡阶段：

```rust
struct LeagueState {
    // existing league fields...
    world: WorldState,
}
```

### Why

这样至少先让：

- League systems
- World systems

不再继续互相缠死在一个顶层对象里。

---

## 2) Inside `WorldState`, split by domain system, not by endpoint

建议目标形态：

```rust
struct WorldState {
    topology: WorldTopologyState,
    presence: WorldPresenceState,
    ventures: WorldVentureState,
    commerce: WorldCommerceState,
    contracts: WorldContractState,
    reputation: WorldReputationState,
    events: WorldEventState,
    routing: WorldRoutingState,
    relationships: WorldRelationshipState,
    indexes: WorldIndexes,
}
```

这比“按 `/map` / `/world` / `/app` endpoint 分文件”更稳，因为 endpoint 会变，领域边界更持久。

---

## 3) Suggested sub-states

## `WorldTopologyState`

承接静态/半静态空间骨架：

- zones
- locations
- map nodes
- node exits
- interaction tags
- region/viewport metadata

当前可吸收：

- `world_zones`
- `world_locations`
- `world_map_nodes`

## `WorldPresenceState`

承接“谁现在在哪里”：

- player positions
- resident entities / NPC-like helpers
- future live presence / nearby actor state

当前可吸收：

- `world_entities`
- `world_player_positions`

## `WorldVentureState`

承接“玩家创造出的长期经营对象”：

- assets
- asset upgrades
- companies
- shops

当前可吸收：

- `world_assets`
- `world_asset_upgrades`
- `world_companies`
- `world_shops`

## `WorldCommerceState`

承接交易和履约主线：

- listings
- purchases
- work orders
- deliveries
- acceptances
- rejections
- reopens
- cancellations
- economy events

当前可吸收：

- `world_listings`
- `world_purchases`
- `world_work_orders`
- `world_work_deliveries`
- `world_work_acceptances`
- `world_work_rejections`
- `world_work_reopens`
- `world_work_cancellations`
- `world_economy_events`

## `WorldContractState`

承接现实任务映射进世界的那条线：

- contracts
- contract completions
- future contract-to-route linkage

当前可吸收：

- `world_contracts`
- `world_contract_completions`

## `WorldReputationState`

承接阵营和声望：

- factions
- faction standings
- future rank thresholds / social trust / unlock gates

当前可吸收：

- `world_factions`
- `world_faction_standings`

## `WorldEventState`

承接事件日志：

- world events
- event-to-task linkage
- event-to-node/location linkage
- future feed/timeline summary materializations

当前可吸收：

- `world_events`

## `WorldRoutingState`

承接 route cockpit / next opportunity / action lane 的派生状态。

注意这里更像**缓存/派生层**，不是源事实层。

建议它不要直接成为真相源，而是：

- 存 route indexes / derived graph caches / latest summaries
- 或者完全按需 derive，但有清楚的 module/system 归属

## `WorldRelationshipState`

承接各种“谁和谁是什么关系”的通用关系层：

- player ↔ asset
- player ↔ company
- player ↔ faction
- entity ↔ location
- future ally/rival/mentor/vendor/client links

当前可吸收：

- `world_relationships`

---

## Entity Taxonomy Recommendation

推荐给 world entities 做一层统一 taxonomy，哪怕一开始只是文档约束：

| Entity family | Current examples | Notes |
| --- | --- | --- |
| Spatial anchor | zone / location / map node | 世界空间骨架 |
| Actor | player / resident entity / agent / NPC | 能行动、能被关联 |
| Venture | asset / company / shop | 长期经营对象 |
| Offer / demand | listing / purchase | 交易层对象 |
| Fulfillment | work order / delivery / acceptance / rejection / reopen / cancellation | 履约生命周期 |
| Mission / task mirror | world event / contract / completion | 现实任务映射层 |
| Reputation / affiliation | faction / standing / relationship | 社会结构层 |

这个 taxonomy 的价值在于：

- route engine 更容易决定哪些对象可成为 focus target
- feed 更容易决定哪些对象值得投影
- mobile/web/Matrix 更容易共享统一 card schema

---

## Component-style Data Groups To Standardize

建议未来逐步显式化这些 component-like 分组：

### Identity

- stable id
- object kind/family
- created_at / updated_at
- status

### Spatial

- zone_id
- location_id
- node_id
- x/y
- region_id
- adjacency / exits

### Ownership / participation

- owner_matrix_user_id
- buyer_matrix_user_id
- seller_matrix_user_id
- actor_matrix_user_id

### Commerce

- price_credits
- revenue_score
- quality_score
- ledger linkage

### Workflow / lifecycle

- status
- judge_status
- reserve/refund/consume status
- cex task/invocation linkage

### Reputation / progression

- reputation_score
- rank
- unlock relevance

这样做的好处是：以后即使 struct 还不是纯 ECS，也能避免每个对象都走“随手长字段”的路线。

---

## Index Strategy

这是最值得尽快落地的部分。

当前很多 world data 是 `Vec`，而 projection / mutation 又经常按这些维度查：

- by `matrix_user_id`
- by `location_id`
- by `node_id`
- by `company_id`
- by `shop_id`
- by `listing_id`
- by `purchase_id`
- by `work_order_id`
- by `contract_id`
- by `faction_id`

建议引入显式 `WorldIndexes`：

```rust
struct WorldIndexes {
    assets_by_owner: HashMap<String, Vec<String>>,
    companies_by_owner: HashMap<String, Vec<String>>,
    shops_by_company: HashMap<String, Vec<String>>,
    listings_by_shop: HashMap<String, Vec<String>>,
    purchases_by_buyer: HashMap<String, Vec<String>>,
    work_orders_by_seller: HashMap<String, Vec<String>>,
    events_by_location: HashMap<String, Vec<String>>,
    standings_by_player: HashMap<String, Vec<String>>,
    contracts_by_actor: HashMap<String, Vec<String>>,
}
```

即使一开始只是 rebuild-on-write，也已经能明显降低 handler/projection 里的扫描噪音。

---

## System Boundaries To Introduce

## 1. Navigation System

负责：

- `/map`
- `/go`
- player position mutation
- node adjacency validation
- viewport focus seed

## 2. World Action System

负责：

- `/world action`
- free-form action -> event -> possible venture/contract seeds
- event creation and classification

## 3. Venture System

负责：

- assets
- upgrades
- company creation
- shop bootstrap

## 4. Commerce System

负责：

- listing publication
- purchase creation
- work-order opening
- economy event emission
- ledger reservation / settlement hooks

## 5. Fulfillment System

负责：

- deliver / accept / reject / reopen / cancel
- work-order lifecycle integrity
- buyer reserve/consume/refund transitions

## 6. Contract System

负责：

- `/contract`
- `/complete`
- task linkage
- judge result -> reward/world upgrades/reputation effects

## 7. Reputation System

负责：

- faction standing mutation
- company/player reputation effects
- unlock-relevant derived thresholds

## 8. Route Derivation System

负责：

- route preview
- route task graph
- next opportunity derivation
- world/app/map shared route projection

## 9. Projection System

负责：

- `/world` JSON
- `/map` JSON
- `/app` world hub JSON
- Matrix fallback card payloads
- feed cards / mobile cards / world panels

---

## Migration Order

## Phase A — low-risk boundary cleanup

1. extract `WorldState` from `LeagueState`
2. move world read/write helpers into world-specific modules
3. keep JSON shape stable
4. keep endpoints stable

## Phase B — index cleanup

1. introduce `WorldIndexes`
2. replace high-frequency scans in `/world`, `/map`, `/app` projections
3. centralize “latest by X” helpers

## Phase C — system cleanup

1. isolate navigation / commerce / contract / fulfillment / reputation mutations
2. stop letting endpoint handlers directly touch raw collections everywhere

## Phase D — projection cleanup

1. make route / feed / map / matrix cards consume stable projection helpers
2. reduce duplicated business derivation across web/mobile/Matrix surfaces

## Phase E — optional deeper ECS-style normalization

Only if it proves useful later:

- unify entity registries further
- componentize hot paths more aggressively
- add cached derived read models for feed/route/progression

---

## Immediate Next Slice Recommendation

最值得现在做的，不是“全面重构”，而是下面这 4 步：

1. 从 `LeagueState` 里抽 `WorldState`
2. 给 world collections 补第一版 `WorldIndexes`
3. 把 `world_route_preview_json(...)` / `world_route_task_graph...` 这类派生逻辑收进 `WorldRoutingState` 或独立 route system module
4. 让 `/world`、`/map`、`/app`、Matrix card 全部只吃 projection helpers，不直接自己扫原始 state

这是当前仓库里**最像 Bevy/ECS 思想、但风险最低**的前进方式。

---

## Final Recommendation

**Borrow Bevy’s architecture, not Bevy’s runtime.**

对 Trillionnium World，最重要的不是“换引擎”，而是先把 world state 从：

- 一个能跑但越来越胖的 giant state bag

推进成：

- entity-aware
- concern-split
- system-owned
- index-backed
- projection-driven

的结构。

这样后面无论：

- map renderer 继续 Leaflet
- future engine 切到 MapLibre
- social backend 借 Nakama
- immersive shell 借 Godot

底层 world model 都不会被拖着一起重写。

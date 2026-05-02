# Trillionnium Mobile Shell v1

## Goal

把当前 `GET /app` 的客户端超级入口，收敛成一个**手机端优先、海外首发、英中双语兼容**的四栏底部导航界面：

- 顶部：Global Search / 全局搜索
- 底部从左到右：**Messages / 消息 · World / 世界 · Feed / 动态 · Me / 我**

其中：

- **Messages / 消息** = Telegram / 微信风格的聊天与任务入口
- **World / 世界** = game-first 主入口，承接 Trillionnium World / route / live event / world action
- **Feed / 动态** = 面向海外用户也能理解的信息流：`world events + contracts + completions + adventure rewards + social updates`
- **Me / 我** = 奖励钱包 + 系统设置 + 个人成长 / 道具 / 安全

首期市场定位：**Global-first / 海外市场首发**。界面文案可以英语优先，但关键行动、导航、状态和验收点必须保留中文对照，避免把中文用户挡在外面；同时不要再退回只有中文的国内产品语感。

> 说明：第四栏我建议用 **「我」** 而不是直接写「设置」或「钱包」，因为奖励钱包、成长、账户、设置本质都属于个人中心。这样更符合微信/Telegram 的熟悉感，也给后续 progression / inventory / skin / faction profile 留出空间。

---

## Design Principles

### 1. Map-first, not chat-first

虽然第一栏是消息，但**产品主引擎仍然是地图**。

- 用户可以从消息进入任务
- 但任务、事件、委托、契约、动态，最终都要能回流到地图
- 地图是现实世界镜像、route cockpit、world action、event focus 的主舞台

### 2. Chat is the operating system

消息不是单纯 IM，而是：

- 人与人聊天
- 人与 Agent 聊天
- 系统通知
- 契约 / 冒险委托 / 奖励状态
- 从 feed / map 一键转成对话或 task follow-up

### 3. Feed is event-driven, not influencer-first

动态页借小红书的“流式浏览感”，但内容核心不是种草，而是：

- live event
- contract progress
- completion / reward
- nearby quest-board activity
- workshop/outpost/quest-board updates
- guild/raid/social activity

### 4. Wallet lives inside identity

奖励、余额、契约、设置、安全、等级、技能、工具、皮肤，都放在「我」里统一管理。

---

## Global Layout

```text
┌──────────────────────────────────┐
│  搜索框: 搜人 / 搜地点 / 搜任务 / 搜动态  │
├──────────────────────────────────┤
│                                  │
│        当前 Tab 主内容区         │
│                                  │
├──────────────────────────────────┤
│ 消息 │ 世界 │ 动态 │ 我          │
└──────────────────────────────────┘
```

### Top Search

搜索栏固定在最顶端，所有 tab 共用，但 placeholder 和结果类型跟随 tab 上下文变化。

#### 搜索默认行为

- 全局输入支持：
  - 人 / 联系人 / Agent
  - 地点 / node / quest board / workshop / outpost
  - task / contract / commission / quest board
  - 动态关键词 / event / tag

#### 各 tab 搜索占位文案

- 消息：`搜索队友、群组、Agent、任务对话`
- 世界：`搜索世界地点、工坊、任务、事件`
- 动态：`搜索动态、话题、事件、冒险记录`
- 我：`搜索奖励、契约、道具、设置`

#### 已落地的 UX readiness 合约

`GET /v1/client/app/:matrix_user_id` 暴露 `mobile_shell_contract.contract_version=trillionnium_mobile_shell_ux_v1`，用于把前端/用户体验从“有 UI”推进到可验证的 100% gate：

- 底部四栏是 ARIA tablist/tab/tabpanel 结构，默认选中 `世界`，并同步 `aria-selected`、`aria-controls`、`aria-hidden`、`tabindex` 和 `hidden`。
- 键盘支持 `ArrowRight` / `ArrowLeft` / `ArrowDown` / `ArrowUp` / `Home` / `End` 在四栏间切换。
- 顶部全局搜索会过滤当前 tab，可显示空结果状态，并提供 `清空` 按钮与 `Escape` 清空。
- `app-ux-live-status` / `app-ux-status-pill` 通过 `aria-live=polite` 播报 tab、feed loading、fallback、offline/online 恢复等状态。
- feed hydration 优先走同源 Web session 路由 `/app/web/feed`，避免公开 `/app` 页面在生产模式下直接撞受 ingress-token 保护的 `/v1/client/feed/:matrix_user_id` 并制造 401；feed API 失败时明确回退 embedded snapshot，恢复 online 时刷新 feed。

这些检查已进入 `trillionnium_world_maturity.beta_readiness`、closed beta / real-user beta / public product launch surface 的 maturity gate，并被 `scripts/check-trillionnium-league-web-e2e.sh` 与 browser E2E 覆盖。

---

## Bottom Navigation

## 1) 消息 Tab

### 定位

Telegram / 微信式消息首页，是最日常的操作入口。

### 结构

1. **置顶搜索**
2. **会话列表**
   - 最近联系人
   - Agent 对话
   - 群聊 / guild / team / raid
   - 系统消息
3. **快捷入口横条**
   - 新建任务
   - 发起 world action
   - 联系委托人 / 队友
   - 奖励确认
4. **未处理事项分组**
   - 待回复
   - 待确认
   - 待提交成果
   - 待领取奖励 / 待退回奖励

### 卡片类型

- 普通聊天
- Agent 协作线程
- Contract / Commission 对话线程
- 奖励通知
- 系统提醒

### 关键动作

- 左滑：标记已读 / 置顶 / 静音
- 长按：转任务 / 转 world action / 转契约
- 点开会话：进入聊天页
- 消息内一键跳地图：查看事件发生地点 / 委托所在 location

### 对应现有能力

- 现有 `/social` 模块
- Matrix / Agent / guild / raid / contract / work thread

---

## 2) 世界 Tab（主入口）

### 定位

这是产品的**主舞台**。默认进入地图 tab 时，优先展示：

- 当前 focus 区域
- live event overlay
- route cockpit
- world action console 入口

### 结构

1. **顶部搜索**
2. **全屏地图主画布**
3. **悬浮信息层**
   - camera summary
   - stream lens
   - overlay toggles
4. **底部可上拉操作面板（bottom sheet）**
   - 焦点摘要
   - event brief
   - route next step
   - route actions
   - open linked event / contract / commission

### 默认信息层

- 活跃区域
- 最近热点
- hottest event
- route next opportunity
- linked task status

### 地图页关键动作

- 点 marker / event pulse -> focus node/event
- 上滑 bottom sheet -> 看 route / event detail / actions
- 一键 world action
- 一键 route next opportunity
- 一键打开 linked event timeline
- 一键联系相关人（跳消息）

### Face Duel 的归位

原本 `/app` 里的 Face Duel 不单独占 tab，建议并入地图：

- 当附近存在可 duel 的 Agent / player / arena
- 在 marker 或 bottom sheet 中显示 `Nearby Duel`
- 这样不破坏四栏结构，也更符合地理上下文

### 对应现有能力

- `/world`
- `/world/web/map-viewport`
- live event stream
- route cockpit / stream lens / overlay focus
- world action / quest lifecycle / contract / linked event

---

## 3) 动态 Tab

### 定位

用小红书式浏览体验承接“事件流”，但内容是 **World / League / Adventure / Social** 的融合动态。

### 结构

1. **顶部搜索**
2. **筛选 chips**
   - 推荐
   - 附近
   - 委托
   - 冒险
   - 工坊
   - 任务牌
   - Guild
   - 关注
3. **信息流列表**
4. **悬浮发布按钮（可后续做）**

### Feed 卡片类型

#### A. Live Event Card

- event kind
- event result
- location
- impact
- task link
- CTA: `去地图看` / `继续推进`

#### B. Contract / Commission Progress Card

- contract/commission 标题
- 当前阶段
- 成果/评级状态
- CTA: `打开任务` / `去处理`

#### C. Completion / Reward Card

- 完成了什么
- 奖励/声望/成长
- 下一机会
- CTA: `复盘` / `继续冒险`

#### D. Workshop / Outpost / Quest-board Card

- 工坊/店铺名
- 委托内容
- 最新冒险 / reputation
- CTA: `去地图` / `联系` / `接取`

#### E. Social / Guild Card

- 某个联系人、guild、raid 的动态
- CTA: `去消息`

### 视觉建议

- 卡片高度可变，偏 feed 感
- 可加入图片占位，但 v1 先不用强依赖图片
- 样式参考小红书，不做双列瀑布流，先用**单列大卡**，更适合任务/事件信息密度

### 关键动作

- 点卡片 -> 打开详情
- 点地点 -> 切地图 tab 并 focus
- 点联系人 -> 切消息 tab
- 点处理 -> 切地图或我 tab 的具体面板

### 对应现有能力

动态 tab 的数据建议优先从现有这些源聚合：

- `world_events`
- `live_event_stream`
- `route_preview`
- `route_task_graph`
- economy events
- contracts / completions / quest accepts / commission lifecycle

> 建议新增一个聚合读模型接口：
>
> - `GET /v1/client/feed/:matrix_user_id`
>
> 把地图、契约、成果提交、冒险、奖励、社交动态统一投影成 feed cards。

---

## 4) 我 Tab（钱包 + 设置 + 成长）

### 定位

第四栏统一承接：

- 奖励钱包
- 个人资料
- progression
- inventory / assets
- settings / security

### 结构

1. **用户头部卡**
   - 头像
   - 昵称
   - faction / level / title
2. **奖励钱包主卡**
   - 可用余额
   - 冻结/预留
   - 最近交易
   - 奖励入口
3. **功能宫格**
   - 契约
   - 合同
   - 道具
   - 工坊
   - 背包
   - 技能
   - 工具
   - 皮肤
4. **设置区**
   - 通知
   - 奖励安全
   - 隐私
   - 账号与设备
   - 实验功能

### 钱包优先级

既然用户明确提出“系统设置和奖励钱包”，建议钱包卡在我页最上方，且有两个一级按钮：

- `领/退奖励`
- `奖励记录`

### Progression 的归位

原来的 `/progression` / `/skills` / `/tools` / `/skins` 统一归入「我」。

### 对应现有能力

- `/pay` / wallet
- `/progression`
- `/skills`
- `/tools`
- `/skins`
- `/inventory`
- assets / workshops / contracts / commission history

---

## Recommended Information Architecture Mapping

把现在的 `/app` 五模块，重新折叠成四 tab：

### 当前 `/app`

- World Map
- Face Duel
- Social
- Wallet
- Progression

### 新手机端 IA

- **消息** ← Social + task conversations + system threads
- **地图** ← World Map + Face Duel nearby + route cockpit + world action
- **动态** ← live event stream + route preview + contract/commission/reward feed
- **我** ← Wallet + Progression + settings + inventory/profile

---

## Mobile Detail Layout Suggestions

## 消息页

```text
[搜索]
[快捷入口: 新任务 | World Action | 待支付 | 待提交成果]
[置顶会话]
[最近消息列表]
```

## 地图页

```text
[搜索]
[地图全屏]
[overlay chips / stream lens]
[bottom sheet: focus / route / event brief / action buttons]
[底部 tab]
```

## 动态页

```text
[搜索]
[筛选 chips]
[单列 feed cards]
[悬浮发布按钮（后续）]
```

## 我页

```text
[搜索]
[个人资料头部]
[钱包卡]
[功能宫格]
[设置列表]
```

---

## Interaction Rules Between Tabs

### 消息 -> 地图

- 点 location / event / contract -> 地图 tab focus 到对应对象

### 地图 -> 消息

- 点 contact / patron / teammate / guild -> 消息 tab 打开对应线程

### 动态 -> 地图

- 点 event / workshop / quest-board / completion -> 地图 tab focus

### 动态 -> 消息

- 点人 / 团队 / 委托人 -> 消息 tab

### 我 -> 地图 / 动态

- 点契约 / 合同 / 道具 -> 跳对应地图 focus 或动态详情

---

## Visual Tone

### 风格关键词

- Telegram / 微信的熟悉感
- 小红书的流式浏览感
- 地图是主操作系统
- 赛博现实、轻策略、强冒险/任务导向

### 配色建议

- 主底色：深色 / 石墨蓝 / 夜间地图友好
- 强调色：青蓝（地图/科技）、金色（奖励/成长）、粉橙（动态热点）
- 状态色：
  - success: 绿
  - warning: 黄
  - blocked/review: 橙/红
  - live event: 青色 pulse

### 图标建议

- 消息：chat bubble
- 地图：map pin / compass
- 动态：spark / feed / pulse
- 我：person / reward pouch

---

## First Implementation Slice (Recommended)

### Slice A: 先重构 `/app` 的手机端壳层

把 `/app` 先做成真正的 mobile shell：

- 顶部固定搜索
- 底部固定四 tab
- 中间内容区根据 tab 切换

### Slice B: 先不追求完整页面跳转，先做四个面板

在同一个 `/app` HTML shell 内做四个主 panel：

- `app-tab-messages`
- `app-tab-map`
- `app-tab-feed`
- `app-tab-me`

### Slice C: feed 先吃现有聚合数据

v1 不必先造复杂社交系统，先用已有：

- `live_event_stream`
- `route_preview`
- `route_task_graph`
- world adventure / contract / completion snapshots

拼出动态流。

### Slice D: 把 wallet/progression 迁到「我」页

保留现有数据源，只改客户端 IA。

---

## Suggested Next Backend/UI Tasks

### UI / Shell

1. `/app` 新增 mobile tab shell
2. 新增顶部全局搜索栏
3. 新增四个 tab panel
4. 把当前 map shell 挪进地图 tab
5. 把 wallet/progression 挪进我 tab

### Data Projection

1. 新增 `client_feed_json(...)`
2. 新增 `GET /v1/client/feed/:matrix_user_id`
3. 把 world event / contract / completion / adventure 投影为 feed cards

### Cross-tab Handoff

1. 消息线程 -> map focus
2. feed card -> map focus / message thread / reward panel
3. reward/quest -> commission / contract / map focus

---

## Final Recommendation

如果现在就进入实现，我建议采用这套最终命名：

- **消息**
- **地图**
- **动态**
- **我**

并遵循两个硬规则：

1. **地图仍然是主引擎**，不是被降级成第二个普通 tab
2. **钱包和设置并入“我”**，避免底部 tab 语义太硬、太工具化

这样既符合你提出的四栏结构，也能最大程度复用 CEX 当前已经做好的 `/app`、`/world`、wallet、progression、route/event 体系。

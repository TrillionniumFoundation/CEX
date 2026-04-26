# Matrix 移动端指令规范 v1（Telegram 风格）

> 目标：先不上 Web App，只用 Element 移动端做 CEX 任务入口。

## 1. 使用模型

移动端消息 = 用户输入文本（消息）

- 纯文本消息：直接当作任务 prompt 创建任务。
- 斜杠命令：`/help`、`/task`、`/status`、`/balance`、`/plans` 触发专用行为。
- Trillionnium League 游戏化命令：`/league`、`/world`、`/season`、`/arena`、`/quest`、`/guild`、`/draft`、`/join`、`/battle`、`/submit`、`/profile`、`/rewards`、`/history`、`/rank`、`/loadout`。

默认链路：

```text
Element 消息 -> matrix-bot-poller -> matrix-bot-relay -> matrix-entry-adapter
-> consumer-entry-api -> CEX Gateway -> execution/ledger/identity/audit
-> matrix-entry-adapter 投影回复 -> matrix-bot-relay 回群（或私聊）
```

## 2. 命令表（v1）

### `/help`

- 语义：返回可用命令列表与示例。
- 示例：
  - `/help`
- 回复：内置说明文本（不创建新任务）。

### `/task <内容>`

- 语义：以 `<内容>` 创建新任务。
- 选填参数：
  - `cap=<capability_id>` 或 `--cap <capability_id>`
  - `account=<account_id>` 或 `--account <account_id>`
- 示例：
  - `/task 这是一段要总结的内容`
  - `/task cap=cap.demo.summarize 把这段转成要点`
  - `/task --cap cap.demo.translate --account acct-123 你好，帮我翻译成英文`
- 回复：按现有链路返回 `任务已创建...`。

### `/status <invocation-id>`

- 语义：查询某条任务的状态投影。
- 示例：
  - `/status 8d6e019b-a9dd-4b1c-aa69-dfd3183fd6fd`
- 回复：同状态投影文本。
- 说明：v1 版本仅支持按任务 ID 查询，不支持 `latest`。

### Trillionnium League 游戏化入口

- `/league` / `/tl`：进入 Trillionnium League 首页卡。
- `/world` / `/map`：查看 **Trillionnium World** 开放世界，包括现实镜像城市、Craft 工坊、市场、League 竞技场、Agent 居民、资产和最近事件。
- `/world action <自由行动>`：在开放世界里自由行动，例如开公司、建工坊、招募 Agent、探索市场、把现实任务映射为世界事件。
- `/assets`：查看 **Trillionnium World** 资产和升级次数。
- `/upgrade <asset-id|latest> <升级内容>`：提交资产升级方案，触发 Judge Pipeline v2 并提高资产等级/价值。
- `/companies` / `/company <asset-id|latest> <公司方案>`：查看公司，或把已有资产启动成公司/店铺/初始服务货架。
- `/shops` / `/sell <company-id|latest> <服务/商品>`：查看店铺和货架，或发布带价格/质量分的服务 listing。
- `/contract <委托内容>` / `/bounty <委托内容>`：把现实客户需求登记成 **World Contract**，并通过 CEX 创建真实任务/调用。
- `/complete <contract-id> <交付内容>` / `/deliver ...`：提交 World Contract 交付，触发 Judge Pipeline v2、Ledger 结算、资产升级和声望成长。
- `/craft <建造内容>`：进入 **Trillionnium Craft** 工坊建造，把创作/自动化/服务方案变成可升级资产。
- `/season`：查看当前赛季。
- `/arena`：查看当前赛场。
- `/quest`：查看今日副本。
- `/guild`：查看公会列表；`/guild <guild-id>` 加入公会。
- `/raid`：查看公会团本；`/raid <raid-id> <行动>` 贡献团队进度。
- `/team`：查看默认团本队伍；`/team <raid-id> <role>` 认领 Scout/Builder/Auditor/Closer 等团本职责。
- `/draft <hero...>`：锁定 Agent 英雄阵容，至少 3 个英雄。
- `/join <match-id>`：加入赛场，例如 `/join daily-dungeon-001`。
- `/battle <match-id> <行动>`：在赛场里出招，并创建带 League metadata 的 CEX 执行任务。
- `/rank`：查看排行榜。
- `/loadout`：查看当前 Agent 阵容。
- `/submit <match-id> <提交内容>`：提交赛果，获得评分和奖励；奖励会尝试通过 ledger grant 真结算，并在卡片里显示 `ledger_status`。
- `/profile`：查看玩家档案。
- `/rewards`：查看奖励记录和累计收益。
- `/inventory` / `/items` / `/bag` / `/背包`：查看 League 背包、装备/徽章和战利品 power。
- `/history`：查看战斗/提交历史。

### `/balance` / `/wallet` / `/余额` / `/钱包`

- 语义：查询当前 Matrix 用户映射到的 CEX 钱包。
- 示例：
  - `/balance`
  - `/钱包`
- 回复：Matrix `m.text` + `formatted_body` 钱包卡片，包含可用余额、预留余额、总额、币种、套餐名、账户 ID。
- 说明：由 `matrix-entry-adapter` 调用 `consumer-entry-api /v1/matrix/users/:matrix_user_id/wallet`；consumer 侧通过 Matrix identity registry 解析账户，再用 ledger admin token 只读查询 ledger account。回复中的 `cex_card` 使用 Matrix homeserver 接受的 JSON-safe 字段值，避免把浮点值直接作为自定义 event content 发送到 Synapse。

### `/plans` / `/plan` / `/package` / `/套餐`

- 语义：展示当前本地套餐/计费模型。
- 示例：
  - `/plans`
  - `/套餐`
- 回复：Matrix `m.text` + `formatted_body` 套餐卡片，包含套餐名、计费方式、能力列表、余额查询入口。

### 未识别命令

- 语义：不创建任务，返回错误提示并建议使用 `/help`。

---

## 3. 回复状态映射

当前移动端仅发 `m.text`，并映射 CEX 状态到用户文案：

- `queued` -> `任务已创建，正在排队中`
- `waiting_for_confirmation` -> `这个任务需要确认后才能继续`
- `processing` -> `任务已经开始处理`
- `done` -> `任务已经完成`
- `failed` -> `任务执行失败`
- `refunded` -> `任务已退款`
- `其它` -> `任务已收到`

## 4. 输入限制（v1）

- 只做文本消息；图片/文件目前不接入任务。
- 不做持久化用户偏好（当前每条命令都是独立解析）。
- 不做按钮式交互（确认、重试、撤销）支持。

## 5. 后续 v2（建议）

- 引入 `/approve` / `/cancel` / `/retry`。
- 绑定 `matrix_user_id -> product_user -> account` 映射表。
- 引导式表单（命令模板提示）。
- 把投影扩展为富文本卡片/按钮。

## 6. 运维与回归

- 修改链路：
  1. `services/matrix-entry-adapter` 增加命令解析。
  2. `/status` 查询走本地 `consumer-entry-api` 的任务投影。
  3. 回复格式仍兼容现有 `matrix-bot-relay` 的投影发送能力。
- 回归用例：
  - `/help` 返回帮助
  - `/task` 走任务创建
  - `/task cap=...` 带参数
  - `/status <id>` 返回对应投影
  - `/balance` / `/wallet` 返回钱包卡片
  - `/plans` / `/package` 返回套餐卡片
  - `/league` / `/world` / `/world action ...` / `/assets` / `/upgrade ...` / `/companies` / `/company ...` / `/shops` / `/sell ...` / `/contract ...` / `/complete ...` / `/craft ...` / `/season` / `/arena` / `/quest` / `/guild` / `/raid` / `/team` / `/draft` / `/join` / `/rank` / `/loadout` 返回 Trillionnium League / World / Craft 游戏卡片
  - `/battle <match-id> <行动>` 透传到 CEX task，并携带 League metadata
  - `/submit <match-id> <提交内容>` 返回 `league_submission` 评分/奖励卡，并校验本地真房间链路里 `ledger_status=settled`
  - `/profile` / `/rewards` / `/inventory` / `/history` 返回玩家档案、奖励、背包、历史卡
  - 未知命令返回 `/help` 提示

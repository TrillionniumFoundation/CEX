# Matrix 移动端指令规范 v1（Telegram 风格）

> 目标：先不上 Web App，只用 Element 移动端做 CEX 任务入口。

## 1. 使用模型

移动端消息 = 用户输入文本（消息）

- 纯文本消息：直接当作任务 prompt 创建任务。
- 斜杠命令：`/help`、`/task`、`/status` 触发专用行为。

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
  - 未知命令返回 `/help` 提示

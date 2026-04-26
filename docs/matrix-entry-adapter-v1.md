# Matrix Entry Adapter v1

## Goal

Provide the first bridge layer between Matrix/Element events and the product-facing `consumer-entry-api`.

This service is intentionally **not** a full Matrix SDK client yet.
It is a small event bridge that accepts Matrix-shaped payloads and forwards them into the CEX consumer entry layer.

## New service

- `services/matrix-entry-adapter`

## Why this layer exists

The stack now becomes:

```text
Element / Matrix client
  -> matrix-entry-adapter
  -> consumer-entry-api
  -> CEX gateway-service
  -> identity / ledger / execution / audit
```

This lets us separate three concerns:

- Matrix event transport
- product-facing task projection
- backend execution core

## Endpoints

### Health

`GET /health`

当前还会暴露 `runtime_profile` 与 `profile_validation`，用于显示当前运行模式（`local_dev|beta|production`）以及该 profile 的最小入口安全要求是否满足。配套还会暴露 `rate_limit_store_path` / `rate_limit_store_enabled` / `rate_limit_bucket_count`，用于显示 durable quota store 是否启用以及当前已加载 bucket 数。现在 `/health` 还会额外内嵌 `consumer_entry_session_auth_governance_overview`，把 matrix 侧 signer selection 与 shared issuer registry 的 operator-glance 状态收口成单个块，至少覆盖 `selection_ok`、`registry_loaded`、`revision_present`、`approval_source_valid`、`approval_coverage_valid`、当前 `selection.status/source/detail`，以及聚合 `status` / `valid` / `expected`。

### Metrics

`GET /metrics`

现在会额外暴露 Prometheus 文本格式指标，至少覆盖 event / projection requests、duplicate events、rate limit、ingress auth failure、ignored self events，以及 profile 安全基线相关 gauge（如 `cex_matrix_entry_profile_validation_ok`、`cex_matrix_entry_ingress_protected`、`cex_matrix_entry_consumer_entry_protected`）。针对 matrix -> consumer session auth signer 这一层，也会暴露 `cex_matrix_entry_consumer_entry_session_auth_selection_ok`、`cex_matrix_entry_consumer_entry_session_auth_selected_from_registry`、`cex_matrix_entry_consumer_entry_session_auth_issuer_registry_loaded`、`cex_matrix_entry_consumer_entry_session_auth_issuer_registry_revision_present`，以及新的聚合 gauge `cex_matrix_entry_consumer_entry_session_auth_governance_valid`。durable quota 相关也会通过 `cex_matrix_entry_rate_limit_store_enabled` 与 `cex_matrix_entry_rate_limit_bucket_count` 暴露。

### Accept a Matrix-shaped event

`POST /v1/matrix/events`

Example:

```json
{
  "event_id": "$event-123",
  "event_type": "m.room.message",
  "room_id": "!roomid:local.dev",
  "sender": "@alice:local.dev",
  "text": "Summarize this document",
  "timestamp_ms": 1776468180000
}
```

The adapter will:

1. ignore events sent by the configured bot user itself
2. extract message text from `text` or `content.body`
3. parse mobile commands when message starts with `/` (e.g. `/help`, `/task`, `/status`, `/balance`, `/plans`)
4. call `consumer-entry-api /v1/matrix/messages` for task creation
5. return a projected Matrix reply payload for the caller to send back to the room
6. for `/status <invocation-id>` it calls local task projection path and returns latest state
7. for `/balance` / `/wallet` it calls the consumer wallet projection path and returns a wallet card
8. for `/plans` / `/package` it returns the current package/plan projection

### Read a projected reply for a task

`GET /v1/matrix/tasks/:id/projection`

This fetches task state from `consumer-entry-api /v1/chat/tasks/:id` and projects it into a Matrix message payload.

### Wallet/package commands

`/balance` / `/wallet` / `/余额` / `/钱包` call `consumer-entry-api /v1/matrix/users/:matrix_user_id/wallet?room_id=...` and project a Matrix-safe wallet card. The adapter keeps numeric display in the message body/HTML, but stores custom `cex_card` numeric fields as strings so Synapse accepts the event content.

`/plans` / `/plan` / `/package` / `/套餐` project the package metadata returned with the wallet projection, using the same `m.text` + `formatted_body` + `cex_card` shape.

## Environment variables

- `MATRIX_ENTRY_RUNTIME_PROFILE` / `CEX_RUNTIME_PROFILE` (optional, `local_dev|beta|production`, default `local_dev`; `MATRIX_ENTRY_RUNTIME_PROFILE` 优先)
- `MATRIX_ENTRY_ADAPTER_BIND_ADDR` (default `127.0.0.1:8091`)
- `CONSUMER_ENTRY_BASE_URL` (default `http://127.0.0.1:8090`)
- `CONSUMER_ENTRY_API_KEY` (optional)
- `CONSUMER_ENTRY_INGRESS_TOKEN` (optional, forwarded as `x-entry-token` to consumer-entry-api)
- `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET` (optional, preferred adapter-side secret for downstream `x-cex-user-session` assertions; falls back to `CONSUMER_ENTRY_SESSION_AUTH_SECRET` for compatibility)
- `CONSUMER_ENTRY_SESSION_AUTH_SECRET` (optional compatibility fallback)
- `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_KEY_ID` (optional, recommended when consumer side uses `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON`, e.g. `v1`)
- `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH` (optional repo-local shared issuer registry JSON path; 示例可从 `./run/local-runtime/session-auth-issuer-registry.example.json` 复制；建议 registry 带 `version` + `revision`，配置后 adapter 可从 registry 自动选 active key 和 signing secret)
- `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH` (optional approved-revision source for the shared issuer registry; 示例可从 `./run/local-runtime/session-auth-issuer-registry-approved-revisions.example.json` 复制)
- `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION` (optional, default `false`; when `true`, matrix side will treat unapproved live registry revisions as governance-invalid)
- `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER` (optional, default `matrix-entry-adapter`)
- `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_AUDIENCE` (optional, default `consumer-entry-api`)
- `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_TTL_SECS` (optional, default `300`)
- `MATRIX_ENTRY_INGRESS_TOKEN` (optional, when set requests must carry `x-entry-token`)
- `MATRIX_ENTRY_MAX_TEXT_CHARS` (optional, default `4000`)
- `MATRIX_ENTRY_RATE_LIMIT_WINDOW_SECS` (optional, default `60`)
- `MATRIX_ENTRY_RATE_LIMIT_MAX_REQUESTS` (optional, default `20`)
- `MATRIX_ENTRY_RATE_LIMIT_STORE_PATH` (optional, when set the in-memory quota buckets are persisted across process restarts)
- `MATRIX_ENTRY_RECENT_EVENT_WINDOW_SECS` (optional, default `600`)
- `MATRIX_ENTRY_RECENT_EVENT_CACHE_SIZE` (optional, default `2048`)
- `MATRIX_ENTRY_RECENT_EVENT_STORE_PATH` (optional, when set duplicate-event cache is persisted across process restarts)
- `MATRIX_BOT_USER_ID` (default `@cex-bot:local.dev`)

### Runtime profile guardrails

- `local_dev`：保持当前本地开发默认值。
- `beta` / `production`：启动时要求至少具备 `MATRIX_ENTRY_INGRESS_TOKEN`、`CONSUMER_ENTRY_INGRESS_TOKEN`、`MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET`（或兼容回退到 `CONSUMER_ENTRY_SESSION_AUTH_SECRET`，或通过 `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH` 解析到可用 key）、`MATRIX_ENTRY_RECENT_EVENT_STORE_PATH`、`MATRIX_ENTRY_RATE_LIMIT_STORE_PATH`，并要求 `MATRIX_BOT_USER_ID` 不再使用默认 `@cex-bot:local.dev`。如果再显式开启 `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION=true`，则还要求 approved-revision source 可加载，且当前 live issuer-registry revision 已被批准。

如果 profile 校验失败，服务会在启动阶段直接拒绝继续运行，而不是带着 local-dev 假设进入 beta/production。

## Run locally

```bash
cd /home/qian-qi/CEX
cargo run -p consumer-entry-api
cargo run -p matrix-entry-adapter
```

> Linux 一键闭环启动（含 relay）
>
> ```bash
> cd /home/qian-qi/CEX
> ./scripts/start-matrix-bot-chain.sh
> ```

该脚本会先启动 `consumer-entry-api` 与 `matrix-entry-adapter`，再启动 `matrix-bot-relay`，并进行 `/health` 就绪校验。若要让 Matrix 事件 dedupe 与 rate-limit quota 一起穿过进程重启，可同时设置 `MATRIX_ENTRY_RECENT_EVENT_STORE_PATH` 与 `MATRIX_ENTRY_RATE_LIMIT_STORE_PATH`。

### Downstream signed session assertions

当配置了 `CONSUMER_ENTRY_SESSION_AUTH_SECRET` 时，`matrix-entry-adapter` 现在会在转发到 `consumer-entry-api /v1/matrix/messages` 时自动附带：

- `x-cex-user-session`
- `x-cex-user-session-signature`

claims 会把 `matrix_user_id` 作为 `subject`，把 Matrix 房间作为 `room_id`，固定 `source_kind=matrix_message`，并默认带 `issuer=matrix-entry-adapter`、可选 `key_id`、`audience=consumer-entry-api`，同时还会附带一个针对当前 Matrix create payload 的 `request_fingerprint`。如果 consumer 侧配置了 `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH`，adapter 现在可以直接复用同一份 repo-local issuer registry 来选择 active key 和 signing secret；若没走 registry，也仍可使用 `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET` + `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_KEY_ID` 的显式方式签发，而不用和 consumer 共用一个全局 secret，也不必把 rotation 直接编码进 issuer 名称。现在这条 signer 选择已经不只停留在 startup snapshot 了，adapter 内部会持有 **live runtime issuer-registry state**，`build_consumer_entry_session_auth_headers(...)`、`/health`、`/metrics` 都读取 runtime state；同时新增受 `x-entry-token` 保护的 focused admin 面：`GET /v1/admin/consumer-entry-session-auth/status`、`POST /v1/admin/consumer-entry-session-auth/validate`、`POST /v1/admin/consumer-entry-session-auth/reload`。其中 `validate` 会对磁盘 candidate 做即时重读，并返回 candidate/live 的 revision 与 selection diff；`reload` 会在 candidate registry load 正常、approval coverage 满足时把 live signer authority 切到新的 runtime registry，而不需要重启服务。`/health.consumer_entry_session_auth` 现在也会直接暴露 runtime registry metadata、approval source state 与 live selection 结果，比如当前是走显式 secret 还是 shared registry、挑中了哪个 `key_id`、registry 是否缺 issuer / active key、approval source 是否可加载、以及当前 live revision 是否已被批准；同时 `/health.consumer_entry_session_auth_governance_overview` 会再把这层 signer selection / registry readiness / approval coverage 收口成 operator-glance block，方便 wrapper 直接提炼 signals。这样 consumer-entry-api 不再只靠共享 ingress token 就信任下游转发过来的 Matrix 用户身份，而且 assertion 也不会那么容易在 TTL 内被拿去复用到另一条 payload。这个方案仍是 shared-secret / docs-mirror first slice，但已经把 downstream user/session auth 链闭成了最小可用基线，并开始显式声明 issuer / key-id / audience / request-bound assertion / shared issuer registry / approved live revision / live runtime signer authority 边界。

## Local smoke

```bash
curl -s http://127.0.0.1:8091/health | jq

curl -s -X POST http://127.0.0.1:8091/v1/matrix/events \
  -H 'content-type: application/json' \
  -d '{
    "event_id":"$event-123",
    "event_type":"m.room.message",
    "room_id":"!roomid:local.dev",
    "sender":"@alice:local.dev",
    "text":"Summarize this document"
  }' | jq
```

## What this service returns

On success it returns:

- the forwarded task response from `consumer-entry-api`, or a command-specific lookup result
- a `projected_reply` object shaped like a Matrix message event content payload

Task card reply shape:

```json
{
  "msgtype": "m.text",
  "body": "🧾 CEX 任务卡\n状态：任务已创建，正在排队中\nTask: <id>\n执行：Queued / manual\n账户：<account-id>\n查看：/status <id>\n余额：/balance",
  "format": "org.matrix.custom.html",
  "formatted_body": "<blockquote>...</blockquote>",
  "cex_task_id": "<id>",
  "consumer_status": "queued",
  "invocation_status": "Queued",
  "cex_card": {
    "type": "task_status",
    "version": 1,
    "task_id": "<id>",
    "consumer_status": "queued",
    "invocation_status": "Queued"
  }
}
```

Wallet/package replies use the same Matrix `m.text` + `formatted_body` model with `cex_card.type = wallet_summary` or `package_summary`.

## What this is NOT yet

Still missing:

- Matrix appservice registration
- confirmation button callbacks
- attachment/media bridging
- richer Element UI customization beyond `formatted_body` cards
- a shared multi-service identity source-of-truth service beyond the current repo-local identity registry
- durable replay controls and distributed rate limiting beyond current local persisted stores

## Recommended next step

当前 v1 已按移动端命令规格落地：

- `/help`
- `/task <text> [cap=<capability_id>] [account=<account_id>]`
- `/status <invocation-id>`
- `/balance` / `/wallet` / `/余额` / `/钱包`
- `/plans` / `/plan` / `/package` / `/套餐`

详见 `docs/matrix-mobile-command-spec-v1.md`。

真实 Matrix/Element 房间验收请使用：

```bash
CEX_ENV_FILE=run/local-production/.env ./scripts/start-matrix-live-stack.sh
./scripts/check-matrix-live-room-e2e.sh
```

After this bridge, deeper product UI work should focus on confirmation callbacks, attachment/media bridging, and optional Element customization beyond HTML-compatible cards.

# Consumer Entry First Slice v1

## Goal

Create the first runnable product-facing layer between Matrix/Element and CEX.

This first slice is intentionally small:

- accept a chat-style message payload
- translate it into a CEX invocation
- project backend runtime state into consumer-friendly task state

## New service

- `services/consumer-entry-api`

## What it exposes today

### Health

`GET /health`

当前会额外暴露 identity binding source metadata，包括 `format`、`version`、`revision`、`source_path`、`source_modified_epoch`、`loaded_at_epoch`、`load_status`、`load_error`。当配置了独立 registry 文件时，还会额外暴露 `identity_registry_metadata`，用于显示 canonical product-user registry 的加载状态和来源。除此之外还会暴露 `identity_binding_audit`，用于显示当前 audit log 路径、最近一次事件类型、最近一次写入时间、最近一次 audit 写入状态和错误；并暴露 `identity_binding_reload_policy` 与 `identity_binding_revision_approval`，用于显示当前 reload governance 开关和 approved revision source 状态。现在 `/health` 还会暴露 `identity_source_of_truth`，用于显示当前是 `inline_bindings`、`product_user_registry` 还是 `mixed` 模式，以及 `product_users` 总数和 `missing_product_user_refs`。`identity_binding_counts` 也会额外包含 `product_users`、`*_product_user_refs`、`inline_*`、`missing_product_user_refs` 与 `source_of_truth_mode`。现在 `/health` 还会暴露 `runtime_profile` 与 `profile_validation`，用于显示当前运行模式（`local_dev|beta|production`）以及该 profile 的最小安全要求是否满足；其中 `profile_validation.checks.identity_governance_valid` 会给出 identity 治理聚合信号，`profile_validation.checks.session_auth_issuer_registry_governance_valid` 会给出 session-auth issuer registry 治理聚合信号。配套还会暴露 `rate_limit_store_path` / `rate_limit_store_enabled` / `rate_limit_bucket_count`，用于显示 durable quota store 是否启用以及当前已加载 bucket 数。当前 `/health` 还会内嵌 `identity_governance_overview`，把 binding load、registry load、ref-integrity、actor gate、approval source、approval coverage 的聚合结果收口成单个 overview；同时也会内嵌 `session_auth_issuer_registry_governance_overview`，把 issuer registry load、revision presence、approval source、approval coverage 的聚合结果收口成单个 operator-glance block。

### Metrics

`GET /metrics`

现在会额外暴露 Prometheus 文本格式指标，至少覆盖 task create / lookup、rate limit、replay hits、identity binding reload / actor rejection / audit failure，以及 profile 安全基线相关 gauge（如 `cex_consumer_entry_profile_validation_ok`、`cex_consumer_entry_ingress_protected`、`cex_consumer_entry_require_identity_binding`）。identity source-of-truth 相关也会通过 `cex_consumer_entry_identity_registry_users`、`cex_consumer_entry_identity_registry_refs`、`cex_consumer_entry_identity_registry_missing_refs` 暴露。durable quota 相关也会通过 `cex_consumer_entry_rate_limit_store_enabled` 与 `cex_consumer_entry_rate_limit_bucket_count` 暴露。现在还会额外暴露 identity governance gauge，包括 `cex_consumer_entry_identity_governance_valid`、`cex_consumer_entry_identity_binding_loaded`、`cex_consumer_entry_identity_registry_loaded`、`cex_consumer_entry_identity_ref_integrity_ok`、`cex_consumer_entry_identity_actor_gate_valid`、`cex_consumer_entry_identity_approval_source_valid` 与 `cex_consumer_entry_identity_approval_coverage_valid`，以及 session-auth issuer registry governance gauge，如 `cex_consumer_entry_session_auth_issuer_registry_governance_valid`、`cex_consumer_entry_session_auth_issuer_registry_approval_source_valid`、`cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid`，用于把 admin / health 面里的治理结论直接变成可 scrape 的 operator signal。

### Web game shells

`GET /league` exposes the Trillionnium League server-rendered game shell.

`GET /world` exposes the Trillionnium World server-rendered open-world shell with zones, locations, Agent residents/NPCs, player assets, asset upgrade form, companies/shops/listings, Commerce / Work Orders, Faction Reputation Map, World Contracts, contract completion form, and world event timeline.

`POST /league/web/action` supports interactive League actions (`join`, `guild`, `team`, `raid`, `draft`, `submit`). `POST /world/web/action` records free-form World actions. `POST /world/web/asset` upgrades an asset from the browser. `POST /world/web/company` launches a company/shop/listing from an asset. `POST /world/web/listing` publishes a priced service listing from a company. `POST /world/web/buy` buys/hires a listing, creates a purchase + work order, credits seller revenue through Ledger when available, and updates faction standing. `POST /world/web/contract` completes an existing contract from the browser. Matrix `/contract` records a task-backed World Contract through the signed Matrix identity path, `/complete` scores/settles the delivery, `/upgrade` grows World assets, `/company` launches operating companies, `/sell` publishes shop listings, `/buy` creates work, `/work` lists commerce, and `/factions` shows reputation. Outside local-dev, browser mutation paths use signed web session + CSRF protection.

### Create a task from a generic chat payload

`POST /v1/chat/tasks`

Example:

```json
{
  "user_id": "user-demo-1",
  "room_id": "room-demo-1",
  "session_id": "sess-demo-1",
  "org_id": "org-demo-1",
  "text": "Summarize this document",
  "capability_id": "cap.demo.summarize",
  "idempotency_key": "chat-demo-req-1"
}
```

### Create a task from a Matrix-shaped message payload

`POST /v1/matrix/messages`

Example:

```json
{
  "matrix_user_id": "@alice:local.dev",
  "room_id": "!roomid:local.dev",
  "session_id": "sess-demo-1",
  "org_id": "org-demo-1",
  "message": "Summarize this document",
  "capability_id": "cap.demo.summarize",
  "event_id": "$example-event-1"
}
```

### Read projected task status

`GET /v1/chat/tasks/:task_id`

### Reload identity bindings

`POST /v1/admin/identity-bindings/reload`

- 使用与其他入口相同的 `x-entry-token` 保护
- 重新从 `CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH` 载入 binding 文件，并在配置了 `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH` 时同步重载独立 registry 文件
- 若配置了 `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH`，会追加一条 JSONL audit event
- 会按当前 reload governance 和 approved revision source 校验 candidate binding，失败时返回 `409` 且不会替换当前 store
- 当 `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR=true` 时，要求从 `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ACTOR_HEADER`（默认 `x-identity-binding-actor`）读取操作者身份，并校验是否在 `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS` 白名单内；未通过时返回 `403`
- `actor` 可读字段：`identity_binding_reload_governance.actor_authorized`、`identity_binding_reload_governance.actor_reason`
  - `actor_missing`：未携带该 header 或值为空
  - `actor_not_allowed`：header 存在但不在白名单
  - `no_allowed_actors_configured`：`REQUIRE_ACTOR=true` 但白名单为空
- 若 candidate binding / registry 会导致 `missing_product_user_refs > 0`（即现有 binding 引用了 registry 中不存在的 `product_user_id`），reload 会返回 `409`，reason 为 `missing_product_user_refs`，避免把 source-of-truth 断链状态切成当前激活版本。
- 返回新的 `identity_binding_metadata`、`identity_registry_metadata`、`identity_binding_counts`、最近一次 `identity_binding_audit`，以及 `identity_binding_reload_governance` / `identity_binding_revision_approval`。当启用独立 registry 文件时，`identity_binding_reload_governance` 还会额外暴露 `current_registry_revision`、`candidate_registry_revision`、`current_effective_revision`、`candidate_effective_revision`、`candidate_registry_load_status`，以及 current/candidate 两侧的 `*_missing_product_user_refs`，用于说明这次 reload 实际按哪组 revision / load status / ref 完整性决策。如果设置了 `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH`，`identity_binding_audit.last_status` 将显示 `written` / `write_error` / `serialize_error` / `disabled`。
- 示例（自定义 actor header）:

  ```bash
  CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ACTOR_HEADER=x-deploy-actor \
  CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS=alice,bob \
  CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR=true \
  curl -sS -H "x-entry-token: local-dev-key" \
    -H "x-deploy-actor: alice" \
    -X POST "http://127.0.0.1:8090/v1/admin/identity-bindings/reload"
  ```

### Reload identity registry only

`POST /v1/admin/identity-registry/reload`

- 使用与 binding reload 相同的 ingress token、actor gate、approved revision 和 rollback governance
- 只重载 `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH` 指向的独立 registry 文件，当前已加载的 binding 文档保持不变
- 适合在 membership 没变、只更新 canonical `product_user_id -> org/account/status` registry 时使用
- 返回体与 binding reload 基本一致，但 audit `event_kind` 会写成 `registry_reload` / `registry_reload_rejected`
- 如果没有配置 `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH`，会返回 `409` 和 `error=identity_registry_not_configured`
- 如果新的 registry 会让当前 binding 出现缺失 `product_user_id` 引用，入口会返回 `409`，并在 `identity_binding_reload_governance.reason` 中给出 `missing_product_user_refs`
- 在 separate registry 模式下，这个入口仍按 combined effective revision 决策，所以单独更新 registry revision 也会改变 `candidate_effective_revision`

### Validate identity registry candidate

`POST /v1/admin/identity-registry/validate`

- 使用与 reload 相同的 ingress token、actor gate、approved revision、rollback 和 ref-integrity governance
- 会从磁盘重新读取当前 registry 文件，构造 candidate，但**不会**替换当前已加载 store，也**不会**追加 audit event
- 返回 `validated=true`、`valid=true|false`、`would_reload=true|false`，并带上与 reload 相同的 `identity_binding_reload_governance`
- 适合在真正执行 `/reload` 前先预览 candidate 的 `candidate_effective_revision`、`candidate_registry_load_status` 与 `missing_product_user_refs`
- 如果没有配置 `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH`，会返回 `409` 和 `error=identity_registry_not_configured`

### Read current identity registry admin status

`GET /v1/admin/identity-registry/status`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 返回当前已加载 store 的 focused admin snapshot，包括：
  - `effective_revision`
  - `missing_product_user_refs`
  - `identity_binding_metadata`
  - `identity_registry_metadata`
  - `identity_binding_counts`
  - `identity_source_of_truth`
  - `identity_binding_reload_policy`
  - `identity_binding_revision_approval`
  - `identity_binding_audit`
- 适合区分“磁盘上的 candidate”和“当前进程里真正已生效的 registry revision / effective revision”

### Read identity registry audit history

`GET /v1/admin/identity-registry/audit?limit=20`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 读取 `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH` 指向的 JSONL audit 文件，但只返回 registry 相关事件：`registry_reload` / `registry_reload_rejected`
- `limit` 可选，默认 `20`，当前实现会限制在 `1..100`
- 返回体包含：
  - `audit_path`
  - `returned_event_count`
  - `parse_error_count`
  - `events[]`（按最新在前返回）
  - 当前 store 的同一份 admin snapshot
- 如果没有配置 `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH`，会返回 `409` 和 `error=identity_audit_not_configured`

### Read current approval source status

`GET /v1/admin/identity-approval/status`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 返回当前 approval source 与 current effective revision 的关系快照
- `identity_approval_checks` 目前至少包含：
  - `status`
  - `current_effective_revision`
  - `current_effective_revision_approved`
  - `current_effective_revision_index`
  - `latest_approved_revision`
  - `latest_approved_revision_index`
  - `current_matches_latest_approved`
  - `approved_revision_count`
  - `rollback_order_available`
- 适合值班时快速判断：当前生效 binding/registry 组合版本是否在 approved revisions 里，以及是否已经追到最新批准版本

### Validate current approval coverage

`POST /v1/admin/identity-approval/validate`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 重新读取当前 approval source，并验证“当前 effective revision 是否被批准”
- 返回 `validated=true`、`valid=true|false`，并附带同一份 `identity_approval_checks`
- 当前会把以下情况视为 validate 失败并返回 `409`：
  - `approval_not_configured`
  - `approval_state_not_loaded`
  - `current_effective_revision_missing`
  - `current_effective_revision_not_approved`

### Read approval source preview

`GET /v1/admin/identity-approval/source?limit=20`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 返回当前 approved revisions source 的 focused preview，而不是只看“当前 effective revision 是否被批准”
- `identity_approval_source` 目前至少包含：
  - `status`
  - `valid`
  - `source_path`
  - `load_status`
  - `revision`
  - `limit`
  - `returned_order`（当前为 `latest_first`）
  - `approved_revision_count`
  - `returned_revision_count`
  - `latest_approved_revision`
  - `latest_approved_revision_index`
  - `current_effective_revision`
  - `current_effective_revision_index`
  - `current_effective_revision_approved`
  - `revisions[]`（每项带 `index` / `revision` / `is_latest` / `is_current_effective`）
- 适合快速预览最近若干条 approved revisions，并判断当前 effective revision 在批准序列中的位置

### Validate approval source itself

`POST /v1/admin/identity-approval/source/validate?limit=20`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 对 approved revisions source 本身做 focused validate，而不是只校验“当前 effective revision 是否已批准”
- 返回 `validated=true`、`valid=true|false`，并附带同一份 `identity_approval_source`
- 当前会把以下情况视为 validate 失败并返回 `409`：
  - `approval_not_configured`
  - `approval_state_not_loaded`
  - `approved_revision_set_empty`

### Read identity governance overview

`GET /v1/admin/identity-governance/status?limit=20`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 返回统一的 identity governance 总览，适合值班时一眼看完当前治理状态
- `identity_governance_overview` 目前至少包含：
  - `status`
  - `valid`
  - `effective_revision`
  - `registry_configured`
  - `missing_product_user_refs`
  - `checks.binding_loaded`
  - `checks.registry_loaded`
  - `checks.ref_integrity_ok`
  - `checks.actor_gate_valid`
  - `checks.approval_source_valid`
  - `checks.approval_coverage_valid`
  - `identity_binding_metadata`
  - `identity_registry_metadata`
  - `identity_binding_counts`
  - `identity_source_of_truth`
  - `identity_binding_reload_policy`
  - `identity_binding_audit`
  - `identity_actor_checks`
  - `identity_approval_checks`
  - `identity_approval_source`
- `limit` 当前用于内嵌 approval source preview 的返回条数

### Validate current identity governance state

`POST /v1/admin/identity-governance/validate?limit=20`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 基于统一 overview 做 focused validate，并返回 `validated=true`、`valid=true|false`
- 当前会把以下治理问题视为 validate 失败并返回 `409`：
  - `identity_bindings_not_loaded`
  - `identity_registry_not_loaded`
  - `missing_product_user_refs`
  - actor gate 不合法时的对应状态（例如 `no_allowed_actors_configured`）
  - approval source 不合法时的对应状态（例如 `approved_revision_set_empty`）
  - approval coverage 不合法时的对应状态（例如 `current_effective_revision_not_approved`）

### Read current actor gate status

`GET /v1/admin/identity-actors/status`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 返回当前 reload actor gate 的 focused admin 视图
- `identity_actor_checks` 目前至少包含：
  - `status`
  - `valid`
  - `require_actor`
  - `actor_header`
  - `actor_header_valid`
  - `allowed_actor_count`
  - `allowed_actors[]`
- 适合快速确认当前 reload actor allowlist 是否已启用、使用哪个 header，以及允许哪些 actor

### Validate current actor gate configuration

`POST /v1/admin/identity-actors/validate`

- 使用与其他 admin 入口相同的 `x-entry-token` 保护
- 对当前 actor allowlist 配置做 focused validate，并返回 `validated=true`、`valid=true|false`
- 当前会把以下情况视为 validate 失败并返回 `409`：
  - `actor_header_missing`
  - `no_allowed_actors_configured`
- 若 `require_actor=false`，则状态为 `disabled`，当前会视为非报错配置并返回 `200`

## Current projection model

CEX runtime state -> consumer task state:

- `Created` -> `received`
- `Queued` -> `queued`
- `AwaitingApproval` -> `waiting_for_confirmation`
- `Approved` / `Dispatching` / `Running` -> `processing`
- `Succeeded` -> `done`
- `Failed` -> `failed`
- `Refunded` -> `refunded`

## Environment variables

- `CONSUMER_ENTRY_RUNTIME_PROFILE` / `CEX_RUNTIME_PROFILE` (optional, `local_dev|beta|production`, default `local_dev`; `CONSUMER_ENTRY_RUNTIME_PROFILE` 优先)
- `CONSUMER_ENTRY_BIND_ADDR` (default `127.0.0.1:8090`)
- `CEX_GATEWAY_BASE_URL` (default `http://127.0.0.1:8080`)
- `CEX_GATEWAY_API_KEY` (default `local-dev-key`)
- `CONSUMER_ENTRY_DEFAULT_CAPABILITY_ID` (optional)
- `CONSUMER_ENTRY_DEFAULT_ACCOUNT_ID` (optional)
- `CONSUMER_ENTRY_INGRESS_TOKEN` (optional, when set requests must carry `x-entry-token`)
- `CONSUMER_ENTRY_REQUIRE_SESSION_AUTH` (optional, default `false`, when `true` create endpoints also require a signed user/session assertion via `x-cex-user-session` + `x-cex-user-session-signature`)
- `CONSUMER_ENTRY_SESSION_AUTH_SECRET` (optional, legacy/global shared HMAC secret used to verify `x-cex-user-session` assertions)
- `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON` (optional JSON object, preferred simple per-issuer form in beta/production, e.g. `{"matrix-entry-adapter":"replace-me"}`)
- `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON` (optional JSON object for rotation-friendly per-issuer key registries, e.g. `{"matrix-entry-adapter":{"v1":"replace-me","v2":"next-secret"}}`)
- `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH` (optional repo-local shared issuer registry JSON path, e.g. `./run/local-runtime/session-auth-issuer-registry.json`; 示例可从 `./run/local-runtime/session-auth-issuer-registry.example.json` 复制；建议带 `version` + `revision` + `issuers.*.keys`，当配置后 consumer 优先按 `(issuer,key_id)` 从该文件选验签 secret)
- `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH` (optional approval source for governed registry revisions; 示例可从 `./run/local-runtime/session-auth-issuer-registry-approved-revisions.example.json` 复制)
- `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION` (optional, default `false`, when `true` higher-profile validation and admin validate flows require the current registry `revision` to appear in the approved revision source)
- `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_ACTOR` (optional, default `false`, when `true` session-auth issuer-registry governance also requires a non-empty actor gate config)
- `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_ACTOR_HEADER` (optional, default `x-session-auth-issuer-registry-actor`)
- `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_ALLOWED_ACTORS` (optional CSV list for the session-auth issuer-registry actor gate)
- `CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS` (optional CSV, recommended in beta/production, e.g. `matrix-entry-adapter,chat-session-gateway`)
- `CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE` (optional, recommended in beta/production, e.g. `consumer-entry-api`)
- `CONSUMER_ENTRY_SESSION_AUTH_MAX_CLOCK_SKEW_SECS` (optional, default `300`)
- `CONSUMER_ENTRY_SESSION_AUTH_MAX_TTL_SECS` (optional, default `900`)
- `CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH` (optional, local JSON file for trusted chat/matrix identity bindings)
- `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH` (optional, separate local JSON file for canonical `product_user_id -> org/account/status` registry; when set it overrides embedded `product_users` in the binding document)
- `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH` (optional, append-only JSONL audit log for startup/reload events)
- `CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH` (optional, local JSON file describing approved revision order)
- `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION` (optional, default `false`, when `true` reload rejects candidate bindings without a non-empty `revision`)
- `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REJECT_SAME_REVISION` (optional, default `false`, when `true` reload rejects candidates whose `revision` equals the currently loaded binding revision)
- `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_LEGACY_FORMAT` (optional, default `true`, when `false` reload only accepts `versioned-document` binding files)
- `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION` (optional, default `false`, when `true` reload requires candidate `revision` to exist in the approved revision source)
- `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_ROLLBACK` (optional, default `true`, when `false` reload rejects revisions that appear earlier than the currently loaded revision in the approved revision order)
- `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR` (optional, default `false`, when `true` reload requires actor identity via the configured header and allow-list)
- `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ACTOR_HEADER` (optional, default `x-identity-binding-actor`)
- `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS` (optional, comma-separated actor identifiers, e.g. `alice,bob`)
- `CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING` (optional, default `false`, when `true` requests without a matching binding are rejected)
- `CONSUMER_ENTRY_MAX_TEXT_CHARS` (optional, default `4000`)
- `CONSUMER_ENTRY_RATE_LIMIT_WINDOW_SECS` (optional, default `60`)
- `CONSUMER_ENTRY_RATE_LIMIT_MAX_REQUESTS` (optional, default `30`, source-scope composite bucket)
- `CONSUMER_ENTRY_RATE_LIMIT_USER_MAX_REQUESTS` (optional, default `0`, disabled when `0`)
- `CONSUMER_ENTRY_RATE_LIMIT_ROOM_MAX_REQUESTS` (optional, default `0`, disabled when `0`)
- `CONSUMER_ENTRY_RATE_LIMIT_SESSION_MAX_REQUESTS` (optional, default `0`, disabled when `0`)
- `CONSUMER_ENTRY_RATE_LIMIT_ORG_MAX_REQUESTS` (optional, default `0`, disabled when `0`)
- `CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH` (optional, when set the in-memory quota buckets are persisted across process restarts)
- `CONSUMER_ENTRY_REPLAY_WINDOW_SECS` (optional, default `600`, old `CONSUMER_ENTRY_MATRIX_EVENT_WINDOW_SECS` still accepted as compatibility alias)
- `CONSUMER_ENTRY_REPLAY_CACHE_SIZE` (optional, default `2048`, old `CONSUMER_ENTRY_MATRIX_EVENT_CACHE_SIZE` still accepted as compatibility alias)
- `CONSUMER_ENTRY_REPLAY_STORE_PATH` (optional, when set request replay state is persisted across process restarts, old `CONSUMER_ENTRY_MATRIX_EVENT_STORE_PATH` still accepted as compatibility alias)

### Runtime profile guardrails

- `local_dev`：保持现在的本地开发宽松默认值。
- `beta`：启动时要求至少具备 `CONSUMER_ENTRY_INGRESS_TOKEN`、`CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true`、`CONSUMER_ENTRY_SESSION_AUTH_SECRET` **或** `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON` **或** `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON` **或** `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH`、`CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS`、`CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE`、`CONSUMER_ENTRY_REPLAY_STORE_PATH`、`CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH`、`CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH`、`CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true`，并要求 `CEX_GATEWAY_API_KEY` 不再是默认 `local-dev-key`。
- `production`：在 `beta` 基础上继续要求 `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH`、`CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH`，以及 `RELOAD_REQUIRE_REVISION=true`、`RELOAD_REQUIRE_APPROVED_REVISION=true`、`RELOAD_REQUIRE_ACTOR=true` 且 `RELOAD_ALLOWED_ACTORS` 非空。

如果 profile 校验失败，服务会在启动阶段直接拒绝继续运行，而不是带着 local-dev 假设进入 beta/production。

### Signed user/session auth slice

`consumer-entry-api` 现在对 `/v1/chat/tasks` 与 `/v1/matrix/messages` 支持一层最小的 real user/session auth baseline。启用 `CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true` 后，请求除了 `x-entry-token` 之外，还必须带：

- `x-cex-user-session`: base64url(JSON claims)
- `x-cex-user-session-signature`: base64url(HMAC-SHA256(secret, assertion_header_value))

当前 claims 至少包含：`version`、`issuer`、`key_id`、`subject`、`source_kind`、`audience`、`request_fingerprint`、`issued_at_epoch`、`expires_at_epoch`，并可继续携带 `room_id` / `session_id` / `org_id` / `account_id`。服务会校验：签名、issuer allow-list、audience、request_fingerprint、时钟偏移、TTL、`source_kind`，以及 claims 与最终 `identity_scope` 的一致性。这里的 `request_fingerprint` 用来把 assertion 绑到这一次 create request 的规范化关键字段上，减少同一张 signed ticket 在 TTL 内被拿去复用到别的 payload。consumer 现在还支持按 `issuer` 选择 secret，优先读取 `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH` 里的 repo-local shared issuer registry，并按 `(issuer,key_id)` 选验签 key；若该 issuer 不在 registry 中，再回退到 `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON`，再回退到 `CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON`，最后才回退到全局 `CONSUMER_ENTRY_SESSION_AUTH_SECRET`。`/health.session_auth` 现在会回显 registry `revision/load_status/issuer_count/key_count` metadata，以及 approval state/checks；`/metrics` 也新增了 registry approval/revision/key gauges。受 ingress token 保护的 focused admin 面现在包括：`POST /v1/admin/session-auth/issuer-registry/reload`、`GET /v1/admin/session-auth/issuer-registry/status`、`POST /v1/admin/session-auth/issuer-registry/validate`、`GET /v1/admin/session-auth/issuer-registry/actors/status`、`POST /v1/admin/session-auth/issuer-registry/actors/validate`、`GET /v1/admin/session-auth/issuer-registry/approval/status`、`POST /v1/admin/session-auth/issuer-registry/approval/validate`，用于查看 **当前 live runtime registry**、allowed-issuer coverage、actor gate readiness、approved revision coverage，以及把磁盘源文件经过 actor/approval gate 后 reload 进 live verifier state，而不需要重启服务。`status` 现在还会直接回显每个 issuer 的 `active_key_id/key_ids` 摘要，`validate` 会返回 `session_auth_issuer_registry_actor_request`、`session_auth_issuer_registry_active_key_diff` 与 `matches_loaded_active_keys`，让 operator 在不真正改 live state 的前提下先看 candidate/live active-key 变化与 actor gate 命中情况；`reload` 则会在通过 gate 后真正切换 verifier 使用中的 live registry。这样 create endpoints 不再只信任调用方直接塞进 payload 的 `user_id/session_id/org_id`。这还是 repo-local / shared-secret first slice，不是完整 auth service，但已经把“只有 ingress token 就能伪造 user/session”这条硬缺口收掉了第一层，并继续把 session assertions 往“issuer + audience + request-bound assertion + shared issuer registry + approved revision governance + actor-gated live authority”方向收口。

### Identity binding file shape

Preferred versioned binding shape:

```json
{
  "version": 1,
  "revision": "2026-04-19-a",
  "chat_users": {
    "user-demo-1": {
      "product_user_id": "pu-demo-1"
    }
  },
  "matrix_users": {
    "@alice:local.dev": {
      "product_user_id": "pu-demo-1"
    }
  }
}
```

Optional separate registry shape:

```json
{
  "version": 1,
  "revision": "registry-2026-04-19-a",
  "product_users": {
    "pu-demo-1": {
      "org_id": "org-demo-1",
      "account_id": "acct-demo-1",
      "status": "active"
    }
  }
}
```

兼容迁移场景时，binding 文档里仍可内嵌 `product_users`，也仍可直接内联 `org_id` / `account_id`；如果同一条 binding 同时声明了 `product_user_id` 与内联字段，则 registry 值被视为 source-of-truth，若两者冲突会在请求解析时直接报错，避免静默漂移。若配置了 `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH`，则独立 registry 会覆盖 binding 文档里嵌入的 `product_users`。

Legacy flat-map shape without `version` / `revision` is still accepted for compatibility.

### Approved revision file shape

```json
{
  "version": 1,
  "revision": "approval-doc-2026-04-19-a",
  "approved_revisions": [
    "2026-04-19-a",
    "2026-04-19-b",
    "2026-04-19-c"
  ]
}
```

当未配置独立 registry 文件时，`approved_revisions` 继续只记录 binding revision。若配置了 `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH` 且启用 revision governance，则系统会把 binding + registry 组合成 effective revision，例如 `binding:2026-04-19-b|registry:registry-2026-04-19-b`；此时 `approved_revisions` 也应记录这种组合值。

`approved_revisions` is ordered from older to newer, and that order is also used as the minimal no-rollback policy boundary when `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_ROLLBACK=false`.

When a binding exists, `consumer-entry-api` will:

- fill missing `org_id` / `account_id` from the binding
- if `product_user_id` is present, resolve canonical `org_id` / `account_id` from `product_users`
- when `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH` is configured, prefer the separate registry file over embedded `product_users`
- reject mismatched caller-supplied `org_id` / `account_id`
- reject bindings that reference an unknown `product_user_id`
- expose the normalized result under `source.identity_scope`
- expose binding source metadata under `source.identity_resolution` and `/health.identity_binding_metadata`
- expose source-of-truth summary under `/health.identity_source_of_truth`
- keep a minimal append-only local audit trail for startup/reload events when `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH` is configured
- evaluate manual reloads through a minimal governance layer (`require_revision`, `reject_same_revision`, `allow_legacy_format`, `require_approved_revision`, `allow_rollback`)
- support manual in-process reload through `POST /v1/admin/identity-bindings/reload`

## Run locally

```bash
cd /home/qian-qi/CEX
cargo run -p consumer-entry-api
```

## Linux one-command matrix chain starter

启动 `consumer-entry-api`、`matrix-entry-adapter`、`matrix-bot-relay`（和可选 `matrix-bot-poller`）的最小闭环命令：

```bash
cd /home/qian-qi/CEX
./scripts/start-matrix-bot-chain.sh [smoke]
```

`start-matrix-bot-chain.sh` 会按以下端口启动并做基础健康检查：

- `consumer-entry-api`: `8090`
- `matrix-entry-adapter`: `8091`
- `matrix-bot-relay`: `8092`

添加 `smoke` 会额外执行 `/health` 和一次 `inbound` 样例投递冒烟。

- `MATRIX_ACCESS_TOKEN` 若未设置，`matrix-bot-poller` 会被跳过；设置后会并行启动。

## Example local smoke

```bash
curl -s http://127.0.0.1:8090/health | jq

curl -s -X POST http://127.0.0.1:8090/v1/matrix/messages \
  -H 'content-type: application/json' \
  -d '{
    "matrix_user_id":"@alice:local.dev",
    "room_id":"!roomid:local.dev",
    "message":"Summarize this document",
    "capability_id":"cap.demo.summarize"
  }' | jq
```

## What this is NOT yet

This is still not a full Matrix appservice integration, but the local Matrix/Element bot path now has a real-room loop: `matrix-bot-poller` reads Synapse `/sync`, `matrix-bot-relay` forwards events to `matrix-entry-adapter`, and replies are sent back to the same Matrix room.

Current local Matrix/Element frontend slice:

- `/task ...` creates a CEX task and projects a structured `🧾 CEX 任务卡`
- `/status <task-id>` reads the CEX task projection and returns the same status card shape
- `/balance` / `/wallet` / `/余额` / `/钱包` calls `GET /v1/matrix/users/:matrix_user_id/wallet` and projects a wallet card
- `/plans` / `/package` / `/套餐` projects the package/plan metadata
- `scripts/start-matrix-live-stack.sh` can start local Synapse + Element Web + relay + poller, and `scripts/check-matrix-live-room-e2e.sh` validates the real room loop end-to-end

Still missing beyond this local production frontend slice:

- Matrix appservice registration
- a shared multi-service identity source-of-truth service, instead of the current repo-local `product_users` registry inside the binding document
- confirmation callbacks
- attachment handling
- richer Element customization beyond HTML-compatible Matrix message cards
- ingress auth beyond shared edge token
- durable replay protection and quota enforcement are still incomplete: `consumer-entry-api` now supports persisted replay for Matrix `event_id` and generic chat `idempotency_key`, exposes user / room / session / org scoped rate-limit buckets with optional local persistence, and can optionally resolve caller identity through a local binding file plus repo-local `product_users` registry into a normalized `identity_scope`, but richer identity-bound quota policy, distributed anti-abuse state, and a shared identity source still remain beyond the current local-file layer

移动端交互规范见：`docs/matrix-mobile-command-spec-v1.md`；真实房间验收见：`docs/matrix-bot-entry-e2e-checklist-v1.md`.

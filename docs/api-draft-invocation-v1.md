# API Draft - Invocation v1

## 1. POST /v1/invocations

### 请求头

- `x-api-key: <api-key>` 或 `Authorization: Bearer <api-key>`

### 请求体

```json
{
  "capability_id": "cap-789",
  "prompt": "Summarize this document"
}
```

### 当前语义
- 创建一次 invocation
- `org_id / actor_id` 不再信任客户端请求体，而是由 gateway 通过 identity-service 解析 API key 后注入
- 如果请求带了 `account_id`，gateway 会先到 ledger-service 查询该账户，并校验它属于 API key 解析出的 `org_id`
- 返回 invocation_id
- 返回 trace_id
- 当前版本仍偏向 internal alpha 的最小鉴权闭环

### 响应示例

```json
{
  "invocation_id": "uuid",
  "trace": {
    "trace_id": "uuid",
    "created_at": "2026-04-07T00:00:00Z"
  },
  "status": "Created",
  "execution_id": "uuid",
  "execution": {
    "dispatch_mode": "queued_worker",
    "attempt_count": 0,
    "max_attempts": 3,
    "attempts_remaining": 3,
    "retry_budget_exhausted": false
  },
  "request": {
    "org_id": "org-123",
    "actor_id": "user-456",
    "capability_id": "cap-789",
    "prompt": "Summarize this document"
  }
}
```

其中 `request.org_id / request.actor_id` 是 gateway 根据 API key 解析后写入的服务端上下文，而不是客户端直传的可信字段。

如果 `account_id` 不属于该 `org_id`，当前 gateway 会拒绝该 invocation，不再继续 reserve / execution。

## 2. GET /v1/invocations/:id

### 请求头

- `x-api-key: <api-key>` 或 `Authorization: Bearer <api-key>`

### 当前语义
- 根据 invocation_id 查询当前记录
- gateway 会先用 API key 解析 auth context，再按 invocation 持久化的 `request.org_id` 做 org boundary
- 如果 invocation 属于别的 org，返回 `403 api key not authorized for org`
- 如果不存在返回 404
- 若 invocation 已绑定 `execution_id`，gateway 现在会附带一个 execution snapshot，包含 `dispatch_mode`、`attempt_count`、`max_attempts`、`attempts_remaining` 与 `retry_budget_exhausted`
- 这份 snapshot 会缓存到 invocation 持久化层；读取时 gateway 会尽量 live refresh execution-service，若 refresh 失败则回退到缓存 snapshot，而不是直接把 invocation read 变成错误

## 3. 后续扩展
- 接 capability registry
- 接 ledger reserve
- 接 policy check
- 接 execution dispatch
- 接 audit event

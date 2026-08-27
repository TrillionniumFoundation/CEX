# ADR-006: Durable Command/Receipt Saga Outbox

- 状态：Proposed / foundation implemented
- 日期：2026-08-27
- Owner：Gateway / Execution / Ledger / Audit
- 关联迁移：`migrations/0057_add_durable_saga_outbox_v1.sql`

## Context

当前 invocation 流程同步调用 ledger、execution、provider 和 audit。部分 provider/ledger HTTP 调用发生在持有数据库事务或行锁期间，形成以下 crash window：

- reserve 成功但 invocation 状态未提交；
- provider 已执行但 result 未持久化；
- consume/refund 已完成但 execution transaction 回滚；
- audit 发送失败被 best-effort 忽略；
- 重试无法区分“未执行”与“执行结果未知”。

单个数据库 transaction 无法原子覆盖外部 HTTP/provider 副作用。

## Decision

采用 durable command + receipt + transition 的 saga/outbox 模型：

```text
persist command
  -> commit
  -> worker claim with short SQL transaction
  -> commit claim
  -> perform external side effect without DB lock
  -> persist idempotent receipt
  -> advance state / enqueue compensation
```

核心原则：

1. 所有外部副作用先有 durable command。
2. Worker claim 使用 `FOR UPDATE SKIP LOCKED` 和短租约。
3. Claim commit 后才能调用外部系统。
4. 每个 command 有稳定 `operation_key`，数据库唯一。
5. 外部结果使用 `(source_system, receipt_key)` 去重。
6. Attempt budget 与 delivery attempt 分离于业务重试原因。
7. provider 结果未知时进入 reconcile，不盲目重复副作用。
8. 状态变化写 append-only transition history。

## V1 schema

### `cex_saga_commands_v1`

保存 workflow、operation key、command kind、状态、attempt budget、available time、worker lease 和 payload。

### `cex_saga_receipts_v1`

保存外部系统 receipt；唯一约束阻止同一 source receipt 重复落地。

### `cex_saga_transitions_v1`

保存状态历史。数据库 trigger 拒绝非法状态跳转并自动记录 transition。

### `cex_claim_saga_commands_v1`

原子选择并 claim 可执行 command。函数本身不执行网络调用。

## Initial command kinds

- `ledger_reserve`
- `execution_create`
- `provider_dispatch`
- `ledger_consume`
- `ledger_refund`
- `audit_deliver`
- `reconcile`

新增 kind 需要 contract 更新、迁移和 worker ownership。

## Rollout

### Phase 1 — Shadow

- 现有同步流程继续 authority；
- 同事务写 shadow command；
- dispatcher 不执行，只验证 payload、operation key 和 backlog；
- reconciliation 比较 shadow command 与现有结果。

### Phase 2 — Ledger commands

- reserve/consume/refund 切换为 command/receipt；
- gateway/execution 等待或读取 receipt；
- idempotency key 使用 operation key。

### Phase 3 — Provider dispatch

- execution transaction 仅创建 provider command；
- provider worker claim 后提交，再调用 provider；
- success/failure/unknown receipt 推进状态；
- unknown 强制 reconcile。

### Phase 4 — Audit outbox

- 业务 transaction 同时写 `audit_deliver` command；
- audit dispatcher at-least-once 发送；
- audit event id 去重；
- backlog age 进入 SLO。

### Phase 5 — Contract

- 删除旧的 transaction 内 HTTP 路径；
- 禁止直接 provider/ledger side-effect 调用；
- 所有跨服务动作都可由 command/receipt 重建。

## Failure semantics

- lease 过期：command 可被重新 claim，attempt 增加；
- attempt budget 耗尽：进入 dead letter，需 operator/reconciler；
- receipt 重复：返回已有结果，不重复推进；
- receipt 不确定：创建 `reconcile` command；
- compensation 失败：保留原 failure 和 compensation command，两者都可查询。

## Consequences

优点：

- 网络调用不再持有业务行锁；
- crash/restart 可恢复；
- side effect 与 receipt 可对账；
- worker 水平扩展时 claim 安全；
- operator 能看到 backlog、lease 和 attempt budget。

代价：

- 最终一致性；
- 需要 dispatcher/reconciler；
- API 必须表达 accepted/pending/unknown；
- 测试需要故障注入和时间控制。

## Validation requirements

- fresh/upgrade migration；
- 双 worker claim competition；
- lease expiry reclaim；
- operation/receipt dedupe；
- illegal transition denial；
- crash after side effect before receipt；
- compensation and dead-letter runbook；
- backlog age metrics and alerting。

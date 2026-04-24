# CEX -> Rust AI-Native Platform

本项目不是传统加密货币交易所，而是一个借鉴开源中心化交易所工程能力的 **Rust AI 原生平台**。

## 目标

- 多租户账户体系
- Credits / 用量 / 预算控制
- 模型、Agent、工具、工作流统一接入
- 调用网关与智能路由
- 可审计执行链路
- 风控、审批、权限治理
- Marketplace 扩展能力

## 当前目录结构

- `docs/`：总体设计、回归覆盖、运行说明
- `architecture/`：系统上下文、边界、模块说明
- `decisions/`：架构决策记录（ADR）
- `research/`：候选开源项目、调研与比较
- `crates/`：共享类型、配置、错误、追踪
- `services/`：当前已落地的服务实现与测试
- `scripts/`：本地 runtime / gate / 兼容探针脚本

## 当前代码状态（2026-04）

当前仓库已经不是“刚起骨架”的阶段了，真实已落地并可构建/测试的主线包括：

- `gateway-service`
- `identity-service`
- `ledger-service`
- `execution-service`
- `audit-service`
- `capability-service`

其中当前最完整的一条运行链是：

`gateway -> identity / capability validation -> ledger -> execution -> audit`

并且 M0 方向已经推进到：

- `x-api-key` / `Authorization: Bearer ...` 驱动的鉴权入口
- gateway 不再信任客户端直传 `org_id / actor_id`
- `identity-service` 支持 DB-backed API key provenance + `issue/list/revoke/resolve`
- `gateway-service` 在 reserve / execution 前校验 `account_id` 是否属于 auth-resolved org
- API key lifecycle 已覆盖 `active / revoked / expired`
- key management 与 audit trace read 都已支持按 scope 解析 admin token
- `audit-service` 的 trace read 已受 `audit:read` 保护
- `capability-service` 已提供最小 capability registry，gateway 在请求带 `capability_id` 时会做真实存在性/启用态校验
- `execution-service` 已落第一条真实 provider handoff 路径，当前最小 adapter 为 `ollama`
- `execution-service` 的 create 响应现在会用显式 `dispatch_mode` 暴露 provider-backed dispatch 意图，当前已区分 `manual` / `immediate` / `queued_worker`；`gateway-service` 只会对 `immediate` 自动触发 execution start，`queued_worker` 则已有最小 `GET /v1/executions/worker-queue`（支持 `limit` / `claimable_only` / `lease_expired_only` / `worker_id` 过滤，并暴露 `attempt_count` / `max_attempts` / `attempts_remaining` / `retry_budget_exhausted`）+ `GET /v1/executions/worker-queue/summary`（现含 `retryable` / `retry_budget_exhausted` 聚合）+ `POST /v1/executions/claim-next`（支持 `lease_expired_only=true`）/ `claim-batch`（支持 `lease_expired_only=true`）+ `POST /v1/executions/reclaim-expired` + `POST /v1/executions/timeout-expired` + `POST /v1/executions/:id/requeue` + `POST /v1/executions/:id/retry` + `POST /v1/executions/:id/renew-lease` + `POST /v1/executions/:id/process` 入口可供后续 worker 接入，并带短租约（`EXECUTION_CLAIM_LEASE_SECONDS`，默认 300 秒）；当前最小 retry budget 语义支持 env 覆盖，`EXECUTION_DEFAULT_MAX_ATTEMPTS` / `EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS` 默认分别为 `1` / `3`，claim/reclaim/requeue/retry 都遵守这一 delivery-attempt budget；gateway 的 invocation create/get 响应也开始附带 execution snapshot，把 `dispatch_mode` 与 attempt/budget metadata 直接透给上层调用方，并把这份 snapshot 缓存到 invocation 持久化层，在 execution-service 临时不可达时回退使用缓存值
- execution policy 不再只靠硬编码关键词，当前已支持通过 env 配置 approval / block 规则与 reserve 硬阈值，例如：`POLICY_APPROVAL_SENSITIVE_KEYWORDS`、`POLICY_BLOCK_KEYWORDS`、`POLICY_APPROVAL_CAPABILITY_PREFIXES`、`POLICY_BLOCK_CAPABILITY_PREFIXES`、`POLICY_HARD_REJECT_RESERVE_THRESHOLD`
- `execution-service` 与 `gateway-service` 现都提供最小 `/v1/info` 运行快照，先暴露 policy/guardrail 配置与核心请求计数，便于本地排障与后续接 Prometheus/OTel 前的过渡观测；其中 execution info 还会直接回显 approval backlog、各 status 总量，以及 queued-worker queue depth / claimable / lease expired / active worker 摘要，并给出基于 `ALERT_APPROVAL_BACKLOG_THRESHOLD` / `ALERT_LEASE_EXPIRED_THRESHOLD` / `ALERT_RETRY_BUDGET_EXHAUSTED_THRESHOLD` / `ALERT_AUDIT_FAILURE_THRESHOLD` / `ALERT_REFUND_FAILURE_THRESHOLD` 的最小 operator signal；gateway info 则开始给出基于 `ALERT_GATEWAY_UPSTREAM_FAILURE_THRESHOLD` 的 upstream BAD_GATEWAY 信号

当前更接近：

- **工程上靠谱的 internal alpha / backend demo**

而不是：

- public beta
- 已完成生产化部署的外部可用平台

## 当前推荐入口

- 本地运行顺序：`docs/local-run-order-v1.md`
- 本地 gate：`docs/CI-GATE.md`
- 回归覆盖矩阵：`docs/REGRESSION-COVERAGE-MATRIX.md`
- 生产硬化清单：`docs/production-hardening-checklist-v1.md`
- operator runbook：`docs/operator-runbook-v1.md`
- alert rules draft：`docs/alert-rules-draft-v1.md`
- OpenClaw operator cron：`docs/openclaw-operator-signal-cron-v1.md`

当前最小 operator signal 检查面已不只覆盖 `gateway + execution`，还会把 `consumer-entry-api /health` 与 `matrix-entry-adapter /health` 一并收进统一 machine-readable 结果，作为 product-edge supporting surfaces。入口层里一小组关键 metrics（rate limit、ingress auth failures、matrix duplicate events）现在也会按阈值被提升成 warn signal。

如果要演练 split admin principals（key management 与 audit read 分权），优先看：

- `docs/admin-token-model.md`
- `.env.split-admin.example`
- `scripts/use-split-admin-env.ps1`
- `scripts/validate-split-admin-env.ps1`

## 下一阶段主线

1. 继续把 M0 的 operator/admin 权限模型做实，而不是长期停留在 shared secret 或最小 scoped token 层
2. 让 key lifecycle / audit / provenance 的回归继续保持 service-local + runtime 双层覆盖
3. 在已经落地的 capability registry + 首条 `ollama` adapter 基础上，继续把 provider handoff 做成更完整执行链
4. 后续再推进 worker/runtime persistence、budget/rate limit guardrail、以及更完整的部署/观测面
5. 把当前最小 `/v1/info` signal 面继续演进成统一 exporter + alert rules + operator dashboard，而不是长期停留在手工 curl/runbook 阶段

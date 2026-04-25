# CEX Alert Rules Draft v1

## Purpose

这份文档把当前仓库里**已经真实暴露出来**的最小 signal，整理成可执行的告警草案。

当前还不是完整 Prometheus / Alertmanager 正式规则库，而是先定义：

- 看哪些字段
- 什么时候算触发
- 严重级别怎么分
- 第一反应该做什么

配套排障动作请同时看：`docs/operator-runbook-v1.md`

---

## 1. Source endpoints

### gateway

- `GET http://127.0.0.1:8080/v1/info`

主要字段：

- `metrics.invocation_create_upstream_failures`
- `operator_signals.invocation_create_upstream_failures`

### execution

- `GET http://127.0.0.1:7003/v1/info`
- `GET http://127.0.0.1:7003/v1/executions/worker-queue/summary`

主要字段：

- `operator_signals.approval_backlog`
- `operator_signals.queued_worker_lease_expired`
- `operator_signals.queued_worker_retry_budget_exhausted`
- `operator_signals.audit_failures`
- `operator_signals.refund_failures`
- `runtime.awaiting_approval`
- `runtime.queued_worker.*`

### product edge (supporting signals)

- `GET http://127.0.0.1:8090/health`
- `GET http://127.0.0.1:8091/health`

这些目前更适合作为辅助判断，不作为主 alert source。

不过当前 `./scripts/check-operator-signals.sh` 已经会把这两条 supporting surface 也并进统一结果里：

- `/health` 可达且 `status=ok` 时，会把关键配置与 metrics 摘要放进 `supporting.consumer_entry` / `supporting.matrix_entry`
- 对 consumer-entry，当前还会把 `identity_governance_overview`、`session_auth_issuer_registry_governance_overview`、`profile_validation.checks.identity_governance_valid` 与 `profile_validation.checks.session_auth_issuer_registry_governance_valid` 一并带进 `supporting.consumer_entry`
- 对 matrix-entry，当前还会把 `consumer_entry_session_auth_governance_overview`、`profile_validation.checks.consumer_entry_session_auth_governance_valid` 与 signer `selection` 摘要一并带进 `supporting.matrix_entry`
- `/health` 不可达时，会产生 `consumer_entry:endpoint_unreachable` 或 `matrix_entry:endpoint_unreachable`，严重级别暂定为 `warn`
- `/health.status != ok` 时，会产生 `health_status_not_ok` 告警，严重级别暂定为 `warn`
- 同时会把一小组入口层 signals 提升成真正的 operator alerts：
  - `consumer_entry:rate_limited_requests`
  - `consumer_entry:ingress_auth_failures`
  - `consumer_entry:identity_governance_invalid`
  - `consumer_entry:identity_binding_not_loaded`
  - `consumer_entry:identity_registry_not_loaded`
  - `consumer_entry:identity_ref_integrity_not_ok`
  - `consumer_entry:identity_actor_gate_invalid`
  - `consumer_entry:identity_approval_source_invalid`
  - `consumer_entry:identity_approval_coverage_invalid`
  - `consumer_entry:session_auth_issuer_registry_governance_invalid`
  - `consumer_entry:session_auth_issuer_registry_not_loaded`
  - `consumer_entry:session_auth_issuer_registry_revision_missing`
  - `consumer_entry:session_auth_issuer_registry_actor_gate_invalid`
  - `consumer_entry:session_auth_issuer_registry_approval_source_invalid`
  - `consumer_entry:session_auth_issuer_registry_approval_coverage_invalid`
  - `matrix_entry:rate_limited_requests`
  - `matrix_entry:ingress_auth_failures`
  - `matrix_entry:duplicate_events`
  - `matrix_entry:consumer_entry_session_auth_governance_invalid`
  - `matrix_entry:consumer_entry_session_auth_selection_invalid`
  - `matrix_entry:consumer_entry_session_auth_issuer_registry_not_loaded`
  - `matrix_entry:consumer_entry_session_auth_issuer_registry_revision_missing`
  - `matrix_entry:consumer_entry_session_auth_approval_source_invalid`
  - `matrix_entry:consumer_entry_session_auth_approval_coverage_invalid`

当前这些入口层 promoted signals 的默认阈值来源是：

- `ALERT_CONSUMER_ENTRY_RATE_LIMITED_THRESHOLD=20`
- `ALERT_CONSUMER_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD=5`
- `ALERT_MATRIX_ENTRY_RATE_LIMITED_THRESHOLD=20`
- `ALERT_MATRIX_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD=5`
- `ALERT_MATRIX_ENTRY_DUPLICATE_EVENT_THRESHOLD=25`

---

## 2. Alert rules

## A1. Gateway upstream failures

### Condition

触发当：

- `gateway.operator_signals.invocation_create_upstream_failures.alert == true`

当前阈值来源：

- `ALERT_GATEWAY_UPSTREAM_FAILURE_THRESHOLD`

默认理解：

- 只要开始累计就值得关注，默认阈值为 `1`

### Severity

- 默认：`high`
- 如果 `gateway /health` 正常但该 signal 持续增长：`high`
- 如果同时 `execution /health` 不通：`critical`

### First action

1. 读 `GET /v1/info` on gateway
2. 读 `GET /v1/info` on execution
3. 判断是 execution 整体挂了，还是 execution 后半段（refund/audit/provider）降级

### Likely causes

- execution-service 不可用
- ledger / audit 降级把 execution 拉成 BAD_GATEWAY
- provider handoff / worker 执行异常

---

## A2. Approval backlog too high

### Condition

触发当：

- `execution.operator_signals.approval_backlog.alert == true`

阈值来源：

- `ALERT_APPROVAL_BACKLOG_THRESHOLD`

### Severity

- 默认：`medium`
- 如果 backlog 连续 15 分钟不下降，或明显影响用户 SLA：`high`

### First action

1. 看 `runtime.awaiting_approval`
2. 看 `policy.*` 最近是否调严
3. 看 approve 流是否实际有人处理

### Likely causes

- policy 关键词 / capability prefix 过宽
- 审批流程没跟上
- product surface 缺审批入口

---

## A3. Worker lease expired churn

### Condition

触发当：

- `execution.operator_signals.queued_worker_lease_expired.alert == true`

阈值来源：

- `ALERT_LEASE_EXPIRED_THRESHOLD`

### Severity

- 默认：`high`
- 如果同时 `runtime.queued_worker.active_workers > 0` 且 `lease_expired` 持续升：`high`
- 如果用户任务明显堆积：`critical`

### First action

1. 看 `runtime.queued_worker.active_workers`
2. 看 `runtime.queued_worker.claimable`
3. 看 worker 是“没人消费”还是“消费了但没完成/没续租”

### Likely causes

- worker crash / hang
- provider 延迟太长
- 续租逻辑缺失或失效

---

## A4. Retry budget exhausted

### Condition

触发当：

- `execution.operator_signals.queued_worker_retry_budget_exhausted.alert == true`

阈值来源：

- `ALERT_RETRY_BUDGET_EXHAUSTED_THRESHOLD`

### Severity

- 默认：`high`
- 如果上升很快，且 backlog 同时扩大：`critical`

### First action

1. 抽样失败 execution
2. 判断是 provider 故障、输入坏数据，还是 worker bug
3. 不要直接批量 requeue

### Likely causes

- provider 一直返回失败
- 某类 capability 输入系统性坏掉
- worker 逻辑在重复消费同类坏任务

---

## A4b. Provider dead-letter / retry budget exhausted

### Condition

触发当：

- `execution.operator_signals.provider_dead_letters.alert == true`
- 或 `execution.operator_signals.provider_retry_budget_exhausted.alert == true`

阈值来源：

- `ALERT_PROVIDER_DEAD_LETTER_THRESHOLD`
- `ALERT_PROVIDER_RETRY_BUDGET_EXHAUSTED_THRESHOLD`

### Severity

- 默认：`critical`

这表示 provider-backed execution 已进入不应盲目自动重试的终态：要么是 billing/auth/unknown 这类不可重试失败，要么是 timeout/rate-limit/unavailable 在耗尽 attempt budget 后仍失败。

### First action

1. 看 `runtime.provider_failures`
2. 先区分 `non_retryable_terminal` 与 `retry_budget_exhausted`
3. 对 billing/auth 类先处理 provider/key/余额，不要批量 retry
4. 对 timeout/unavailable 类先看 provider 健康与 OpenClaw bridge 日志，再决定是否切 provider 或手动 retry

### Likely causes

- provider 余额不足或 key/auth 错误
- provider 持续 timeout / unavailable
- 某个 provider bridge 出口退化
- retry/backoff 已尽但上游仍未恢复

---

## A5. Audit write failures

### Condition

触发当：

- `execution.operator_signals.audit_failures.alert == true`

阈值来源：

- `ALERT_AUDIT_FAILURE_THRESHOLD`

### Severity

- 默认：`high`
- 如果主链仍在继续写业务状态但 audit 全失败：`high`
- 如果同时 execution 主链也不稳定：`critical`

### First action

1. 看 `GET http://127.0.0.1:7004/health`
2. 核对 `AUDIT_BASE_URL`
3. 记录为审计降级 incident

### Likely causes

- audit-service down
- audit URL 漂移
- 网络/路由问题

---

## A6. Refund failures

### Condition

触发当：

- `execution.operator_signals.refund_failures.alert == true`

阈值来源：

- `ALERT_REFUND_FAILURE_THRESHOLD`

### Severity

- 默认：`critical`

因为这直接碰到账务一致性。

### First action

1. 看 `GET http://127.0.0.1:7002/health`
2. 核对 ledger manage token
3. 抽样核对 invocation / execution / ledger reserve-refund 状态

### Likely causes

- ledger-service 不可用
- token / admin auth 问题
- refund request 参数或幂等链路异常

---

---

## A7. Consumer-entry identity governance degraded

### Condition

触发当：

- `consumer_entry:identity_governance_invalid`
- 或更细粒度的：
  - `consumer_entry:identity_binding_not_loaded`
  - `consumer_entry:identity_registry_not_loaded`
  - `consumer_entry:identity_ref_integrity_not_ok`
  - `consumer_entry:identity_actor_gate_invalid`
  - `consumer_entry:identity_approval_source_invalid`
  - `consumer_entry:identity_approval_coverage_invalid`

这些 signal 当前由 `./scripts/check-operator-signals.sh` 从 `consumer-entry-api /health` 的 `identity_governance_overview` 与 `session_auth_issuer_registry_governance_overview` 直接提炼，不需要额外调 admin endpoint。

### Severity

- 默认：`high`
- 如果只是 actor gate / approval source / approval coverage 缺口，且当前入口流量仍正常：`high`
- 如果已经出现 `binding_loaded=false`、`registry_loaded=false` 或 `ref_integrity_ok=false`，应视为更接近“身份治理面失守”，按 `critical incident candidate` 处理

### First action

1. 看 `GET http://127.0.0.1:8090/health | jq '.identity_governance_overview'`
2. 看失败的是哪一类 check：binding / registry / ref-integrity / actor / approval source / approval coverage
3. 若需要更细节，再看：
   - `GET /v1/admin/identity-governance/status?limit=20`
   - `GET /v1/admin/identity-approval/source?limit=20`
   - `GET /v1/admin/identity-actors/status`
   - `GET /v1/admin/identity-registry/status`
4. 不要上来就 reload/rollback，先判断是“当前 live state 坏了”还是“governance policy 本身没满足”

### Likely causes

- binding 文件未加载或路径漂移
- separate registry 文件未加载
- `product_user_id` 引用断裂，导致 ref-integrity 失效
- actor allowlist/header 配置不完整
- approved revisions source 为空、未加载或当前 effective revision 未被批准

## 3. Correlation rules

这些不是单条 alert，而是组合判断。

### C1. Gateway upstream failures + execution refund failures

含义：

- 更像 execution 后半段资金/退款链路降级
- 不是纯入口问题

优先级：

- 先看 ledger / refund，再看 gateway

通知编排建议：

- 这类组合很适合放进 `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON`，作为一个 batch/correlation rule 单独路由，而不是只按其中某一条 signal 或 `critical` 级别分流
- 如果不想因为一次瞬时抖动就升级 incident，可再给该 rule 加 `minOccurrences` / `minActiveSeconds`；若要表达“5 分钟内出现 3 次”或“间隔太久就不算同一波”，可继续叠加 `occurrenceWindowSeconds` / `maxGapSeconds`；若想表达“这一整类 incident 在 10 分钟里累计出现 3 次”，可给这组规则共享 `groupKey` 并设置 `groupMinOccurrences` / `groupOccurrenceWindowSeconds`；若这个 family 进入 incident 后还持续恶化，可继续加 `groupEscalateAfterOccurrences` / `groupEscalateAfterSeconds` / `groupEscalationCommand` / `groupEscalationRoute`；若该 rule 只应在某些排除信号不存在时成立，可加 `matchNone`；若它本身就比其他 incident 更重要，可显式给更高 `priority`；若它只是更宽泛的兜底 incident，还可加 `suppressedByPolicies`，避免在更具体 policy 已成立时继续抢 route；若想让某个 family 整体压制另一个 family，还可加 `suppressedByGroups`；若它持续太久后需要升级到更重通知链路，还可加 `escalateAfterOccurrences` / `escalateAfterSeconds` / `escalationCommand` / `escalationRoute`

### C2. Gateway upstream failures + execution audit failures

含义：

- 更像 execution 还能跑一部分，但审计面已经坏了
- 需要判断是否进入“允许降级继续跑”状态

优先级：

- 先看 audit-service，再评估是否暂停高风险操作

### C3. Approval backlog high + worker queue healthy

含义：

- 更像 policy/operator bottleneck
- 不是 worker 故障

优先级：

- 先看 policy 和审批流，不要误判成消费端故障

### C4. Lease expired high + active_workers > 0

含义：

- worker 不是没起，而是起了但处理不稳

优先级：

- 先查 worker/provider latency，不要只补 worker 数量

### C5. Consumer-entry identity governance degraded + ingress auth healthy

含义：

- 更像 entry 身份治理配置/控制面问题
- 不是简单的入口 token/auth 问题

优先级：

- 先查 `identity_governance_overview` 与各 focused admin surface
- 不要误判成 matrix/consumer ingress 整体不可用

---

## 4. Suggested severity mapping

- `medium`
  - approval backlog 单独越线
- `high`
  - gateway upstream failures
  - lease expired churn
  - retry budget exhausted
  - audit failures
- `critical`
  - refund failures
  - gateway upstream failures + execution health fail
  - retry budget exhausted 快速上升且 backlog 扩大
  - consumer-entry identity governance degraded 且同时出现 `binding_loaded=false` / `registry_loaded=false` / `ref_integrity_ok=false`

---

## 5. Suggested first delivery channels

在正式监控体系之前，建议先用这三种最小方式：

1. 本地/内网 dashboard 轮询 `/v1/info`
2. 直接跑 `./scripts/check-operator-signals.sh`，按 exit code 做 `ok/warn/critical` 分流
3. 用 `./scripts/run-operator-signal-check.sh` 做 cron-ready 包装，落盘最近结果并在 `warn/critical` 时触发通知命令
4. 通过默认开启的 `OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY=1` 抑制 unchanged repeat alerts，避免 cron 每轮重复刷同一状态
5. 通过默认 `OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS=1800` 给持续 30 分钟未恢复的同态告警补发 reminder
6. 利用 `OPERATOR_SIGNAL_NOTIFY_WARN_COMMAND` / `OPERATOR_SIGNAL_NOTIFY_CRITICAL_COMMAND` 做轻重告警分流，避免 warn 和 critical 走同一条通知路径
7. 若多条 signal 的组合本身更接近 incident，可通过 `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON` 做 batch/correlation routing，它会覆盖 per-signal 与普通 severity 路由；规则还可加 `priority` / `groupKey` / `groupMinOccurrences` / `groupMinActiveSeconds` / `groupOccurrenceWindowSeconds` / `groupMaxGapSeconds` / `groupEscalateAfterOccurrences` / `groupEscalateAfterSeconds` / `groupEscalationCommand` / `groupEscalationRoute` / `matchNone` / `minOccurrences` / `minActiveSeconds` / `occurrenceWindowSeconds` / `maxGapSeconds` / `suppressedByPolicies` / `suppressedByGroups` / `escalateAfterOccurrences` / `escalateAfterSeconds` / `escalationCommand` / `escalationRoute`，避免一次短抖动就直接升级、避免很久以前的一次命中被错误并入当前 incident、把同一家族的多条 policy 累计成一个更稳定的 incident 门槛、在 family 整体持续恶化时统一升级到更重 route、让一个 family 整体压制另一个 family、避免宽泛 policy 抢走更具体 incident 的通知路由、要求“没有某个更严重 signal 时才成立”、显式指定“哪条 incident 更重要”，或让同一 incident 在持续过久后自动升级到更重 route
8. 若某些特定 signal 需要专线处理，可通过 `OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON` 做 per-signal routing，它会覆盖普通 severity 路由
9. 打开 `OPERATOR_SIGNAL_NOTIFY_RECOVERY=1` 并按需配置 `OPERATOR_SIGNAL_NOTIFY_RECOVERY_COMMAND`，让已通知异常在恢复到阈值以下时补发 resolved/recovery 通知
10. 关键 `critical` 项先走 chat/IM 通知

---

## 6. Focused example files

当前仓库已经补了两组 focused example，先把两条最值钱的线落成真正可抄的规则/路由样板：

- consumer-entry identity governance:
  - Prometheus rules:
    - `ops/monitoring/prometheus/consumer-entry-identity-governance-alerts.example.yml`
  - Alertmanager routing/inhibition:
    - `ops/monitoring/alertmanager/consumer-entry-identity-governance-routing.example.yml`
- cross-service core runtime signals (gateway + execution, from the unified wrapper JSON):
  - Prometheus rules:
    - `ops/monitoring/prometheus/core-runtime-operator-signals-from-wrapper.example.yml`
  - Alertmanager routing/inhibition:
    - `ops/monitoring/alertmanager/core-runtime-operator-signals-from-wrapper-routing.example.yml`
- product-edge wrapper-derived signals (consumer-entry + matrix-entry endpoint/abuse line):
  - Prometheus rules:
    - `ops/monitoring/prometheus/product-edge-operator-signals-from-wrapper.example.yml`
  - Alertmanager routing/inhibition:
    - `ops/monitoring/alertmanager/product-edge-operator-signals-from-wrapper-routing.example.yml`
- monitoring deploy wrapper-derived signals (latest deploy verdict / metadata readability line):
  - Prometheus rules:
    - `ops/monitoring/prometheus/monitoring-deploy-operator-signals-from-wrapper.example.yml`
  - Alertmanager routing/inhibition:
    - `ops/monitoring/alertmanager/monitoring-deploy-operator-signals-from-wrapper-routing.example.yml`
- combined starter bundle + machine-readable inventory:
  - Prometheus bundle:
    - `ops/monitoring/prometheus/minimal-wrapper-monitoring-bundle.example.yml`
  - Alertmanager bundle:
    - `ops/monitoring/alertmanager/minimal-wrapper-monitoring-bundle.example.yml`
  - Manifest:
    - `ops/monitoring/monitoring-bundle-manifest.example.yml`
  - Rebuild/check helper:
    - `scripts/assemble-monitoring-bundles.sh`
    - `scripts/assemble-monitoring-bundles.sh --check`
  - Export helper:
    - `scripts/export-monitoring-bundles.sh`
    - `scripts/export-monitoring-bundles.sh --output-dir /tmp/cex-monitoring-export --include-focused`
  - Install helper:
    - `scripts/install-monitoring-bundles.sh`
    - `scripts/install-monitoring-bundles.sh --install-dir /tmp/cex-monitoring-install --include-focused`
  - Symlink/overlay helper:
    - `scripts/overlay-monitoring-bundles.sh`
    - `scripts/overlay-monitoring-bundles.sh --target-root /tmp/cex-monitoring-overlay --include-focused`
  - Live-target deploy helper:
    - `scripts/deploy-monitoring-bundles.sh`
    - `scripts/deploy-monitoring-bundles.sh --deploy-root /tmp/cex-monitoring-live-target --include-focused`
    - `scripts/deploy-monitoring-bundles.sh --from-overlay-root /tmp/cex-monitoring-overlay --mode copy`
    - `scripts/deploy-monitoring-bundles.sh --reload --reload-dry-run`
  - Post-deploy reload helper:
    - `scripts/reload-monitoring-targets.sh`
    - `scripts/reload-monitoring-targets.sh --mode command --prometheus-command 'systemctl reload prometheus' --alertmanager-command 'systemctl reload alertmanager'`
    - `scripts/reload-monitoring-targets.sh --mode command --failure-policy restart --prometheus-command 'systemctl reload prometheus' --prometheus-restart-command 'systemctl restart prometheus'`
  - Post-deploy health verification helper:
    - `scripts/verify-monitoring-targets.sh`
    - `scripts/verify-monitoring-targets.sh --attempts 5 --delay-secs 2`
    - `scripts/deploy-monitoring-bundles.sh --verify --verify-attempts 5 --verify-delay-secs 2`

这两份 example 当前覆盖 `entry-identity` family，重点是把下面几类信号变成正式 rule syntax：

- generic governance invalid:
  - `cex_consumer_entry_identity_governance_valid == 0`
  - `cex_consumer_entry_session_auth_issuer_registry_governance_valid == 0`
- hard failures / page candidates:
  - `cex_consumer_entry_identity_binding_loaded == 0`
  - `cex_consumer_entry_identity_registry_loaded == 0`
  - `cex_consumer_entry_identity_ref_integrity_ok == 0`
- soft governance gaps / chat candidates:
  - `cex_consumer_entry_identity_actor_gate_valid == 0`
  - `cex_consumer_entry_identity_approval_source_valid == 0`
  - `cex_consumer_entry_identity_approval_coverage_valid == 0`
  - `cex_consumer_entry_session_auth_issuer_registry_actor_gate_valid == 0`
  - `cex_consumer_entry_session_auth_issuer_registry_approval_source_valid == 0`
  - `cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid == 0`

Alertmanager example 里还额外给了最小 inhibition 语义，避免在 `binding_not_loaded` / `registry_not_loaded` / `ref_integrity_broken` 这类更硬的故障已经成立时，再重复打一条泛化的 `identity_governance_invalid`。同一思路现在也适用于 matrix 侧 session-auth signer readiness，优先看更细的 `selection_invalid` / `issuer_registry_not_loaded` / `issuer_registry_revision_missing`，再看泛化的 `consumer_entry_session_auth_governance_invalid`。

除此之外，仓库现在还补了一个 repo-local bridge 脚本：

- `scripts/render-operator-signals-prometheus.sh`

它会把 `./scripts/check-operator-signals.sh --compact` 或 `run/operator-signals/last.json` 的统一 JSON 结果渲染成 Prometheus text exposition，让 gateway / execution 这种目前还没有原生 `/metrics` 的 runtime 也能先挂到 Prometheus / node_exporter textfile collector 一类最小接线里。

这些 example / bridge 的定位是：

- 给 repo-local 自托管环境一个真正的起步模板
- 给后续 Prometheus / Alertmanager 接线提供等价语义参考
- 不是完整生产规则库，也还没覆盖 matrix-entry 与所有 product-edge / policy 组合场景

## 7. What is still missing

这份 alert 草案当前**还不包含**：

- 面向 gateway / execution / matrix-entry / product-edge / monitoring-deploy 全面的 Prometheus rule library（当前只有 focused consumer-entry identity-governance examples + wrapper-derived core runtime/product-edge/monitoring-deploy examples）
- 面向全仓 incident family 的完整 Alertmanager routing / inhibition / silence 设计（当前只有 focused entry-identity + core-runtime + product-edge examples）
- mute / silence policy
- SLO burn-rate rules
- consumer-entry / matrix-entry 更完整的统一告警规则（当前 consumer-entry identity governance 已起步，但还不是完整 rulebook）
- incident owner / escalation chain

所以它现在的定位是：

- **alert design draft**
- 不是最终生产规则库

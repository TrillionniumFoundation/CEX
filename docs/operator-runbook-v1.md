# CEX Operator Runbook v1

## Scope

这份 runbook 只覆盖当前仓库里**已经真实可观测**的最小运行面，主要针对：

- `gateway-service`
- `execution-service`
- `consumer-entry-api`
- `matrix-entry-adapter`

目标不是替代完整 on-call 手册，而是把“信号亮了以后先看什么、怎么缩小范围、下一步做什么”先固定下来。

---

## 1. First 60 seconds

先不要猜，按这个顺序看：

1. 基础存活
   - `GET http://127.0.0.1:8080/health`
   - `GET http://127.0.0.1:7003/health`
   - `GET http://127.0.0.1:8090/health`
   - `GET http://127.0.0.1:8091/health`
2. 核心运行快照
   - `GET http://127.0.0.1:8080/v1/info`
   - `GET http://127.0.0.1:7003/v1/info`
3. 如果是入口侧问题，再看：
   - `GET http://127.0.0.1:8090/health`
   - `GET http://127.0.0.1:8091/health`
4. 如果是 worker / approval / refund 问题，再看：
   - `GET http://127.0.0.1:7003/v1/executions/worker-queue/summary`

---

## 2. What each info endpoint means

### 2.1 `gateway-service` → `GET /v1/info`

当前主要看两块：

- `metrics`
  - `invocation_create_requests`
  - `invocation_create_auth_failures`
  - `invocation_create_capability_failures`
  - `invocation_create_upstream_failures`
- `operator_signals`
  - `invocation_create_upstream_failures`

#### How to read it

- `auth_failures` 上升
  - 先怀疑 API key / Bearer token 问题
  - 再查 identity-service 或调用方配置
- `capability_failures` 上升
  - 先怀疑 capability id 配错、被禁用、或 capability-service 不可用
- `upstream_failures` 上升
  - 说明 gateway 创建 invocation 后，下游执行链给了 BAD_GATEWAY 级失败
  - 优先继续看 `execution-service /v1/info`

### 2.2 `execution-service` → `GET /v1/info`

当前主要看三块：

- `policy`
  - 当前生效的 approval/block 关键词、capability prefix、reserve 阈值
- `metrics`
  - create / blocked / awaiting approval / auto approved
  - approve / reject / claim / retry / cancel
  - `audit_failures`
  - `refund_failures`
- `runtime`
  - 各 execution status 总量
  - queued-worker backlog 摘要
- `operator_signals`
  - `approval_backlog`
  - `queued_worker_lease_expired`
  - `queued_worker_retry_budget_exhausted`
  - `audit_failures`
  - `refund_failures`

#### How to read it

- `approval_backlog.alert=true`
  - 说明 pending approval 堆积已超过阈值
  - 下一步先确认是否真的缺审批人，还是 policy 过严
- `queued_worker_lease_expired.alert=true`
  - 说明 worker claim 后有一批 lease 过期
  - 优先怀疑 worker 不稳定、处理超时、或没有续租
- `queued_worker_retry_budget_exhausted.alert=true`
  - 说明一批任务已耗尽 retry budget
  - 优先检查 provider 故障、输入坏数据、或 worker bug
- `audit_failures.alert=true`
  - 审计写入失败开始累计
  - 先看 audit-service 存活，再看网络/地址配置
- `refund_failures.alert=true`
  - refund 调 ledger 失败开始累计
  - 先看 ledger-service，再看 admin token / ledger 状态 / DB 一致性

---

## 3. Triage by symptom

### 3.1 Symptom: 用户请求进来了，但 gateway 大量 BAD_GATEWAY

先看：

- `GET http://127.0.0.1:8080/v1/info`
- `GET http://127.0.0.1:7003/v1/info`

判断：

- 如果 gateway 的 `invocation_create_upstream_failures` 上升，同时 execution `/health` 正常
  - 看 execution `operator_signals`
  - 如果 `refund_failures` 或 `audit_failures` 也在升，基本不是入口问题，而是 execution 后半段降级
- 如果 execution `/health` 本身不通
  - 属于 execution 整体不可用
  - gateway BAD_GATEWAY 只是表象

建议动作：

1. 先确认 execution-service 进程是否活着
2. 再确认 audit / ledger 是否活着
3. 必要时重启 execution 及其依赖，而不是只重启 gateway

### 3.2 Symptom: 大量任务卡在 awaiting approval

先看：

- `GET http://127.0.0.1:7003/v1/info`

重点字段：

- `runtime.awaiting_approval`
- `operator_signals.approval_backlog`
- `policy.*`

判断：

- 如果 backlog 突然上升，同时 policy 最近刚改
  - 优先怀疑 policy 太严或关键词过宽
- 如果 backlog 稳定高位但 approve 请求几乎不增长
  - 更像 operator 流程没跟上，不是系统故障

建议动作：

1. 看是否需要临时放宽 policy keyword / prefix
2. 看是否需要补审批人或补 product-facing approval surface
3. 不要直接把 approval backlog 当 worker 故障

### 3.3 Symptom: queued-worker 队列堆积或频繁超时

先看：

- `GET http://127.0.0.1:7003/v1/info`
- `GET http://127.0.0.1:7003/v1/executions/worker-queue/summary`

重点字段：

- `runtime.queued_worker.total`
- `runtime.queued_worker.claimable`
- `runtime.queued_worker.lease_expired`
- `runtime.queued_worker.retry_budget_exhausted`
- `runtime.queued_worker.active_workers`

判断：

- `claimable` 高但 `active_workers=0`
  - 没 worker 在消费
- `lease_expired` 高
  - worker 在 claim，但没处理完或没续租
- `retry_budget_exhausted` 高
  - 不是简单 backlog，而是失败循环已经跑穿 budget

建议动作：

1. 先判断是“没人消费”还是“消费失败”
2. 若 `lease_expired` 高，优先看 worker 稳定性与 provider latency
3. 若 `retry_budget_exhausted` 高，优先抽样失败 execution，而不是盲目 requeue

### 3.4 Symptom: refund 失败

先看：

- `GET http://127.0.0.1:7003/v1/info`
- `GET http://127.0.0.1:7002/health`

重点字段：

- `metrics.refund_failures`
- `operator_signals.refund_failures`

判断：

- 如果 refund failure 在升，而 ledger health 不通
  - 大概率 ledger-service 不可用
- 如果 ledger health 正常但 refund failure 继续升
  - 优先怀疑 admin token、请求参数、DB 状态或幂等链路

建议动作：

1. 先确认 ledger-service 存活
2. 再确认 execution 使用的 ledger manage token 配置
3. 再抽样失败 invocation / execution 核对 reserve/refund 状态是否已漂移

### 3.5 Symptom: audit 写失败

先看：

- `GET http://127.0.0.1:7003/v1/info`
- `GET http://127.0.0.1:7004/health`

重点字段：

- `metrics.audit_failures`
- `operator_signals.audit_failures`

判断：

- audit failure 升，不代表 execution 主链一定停了
- 但说明审计完整性已经开始降级

建议动作：

1. 先确认 audit-service 存活
2. 再确认 `AUDIT_BASE_URL` 是否漂移
3. 若主链仍需继续跑，至少要把这次降级记成 incident，而不是当作无害 warning

### 3.6 Symptom: consumer-entry identity governance invalid

先看：

- `GET http://127.0.0.1:8090/health`
- `GET http://127.0.0.1:8090/health | jq '.identity_governance_overview'`

重点字段：

- `/health.identity_governance_overview.status`
- `/health.identity_governance_overview.valid`
- `/health.identity_governance_overview.checks.*`
- `/health.profile_validation.checks.identity_governance_valid`

判断：

- `binding_loaded=false`：先怀疑 binding 文件没加载、路径漂移、或 live store 未成功初始化
- `registry_loaded=false`：若启用了 separate registry，优先怀疑 registry 文件未加载或加载失败
- `ref_integrity_ok=false`：说明当前 live binding/registry 组合里已经存在断裂的 `product_user_id` 引用，应按高优先级配置事故处理
- `actor_gate_valid=false`：更像 actor header / allowlist 配置缺失，不一定立刻影响现有流量，但说明治理面没达到预期护栏
- `approval_source_valid=false` 或 `approval_coverage_valid=false`：说明 approved revision source 本身或当前 effective revision 覆盖不满足治理要求

建议动作：

1. 先确认失败的是哪一类 check，而不是直接 reload/rollback
2. 需要更细时继续看：
   - `GET /v1/admin/identity-governance/status?limit=20`
   - `GET /v1/admin/identity-registry/status`
   - `GET /v1/admin/identity-actors/status`
   - `GET /v1/admin/identity-approval/status`
   - `GET /v1/admin/identity-approval/source?limit=20`
3. 若是 `ref_integrity_ok=false`，优先修 registry/binding 引用，不要先动 approval / actor gate
4. 若只是 actor/approval 失效，而 live resolution 仍正常，可按治理缺口处理，不必把它误判成入口服务宕机

### 3.7 Symptom: identity binding reload 失败或无审计写回

先看：

- `GET http://127.0.0.1:8090/health`
- `POST http://127.0.0.1:8090/v1/admin/identity-bindings/reload`（注意加上 `x-entry-token`，按需加 actor header）

重点字段：

- `/health` 下的 `identity_binding_audit`：`last_status` / `path` / `last_error`
- 重载响应的 `identity_binding_audit.last_status`
- 重载响应的 `identity_binding_reload_governance.actor_*`（如启用 actor gate）

判断：

- `identity_binding_audit.last_status=write_error|serialize_error`：说明审核日志链路有问题，先检查文件路径与磁盘权限，不要误以为 reload 没生效
- `identity_binding_audit.last_status=disabled`：通常是未配审计路径或未启用审计时的正常状态；若本应有审计链则视为配置缺失
- `reload` 仍返回 409/403：先看 `identity_binding_reload_governance.reason`（如 `same_revision_rejected`/`actor_not_authorized`），再决定是否调整配置再试

建议动作：

1. 先核对 `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH` 是否可写
2. 再在本地先手工触发一次 `POST /v1/admin/identity-bindings/reload`（可配 `..._REQUIRE_ACTOR`）
3. 若只想验证治理状态，不必盲目回滚：重点看 `identity_binding_reload_governance.reason` 与 `identity_binding_audit.last_status`，确认控制面的决策路径再修复配置

### 3.8 Symptom: session-auth registry 想切 live revision，但不想重启 consumer / matrix

先看：

- `GET http://127.0.0.1:8090/v1/admin/session-auth/issuer-registry/status`
- `GET http://127.0.0.1:8091/v1/admin/consumer-entry-session-auth/status`
- 或直接跑：
  - `./scripts/reload-session-auth-runtime.sh --action status --service both`
  - `./scripts/reload-session-auth-runtime.sh --action validate --service both`

重点字段：

- consumer 侧 live verifier 的 `session_auth_issuer_registry.metadata.revision`
- matrix 侧 live signer 的 `consumer_entry_session_auth.issuer_registry_metadata.revision`
- 两边各自的 `selection` / `governance` / `approval` 块
- 协调脚本输出里的 `results[].statusCode`、`results[].body.status`、`failedServices[]`
- `postStatus.consumerLiveRevision` / `postStatus.matrixLiveRevision` / `postStatus.liveRevisionMatch`

判断：

- `validate` 某一边返回 `409`，先看那一边的 `status` 与 `governance.status`
  - 常见是 `current_revision_not_approved`
  - 或 registry `read_error` / `parse_error` / `invalid_active_key`
- matrix selection 不是 `ok`，但 consumer verifier 已经可 reload
  - 说明 live signer 还不能安全切新 revision，先修 matrix 侧 registry / active key / approval
- 两边都可 validate，但 reload 时仍失败
  - 优先确认 admin token 是否分别带到了 `consumer` 与 `matrix`

建议动作：

1. 先跑 `./scripts/reload-session-auth-runtime.sh --action validate --service both --compact`
2. 真正切换时用 `--action reload --service both`
3. 保持 **consumer first, matrix second** 的顺序，不要让 matrix signer 先签发新 key，而 consumer verifier 还停在旧 revision
4. reload 返回后先看同一份结果里的 `postStatus.liveRevisionMatch`；若不是 `true`，按 split-brain 风险处理
5. 如需人工复核，再跑一次 `--action status --service both`，确认两边 live revision 已一致
6. 如果这次变更本身来自 repo-local candidate 文件 promotion，优先改用 repo-root one-liner：
   - `./activate-session-auth-runtime.sh --candidate-file <name-or-path>`
   默认目录契约是：
   - `run/local-runtime/session-auth-candidates/<candidate>`
   - `run/local-runtime/session-auth-issuer-registry.json`
   - `run/local-runtime/session-auth-issuer-registry-approved-revisions.json`
   - `run/session-auth-runtime-backups/`
   - `run/session-auth-runtime-activation/last.json`
   所以传 basename 时，会先到 candidate dir 里找；若默认 approved-revisions 文件存在，也会自动纳入 precheck
7. 若想确认当前脚本会解析到哪些默认路径，可先跑：
   - `./activate-session-auth-runtime.sh --print-defaults`
8. 若激活之后需要显式回滚，而不是只依赖激活失败时的自动回滚，可直接跑：
   - `./rollback-session-auth-runtime.sh`
   - `./rollback-session-auth-runtime.sh --backup-file <backup>`
   它会把选中的 backup 恢复到 live file，然后再次执行 coordinated reload
9. 这两类动作现在都会追加写入：
   - `run/session-auth-runtime-history/history.jsonl`
   若想快速看最近几次 activation / rollback，可直接跑：
   - `./read-session-auth-runtime-history.sh`
   - `./read-session-auth-runtime-history.sh --latest`
   - `./read-session-auth-runtime-history.sh --action activation --status activated --limit 5`
   若想直接看 operator 级 summary / guard，可跑：
   - `./read-session-auth-runtime-history.sh --summary`
   - `./read-session-auth-runtime-history.sh --summary --compact`
   - `./read-session-auth-runtime-history.sh --summary --require-latest-converged`
10. 如果不想记多个 helper，现在也可以统一走：
   - `./session-auth-runtime.sh status ...`
   - `./session-auth-runtime.sh validate ...`
   - `./session-auth-runtime.sh reload ...`
   - `./session-auth-runtime.sh activate ...`
   - `./session-auth-runtime.sh rollback ...`
   - `./session-auth-runtime.sh history --summary`
   - `./session-auth-runtime.sh last --require-converged`
   - `./session-auth-runtime.sh --examples`
   - `./session-auth-runtime.sh --print-run-command`
   - `./session-auth-runtime.sh --help-json`
   - `./session-auth-runtime.sh --summary-json`
   - `./session-auth-runtime.sh --summary-compact`
   - `./session-auth-runtime.sh --summary-field overall.surfaceCount`
   - `./session-auth-runtime.sh --schema`
   - help-json / summary-json 里现在还能直接发现 `catalogEntry`、`metaDiscoverability`、`surfaceCapabilities`、`surfaceProfiles`、`consumerProfiles`、`profileSelectionGuide`、`profileSelectionTrace`、`lifecycle`、`maturity`、`stabilityPolicy`、`summarySurfaceGuide`、`summarySurfaceGuideContractPath`、`recommendedConsumptionContractPath`
   - `lifecycle` / `maturity` 会直接告诉你当前自描述层的阶段、演进方式，以及哪些 surface family 已可按 stable 对待
   - `compatibilityPolicy` 会直接说明 additive 兼容约束、stable anchors，以及 deprecation 应如何先加 replacement 再进入移除窗口
   - `contractGovernance` 会把 `lifecycle`、`maturity`、`compatibilityPolicy`、`stabilityPolicy` 汇总成一个顶层治理视图，适合先读总规则再往下消费具体 path
   - `surfaceLifecycleMatrix` / `contractStatusMatrix` 会再给你一眼能看完的矩阵视图，分别回答“每个 surface 现在算 stable 还是 best-effort”以及“每个顶层 contract 当前由谁治理、是否是 preferred 读法”
   - `./session-auth-runtime.sh --status-json`
   - `./session-auth-runtime.sh --status-compact`
   - `./session-auth-runtime.sh --status-field overall.lastKnownStatus`
   - `./session-auth-runtime.sh --doctor`
   - `./session-auth-runtime.sh --doctor-json`
   - `./session-auth-runtime.sh --doctor-compact`
   - `./session-auth-runtime.sh --doctor-field overall.status`
11. 事后看最新激活结果时，不必手工翻原始 JSON，可直接用：
   - `./read-session-auth-runtime-activation-status.sh`
   - `./read-session-auth-runtime-activation-status.sh --compact`
   - `./read-session-auth-runtime-activation-status.sh --require-converged`

---

## 4. Restart guidance

### 4.1 Safe restart order for core runtime

如果要做最小影响重启，优先顺序：

1. `audit-service`
2. `ledger-service`
3. `execution-service`
4. `gateway-service`

原因：

- gateway 依赖 execution
- execution 依赖 ledger / audit
- 先重启上游依赖，再重启调用方，状态更干净

### 4.2 When not to restart immediately

先不要急着重启，如果你看到的是：

- `approval_backlog` 高，但系统其他部分正常
- `retry_budget_exhausted` 高，但 health 都正常

这两类更像：

- policy/operator 流程问题
- worker/provider 逻辑问题

不是“重启一下就会好”的故障。

---

## 5. Partial outage playbook

### 5.1 audit degraded

表现：

- `execution-service /health` 还活着
- `audit_failures.alert=true`

处理：

1. 确认 audit-service 健康
2. 记录当前为“审计降级”
3. 避免在这个窗口做高风险变更
4. 恢复后再补核对关键 trace

### 5.2 ledger degraded

表现：

- refund failure 升高
- invocation create / execution fail path 开始异常

处理：

1. 先确认 ledger health
2. 暂停把失败任务继续往前推
3. 恢复 ledger 后再人工检查 reserve/refund 一致性

### 5.3 execution degraded

表现：

- gateway upstream failures 升高
- execution `/health` 不通或 `/v1/info` runtime_error 非空

处理：

1. 先看 execution 进程/依赖
2. 先恢复 execution，再观察 gateway 是否自然恢复
3. 不要只重启 gateway 掩盖问题

---

## 6. Recommended operator commands

```bash
curl -s http://127.0.0.1:8080/health
curl -s http://127.0.0.1:8080/v1/info | jq
curl -s http://127.0.0.1:7003/health
curl -s http://127.0.0.1:7003/v1/info | jq
curl -s http://127.0.0.1:7003/v1/executions/worker-queue/summary | jq
curl -s http://127.0.0.1:7004/health
curl -s http://127.0.0.1:7002/health
curl -s http://127.0.0.1:8090/health
curl -s http://127.0.0.1:8090/health | jq '.identity_governance_overview'
curl -s -H 'x-entry-token: <token>' http://127.0.0.1:8090/v1/admin/identity-governance/status?limit=20 | jq
curl -s -H 'x-entry-token: <token>' http://127.0.0.1:8090/v1/admin/identity-approval/source?limit=20 | jq
curl -s -H 'x-entry-token: <token>' http://127.0.0.1:8090/v1/admin/identity-actors/status | jq
./scripts/check-operator-signals.sh
./scripts/run-operator-signal-check.sh
```

其中 `./scripts/check-operator-signals.sh` 会直接输出 machine-readable JSON，并返回：`0=ok`、`1=warn`、`2=critical`。当前它除 `gateway /v1/info`、`execution /v1/info` 外，也会把 `consumer-entry-api /health` 与 `matrix-entry-adapter /health` 的 supporting surface 一并纳入结果；若这两条入口侧 `/health` 不可达，会以 `warn` 形式反映在 alerts 里。

另外，这个脚本现在还会把一小组入口层 signals 直接提升成 operator alerts：

- consumer-entry: `rate_limited_requests`、`ingress_auth_failures`
- consumer-entry identity governance: `identity_governance_invalid`、`identity_binding_not_loaded`、`identity_registry_not_loaded`、`identity_ref_integrity_not_ok`、`identity_actor_gate_invalid`、`identity_approval_source_invalid`、`identity_approval_coverage_invalid`
- matrix-entry: `rate_limited_requests`、`ingress_auth_failures`、`duplicate_events`

默认阈值可通过 `ALERT_CONSUMER_ENTRY_*` / `ALERT_MATRIX_ENTRY_*` env 调整。

`./scripts/run-operator-signal-check.sh` 则在此基础上增加了几层：

- 持久化最近一次结果到 `run/operator-signals/last.json`
- 可通过 `OPERATOR_SIGNAL_NOTIFY_ON` + `OPERATOR_SIGNAL_NOTIFY_COMMAND` 在 `warn/critical` 时触发外部通知命令
- 可进一步用 `OPERATOR_SIGNAL_NOTIFY_WARN_COMMAND` / `OPERATOR_SIGNAL_NOTIFY_CRITICAL_COMMAND` 做 severity routing，未命中时再回退到通用 command
- 若多条 signal 组合本身就代表一个 incident，可用 `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON` 做 batch/correlation routing，它的优先级高于 per-signal 与 warn/critical 路由；规则支持 `priority` / `groupKey` / `groupMinOccurrences` / `groupMinActiveSeconds` / `groupOccurrenceWindowSeconds` / `groupMaxGapSeconds` / `groupEscalateAfterOccurrences` / `groupEscalateAfterSeconds` / `groupEscalationCommand` / `groupEscalationRoute` / `matchAny` / `matchNone` / `minOccurrences` / `minActiveSeconds` / `occurrenceWindowSeconds` / `maxGapSeconds` / `suppressedByPolicies` / `suppressedByGroups` / `escalateAfterOccurrences` / `escalateAfterSeconds` / `escalationCommand` / `escalationRoute`，可表达“连续出现 N 次”“X 秒内出现 N 次”“同一家族累计 N 次后才升级”“family 持续太久后统一升级到更重通知路径”“family 压 family”“同一 policy 持续过久后改走更重通知路径”“某条 policy 需要排除某些更严重 signal 才成立”“更重要 incident 显式压过其他 incident”以及“更宽泛的 incident 在更具体 incident 已成立时不要再抢 route”。现在结果里还有 `notify.policy_selection_trace`，适合在排障时直接读 winner/loser explain，而不是自己手工拼所有 candidate 字段。
- 若某些具体 signal 需要单独路径，可用 `OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON` 做 per-signal routing，它的优先级高于 warn/critical 路由
- 默认开启 `OPERATOR_SIGNAL_NOTIFY_RECOVERY=1`，异常降到阈值以下时会补发一条 recovery/resolved 通知，可用 `OPERATOR_SIGNAL_NOTIFY_RECOVERY_COMMAND` 单独分流
- 默认开启 `OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY=1`，相同告警状态不会每个 cron tick 都重复通知
- 默认 `OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS=1800`，若同一告警持续 30 分钟未变，会补发 reminder
- 最近一次已发送通知的归一化状态会落到 `run/operator-signals/last-notify-state.json`

如果只想先落一个 repo-local 最小通知模板，可把 `OPERATOR_SIGNAL_NOTIFY_COMMAND` 指向：

```bash
./scripts/notify-operator-signals-example.sh
```

它默认写 `run/operator-signals/notifications.log`，也支持可选 webhook；若要在不同通知面选择不同密度的策略摘要，可配 `OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_LEVEL=ultra_short|short|full`。当前这个 level 不只是切换 summary 行，也会切换整条通知正文的详略程度。现在还额外提供一个单行 `family_brief`，适合直接塞到频道 topic、标题位或值班副标题。full 本地日志现在还会附带一个紧凑的 `family_grouped_alerts:` 多行块，以及一个 `summary_views_json:` 单行机器可读块，方便直接在机器上看 family 归并后的故障分布，或被本机脚本二次消费。若走 webhook JSON，通知模板还会把最终渲染结果直接打包成 `selected_level`、`rendered_text`、`alerts_brief`、`family_brief`、`family_grouped_alerts` 和 `summary` 字段，并额外给出 `render.summary_views` / `summary.summary_views` 统一容器，方便外部 on-call 系统一次性取整套摘要视图。该容器当前带 `version: 1`，便于后续 schema 演进时做兼容判断。当前 `alerts_brief` 已按 critical-first，并按 family/service 聚合压缩，适合作为值班频道或标题位里的 incident headline；默认 family 规则现在还会把 consumer-entry 的 identity/governance 告警单独归到 `entry-identity`，避免和普通 `entry-abuse` / 通用 entry 告警混在一起；如需更贴合你自己的信号命名，可通过 `OPERATOR_SIGNAL_NOTIFY_ALERT_FAMILY_RULES_JSON` 覆盖默认 family 规则。

如果本机 runtime 是脚本启动的，也可配合：

```bash
powershell -ExecutionPolicy Bypass -File scripts/status-local-runtime.ps1
powershell -ExecutionPolicy Bypass -File scripts/start-local-runtime-detached.ps1 -SkipBuild -Restart
```

---

## 7. Current limitations of this runbook

这份 runbook 还是最小版，当前明确还没覆盖：

- Prometheus / OTel exporter
- 自动告警规则
- dashboard
- 真正的 on-call / incident lifecycle
- rollback / DR drill

所以它现在更适合：

- 本地 dev
- internal alpha
- 小范围 beta 预演

还不算完整生产值班手册。

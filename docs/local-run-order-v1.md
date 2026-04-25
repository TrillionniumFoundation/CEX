# Local Run Order v1

## 1. 先启动基础环境

```powershell
docker compose up -d
```

## 2. 先种本地 dev 租户与 API key（推荐）

如果本地 runtime 连着 Postgres，先跑一次：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\seed-local-dev.ps1
```

这一步会补齐本地默认组织、用户，以及 `local-dev-key` 的 DB provenance。

## 3. 再启动服务

建议按顺序启动：

```powershell
cargo run -p ledger-service
cargo run -p execution-service
cargo run -p identity-service
cargo run -p audit-service
cargo run -p capability-service
cargo run -p gateway-service
```

### Matrix C 端链路快速启动（Linux）

如果你在开发 matrix bot 链路（`consumer-entry-api`、`matrix-entry-adapter`、`matrix-bot-relay`），可直接用：

```bash
cd /data/home-data/CEX
./scripts/start-matrix-bot-chain.sh [smoke]
```

默认启动 8090/8091/8092；如果配置了 `MATRIX_ACCESS_TOKEN`，会尝试一并启动 `matrix-bot-poller`。

验收建议可直接跑：

```bash
cd /data/home-data/CEX
./scripts/check-matrix-bot-entry-v1.sh
```

对应标准清单见：

- `docs/matrix-bot-entry-e2e-checklist-v1.md`

移动端命令规范见：

- `docs/matrix-mobile-command-spec-v1.md`


## 4. 为什么这个顺序更合理

因为 gateway-service 现在会主动调用：
- identity-service (`http://127.0.0.1:7001`)
- ledger-service (`http://127.0.0.1:7002`)
- execution-service (`http://127.0.0.1:7003`)
- capability-service (`http://127.0.0.1:7005`，当请求带 `capability_id` 时）

所以如果 ledger/execution 没先起来，gateway 在创建 invocation 时：
- reserve 会失败
- execution 创建会返回空

## 5. 最小测试顺序

### 5.1 创建账户

POST `http://127.0.0.1:7002/v1/accounts`

请求头至少带一个：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 路径：`x-admin-token: ledger-manage-token`
- split-admin 路径：`Authorization: Bearer ledger-manage-token`

如果 ledger principal 配了 `org_ids`，那么 `POST /v1/accounts` 现在只允许创建这些 org 下的账户。

### 5.2 创建 invocation

POST `http://127.0.0.1:8080/v1/invocations`

请求头至少带一个：
- `x-api-key: local-dev-key`
- `Authorization: Bearer local-dev-key`

请求体里可带：
- `account_id`
- `reserve_amount`
- `capability_id`
- `prompt`

如果带了 `capability_id`，gateway 现在会先去 capability registry 校验这个 capability 是否存在且启用。不存在会直接返回 `400 invalid capability id`，已禁用会返回 `400 capability disabled`。

如果该 capability 带真实 provider metadata（当前会跳过本地 `demo` 占位 provider），execution create 响应现在会显式返回 `dispatch_mode`。当前 policy 已开始区分三类：普通/无 provider target 为 `manual`，当前已支持本机同步 handoff 的 `ollama` 为 `immediate`，其余 provider-backed 目标会落到 `queued_worker`。gateway 目前只在 `dispatch_mode=immediate` 时自动触发一次 execution start。对于 `queued_worker`，execution-service 现已提供最小 worker 入口：`GET /v1/executions/worker-queue` 用于观察当前 queued-worker 队列（含 claimable/worker/lease 信息，并支持 `limit`、`claimable_only=true`、`lease_expired_only=true`、`worker_id=<id>` 这些最小过滤参数；当前也会回显 `attempt_count` / `max_attempts` / `attempts_remaining` / `retry_budget_exhausted`），`GET /v1/executions/worker-queue/summary` 用于读取当前 queued-worker 的总量/claimable/expired/active-worker 统计，并新增 `retryable` / `retry_budget_exhausted` 聚合字段，`POST /v1/executions/claim-next` 用于领取下一个 queued-worker execution，并支持 `lease_expired_only=true` 只回收过期租约任务，`POST /v1/executions/claim-batch` 用于按 oldest-first 批量领取多条 queued-worker execution，并支持 `lease_expired_only=true` 只回收过期租约任务，`POST /v1/executions/reclaim-expired` 用于显式 sweep 当前所有已过期租约并把它们放回 `Queued`，但只会回收仍有 retry budget 的项，`POST /v1/executions/timeout-expired` 用于显式把已过期租约任务终止到 `TimedOut`（并在 DB/runtime 路径上复用既有 refund 语义），`POST /v1/executions/:id/requeue` 用于当前 lease holder 主动把已 claim 的 queued-worker 任务放回队列，但 exhausted budget 会被 `409 execution retry budget exhausted` 拦住，`POST /v1/executions/:id/retry` 用于把仍有 retry budget 且未 refund 的 `Failed` / `TimedOut` queued-worker execution 重新放回 `Queued`，`POST /v1/executions/:id/renew-lease` 用于同一 worker 续租，`POST /v1/executions/:id/process` 用于实际处理该 execution。Linux detached runtime 现在会默认拉起 `scripts/execution-queued-worker.sh` 自动 claim/process，并在长调用时周期性续租；如需无后台消费的调试/回归环境，可设置 `CEX_ENABLE_QUEUED_WORKER=0`。当前最小 retry budget 语义已支持 env 覆盖：`EXECUTION_DEFAULT_MAX_ATTEMPTS` 控制非 queued-worker 默认值，`EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS` 控制 queued-worker 默认值，默认仍分别是 `1` 和 `3`。此外 execution policy 也已支持 env 覆盖：`APPROVAL_RESERVE_THRESHOLD` 继续控制审批阈值，`POLICY_HARD_REJECT_RESERVE_THRESHOLD` 可做硬拒阈值，`POLICY_APPROVAL_SENSITIVE_KEYWORDS` / `POLICY_BLOCK_KEYWORDS` 控制 prompt 级规则，`POLICY_APPROVAL_CAPABILITY_PREFIXES` / `POLICY_BLOCK_CAPABILITY_PREFIXES` 控制 capability 前缀规则。被 block 的 execution 会直接返回 `400 execution blocked by policy`，由 gateway 侧走 invocation fail/refund 补偿。成功时 invocation 会直接落到 `Succeeded`；provider 失败时会走 execution 侧的失败/退款语义。provider-backed 任务如果进入不可继续自动重试的终态，可用带 execution admin token 的 `GET /v1/executions/provider-dead-letters` 拉取值班清单；该接口支持 `limit`、`kind=billing|timeout|auth|rate_limited|unavailable|unknown`、`retry_budget_exhausted_only=true`、`non_retryable_only=true`、`include_acknowledged=true`、`acknowledged_only=true` 过滤，并返回 `dead_letter_reason`、ack 状态与截断后的 provider error 摘要，便于从 alert 直接 drill 到具体 execution。确认根因已被外部 incident/人工处理接管后，可用 `POST /v1/executions/:id/provider-dead-letter/ack`（`executions:manage`）把单条 dead-letter 从 active signal 中移出，但这不等于 provider 已恢复；恢复仍要靠 retry/live provider probe 验证。

### 5.2.0 可选：查看 capability registry

GET `http://127.0.0.1:7005/v1/capabilities`

或读取单条：

GET `http://127.0.0.1:7005/v1/capabilities/<capability_id>`

当前本地 dev 默认会带一组最小静态 registry，也支持用 `CAPABILITY_STATIC_REGISTRY_JSON` 注入自定义 capability 列表。

如果要走第一条真实 provider path，当前 execution-service 已接入最小 `ollama` adapter。最简单做法是把 capability 配成类似：`{"capability_id":"cap.ollama.local-chat","kind":"model","provider":"ollama","provider_ref":"qwen2.5:3b","display_name":"Local Ollama Chat","version":"v1","enabled":true}`，并确保 `OLLAMA_BASE_URL` 指向可用的 Ollama 实例。

如需快速确认当前 execution policy 配置已生效，可读：`GET http://127.0.0.1:7003/v1/info`。该接口现在会回显当前 approval/block 关键词、capability prefix 规则、reserve 阈值，以及最小运行计数（如 policy block / awaiting approval / auto-approved / claim / retry / cancel 等）。同时也会附带 runtime backlog 快照，包括各 execution status 计数，以及 queued-worker 的队列深度 / claimable / lease expired / active worker 等摘要。

如果要把它临时当作 operator signal 输入，还可以通过 env 调阈值：`ALERT_APPROVAL_BACKLOG_THRESHOLD`、`ALERT_LEASE_EXPIRED_THRESHOLD`、`ALERT_RETRY_BUDGET_EXHAUSTED_THRESHOLD`、`ALERT_AUDIT_FAILURE_THRESHOLD`、`ALERT_REFUND_FAILURE_THRESHOLD`。`/v1/info` 会直接给出这些信号当前是否越线。

`GET http://127.0.0.1:8080/v1/info` 当前也会附带 gateway 的最小 operator signal，先覆盖 `invocation_create_upstream_failures`，阈值由 `ALERT_GATEWAY_UPSTREAM_FAILURE_THRESHOLD` 控制。

如需直接拿 machine-readable 检查结果，可跑：`./scripts/check-operator-signals.sh`。当前它会汇总 gateway + execution 的 `/v1/info`，并额外纳入 `consumer-entry-api /health` 与 `matrix-entry-adapter /health` 这两条 supporting surface，输出统一 JSON，并按结果返回 exit code：`0=ok`、`1=warn`、`2=critical`。入口层里一小组 metrics 也已被提升成真正的 `warn` signal，阈值通过 `ALERT_CONSUMER_ENTRY_*` / `ALERT_MATRIX_ENTRY_*` env 控制。

如果想把这份统一 JSON 再接到 Prometheus，而不先改 gateway/execution 服务本体，可继续用：`./scripts/render-operator-signals-prometheus.sh`。它支持直接读 `check-operator-signals.sh --compact` 的 stdout，或读 `run/operator-signals/last.json`，并输出 Prometheus text exposition。当前 repo-local 监控样板已分成三层：focused consumer-entry identity rules、wrapper-derived core-runtime rules、wrapper-derived product-edge rules；如果只想先拿“一份 Prometheus + 一份 Alertmanager”起步，可直接从 `ops/monitoring/prometheus/minimal-wrapper-monitoring-bundle.example.yml`、`ops/monitoring/alertmanager/minimal-wrapper-monitoring-bundle.example.yml` 开始，组件清单见 `ops/monitoring/monitoring-bundle-manifest.example.yml`。后续若修改 focused files，可用 `./scripts/assemble-monitoring-bundles.sh` 重新生成 combined bundles，并用 `./scripts/assemble-monitoring-bundles.sh --check` 做 drift 校验；如果要把当前结果直接打包导出到别的目录给 Prometheus/Alertmanager 侧引用，可用 `./scripts/export-monitoring-bundles.sh --output-dir <dir>`，需要把 focused components 一并带走时再加 `--include-focused`；如果要进一步落到 repo-local 安装目录，可用 `./scripts/install-monitoring-bundles.sh --install-dir <dir>`，需要重用已有 export 结果时可再加 `--from-export-dir <dir>`；如果想在另一个根目录上做 symlink-based overlay，可用 `./scripts/overlay-monitoring-bundles.sh --target-root <dir>`，需要复用现成 install layout 时再加 `--from-install-dir <dir>`；如果想直接映射到更接近真实部署的 live-target 目录（如 `prometheus/rules.d` 与 `alertmanager/conf.d`），可用 `./scripts/deploy-monitoring-bundles.sh --deploy-root <dir>`，默认是 symlink mode，需要实体文件时可再加 `--mode copy`，若要复用现成 overlay 则改用 `--from-overlay-root <dir>`；若部署完成后想立刻触发目标系统 reload，可直接在 deploy 时加 `--reload`，或单独运行 `./scripts/reload-monitoring-targets.sh`，默认会走 Prometheus `http://127.0.0.1:9090/-/reload` 与 Alertmanager `http://127.0.0.1:9093/-/reload`，也可切到 `--mode command` 用本地 reload 命令；如果值班策略更偏保守，还可以加 `--failure-policy restart` 并配置 `--prometheus-restart-command` / `--alertmanager-restart-command`，让 reload 失败时再做 service-aware restart fallback。再往前一步，若想确认 deploy/reload 后目标真的恢复健康，可单独跑 `./scripts/verify-monitoring-targets.sh`（默认打 `/-/healthy`），或在 deploy 时直接加 `--verify --verify-attempts <n> --verify-delay-secs <n>`，让 helper 继续做 post-reload health verification。

如需做定时检查而不是一次性人工执行，可跑：`./scripts/run-operator-signal-check.sh`。它会把最新结果写到 `run/operator-signals/last.json`，并可通过 `OPERATOR_SIGNAL_NOTIFY_ON` / `OPERATOR_SIGNAL_NOTIFY_COMMAND` 在 `warn` 或 `critical` 时触发外部命令。若需要轻重告警分流，也可分别配置 `OPERATOR_SIGNAL_NOTIFY_WARN_COMMAND` 与 `OPERATOR_SIGNAL_NOTIFY_CRITICAL_COMMAND`；若需要“恢复已正常”的单独通知路径，也可配置 `OPERATOR_SIGNAL_NOTIFY_RECOVERY_COMMAND`；若需要某个具体信号单独走专线，可配置 `OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON`；若需要多条信号组合成一个 incident 再分流，可配置 `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON`，并可选加 `priority` / `groupKey` / `groupMinOccurrences` / `groupMinActiveSeconds` / `groupOccurrenceWindowSeconds` / `groupMaxGapSeconds` / `groupEscalateAfterOccurrences` / `groupEscalateAfterSeconds` / `groupEscalationCommand` / `groupEscalationRoute` / `matchAny` / `matchNone` / `minOccurrences` / `minActiveSeconds` / `occurrenceWindowSeconds` / `maxGapSeconds` / `suppressedByPolicies` / `suppressedByGroups` / `escalateAfterOccurrences` / `escalateAfterSeconds` / `escalationCommand` / `escalationRoute`，让某条 policy 只在“候选信号出现但排除信号没出现”“同一家族累计足够多次”“family 已持续到值得统一升级通知等级”“没有被另一整个 family 压制”“持续足够久”“最近窗口里出现足够多次”“确实连续出现”“没有被更具体 incident 压制”“priority 更高”或“已经持续到值得升级通知等级”时才切到对应 route。现在最终输出还会带 `notify.policy_selection_trace`，可直接看每个 candidate 的 `decision_summary` / `decision_detail`，同时也会带一个更短的 `notify.policy_summary` 方便直接贴进通知，而且这段摘要会按 warn / critical / recovery / routing_changed / reminder 等场景自动换措辞。进一步地，wrapper 现在还会额外输出 `notify.policy_summary_levels.ultra_short|short|full` 三档版本，通知模板可用 `OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_LEVEL` 选择更短或更完整的摘要。当前默认还会启用 `OPERATOR_SIGNAL_NOTIFY_RECOVERY=1`，表示异常从已通知状态降到阈值以下时会补发 recovery 通知；同时启用 `OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY=1`，避免相同告警状态在每个巡检周期重复通知；并默认 `OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS=1800`，表示同一告警持续 30 分钟仍未恢复时会补发 reminder。当前仓库还提供了两个可直接复用的 routing 样板：`scripts/operator-signal-policy-entry-identity.example.json` 与 `scripts/operator-signal-policy-monitoring-deploy.example.json`；若你只是想先把这条线接起来，优先建议直接设 `OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE=default`。若只想启用单一 family，也可改成 `identity` 或 `deploy`；需要更细控制时，wrapper 也仍兼容 `OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE=baseline` 与 `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(./scripts/render-operator-signal-policy-bundle.sh --profile default)"` 这两层；在 OpenClaw cron helper 上则对应 `-PolicyProfile default`，兼容旧的 `-PolicyBundle baseline`、`-UseEntryIdentityPolicyExample` 与 `-UseMonitoringDeployPolicyExample`。

如果要先用仓库内置模板把通知链接起来，可把 `OPERATOR_SIGNAL_NOTIFY_COMMAND` 指到 `./scripts/notify-operator-signals-example.sh`。该脚本默认写本地 `run/operator-signals/notifications.log`，也支持额外 webhook；若 wrapper 已提供 `notify.policy_summary`，模板会优先把这段更短的人话摘要写进通知内容，并支持通过 `OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_LEVEL=ultra_short|short|full` 选择摘要密度。现在该 level 还会同时控制整条通知正文的密度，例如 `ultra_short` 只保留最关键几行，`full` 则保留完整元数据与逐条 alerts。其 `alerts_brief` 也已改成 critical-first，并会按 family/service 归类成诸如 `refund`、`audit`、`upstream`、`worker`、`entry-abuse`、`monitoring-deploy` 这样的块，更适合当值班摘要标题；若默认 heuristics 不合适，也可通过 `OPERATOR_SIGNAL_NOTIFY_ALERT_FAMILY_RULES_JSON='[{"name":"family","matchAny":["regex1","regex2"]}]'` 覆盖。现在还新增了单行 `family_brief`，适合作为标题/副标题/频道 topic；而 full 本地日志里也会额外落一个紧凑的 `family_grouped_alerts:` 多行块，以及可直接 grep/解析的 `summary_views_json:` 单行块，方便在机器上直接排障。若 webhook 走 `json` 模式，payload 里还会直接带 `selected_level`、`rendered_text`、`alerts_brief`、`family_brief`、结构化 `family_grouped_alerts` 和 `summary`，并新增稳定容器 `render.summary_views` / `summary.summary_views`，外部系统无需再自己拼这些视图字段；该容器现在带 `version: 1`，便于后续演进。字段最小 schema 示例可直接参考 `docs/openclaw-operator-signal-cron-v1.md` 中的 `summary_views` 小节。

如需看 gateway 入口侧的最小运行计数，可读：`GET http://127.0.0.1:8080/v1/info`。当前会回显 invocation create/get、auth failure、capability failure，以及 approve/retry/cancel 入口请求计数。

### 5.2.1 可选：读取 invocation 当前状态

GET `http://127.0.0.1:8080/v1/invocations/<invocation_id>`

请求头至少带一个：
- `x-api-key: local-dev-key`
- `Authorization: Bearer local-dev-key`

gateway 会先解析 API key，再按 invocation 持久化的 `request.org_id` 做 org boundary。不同 org 的 API key 现在不能再直接读别人的 invocation。

如果 invocation 已经绑定 `execution_id`，gateway 现在还会附带一个 execution snapshot，至少包含：
- `dispatch_mode`
- `attempt_count`
- `max_attempts`
- `attempts_remaining`
- `retry_budget_exhausted`

这份 snapshot 现在也会缓存到 invocation 持久化层；因此即使 execution-service 临时不可达，gateway 读 invocation 时仍能先返回最近一次缓存的 execution metadata，再尽量做 live refresh。

### 5.3 可选：发一个新的 DB-backed API key

POST `http://127.0.0.1:7001/v1/api-keys`

请求头至少带一个：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 路径：`x-admin-token: identity-manage-token`
- split-admin 路径：`Authorization: Bearer identity-manage-token`

当前本地 dev 默认仍兼容单个 `IDENTITY_ADMIN_TOKEN`，但更正式的配置入口已升级为 `IDENTITY_ADMIN_TOKENS_JSON`，可以给不同管理 token 分配不同 `actor_id / actor_label / scopes`，并可选附带 `org_ids` 把 identity principal 限定到特定 org。当前如果同一把 token 既要管 key lifecycle、又要读 audit trace，至少给它 `api_keys:manage` 和 `audit:read` 两个 scope。

完整 precedence / scope / shared-vs-split 规则见：
- `docs/admin-token-model.md`

如果想把两类权限拆开，当前也支持：
- `IDENTITY_ADMIN_TOKENS_JSON` 只放 key-management token（例如仅 `api_keys:manage`）
- `AUDIT_ADMIN_TOKENS_JSON` 单独放 audit-read token（例如仅 `audit:read`）

可直接参考仓库里的现成样板：
- `/data/home-data/CEX/.env.split-admin.example`

如果是在 Windows 主机上直接切换到这套配置，也可以运行：
- `powershell -ExecutionPolicy Bypass -File .\scripts\use-split-admin-env.ps1`

切换后建议先做一次 split-admin 预检：
- `powershell -ExecutionPolicy Bypass -File .\scripts\validate-split-admin-env.ps1`

这个预检会确认：
- `IDENTITY_ADMIN_TOKENS_JSON` 里至少有一把 `api_keys:manage` token
- `AUDIT_ADMIN_TOKENS_JSON` 里至少有一把 `audit:read` token
- `EXECUTION_ADMIN_TOKENS_JSON` 里至少有一把 `executions:manage` token
- `LEDGER_ADMIN_TOKENS_JSON` 里至少有一把 `ledger:manage` token
- identity manage token 与 audit read token 不会意外重叠成同一把
- 如果声明了 `org_ids`，它不是空数组且不包含空白项
- legacy `IDENTITY_ADMIN_TOKEN` / `AUDIT_ADMIN_TOKEN` / `EXECUTION_ADMIN_TOKEN` / `LEDGER_ADMIN_TOKEN` 在 split 演练中保持 unset

补一句，repo 里的 ignored runtime blackbox 现在会按这些 env 自动挑对应 scope 的 token，不再硬编码 `local-dev-admin-token`，所以 split 配置不会天然把 live runtime tests 弄坏。

```json
{
  "org_id": "00000000-0000-0000-0000-00000000ce01",
  "user_id": "00000000-0000-0000-0000-00000000ce11",
  "label": "Local Dev Rotation Key"
}
```

响应会返回：
- 一次性明文 `api_key`
- 不含明文的持久化 `record`

### 5.4 可选：列出某个 org 的 API keys

GET `http://127.0.0.1:7001/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01`

请求头至少带一个：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 读路径：`x-admin-token: identity-read-token`
- split-admin 读路径：`Authorization: Bearer identity-read-token`
- split-admin 管理路径：`x-admin-token: identity-manage-token`
- split-admin 管理路径：`Authorization: Bearer identity-manage-token`

返回的 `status` 目前会按运行时语义显示为 `active` / `expired` / `revoked`。

如果你单独配了只读管理 principal，这个 list 接口也可以用仅带 `api_keys:read` 的 token；`api_keys:manage` 也同样允许访问。若该 principal 配了 `org_ids`，list 现在也只允许查询这些 org。

### 5.5 可选：撤销某把 API key

POST `http://127.0.0.1:7001/v1/api-keys/<api_key_id>/revoke`

请求头至少带一个：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 路径：`x-admin-token: identity-manage-token`
- split-admin 路径：`Authorization: Bearer identity-manage-token`

```json
{
  "reason": "rotated"
}
```

已撤销 key 后续再调 gateway，不会再因为 static fallback 被放行；DB-backed revoke 现在会优先拦住它。若管理 principal 配了 `org_ids`，revoke 也只允许作用于这些 org 下的 key。
当前 revoke 响应与 key list 里也会带上 `revoked_reason`（如果提供了 reason）。
另外，identity 的 key lifecycle audit 目前使用 `api_key_id` 作为 audit trace id，因此可直接用：
- `GET /v1/audit/events/trace/<api_key_id>`
来核对 `identity.api_key.issued` / `identity.api_key.revoked` 事件。

注意，这个 audit trace 查询现在也要求 admin token，并且该 token 需要带 `audit:read` scope，例如：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 路径：`x-admin-token: audit-read-token`
- split-admin 路径：`Authorization: Bearer audit-read-token`

如果 audit principal 配了 `org_ids`，那么它现在只能读取这些 org 下的 trace；trace 本身也必须带上明确的 org boundary metadata，不能再是“有 trace_id 就默认全局可读”。

### 5.6 可选：直接读某个账户

GET `http://127.0.0.1:7002/v1/accounts/<account_id>`

请求头至少带一个：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 读路径：`x-admin-token: ledger-read-token`
- split-admin 读路径：`Authorization: Bearer ledger-read-token`
- split-admin 管理路径：`x-admin-token: ledger-manage-token`
- split-admin 管理路径：`Authorization: Bearer ledger-manage-token`

这个 read 接口接受 `ledger:read`，也兼容更宽的 `ledger:manage`。如果 ledger principal 配了 `org_ids`，它只能读取这些 org 下的 account。

### 5.7 可选：直接做 ledger reserve / consume / refund

例如 reserve：

POST `http://127.0.0.1:7002/v1/ledger/reserve`

请求头至少带一个：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 路径：`x-admin-token: ledger-manage-token`
- split-admin 路径：`Authorization: Bearer ledger-manage-token`

```json
{
  "account_id": "<account_id>",
  "amount": 5.0,
  "reference_id": "manual-reserve",
  "idempotency_key": "manual-reserve-1"
}
```

其余 direct ledger 写端点也统一要求 `ledger:manage`：
- `POST /v1/ledger/consume`
- `POST /v1/ledger/refund`

如果 ledger principal 配了 `org_ids`，这些写路径同样只允许作用于对应 org 下的 account。

### 5.8 可选：直接读某条 execution

GET `http://127.0.0.1:7003/v1/executions/<execution_id>`

请求头至少带一个：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 读路径：`x-admin-token: execution-read-token`
- split-admin 读路径：`Authorization: Bearer execution-read-token`
- split-admin 管理路径：`x-admin-token: execution-manage-token`
- split-admin 管理路径：`Authorization: Bearer execution-manage-token`

这个 read 接口接受 `executions:read`，也兼容更宽的 `executions:manage`。如果 execution principal 配了 `org_ids`，它只能读取这些 org 下的 execution；历史 execution 若缺 `org_id` metadata，在 scoped principal 下会 fail closed。

### 5.7 可选：直接推进 execution 生命周期

例如 approve：

POST `http://127.0.0.1:7003/v1/executions/<execution_id>/approve`

请求头至少带一个：
- 兼容 shared-token 路径：`x-admin-token: local-dev-admin-token`
- 兼容 shared-token 路径：`Authorization: Bearer local-dev-admin-token`
- split-admin 路径：`x-admin-token: execution-manage-token`
- split-admin 路径：`Authorization: Bearer execution-manage-token`

```json
{
  "approved_by": "local-operator",
  "note": "manual approval"
}
```

其余 direct execution lifecycle 端点也统一要求 `executions:manage`：
- 对带 `provider_target` 的 execution，`POST /v1/executions/:id/start` 现在会尝试直接把 prompt handoff 到 provider adapter；当前第一条真实路径是 `ollama`。
- `POST /v1/executions/:id/reject`
- `POST /v1/executions/:id/dispatch`
- `POST /v1/executions/:id/start`
- `POST /v1/executions/:id/cancel`
- `POST /v1/executions/:id/timeout`
- `POST /v1/executions/:id/succeed`
- `POST /v1/executions/:id/fail`

如果 execution principal 配了 `org_ids`，这些写路径同样只允许操作对应 org 下的 execution。

## 6. 当前限制

当前仍然是：
- identity-service 虽已支持 DB-backed API key resolve，但仍保留 static fallback，离完整的 key rotation / revoke / self-service 管理还差一截
- invocation / execution 主链仍未完成真正的 provider / worker / capability 落地
- 无完整统一错误模型

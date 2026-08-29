# CEX Development Plan v2

- 状态：Active
- 制定日期：2026-08-27
- 基线提交：`b29fffa7e09c7c04bf66faa7a2910ed248e40ece`
- 实施分支：`feature/hepta-production-baseline-p0`
- 目标：把当前功能丰富的 internal alpha 收敛为可审计、可回滚、可发布的生产候选基线

## 1. 总体判断

CEX 已经具备 identity、ledger、gateway、execution、audit、capability、product edge、Paper Raid 与 Hepta control-plane 等较完整实现，也已经形成 worker lease、retry budget、provider failure taxonomy、metrics、readiness、soak 和本地恢复演练等工程能力。

下一阶段不再以继续扩充表面功能为第一目标，而以建立系统级不变量为主：

1. 任何可发布版本必须绑定唯一 commit、CI 证据、迁移头、依赖锁、镜像 digest 和运行配置。
2. production-like 环境必须 fail closed，不能因缺配置、数据库异常或默认值退回开发凭据和内存成功路径。
3. 跨 gateway、execution、ledger、provider、audit 的流程必须采用 durable saga，而不是把网络调用包进数据库事务。
4. 账本必须使用精确金额模型，并能从流水重建、核对和修复投影。
5. 内部服务写接口必须验证 workload identity；localhost 不是身份机制。
6. 审计事件必须具备认证写入、可靠投递、顺序/完整性证据和保留策略。
7. 文档必须是可执行规格，并通过 CI 检查与实现、路由、配置和迁移保持一致。

## 2. 计划边界

### 2.1 本计划覆盖

- 仓库治理与 release baseline
- runtime profile、启动配置和 secret posture
- API key、管理员与服务身份
- ledger 精确金额、不变量、幂等和 reconciliation
- invocation/execution 状态机、worker、provider 和补偿流程
- audit 完整性与可靠投递
- OpenAPI、配置、迁移和 ADR 的自动校验
- CI、供应链、部署、SLO、DR 和 incident readiness
- 代码模块化与 bounded-context 收口

### 2.2 暂不作为 P0 阻断项

- Marketplace 全功能
- 多区域主动-主动部署
- 全部 provider 商业账户开通
- 公网大规模增长功能
- 非核心玩法与客户端体验扩展

这些工作只有在 P0/P1 不变量通过后才进入主线。

## 3. 目标架构原则

### 3.1 Authority 单一

- Identity 数据库是 production API key authority。
- Ledger 数据库是账户余额、预留和流水 authority。
- Execution 数据库是 invocation/execution 生命周期 authority。
- Audit store 是事件证据 authority，但不替代业务表。
- 本地 JSON、内存 map 和固定 token 仅允许 test/local/dev profile。

### 3.2 持久化先于副作用

任何外部副作用必须先有 durable intent：

`command persisted -> outbox claimed -> side effect -> receipt persisted -> state advanced`

禁止在持有业务行锁的数据库事务中执行 provider、ledger 或其他 HTTP 调用。

### 3.3 默认拒绝

production-like profile 包括 `beta`、`staging` 和 `production`。这些 profile 下：

- profile 必须显式配置；
- 数据库必须可连接；
- 关键服务必须显式 fail-fast；
- 已知开发 key/token/secret 必须拒绝启动；
- 不允许静态身份回退和内存成功回退；
- 关键写 API 必须有服务身份；
- readiness 失败时不接收业务流量。

### 3.4 证据优先

“已完成”“已通过”“production ready”必须指向机器可验证证据。手工文档中的 PASS 不自动继承到后续 commit。

## 4. 分阶段路线

## P0 — Production Baseline

### P0-A 仓库与发布基线

交付物：

- 唯一 canonical trunk 与 integration PR；
- `main` 分支保护/ruleset；
- required checks；
- release manifest、签名 tag、SBOM、provenance 和镜像 digest；
- 长期 feature branch 归档策略；
- CODEOWNERS 与 bounded-context owner。

验收：

- 没有 PR 和绿色 required checks 的提交不能进入 trunk；
- 每个 RC 能从 manifest 追溯到源码、迁移、构建物和测试证据；
- 部署只接受 tag/digest，不接受浮动分支。

### P0-B 运行时配置与 fail-closed

交付物：

- 统一 `runtime-guard` crate；
- 明确 `dev/test/local/beta/staging/production` profile；
- profile 冲突检测；
- production-like 数据库预检；
- 服务级 fail-fast 要求；
- 已知开发凭据和 placeholder 检测；
- liveness/readiness 分离；
- 配置 reference 和启动失败码。

验收：

- 未配置 profile 时生产二进制拒绝启动；
- production-like 使用 `local-dev-key`、`local-dev-admin-token`、`replace-me` 等值时拒绝启动；
- 数据库不可达时 production-like 服务不进入监听状态；
- dev/test 行为保持可重复。

### P0-C Workload Identity 与内部 API

交付物：

- 服务 token/JWT 或 mTLS 基线；
- issuer、audience、service id、scope、org 限制；
- 逐路由调用方矩阵；
- token rotation 和双 key 窗口；
- 网络策略作为第二层控制。

优先保护：

1. `POST /v1/executions`
2. `POST /v1/audit/events`
3. `POST /v1/auth/resolve`
4. capability internal read/write
5. worker claim/process/renew

验收：

- 同机匿名进程不能创建 execution 或伪造 audit event；
- audit 的 `actor_type` 来自已认证服务身份，而不是完全信任请求体；
- token 有明确轮换和吊销路径。

### P0-D Identity Authority 收口

交付物：

- production 禁止 static API key authority；
- DB 查询异常不回退到另一套认证 authority；
- key issue/list/revoke/resolve 的稳定错误码；
- API key hash version、rotation、last-used 异步更新；
- admin principal 最小权限和 org scope；
- auth backend readiness、错误率和异常告警。

验收：

- revoked/expired key 在 DB 异常时不会重新有效；
- 认证错误不返回 SQL/驱动内部详情；
- 所有管理操作都有可靠 audit intent。

### P0-E Ledger 精确金额与不变量

交付物：

- 统一 integer minor unit 或 Decimal；
- currency scale/rounding/limits；
- 开户初始余额由 genesis entry 表达；
- reserve/consume/refund/release/adjust 的正式状态模型；
- SQL constraints；
- 作用域化 idempotency key 和 replay result；
- projection rebuild 与 reconciliation；
- 并发与 property-based 测试。

核心不变量：

- `balance >= 0`
- `reserved >= 0`
- `reserved <= balance`
- entry amount 为正且 scale 合法
- 同一业务操作最多产生一个有效 effect
- account summary 可由 authoritative entries 重建

验收：

- 代码路径不再使用 `f64` 表示价值；
- 数据库不可用时不返回非持久化成功；
- 并发 reserve/consume 不超扣；
- reconciliation 能定位并修复投影偏差。

### P0-F Invocation/Execution Durable Saga

交付物：

- durable command/outbox/inbox；
- operation id、attempt id、provider idempotency contract；
- 外部调用移出数据库事务；
- crash-point matrix；
- compensation/reconciliation worker；
- lease、heartbeat、timeout、retry 和 dead-letter 精确定义；
- invocation/execution/ledger 状态一致性检查。

推荐主流程：

1. Gateway 短事务创建 invocation 和 reserve command。
2. Ledger 执行 reserve 并持久化 receipt。
3. Orchestrator 根据 receipt 创建 execution command。
4. Worker claim 后提交 claim，再调用 provider。
5. Provider receipt 持久化后推进 execution。
6. 成功生成 consume command；失败生成 refund command。
7. Reconciler 处理长期不一致和未知结果。

验收：

- provider 调用期间不持有业务 SQL transaction；
- 任一 crash point 重放不会重复扣费；
- provider 已执行但本地未知时进入 explicit `unknown/reconcile_required`，不盲目重试；
- 所有补偿都有可查询状态和告警。

### P0-G Audit Integrity

交付物：

- authenticated writer；
- transactional audit outbox；
- at-least-once delivery 和 dedupe；
- event schema version；
- tenant sequence、prev-hash/hash chain 或签名 receipt；
- append-only DB 权限；
- PII/secret redaction；
- retention、export、legal hold 和丢失告警。

验收：

- 关键业务状态提交后，audit intent 不会静默丢失；
- 未认证服务不能伪造 writer identity；
- 可检测删除、篡改、顺序断裂和 delivery backlog。

### P0-H CI、迁移与供应链

交付物：

- broad path trigger；
- fmt、clippy、unit、contract、migration、integration、audit、deny；
- fresh DB 和 upgrade DB migration tests；
- secret scan、license、SBOM、image scan；
- action 和 Rust toolchain pin；
- release artifact signing。

验收：

- migrations/config/ops/docs 的变更会触发正确门禁；
- 所有构建使用 `--locked`；
- RC 生成可验证 SBOM/provenance；
- migration rollback/roll-forward 有演练证据。

## P1 — Closed Beta Hardening

- service-backed product identity/session authority；
- 分布式 rate limit、dedupe 和 quota；
- org/account/capability policy bundles；
- approval、retry、cancel 的安全产品面；
- operator dashboard、SLO、error budget；
- 正式 secret manager；
- 外部 provider 多账户和熔断；
- 多节点 latency/soak/chaos；
- on-call ownership 和 incident lifecycle。

## P2 — Public Launch Readiness

- 商业 billing/subscription/entitlement；
- 多区域 DR 和恢复演练；
- 合规审计导出；
- 外部渗透和 abuse review；
- 容量模型和成本治理；
- 正式 SLA、支持和变更管理。

## 5. 实施顺序与依赖

### Sprint 0：可信基线

1. 建立工作分支和计划文档。
2. 接入 runtime guard。
3. 扩大 CI path coverage。
4. 固化第一份 RC manifest 格式。
5. 建立 integration PR。

### Sprint 1：认证与审计入口

1. workload identity contract。
2. 保护 execution create 与 audit write。
3. gateway、identity、execution 等客户端携带服务身份。
4. audit writer identity 与 payload identity 分离。
5. rotation 与拒绝测试。

### Sprint 2：账本内核

1. Money 类型 ADR。
2. 新字段/新 API 双写。
3. backfill 和 reconciliation。
4. 切读到精确金额。
5. 删除 `f64` 路径。

### Sprint 3：Saga/outbox

1. outbox schema 和 dispatcher。
2. reserve/consume/refund command receipt。
3. provider receipt/inbox。
4. crash injection。
5. reconciliation 和 operator surface。

### Sprint 4：发布与运行证据

1. 正式 migration upgrade matrix。
2. staging deployment。
3. soak/chaos/restore。
4. SLO 与 incident drill。
5. RC signoff。

依赖关系：

`runtime profile -> workload identity -> audit outbox -> ledger precision -> execution saga -> beta signoff`

## 6. 测试体系

### 6.1 单元与属性测试

- profile/config parser；
- money arithmetic；
- state transition；
- policy evaluation；
- idempotency normalization；
- signature/token validation。

### 6.2 数据库测试

- fresh migration；
- 从每个受支持旧版本升级；
- rollback/roll-forward；
- constraint violations；
- concurrent reserve/consume；
- SKIP LOCKED worker 竞争；
- outbox/inbox dedupe。

### 6.3 故障注入

必须覆盖：

- reserve 成功后 gateway crash；
- provider 成功后 receipt 写入失败；
- consume 成功后 execution commit 失败；
- audit store 不可达；
- worker lease 过期与双 worker；
- provider timeout 但实际完成；
- DB failover 与连接池耗尽。

### 6.4 安全测试

- anonymous internal writes；
- wrong audience/issuer；
- revoked/expired/rotated token；
- org boundary；
- replay；
- secret/PII logging；
- oversized provider output；
- child-process environment leakage。

## 7. 数据迁移策略

所有破坏性模型变更采用 expand/migrate/contract：

1. Expand：添加新字段/表/约束，不移除旧路径。
2. Dual write：新旧格式同时写，记录偏差。
3. Backfill：可重放、有 checkpoint、有限速。
4. Verify：全量 reconciliation 和抽样人工检查。
5. Cutover：配置开关切读。
6. Contract：至少一个稳定周期后删除旧字段。

每一步必须有：

- 前置检查；
- 运行时指标；
- abort 条件；
- rollback/roll-forward；
- 数据修复脚本；
- 证据文件。

## 8. SLO 与运行指标

P1 前至少定义：

- authenticated invocation availability；
- invocation p50/p95/p99 latency；
- reserve/consume/refund success；
- reconciliation mismatch age；
- execution queue age；
- approval backlog age；
- provider unknown-result count；
- audit outbox oldest age；
- key resolution error rate；
- database pool saturation。

告警基于 burn rate 和 backlog age，不只基于进程级累计计数。

## 9. 文档治理

权威文档类别：

- ADR：为什么这样设计；
- Contract/OpenAPI：系统对外承诺；
- Runbook：故障如何处理；
- Evidence：某个 commit 实际通过了什么；
- Plan：后续如何推进。

CI 自动检查：

- route inventory 与 OpenAPI；
- env 变量与 configuration reference；
- migration head 与 index；
- workspace member 与 owner；
- evidence commit 与当前 commit；
- ADR 状态和 superseded 链。

## 10. 风险登记

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 长期 feature branch 继续分叉 | 无可信发布基线 | integration PR + trunk ruleset |
| fail-open 修复破坏本地体验 | 开发阻塞 | 明确 dev profile 和 fixtures |
| Money 类型迁移影响 API | 客户端不兼容 | versioned schema + 双写 |
| Saga 改造期双系统不一致 | 价值风险 | shadow/reconciliation + feature flag |
| 服务认证轮换失误 | 内部调用中断 | 双 key、audience metrics、回退窗口 |
| 审计 backlog 放大 | 合规证据缺口 | durable outbox、age alert、backpressure |

## 11. 当前实施批次

本分支第一批交付：

- 新增统一 runtime startup guard；
- 所有核心服务接入显式 profile 与 production-like DB 预检；
- production-like 拒绝已知开发凭据和 placeholder；
- identity production-like 注入不可知的 deny-only static sink，作为彻底移除 static fallback 前的过渡防线；
- 增加 ledger 数据库不变量迁移；
- 扩大 hosted CI 的触发范围；
- 新增运行时配置文档和回滚说明。

下一批立即推进：

1. workload identity 与 internal write auth；
2. identity DB authority 完全 fail closed；
3. audit authenticated writer + outbox；
4. Money v2 ADR、schema 和双写；
5. execution/ledger saga outbox。

## 12. Definition of Done

一个工作项只有同时满足以下条件才算完成：

- 有明确 contract/ADR；
- 有实现；
- 有正向、拒绝、重放和故障测试；
- 有 migration/rollback；
- 有 metrics/alert；
- 有 operator runbook；
- 有 CI 证据并绑定 commit；
- 没有未记录的开发默认值或权限扩大；
- 文档中的状态与代码一致。

# 全工作区开发交接：实现边界、字段与验证入口

Status: proposed technical handoff supplement; not an acceptance attestation  
Production authorization: `not_granted`

本文件补充而不取代 [模块索引](../modules/index.md)、已接受 ADR 和版本化协议。
覆盖 23 个 package 的修改路径；它不是把 69 个验收主题重新计作已通过测试。
构建目标列表来自审阅基线的 Cargo metadata；有条件的构建目标在相关模块另行说明。
列出 target 不代表该 target 在默认配置或每个平台都执行；不把 test target 数当测试用例数。
配置、接口与存储的缺口继续按 acceptance-plan-v1.json 保持 not_reviewed；独立走查后才可接受。

## 字段级对照：不得把类型校验等同于业务授权

### LedgerEffectRequestV1

源定义：[ledger_v2.rs](../../crates/shared-types/src/ledger_v2.rs)。

| 字段 | Rust/wire 类型 | 已观察的类型校验与额外责任 |
|---|---|---|
| account_id | UUID | 非 nil；账户归属仍需服务鉴权 |
| trace_id | Option<UUID> | Some 时非 nil；require_explicit_trace=true 时必填 |
| operation_id | Option<UUID> | Some 时非 nil；类型可选不放宽生产路径的稳定操作身份 |
| operation_kind | enum | reserve / consume / refund / grant；操作角色由服务判断 |
| currency_unit | string | MoneyAmount normalization；与账户币种/scale 匹配由权威端确认 |
| currency_scale | u8 | 最大 6；禁止推断历史精度 |
| amount_minor | i64，JSON string | effect validate 要求 >0；完整入口还要证明 canonical string 规则 |
| reference_type / reference_id | Option<string>/Option<UUID> | 两者同时存在或同时缺失；引用是否属于本次业务另行核验 |
| idempotency_scope | string | trimmed 值非空，最长 160，字符限 ASCII 字母数字及 ._:- |
| idempotency_key | string | trimmed 值非空、最多 256 个字符、无控制字符；存储比较/归一化规则由调用方固定 |

错误 code 包括 invalid_account_id、invalid_trace_id、invalid_operation_id、explicit_trace_required、
invalid_amount_minor、invalid_currency_unit、invalid_currency_scale、invalid_idempotency_scope、
invalid_idempotency_key、invalid_reference_binding。HTTP 状态必须在真实 route 回归中验证。
金额字符串的共享反序列化使用整数 parse；本表不声称它本身拒绝每一种可解析但非规范的拼写。

### Identity 签发、读取与撤销

源定义：[identity-service/src/lib.rs](../../services/identity-service/src/lib.rs)。

| 操作 | 当前接口/字段 | 开发注意事项 |
|---|---|---|
| 解析 key | POST /v1/auth/resolve | 请求由 ApiKeyResolveRequest 定义，不允许自行拼出 AuthContext 代替真实解析 |
| 签发 | POST /v1/api-keys；org_id；可选 user_id、label、expires_at | 原始 key 只在签发响应 api_key 中返回；列表不复制它 |
| 列表 | GET /v1/api-keys?org_id=... | 返回 items: ApiKeyRecordView[]；必须实施管理 scope 和租户限制 |
| 撤销 | POST /v1/api-keys/:id/revoke；可选 reason | 记录 revoked_at/revoked_reason，验证撤销后消费者不能继续授权 |
| 记录视图 | api_key_id、org_id、user_id、key_prefix、label、status、expires_at、last_used_at、revoked_at、revoked_reason、created_at | 视图不得泄漏原始 key 或 hash；last_used_at 是遥测而非权限变更 |

本补充未发现独立 rotate route 的定义，不为实现虚构接口。轮换、幂等签发与缓存撤销仍须通过
真实 API/持久化路径验证。/health 的静态字符串不能代替 readiness/启动保护。

### AuditEventCreateRequestV2 / RecordV2

源定义：[audit_v2.rs](../../crates/shared-types/src/audit_v2.rs)。

请求字段是 event_id、trace_id、org_id?、actor_type、actor_id?、event_type、schema_version、
occurred_at、payload。event_id 和 trace_id 不能为 nil；component trimmed 后最多 128 bytes、
只使用 ASCII 字母数字及 ._:-；actor_id trimmed 后 1–256 字符；payload 必须是 object，
请求方不能设置 _cex_audit_writer；occurred_at 不能超过校验时刻五分钟。

RecordV2 另有 chain_key、tenant_sequence、previous_event_hash?、event_hash、writer_service_id、
writer_auth_scheme、received_at。不要从请求复制出可信 writer，不要把 occurred_at 当接收顺序。
共享 validate 检查 schema_version 的形状，不等于允许任意业务 schema；版本准入是服务义务。
Error Display 可能携带无效字段值，服务边缘应输出受控 code，不能直接回显私人输入。

## 模块导航

[shared-types](#shared-types) · [shared-errors](#shared-errors) · [shared-tracing](#shared-tracing) · [shared-config](#shared-config) · [hepta-paper-raid-contracts](#hepta-paper-raid-contracts) · [identity-service](#identity-service) · [ledger-service](#ledger-service) · [trnm-economy-service](#trnm-economy-service) · [gateway-service](#gateway-service) · [execution-service](#execution-service) · [audit-service](#audit-service) · [capability-service](#capability-service) · [consumer-entry-api](#consumer-entry-api) · [hepta-research-league](#hepta-research-league) · [paper-raid-bff](#paper-raid-bff) · [matrix-entry-adapter](#matrix-entry-adapter) · [matrix-bot-relay](#matrix-bot-relay) · [matrix-bot-poller](#matrix-bot-poller) · [trnm-economy-protocol](#trnm-economy-protocol) · [trnm-finality-types](#trnm-finality-types) · [trnm-finality-verifier](#trnm-finality-verifier) · [trnm-protocol](#trnm-protocol) · [trnm-research-protocol](#trnm-research-protocol)

<a id="shared-types"></a>

## 01. shared-types

责任岗位：`platform-foundations`。既有契约：[shared-types](../../docs/modules/shared-types.md)。

**实现边界。** 该库只定义值、回执、事件和 Saga 词汇，不承担鉴权或存储。MoneyAmount 的 currency/scale/minor_units 与 LedgerEffectRequestV1 的 currency_unit/currency_scale/amount_minor 不是同一 JSON 字段集合。整数在 JSON 中以字符串编码；scale 最大为 6。MoneyAmount 构造允许有符号余额，Ledger effect validate 则要求正金额，不能把两者的正负值规则混用。

**修改与测试路径。** 修改方法：先确定改变的是领域值、wire encoding 还是入口校验。保留原始字节向量；对 i64 两端、零、负值、溢出、不同 currency/scale、非规范表示分别建立测试。共享字符串反序列化能解析整数不等于 HTTP 接口已经拒绝所有非规范字符串；由真实 ingress 回归证明，不从类型名推断。

**兼容与运行限制。** 消费者必须独立验证租户、角色、完整回执和数据库幂等约束。不得把 parse_decimal 的兼容/显示能力重新接入禁止历史精度推断的权威写入路径。

Cargo targets（名称/类型；不是用例数）：shared_types/lib。

本模块的三项交接验收关联：`shared-types:1`, `shared-types:2`, `shared-types:3`。

完整 Linux package 验证：`cargo test --locked -p shared-types --all-targets`；`cargo clippy --locked -p shared-types --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="shared-errors"></a>

## 02. shared-errors

责任岗位：`platform-foundations`。既有契约：[shared-errors](../../docs/modules/shared-errors.md)。

**实现边界。** 这里拥有共享错误词汇，而非各服务的全部 HTTP 状态、重试策略或事故状态。调用方负责把错误映射为验证失败、权限拒绝、内容冲突、依赖不可达或结果未知。

**修改与测试路径。** 修改方法：为新增错误给出序列化往返、稳定 code、脱敏后展示以及每个消费者的映射用例。测试不能只匹配包含某段文本；应断言机器字段和禁止泄漏的输入。

**兼容与运行限制。** 超时如果发生在可能产生远程副作用之后，必须保留结果未知语义。重试策略不得只根据 HTTP 5xx 或共享错误 Display 文本决定。

Cargo targets（名称/类型；不是用例数）：shared_errors/lib。

本模块的三项交接验收关联：`shared-errors:1`, `shared-errors:2`, `shared-errors:3`。

完整 Linux package 验证：`cargo test --locked -p shared-errors --all-targets`；`cargo clippy --locked -p shared-errors --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="shared-tracing"></a>

## 03. shared-tracing

责任岗位：`platform-foundations`。既有契约：[shared-tracing](../../docs/modules/shared-tracing.md)。

**实现边界。** 该库负责 tracing 初始化；日志和 span 不是 Audit 历史或资金事实。进程崩溃时未刷新的 telemetry 不能被用于恢复业务状态。

**修改与测试路径。** 修改方法：在依赖服务中验证初始化幂等行为、过滤配置失败路径、任务间关联传播和敏感输入不进入输出。指标使用路由模板及有界错误类别；将租户、账号和 operation ID 作为无限枚举标签前需专门审查。

**兼容与运行限制。** exporter 缓冲、采样、丢弃和 backpressure 是部署/服务契约，不由库文档自动保证。保留低基数计数与受控审计的不同职责。

Cargo targets（名称/类型；不是用例数）：shared_tracing/lib。

本模块的三项交接验收关联：`shared-tracing:1`, `shared-tracing:2`, `shared-tracing:3`。

完整 Linux package 验证：`cargo test --locked -p shared-tracing --all-targets`；`cargo clippy --locked -p shared-tracing --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="shared-config"></a>

## 04. shared-config

责任岗位：`platform-foundations`。既有契约：[shared-config](../../docs/modules/shared-config.md)。

**实现边界。** Matrix 的纯 resolve_profiles 解析与已有服务 RuntimeProfile 是不同接口。Matrix 的所有显式来源必须解析并相等；staging 和 production 比较时仍不同，即使旧调用层都映射为 production。无显式来源才可采用 local。

**修改与测试路径。** 修改方法：枚举每个来源的缺失、空、未知、非 Unicode、合法别名和冲突组合。每个调用进程负责在创建 Tokio runtime、监听器或 worker 之前读取和验证；仅重导出类型不能证明调用次序。

**兼容与运行限制。** 变更共享默认值需要所有依赖服务的启动回归。动态配置重载没有已接受协议时不实现；不要静默回退到内存数据库或弱 token。

Cargo targets（名称/类型；不是用例数）：shared_config/lib。

本模块的三项交接验收关联：`shared-config:1`, `shared-config:2`, `shared-config:3`。

完整 Linux package 验证：`cargo test --locked -p shared-config --all-targets`；`cargo clippy --locked -p shared-config --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="hepta-paper-raid-contracts"></a>

## 05. hepta-paper-raid-contracts

责任岗位：`hepta-contracts`。既有契约：[hepta-paper-raid-contracts](../../docs/modules/hepta-paper-raid-contracts.md)。

**实现边界。** 这里拥有已签命令、回执和 hash frame；不拥有科研真实性、论文发布或 Chain 最终性。domain、版本、身份、nonce、期限、expected version 与 payload commitment 必须跟随所属帧被签名。

**修改与测试路径。** 修改方法：先冻结旧向量，再加入新版本的正向签名向量；逐字段替换、变更字段顺序、错误 key、过期和重放作为负例。签名互通必须测试实际消费方而不是重复调用同一 helper。

**兼容与运行限制。** 不独立部署。协议变更不能无声覆盖已有 golden 文件；旧 reader 的拒绝行为、新 reader 的兼容范围和退役条件需要一起审查。

Cargo targets（名称/类型；不是用例数）：hepta_paper_raid_contracts/lib。

本模块的三项交接验收关联：`hepta-paper-raid-contracts:1`, `hepta-paper-raid-contracts:2`, `hepta-paper-raid-contracts:3`。

完整 Linux package 验证：`cargo test --locked -p hepta-paper-raid-contracts --all-targets`；`cargo clippy --locked -p hepta-paper-raid-contracts --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="identity-service"></a>

## 06. identity-service

责任岗位：`identity-control-plane`。既有契约：[identity-service](../../docs/modules/identity-service.md)。

**实现边界。** 当前 build_router 注册 /v1/auth/resolve、/v1/api-keys 及 /v1/api-keys/:id/revoke。签发响应是 api_key 与 record；列表返回 items: ApiKeyRecordView[]。上方字段表来自实际 Rust 定义，不能据此假定另有独立 rotate endpoint。/health 当前是静态进程响应，不代表数据库或凭证托管已验收。

**修改与测试路径。** 修改方法：用真正管理 principal 分别执行同租户签发/查询/撤销和跨租户拒绝；验证过期、重复操作、记录脱敏及 last_used_at 不改变权限修订。为 issue 请求的超时重试定义可验证的幂等/碰撞行为，不能从文档愿望推断已实现。

**兼容与运行限制。** 轮换应明确旧新 key 的重叠窗口、撤销传播、消费者缓存和事故失效策略。实际托管与轮换演练属于外部条件；不得把静态 api_keys 测试构造器当生产 fallback。

Cargo targets（名称/类型；不是用例数）：identity_service/lib; identity-service/bin; http_flow/test; runtime_blackbox/test。

本模块的三项交接验收关联：`identity-service:1`, `identity-service:2`, `identity-service:3`。

完整 Linux package 验证：`cargo test --locked -p identity-service --all-targets`；`cargo clippy --locked -p identity-service --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="ledger-service"></a>

## 07. ledger-service

责任岗位：`ledger-authority`。既有契约：[ledger-service](../../docs/modules/ledger-service.md)。

**实现边界。** 链下账户、effects、reserve/consume/refund、operation 与回执是这里的权威。Chain 最终性不在此。账户非零初值必须通过 genesis-as-entry；投影与兼容列不能创建新金额权威。上方 Ledger 请求表区分共享类型的可选字段与生产路径更强的限制。

**修改与测试路径。** 修改方法：先定义同一业务 operation 的不可变 tuple，断言精确重放只返回原结果、改变账户/金额/scale/reference/key 则冲突。并发 consume 与 refund 必须互斥；对账前后比较原始 entries 与投影，不只比较总余额。

**兼容与运行限制。** 注入 Ledger 提交成功但响应丢失，Gateway/Execution 必须用同一身份恢复。未知结果禁止换 operation ID。测试保留未解释差异为失败，不以人工调余额或删除 entries 结束测试。

Cargo targets（名称/类型；不是用例数）：ledger_service/lib; ledger-service/bin; http_flow/test。

本模块的三项交接验收关联：`ledger-service:1`, `ledger-service:2`, `ledger-service:3`。

完整 Linux package 验证：`cargo test --locked -p ledger-service --all-targets`；`cargo clippy --locked -p ledger-service --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="trnm-economy-service"></a>

## 08. trnm-economy-service

责任岗位：`trnm-economy`。既有契约：[trnm-economy-service](../../docs/modules/trnm-economy-service.md)。

**实现边界。** 服务冻结签名 economic intent 原始字节并协调 Ledger receipt；不拥有 Chain 共识或钱包托管。其 settlement_v1.sql 独立于根编号迁移链；数据库 readiness 必须覆盖本地 schema。

**修改与测试路径。** 修改方法：按 exact intent bytes、intent ID/hash、issuer/key、audience、期限、nonce 和 Ledger authority 验证；针对响应丢失必须恢复同一 intent。注册、持久化、调用和 outcome 的事务窗口明确分开。

**兼容与运行限制。** 签发、Game、session、Ledger 管理凭证不得复用。构建产物要记录 committed lock、source、binary 和 migration 的一致性；SBOM/构建元数据不替代真实 Chain 接受。

Cargo targets（名称/类型；不是用例数）：trnm_economy_service/lib; trnm-economy-service/bin; durable_bytes_immutability/test; settlement_contract/test。

本模块的三项交接验收关联：`trnm-economy-service:1`, `trnm-economy-service:2`, `trnm-economy-service:3`。

完整 Linux package 验证：`cargo test --locked -p trnm-economy-service --all-targets`；`cargo clippy --locked -p trnm-economy-service --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="gateway-service"></a>

## 09. gateway-service

责任岗位：`gateway-runtime`。既有契约：[gateway-service](../../docs/modules/gateway-service.md)。

**实现边界。** Gateway 负责 Invocation ingress 和 exact reserve 命令；Ledger 负责资金结果，Execution 负责后续任务终态。API、domain、application、infrastructure 以及独立 exact-reserve API/worker 共同构成调用路径。

**修改与测试路径。** 修改方法：验证认证 service principal 与 x-cex-service-id 一致，拒绝 dual exact/legacy 金额。注册事务冻结 Invocation contract 与 reserve command；claim 提交后才能发 Ledger 请求。ack 要同时匹配 claim owner、lease 和完整回执，不只看 HTTP 200。

**兼容与运行限制。** 测试覆盖注册后崩溃、claim 后未发送、发送后响应丢失、超期 owner ACK、耗尽重试和 operator requeue。shadow 命令不得意外进入 active worker。停准入和停 worker 是不同回滚动作。

Cargo targets（名称/类型；不是用例数）：gateway_service/lib; gateway-exact-reserve-api/bin; gateway-exact-reserve-worker/bin; gateway-service/bin; http_flow/test; runtime_approval_probe/test; runtime_blackbox/test。

本模块的三项交接验收关联：`gateway-service:1`, `gateway-service:2`, `gateway-service:3`。

完整 Linux package 验证：`cargo test --locked -p gateway-service --all-targets`；`cargo clippy --locked -p gateway-service --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="execution-service"></a>

## 10. execution-service

责任岗位：`execution-runtime`。既有契约：[execution-service](../../docs/modules/execution-service.md)。

**实现边界。** 生命周期、外部 Agent 证据关联和 exact consume/refund command 独立于模型执行。默认构建没有本地 Agent/provider 执行。Cargo metadata 会列出受 required-features 约束的历史 worker，这不等于该 binary 在默认运行中启用。

**修改与测试路径。** 修改方法：把业务 terminal、settlement command 与 Ledger receipt 作为独立状态；针对同一 request 的重复/冲突、终态竞争、过期 claim、错误 receipt 和未知 outcome 写数据库断言。注册、source Audit/outbox enqueue 在同一事务内，远程调用在事务外。

**兼容与运行限制。** 历史 provider 0088 的 live/reconciled 区别继续保留，但不能恢复 /process 或本地推理。修复不确定结果时保留原 attempt/actor/reason/evidence；无独立证据不得授权新 attempt。

Cargo targets（名称/类型；不是用例数）：execution_service/lib; execution-provider-dispatch-worker/bin（required-features=legacy-local-provider-dispatch；不属于默认生产构建）; execution-service/bin; execution-settlement-worker/bin; external_agent_boundary/test。

本模块的三项交接验收关联：`execution-service:1`, `execution-service:2`, `execution-service:3`。

完整 Linux package 验证：`cargo test --locked -p execution-service --all-targets`；`cargo clippy --locked -p execution-service --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="audit-service"></a>

## 11. audit-service

责任岗位：`audit-integrity`。既有契约：[audit-service](../../docs/modules/audit-service.md)。

**实现边界。** 事件请求与持久化 record 字段见上方。writer_service_id、tenant_sequence、previous_event_hash/event_hash 是服务生成的事实，不由调用方声称。事件 payload 必须是对象，保留键 _cex_audit_writer 不能由请求方填入。

**修改与测试路径。** 修改方法：验证 writer 凭证到身份的绑定、event ID 内容冲突、组织 scope、序列/哈希链和 ACK loss 重放。source baseline 的 cursor 前进与 outbox intent 必须原子提交；压测 backlog 时 baseline 应暂停而不是绕过队列限制。

**兼容与运行限制。** 导出/归档后必须从已知链头验证连续性。脱敏和合法删除策略与 append-only 内容范围应分别设计；不要把原始 key、prompt 或私人论文体直接存进不可删除记录。

Cargo targets（名称/类型；不是用例数）：audit_service/lib; audit-outbox-dispatcher/bin; audit-service/bin; http_flow/test; runtime_blackbox/test。

本模块的三项交接验收关联：`audit-service:1`, `audit-service:2`, `audit-service:3`。

完整 Linux package 验证：`cargo test --locked -p audit-service --all-targets`；`cargo clippy --locked -p audit-service --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="capability-service"></a>

## 12. capability-service

责任岗位：`capability-registry`。既有契约：[capability-service](../../docs/modules/capability-service.md)。

**实现边界。** 唯一能力输入是 CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON；启动加载不可变读快照，没有写接口。记录使用 cap.external-agent.*、kind=external_agent_capability、provider=external-agent、稳定 Agent 引用及明确版本。容量为 1 MiB、1024 条；不执行本地发现。

**修改与测试路径。** 修改方法：为未知字段、重复 ID、控制字符、空值、超限及错误引用写加载失败测试；验证输出排序稳定。开发空 registry 只能是不 ready，生产类缺失配置必须在监听前失败。

**兼容与运行限制。** 发布/撤销 registry snapshot 应保存 digest、来源、审批和 consumer 兼容范围。registry ready 仅表示声明有效，不代表 Agent 在线或具科学质量/支付权限。

Cargo targets（名称/类型；不是用例数）：capability_service/lib; capability-service/bin; http_flow/test。

本模块的三项交接验收关联：`capability-service:1`, `capability-service:2`, `capability-service:3`。

完整 Linux package 验证：`cargo test --locked -p capability-service --all-targets`；`cargo clippy --locked -p capability-service --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="consumer-entry-api"></a>

## 13. consumer-entry-api

责任岗位：`consumer-edge`。既有契约：[consumer-entry-api](../../docs/modules/consumer-entry-api.md)。

**实现边界。** 拥有边缘会话、绑定治理、CSRF、replay 与产品投影，不拥有 Ledger/World/科研权威。Matrix result route 根据 event 定位候选记录，但授权必须再次匹配 delivery/payload/fingerprint/room/user 和完整八字段绑定。

**修改与测试路径。** 修改方法：验证 task_id 与 raw.invocation_id 必须非空且相同，top-level 与 nested binding 一致。读取快照要拒绝替换、符号链接、硬链接、超限、过期、未来时间和缺失结果。测试查询只能返回已有结果，不能重新发任务。

**兼容与运行限制。** 分别管理 browser 与 adapter 凭证；管理入口不能借投影 route 写权威表。World/League 新路径先声明真正 owner，再加入 parser 与 SQL 权限负例。

Cargo targets（名称/类型；不是用例数）：consumer_entry_api/lib; consumer-entry-api/bin。

本模块的三项交接验收关联：`consumer-entry-api:1`, `consumer-entry-api:2`, `consumer-entry-api:3`。

完整 Linux package 验证：`cargo test --locked -p consumer-entry-api --all-targets`；`cargo clippy --locked -p consumer-entry-api --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="hepta-research-league"></a>

## 14. hepta-research-league

责任岗位：`hepta-research`。既有契约：[hepta-research-league](../../docs/modules/hepta-research-league.md)。

**实现边界。** 研究、Agent key epoch、团队同意、贡献、Review Raid、申诉与 finality projection 保持独立。存储 reproducing 对作者展示为 reproduction_readiness；独立复现只能进入 Review Raid。更细规则由 service README 和相应协议控制。

**修改与测试路径。** 修改方法：为授权表列出作者、evaluator、两名独立 reviewer、reproducer 的可执行命令；测试同一人/Agent 换 UUID 不能占两个独立席位。变更 lease/fence/work version、manifest commitment 和 key epoch 应失败。

**兼容与运行限制。** 贡献 ID/manifest 唯一性、冻结作者集合与提交保留在事务内。完整 PostgreSQL suite 还要验证重启、并发 claim、wrong-owner ACK、过期恢复；pending_finality 不得放开排名、支付或发布。

Cargo targets（名称/类型；不是用例数）：hepta_research_league/lib; hepta-research-league/bin; hepta-trnm-command-signer/bin; hepta-paper-raid-fixture/example; http_flow/test; nakama_authorization_contract/test; paper_collaboration_contract_golden/test; paper_raid_contract_golden/test; paper_review_contract_golden/test; postgres_recovery/test; research_control_golden/test; research_session_golden/test; research_workflows/test。

本模块的三项交接验收关联：`hepta-research-league:1`, `hepta-research-league:2`, `hepta-research-league:3`。

完整 Linux package 验证：`cargo test --locked -p hepta-research-league --all-targets`；`cargo clippy --locked -p hepta-research-league --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="paper-raid-bff"></a>

## 15. paper-raid-bff

责任岗位：`paper-raid-edge`。既有契约：[paper-raid-bff](../../docs/modules/paper-raid-bff.md)。

**实现边界。** Alpha edge 拥有 session、CSRF、invite、pairing digest 和 assertion replay，而非科研或 Nakama 状态。固定 Alpha 和 invite Alpha 身份源互斥；最低角色拓扑是三位作者和四位独立审查人员。Quick Raid 预览不能成为 portable/finality/economy 权威。

**修改与测试路径。** 修改方法：验证 session/revocation generation、一次性 CSRF、邀请签发/兑换/暂停/撤销/轮换和 quota；同时跑浏览器 E2E、移动端可访问性和 SQL catalog drift。schema owner 只用于一次性 operator 操作，不驻留 BFF。

**兼容与运行限制。** OIDC foundation 不等于公开登录。公开 Beta 必须独立验收真实 issuer/JWKS/token exchange/callback、HTTPS/Secure Cookie、账户恢复及滥用处置；禁止只将 Alpha 地址改为公网。

Cargo targets（名称/类型；不是用例数）：paper_raid_bff/lib; paper-raid-accessctl/bin; paper-raid-bff/bin。

本模块的三项交接验收关联：`paper-raid-bff:1`, `paper-raid-bff:2`, `paper-raid-bff:3`。

完整 Linux package 验证：`cargo test --locked -p paper-raid-bff --all-targets`；`cargo clippy --locked -p paper-raid-bff --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="matrix-entry-adapter"></a>

## 16. matrix-entry-adapter

责任岗位：`matrix-integration`。既有契约：[matrix-entry-adapter](../../docs/modules/matrix-entry-adapter.md)。

**实现边界。** 传输迁移到 0005，operator 迁移到 0006；runtime 只可执行 v3。adapter 校验 relay headers、canonical payload hash，拒绝用户带入保留绑定再注入八字段 commitment。恢复查询必须 read-only。

**修改与测试路径。** 修改方法：同时测试 source contract、完整 Rust facade、服务 HTTP 边界、SQL role 与真实 CLI。新增 test-matrix-cli-postgres.py 专门覆盖真实 CLI→psql→v3 的组合，HTTP 端仍为显式 fixture，不得写成真实 homeserver E2E。

**兼容与运行限制。** 迁移顺序是权限契约。先运行历史阶段回归再施加后续 revoke；在最终 schema 上不能为旧测试恢复 v1/v2 runtime 权限。事故时保留投递/观察，不能删表重置解决未知 outcome。

Cargo targets（名称/类型；不是用例数）：matrix_entry_adapter/lib; matrix-entry-adapter/bin。

本模块的三项交接验收关联：`matrix-entry-adapter:1`, `matrix-entry-adapter:2`, `matrix-entry-adapter:3`。

完整 Linux package 验证：`cargo test --locked -p matrix-entry-adapter --all-targets`；`cargo clippy --locked -p matrix-entry-adapter --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="matrix-bot-relay"></a>

## 17. matrix-bot-relay

责任岗位：`matrix-integration`。既有契约：[matrix-bot-relay](../../docs/modules/matrix-bot-relay.md)。

**实现边界。** 持久化 ingress、delivery claim、adapter 调用、Matrix send receipt 分属不同事实。delivery UUID 同时用于 Matrix transaction ID。destination/account/credential scope 固定；变更 scope 不能作为跨 dedup 范围重发的借口。

**修改与测试路径。** 修改方法：对 adapter timeout/中断/未知 HTTP/坏 JSON/超限响应验证 dead-letter hold；真实 send 只接受 200、合法 event ID 且无 Matrix error。room 和回复字段必须绑定原事件。

**兼容与运行限制。** 恢复 adapter leg 不自动重放业务、不自动生成新 send。operator evidence 的新观察可追加，但 stable result 改变必须冲突；用户界面应区分业务已恢复和消息已送达。

Cargo targets（名称/类型；不是用例数）：matrix-bot-relay/bin。

本模块的三项交接验收关联：`matrix-bot-relay:1`, `matrix-bot-relay:2`, `matrix-bot-relay:3`。

完整 Linux package 验证：`cargo test --locked -p matrix-bot-relay --all-targets`；`cargo clippy --locked -p matrix-bot-relay --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="matrix-bot-poller"></a>

## 18. matrix-bot-poller

责任岗位：`matrix-integration`。既有契约：[matrix-bot-poller](../../docs/modules/matrix-bot-poller.md)。

**实现边界。** poller 以 PostgreSQL fence 和 opaque cursor 管理 /sync。受限 gap recovery 需保留原 account/filter scope；ID filter 先解析并核验 exact bytes digest，再用 pinned definition 执行 sync 与 /messages。

**修改与测试路径。** 修改方法：测试 start_now 初始只记游标不执行历史、page token 循环/空 chunk/缺失 end、错误 room、重复 JSON key、显式 null 和 bytes/time/page budget。毒消息证据必须先单独提交，未确认 poison 阻止 cursor 前进。

**兼容与运行限制。** 真正权限缺失或 redaction 后不可见历史不许虚构。大缺口超预算进入人工 hold，不能跳过。重启/多实例测试必须证明同一事件不重复创建业务且 lease 过期不能复活。

Cargo targets（名称/类型；不是用例数）：matrix-bot-poller/bin。

本模块的三项交接验收关联：`matrix-bot-poller:1`, `matrix-bot-poller:2`, `matrix-bot-poller:3`。

完整 Linux package 验证：`cargo test --locked -p matrix-bot-poller --all-targets`；`cargo clippy --locked -p matrix-bot-poller --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="trnm-economy-protocol"></a>

## 19. trnm-economy-protocol

责任岗位：`trnm-integration`。既有契约：[trnm-economy-protocol](../../docs/modules/trnm-economy-protocol.md)。

**实现边界。** 版本 2.4.0 通过 term-exchange-protocol alias 引入。entitlement signing_payload 清空 signature 后序列化；validate_shape 只检查形状，不验证真实 issuer 信任、当前有效期、nonce 消费或跨实例预算。

**修改与测试路径。** 修改方法：消费服务同时验证 issuer/key、签名、时间、subject/account、intent 和预算；相同 intent 重放安全，改变内容冲突。whole credits 只能 checked scale multiplication；默认 policy 值不等于实际执行累计预算。

**兼容与运行限制。** 该包不能借用另外四个 Chain crate 的 provenance。独立记录上游来源、许可、精确 revision、字节及消费方向量；来源未核验不能关闭验收。

Cargo targets（名称/类型；不是用例数）：trnm_economy_protocol/lib。

本模块的三项交接验收关联：`trnm-economy-protocol:1`, `trnm-economy-protocol:2`, `trnm-economy-protocol:3`。

完整 Linux package 验证：`cargo test --locked -p trnm-economy-protocol --all-targets`；`cargo clippy --locked -p trnm-economy-protocol --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="trnm-finality-types"></a>

## 20. trnm-finality-types

责任岗位：`trnm-integration`。既有契约：[trnm-finality-types](../../docs/modules/trnm-finality-types.md)。

**实现边界。** 该包拥有 receipt/quorum/proof 词汇。legacy FinalityReceipt 与 CometBFT AppHash proof 是不同类型与验证链；正确反序列化并不意味着 validator set 已获信任。

**修改与测试路径。** 修改方法：每个字段编码、domain separator、ordered hash inputs 与 version 都有跨语言正反向量。解析失败不能替换成零 hash、空 proof 或默认 checkpoint。

**兼容与运行限制。** 消费服务保存原始 receipt、协议版本和独立配置的 trust anchor。更新可信 checkpoint 的并发/恢复规则由 owner 执行，不能从待验证 receipt 自举。

Cargo targets（名称/类型；不是用例数）：trnm_finality_types/lib。

本模块的三项交接验收关联：`trnm-finality-types:1`, `trnm-finality-types:2`, `trnm-finality-types:3`。

完整 Linux package 验证：`cargo test --locked -p trnm-finality-types --all-targets`；`cargo clippy --locked -p trnm-finality-types --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="trnm-finality-verifier"></a>

## 21. trnm-finality-verifier

责任岗位：`trnm-integration`。既有契约：[trnm-finality-verifier](../../docs/modules/trnm-finality-verifier.md)。

**实现边界。** 同时包含 verification library 和 Unix trnm-research-receipt-v2 CLI。签名工具产生的数据不是独立证明；验证只相对于明确输入的 trust anchor 有效。Windows 的 library 例外不扩展为 Unix utility 已被验证。

**修改与测试路径。** 修改方法：验证 chain/header/height、validator identity、quorum signature、transaction/object Merkle 关系和 receipt hash；用错误 anchor、过期 trust context、文件替换与中断输出测试失败关闭。

**兼容与运行限制。** 上游 manifest 和允许的 test-only overlay 是两种独立身份。运行逻辑的 vendor 改动不能通过沿用 test-only patch policy 授权；完整 Linux targets 与实际 CLI 是不同测试义务。

Cargo targets（名称/类型；不是用例数）：trnm_finality_verifier/lib; trnm-research-receipt-v2/bin。

本模块的三项交接验收关联：`trnm-finality-verifier:1`, `trnm-finality-verifier:2`, `trnm-finality-verifier:3`。

完整 Linux package 验证：`cargo test --locked -p trnm-finality-verifier --all-targets`；`cargo clippy --locked -p trnm-finality-verifier --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="trnm-protocol"></a>

## 22. trnm-protocol

责任岗位：`trnm-integration`。既有契约：[trnm-protocol](../../docs/modules/trnm-protocol.md)。

**实现边界。** CanonicalTx/ResearchTx 保存 sender、nonce、gas/fee 和签名 command bytes。canonical_bytes 的输出是协议；from_canonical_bytes 会比较重新编码后的字节，不能用对象相等替代字节相等。

**修改与测试路径。** 修改方法：对 field order、whitespace、escaping、重复/未知字段、数值拼写、signed payload 与 outer wrapper 的绑定分别生成负例；保留原 nonce 的响应丢失恢复。

**兼容与运行限制。** 编码检查不消耗 nonce、不执行 Chain、不产生 finality。升级同时验证已接受交易及 applied-record 版本，不能改写旧签名体。

Cargo targets（名称/类型；不是用例数）：trnm_protocol/lib。

本模块的三项交接验收关联：`trnm-protocol:1`, `trnm-protocol:2`, `trnm-protocol:3`。

完整 Linux package 验证：`cargo test --locked -p trnm-protocol --all-targets`；`cargo clippy --locked -p trnm-protocol --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

<a id="trnm-research-protocol"></a>

## 23. trnm-research-protocol

责任岗位：`trnm-integration`。既有契约：[trnm-research-protocol](../../docs/modules/trnm-research-protocol.md)。

**实现边界。** 确定性 CBOR 使用固定 discriminant、最短整数/长度、定长数组与 32-byte hashes；不接受 map、float 或不定长元素。authority set 明确限制 signer DID/role/key，Nakama match 签名不等于 Hepta 评估。

**修改与测试路径。** 修改方法：测试 canonical roundtrip、未知 tag、非最短编码、尾随 bytes、AlteredReplay 与 snapshot graph 完整性；restore 应恢复 authority 与 replay history，而不只恢复投影。

**兼容与运行限制。** 跨系统共同约定命令版本、hash namespace、签名材料、应用顺序与 snapshot migration。库内 apply 成功不等于外部 Chain 已接受。

Cargo targets（名称/类型；不是用例数）：trnm_research_protocol/lib; protocol_v1/test。

本模块的三项交接验收关联：`trnm-research-protocol:1`, `trnm-research-protocol:2`, `trnm-research-protocol:3`。

完整 Linux package 验证：`cargo test --locked -p trnm-research-protocol --all-targets`；`cargo clippy --locked -p trnm-research-protocol --all-targets -- -D warnings`。仅编译为依赖不会运行该依赖自己的 unit tests。

## 独立交接验收

评审者选择一个实际变更，记录触及的公共字段/状态/数据库对象、旧客户端影响、失败恢复步骤、
精确 test target 与已执行结果。存在引用路径但字段含义不清、操作步骤需要作者口头补充，
或没有真实覆盖负向/恢复路径时，仍不得把 detailed-design 标为接受。
本补充不承诺全部 route 参数与配置名称已全量提取；这些需要在完整 checkout 的源码 inventory、
服务测试与人工审查之间逐项闭合。

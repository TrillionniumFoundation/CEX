# CEX 全量整改执行与验收方案

状态：待审查的执行契约，不是完成证明。生产授权：`not_granted`。

本方案承接 2026-09-12 的逐模块审阅，基线是 `74418fcb0ec6c179902ee593dafab2b73055a66c`，tree 为 `0a07f71247bb18ac70927437a3490c8a02ad3b9a`。这个 SHA 仅说明问题识别来源，不是新整改提交的资格证据。既有 `docs/index.md`、已接受 ADR、v12 计划与当前外部证据契约继续优先。本文件不得另立候选冻结点、缩减必需检查或改变生产批准权。

## 1. 顺序、依赖与完成定义

优先级不是强制的全局串行：P0 runner/治理先处理；不依赖真实运行的文档修正可以并行准备，但不能越过其最终执行验收。P2 风险重构必须等待相应 P1 行为基线通过。最终人类 go/no-go 是 P0 级发布阻断项，但位于依赖链末端。

逐项状态只在独立跟踪系统中依据证据推进：开放 → 方案/代码已提交 → 局部验证 → 当前候选执行通过 → 独立验收。这里的机器清单保留 `open` / `not_reviewed`，不把源代码中的布尔值作为完成证明。结构检查返回 `ok` 只表示计划结构与映射有效，绝不表示整改完成。

验收必须注明：要求编号、被测提交/tree/base/实际 merge、工具链和依赖、迁移与配置、测试输入与预期、实际结果、日志及产物摘要、执行与审核身份、未覆盖边界。出现新提交或合并对象变动时，按照既有精确候选规则重新资格化；不转移旧 SHA 的信用。

## 2. 全量工作包

机器清单：[acceptance-plan-v1.json](acceptance-plan-v1.json)。所有角色是责任岗位，具体人员由仓库维护者在跟踪项分配；这不是声称某人已经接单或批准。

| 编号 | 优先级 | 责任岗位 | 范围 | 验收核心 |
|---|---|---|---|---|
| P0-01 | P0 | `runner-owner` | administrative | Restore the required runner path；Read both queued job labels, runner-group access, online/idle state and queue cause. |
| P0-02 | P0 | `repository-admin` | administrative | Apply and read back protected admission；Read the effective policy including inherited rules and every bypass actor. |
| P0-03 | P0 | `release-owner` | mixed | Verify one immutable candidate；Bind source, tree, base, actual merge, toolchain, lock, migrations, workflow attempts and artifact digests. |
| P1-01 | P1 | `matrix-integration` | repository | Converge Matrix documentation；Relay, Adapter, Consumer, canonical CLI and migrations agree on v3/0006. |
| P1-02 | P1 | `hepta-research` | repository | Converge research state semantics；Keep stored reproducing; present author reproduction_readiness and assign independent reproduction to Review Raid. |
| P1-03 | P1 | `platform-foundations` | repository | Complete all module detailed designs；All actual Cargo members and discovered targets map to contracts and accountable owner roles. |
| P1-04 | P1 | `ledger-authority` | mixed | Qualify the exact-money end-to-end chain；Ingress/reserve/execution/consume-or-refund/Audit share stable immutable operation identities. |
| P1-05 | P1 | `matrix-integration` | mixed | Qualify actual Matrix v3 recovery；Run actual canonical CLI as the runtime role after complete migrations, including honest replay with fresh envelopes. |
| P1-06 | P1 | `hepta-research` | mixed | Qualify the independent research workflow；Three author roles and distinct independent evaluator/reviewers/reproducer complete the intended flow. |
| P1-07 | P1 | `database-operations` | mixed | Unify multi-schema migration acceptance；Inventory global, Hepta, BFF, Matrix transport/operator and TRNM bootstrap streams from actual source. |
| P1-08 | P1 | `identity-control-plane` | mixed | Unify identities and configuration；Specify issuer/principal, tenant, credential scope, TTL, rotation/revocation and cache propagation for every boundary. |
| P1-09 | P1 | `sre-capacity` | external | Freeze numerical performance and recovery targets；Approve workload, topology, dataset, concurrency, throughput, p95/p99 latency, queue age, RTO/RPO and endurance targets before execution. |
| P1-10 | P1 | `sre-capacity` | external | Execute representative load and disaster recovery；Retain load generation, monitoring, fault schedule and restore/PITR artifacts with exact candidate and topology binding. |
| P1-11 | P1 | `paper-raid-edge` | mixed | Define and qualify Alpha-to-public-Beta promotion；Keep loopback/SSH Alpha separate from public HTTPS, real OIDC/JWKS, account recovery, abuse controls and support. |
| P1-12 | P1 | `trnm-integration` | mixed | Complete provenance and bounded advisory disposition；Record independent provenance for trnm-economy-protocol without borrowing the four-crate Chain manifest. |
| P1-13 | P1 | `integration-owner` | external | Accept the cross-repository tuple；World fixture, Game/Nakama, Chain, CEX and contract bytes use immutable accepted identities. |
| P2-01 | P2 | `matrix-integration` | repository | Replace runtime monkey-patching with explicit composition；Propose a typed transport/validation/SQL boundary with direct v3 function construction. |
| P2-02 | P2 | `platform-architecture` | repository | Reduce internal coupling and review size；Separate bounded domains and fixtures before creating additional services. |
| P2-03 | P2 | `documentation-owner` | repository | Maintain living technical specifications；Generate supported API/configuration inventories from source and validate linked schema/examples. |
| P0-04 | P0 | `accountable-release-owner` | external | Decide final production go/no-go；Verify all applicable V12-X1 through V12-X8 records, issuer independence, custody, retention and revocation. |

## 3. 开发文档的类型化完成标准

服务、应用及适配器必须提供以下可定位内容，而非仅出现相应标题：

| 维度 | 具体交付 | 拒绝条件 |
|---|---|---|
| 权责 | 唯一事实所有者、信任边界、上下游和不属于本模块的行为 | 缓存/投影/HTTP 成功被写成业务权威 |
| 接口 | 每条受支持接口的字段类型、必填/可空、范围、认证、租户、例子、错误码 | 只写“应当校验”；规范和实际路由不对应 |
| 状态机 | 状态、命令、主体、前置条件、事务效果、版本、失败和恢复 | 枚举名字相同但权限/展示语义不同且未映射 |
| 持久化 | 表/索引/约束/锁/隔离级别、迁移所有者、幂等及 outbox 原子性 | 仅靠进程内状态保证多实例唯一性 |
| 配置 | 变量、默认值、优先级、上下界、别名、敏感性、重载和启动失败条件 | 示例占位值被当作生产配置；各模块优先级冲突 |
| 安全 | 认证与授权、密钥/撤销、输入大小、重放、脱敏、最小权限 | caller 字段替代认证主体；管理凭证驻留运行服务 |
| 验证 | 每项不变量到真实测试、正反例、故障窗口及执行门禁的关联 | 文件存在、注释关键词、零步骤、跳过或旧提交被计为通过 |
| 运维 | readiness、数值 SLO/告警、诊断查询、停写/隔离、恢复与回退 | 只写“监控队列”“支持回滚”而没有安全操作边界 |
| 兼容 | 读/写/存储/展示版本矩阵、旧新实例共存、退役与迁移 | 根据时间或名称自动推定兼容 |

非部署库采用对应标准：公共 API/类型、确定性编码与签名帧、错误映射、边界值、并发/全局状态、消费方兼容向量、来源/许可证和变更纪律。确实不适用的部署/数据库项应写原因，不复制监听器、数据库和远程调用模板。

每模块至少完成机器清单中的三个特定验收主题；主题是需要展开为多用例的工作，不是已经存在或已经通过的测试。全部 23 个模块都必须由非原始实现者完成一次基于文档的修改、测试或恢复走查。数量覆盖和技术深度分别报告。

## 4. 跨模块端到端验收

### 资金链路

在入口 → Gateway reserve → Execution → Ledger consume/refund → Audit 全链路保留同一不可变操作映射。覆盖远程调用前后崩溃、响应丢失、租约过期、并发终态、重复与碰撞、跨租户和对账后重放。断言不仅检查 HTTP，还检查余额、预留、唯一终态、原始意图和审计链。未知结果不创建第二个操作，不凭超时推定失败。

### Matrix 链路

真实 homeserver → poller → durable relay → Adapter → Consumer → 真实 CLI → PostgreSQL v3。全量 operator migrations 后，以仅继承 `cex_matrix_reconciler_runtime` 的凭证调用真实入口。正确恢复只关闭原 adapter 投递；新观察可追加，稳定结果不变。分别改变 task/invocation、delivery、payload、event、room、principal、fingerprint 和嵌入绑定均必须拒绝。运行身份不能执行 v1/v2 或直接修改 transport 表。Matrix 消息是否发送另以不可变 send receipt 证明，不能把 adapter `sent` 混为回复已送达。

### 研究链路

作者准备与独立评审/复现分离。覆盖七个人类身份所需的三个作者岗位及独立 evaluator、两名 reviewer、reproducer；外部 Agent 拥有独立密钥。租约、key epoch、贡献唯一性、评审独立性、申诉/重做、待最终性与发布同意均不能被边缘投影替代。CI 的合成身份不证明真实参与者的独立性。

## 5. 多 schema 升级与灾难恢复

从实际源码登记全局迁移、Hepta、BFF、Matrix transport、Matrix operator 与 TRNM bootstrap；记录每套 head/hash、数据库/角色、先后依赖、停写与锁预算、兼容二进制和前向修复方案。不得仅以全局 0088 宣称全系统已迁移。

在可销毁且明确授权的数据库上执行新安装、含历史数据升级、不能安全自动回填的历史行、重复执行、升级中断、旧新服务并存、运行角色拒绝 DDL/历史修改、恢复后应用级对账。代表性恢复/PITR 必须使用独立批准的数据规模、存储和密钥拓扑；不得触碰生产或使用生产秘密制作公开工件。

## 6. 数值 SLO 与容量验收

不得把缺少业务输入时随意填写的延迟、吞吐或 RTO 当作项目要求。性能负责人须在测试之前签定：场景、数据量/增长率、租户分布、并发、到达率、持续时间、冷/热缓存、依赖失败模型、p95/p99 延迟、错误预算、最大队列年龄、积压清空时限、各故障域 RTO/RPO 与成本边界。任何字段为空即尚未冻结，不能通过生产容量门槛。

分别记录业务目标、代表性资格测试实测值、生产观测值。对进程、主库、可用区/整站损失分别解释 RPO=0 依赖什么确认写入和复制机制。吞吐达标但出现重复资金效果、丢失审计或错误最终性时，整体仍失败。保留负载生成器版本、时间同步、故障注入时点和原始指标。测试后不得倒改阈值来获得通过。

## 7. 当前实现批次与剩余边界

本批次修正 Relay 的 v3/0006 文档与 Hepta 作者复现准备语义，增加全模块验收映射和结构/语义回归检查。没有声称完成其余模块字段级规范、真实 Cargo/数据库/Matrix 执行、保护设置、独立审查、性能或生产批准。

运行补充检查：

```text
python3 scripts/check-remediation-acceptance.py --contract-only
python3 scripts/check-remediation-acceptance.py --self-test
```

检查在既有 development-document 入口中运行，不建立新的 workflow 或 status 来替代原门禁。它只证明清单和两项限定语义规则；不证明完整运行语义。测试命令不读取凭证，不启动服务，不修改数据库，也不自动提交、合并或部署。

新源树仍需原有全量 Cargo、格式、严格 Clippy、数据库、供应链、模块、集成与真实 prospective-merge 门禁，以及新鲜独立审查。最终 production authorization 始终由既有 V12 外部证据与负责人的明确决定控制。

## 8. 第二批交付与执行入口

- [全工作区开发交接](developer-handoff-v1.md)：23 个包的源码/类型、真实构建目标、修改与测试路径；仍需逐模块独立走查。
- [真实 Matrix CLI / PostgreSQL 回归](matrix-cli-postgres-v1.md)：实际进程与普通 LOGIN，区别于手写 SQL 回归；HTTP 对端明确为合成 fixture。
- [系统级演练矩阵](system-rehearsal-v1.md)：多 schema、资金、Matrix、研究、产品和容量恢复的具体操作与拒绝条件。

新增回归接入既有 Matrix 工作流，不替代原有全量检查。文档提交、self-test、实际数据库回归、真实服务演练和独立接受分别记账。没有独立生产证据时，所有发布权仍保持原限制。

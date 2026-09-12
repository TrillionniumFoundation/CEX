# 系统级验收操作矩阵：部署、故障、恢复与产品边界

Status: proposed rehearsal protocol; no scenario is marked passed by this file  
Production authorization: `not_granted`

本文件深化 P1-04 至 P1-13 的操作步骤，不能代替既有迁移、角色、版本与外部证据契约。
所有破坏性动作仅能在获得明确授权、已隔离的可销毁或代表性演练环境执行。
实际部署拓扑、业务负载与签字人没有确定之前不得推断通过。

## 1. 开始前的唯一被测对象

从实际 checkout 和 GitHub 对象取得 head/tree/base/prospective merge，并记录工具链、Cargo.lock、
镜像摘要、所有 schema head/hash、外部组件不可变 revision 和运行配置摘要。凭证只记录 ID/版本，不保存值。
确认演练目标与生产隔离、可恢复、没有生产流量和生产钥匙。任何必需身份或 schema 缺失先停止，不走备用弱身份。
记录准入关闭、运行角色分离、负载停止及恢复负责人。不能把 PR mergeable 当作有权部署。

## 2. 多 schema 的真实部署视图

| 数据范围 | 已知源码入口/版本边界 | 启动与验收责任 |
|---|---|---|
| 全局控制平面 | `migrations/`，当前编号终点 0088 | 核对连续编号及 chain digest；fresh 和 existing-row 升级；Ledger/Gateway/Execution/Audit 的权限及原子性 |
| Matrix transport | `services/matrix-entry-adapter/migrations/`，0001–0005 | 游标 fencing、inbox/outbox、source observation、poison、stream/filter 和 send receipt |
| Matrix operator | `services/matrix-entry-adapter/operator-migrations/`，0001–0006 | 必须先有 transport；v3 接受、v1/v2 运行权限拒绝、最小权限实际 CLI |
| TRNM settlement | `services/trnm-economy-service/migrations/settlement_v1.sql` | 独立 bootstrap，不属于全局编号终点；不可变意图字节及 receipt recovery |
| Hepta | 从 service README 与实际启动/迁移实现导出，不能用全局0088替代 | 研究/Agent/协作/评审/贡献表的真实 head、parity、key epoch、lease 和角色 |
| Paper Raid BFF | 从 BFF README、db/access 和 operator CLI 导出 | session/access/quota/replay；schema/activation 与 runtime ACL 共同匹配 |

Hepta/BFF 不能仅凭本表认定枚举完所有 SQL：执行者必须输出实际文件列表和 hash，并与启动实现对照。
扩展本 inventory 时保留独立 stream，不发明一个全局版本号掩盖不同 schema 的兼容窗口。

一次完整升级依次完成：停止新准入和相应 claims；确认已知/未知远程结果；备份及回读验证；
按依赖应用 expand 迁移；用各运行角色检查实际 grants 和 readiness；启动兼容消费者与 worker；
验证已有记录与新请求；经批准恢复准入。历史行无法安全 backfill 时必须产生明确 hold，不可补造签名/精度。

| 升级用例 | 注入/操作 | 接受断言 |
|---|---|---|
| 新建 | 空的授权测试数据库 | 所有实际 stream 都存在，runtime 不能获得 schema owner 权限 |
| 历史行 | 含旧版本和不可自动补全记录 | 可兼容记录保留身份；不可恢复的签名/精度/ledger ID 不被猜测 |
| 中断重启 | 指定 migration 前、事务中、commit 后断开 | 要么未提交，要么完整提交；重试不重复有效事实 |
| 新旧实例并存 | 在明确兼容窗口运行旧 reader/新 writer及反向组合 | 支持组合通过，禁止组合启动或请求失败关闭；不靠偶然字段默认值兼容 |
| 回退 | 停止准入和 claims，换回 schema-compatible binary | 不降级到已撤销 v1/v2 写入，不删除 append-only history，不新增副作用身份 |
| 恢复 | 从独立验证备份恢复到隔离目标 | 行数、金额、预留、操作身份、内容 hash、pending/unknown、Audit 和最终性投影全部对账 |

## 3. 资金链路：验收必须看持久化事实

建立固定测试租户与账户，只通过允许的 genesis/account-opening 契约给定初始值。
记录每个请求的 tenant、account、trace、invocation、reserve operation、terminal operation、scope/key、
金额/币种/scale 和 intent hash。总账结果以真实 Ledger 状态为准，不以边缘缓存或 HTTP 状态为准。

| 用例 | 故障窗口 | 接受断言 |
|---|---|---|
| MONEY-01 | 正常 reserve→外部工作→consume 或 refund | 初始与最终金额、预留和 append-only entry 对账；终态互斥；Audit可关联 |
| MONEY-02 | durable claim commit 前进程退出 | 没有未持久化的远程调用；恢复保持同一操作身份 |
| MONEY-03 | claim 已提交、远程调用前退出 | 严格按状态/租约规则处理；不以过期本身证明未执行 |
| MONEY-04 | Ledger 已执行、响应丢失或 outcome 事务失败 | 同一 intent receipt lookup/replay 恢复，不能新建第二次 charge/refund |
| MONEY-05 | 两实例同时 consume/refund、旧 owner ACK | 数据库只能接受合法终态及 owner；不能双消费或消费后再退款 |
| MONEY-06 | 相同 key相同内容，随后同 key改金额/租户 | 原结果重放；改内容拒绝且余额/记录不变 |
| MONEY-07 | 依赖不可达、预算耗尽或审计压力 | 明确错误/积压/hold，不用内存或 legacy float 路径冒充成功 |
| MONEY-08 | 恢复后重复旧请求及补发Audit | 原始身份保留，审计不丢不改；任何未解释差异令整体验收失败 |

测试工具必须把注入时刻与数据库/服务日志对应；仅 kill 进程但不知道发生在哪个窗口不算覆盖该分支。
潜在执行的外部 Agent/provider 超时只能保留未知结果；确认未执行需要外部证据，而不是第二次调用验证。

## 4. Matrix：分开业务结果和消息发送

先运行 [真实 CLI 数据库回归](matrix-cli-postgres-v1.md)，再执行真实服务链路演练。
每个原 event/delivery/payload/task 关联只允许唯一业务效果；游标前进必须与持久化 admission 一致。

| 用例 | 操作 | 接受断言 |
|---|---|---|
| MATRIX-01 | 限量timeline、多页gap、空继续页、循环token、过滤器变更 | 可恢复页保持范围；越界、预算耗尽或错scope不推进游标 |
| MATRIX-02 | poison出现后重启或后续sync不再返回该事件 | poison仍阻断；授权 quarantine保留原始受限证据，不暗中执行 |
| MATRIX-03 | Consumer已提交，屏蔽Adapter响应 | relay标记unknown hold；真实CLI取回原绑定结果；业务不再发起 |
| MATRIX-04 | 首次恢复提交后CLI输出丢失，再运行CLI | 新观察可追加，稳定结果仅一条终态转换 |
| MATRIX-05 | 同event但换payload/task/room/principal，或漏invocation | 每个边界拒绝；缓存event命中不授予恢复权 |
| MATRIX-06 | Matrix send成功、回执响应或本地ACK丢失 | 保持原transaction ID和send scope；错误凭证/endpoint切换必须hold |

真实演练保留 Adapter/Consumer/Relay 的关联证据与发送回执，去除token及私人消息正文。
不得用 MATRIX-03 的 adapter sent 声称 MATRIX-06 的用户消息已送达。

## 5. 研究与产品可玩性

使用三位独立作者覆盖 Captain/Evidence/Experiment，加上不属于作者组的 evaluator、
两位reviewer、reproducer；真实独立性需要人类治理而不是测试UUID即可证明。
每位参与Agent有独立私钥托管、绑定和epoch，CEX不运行模型也不保管Agent私钥。

依次演练组队/全员同意、预注册、材料与实验谱系、草稿、完整性检查、作者复现准备、
作者批准、Review Raid独立评审/复现、拒绝后重做或申诉、bundle冻结、pending或独立验证finality。
跨越每个阶段时检查实际命令主体、lease/fence/version、签名快照、贡献唯一性和追加式记录。
被撤销Agent key、过期评审租约、重复贡献、作者冒充独立评审、改变旧bundle必须拒绝。
论文完成、Nakama completed、Chain finalized和PublicationRelease分别接受，不能一项推出另一项。

公开Beta另需真实HTTPS、Secure cookie及CSRF检查、IdP签名/JWKS轮换与撤销、账号恢复/封禁、
租户/配额/滥用、保留/删除政策、用户支持和凭证托管。Alpha loopback/SSH及OIDC foundation不替代这些。
没有公开Beta授权时，演练不得开放公网入口、市场、奖励或托管能力。

## 6. 预先冻结容量与恢复目标

SRE与业务负责人在运行前签定每个指标的数值、单位、统计窗口和适用故障域。以下是必填输入，
不是本文件代为批准的数值：数据量及增长率、租户分布、到达率/并发、持续时长、冷/热缓存、
p95/p99延迟、错误预算、队列最大年龄、清空积压期限、成本预算，以及进程/主库/区域损失各自RTO/RPO。
空值或只写“高性能”意味着未冻结；CI小数据通过不能补这个缺口。

验收记录比较预先批准的目标与实测值；负载生成器、拓扑/副本/确认写入模式、storage、时钟偏差、
故障注入、WAL/PITR目标时刻和恢复程序都有版本。分别测量失效发现、隔离、恢复可读、恢复可写和全部对账时间。
RPO=0只在所声明的确认与复制故障模型成立时接受；全站损失不能从单进程恢复结果推断。

任何重复资金效果、committed事实丢失、Audit不一致、越权或错误finality立即判失败，即便延迟达标。
先停止准入、保留证据、分析，再通过新的批准对象重测；禁止修改阈值追求绿色结果。

## 7. 签收与P2前置条件

每项记录 requirement ID、候选tuple、环境/迁移/config摘要、输入和预期、实际执行与原始日志/产物摘要、
执行者及独立审核者、残余风险。运行者不能把自己的文档或fixture签成外部事实。
P2动态wrapper替换等重构须先取得相应P1真实行为基线，再证明改前改后正反例一致；不靠删除旧失败路径过关。
代表性恢复、部署/rollback、外部Agent/Matrix、托管、endurance、独立安全/财务/法律和最终go/no-go
仍按既有V12-X1–X8接受。Issue保持开放直到证据可验证，源代码中的状态字段不授予发布权限。

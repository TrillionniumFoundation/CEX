# Migrations

本目录用于存放数据库迁移文件。

当前策略：
- 先保留 `deploy/sql/schema-v1.sql` 作为一次性全量草案
- 再逐步拆分为按时间戳排序的 migration 文件

建议命名：
- `0001_init_core_tables.sql`
- `0002_add_policy_tables.sql`
- `0003_add_provider_configs.sql`

当前原生 TRNM 经济迁移已推进到
`0029_add_trnm_value_entitlements_and_player_sessions.sql`。后续变更只通过
排序后的增量 migration 追加；不得回写已部署迁移的历史语义。

Hepta Paper Raid 当前增量迁移推进到
`0054_bind_challenge_pack_activation_chain_proof.sql`。0048 对已有 V1 finality
projection 明确失败关闭，不从其他表猜测丢失的 reproduction/resolution
绑定；此类记录必须由运维执行显式重新验证。0050 增加不可变 Author
rework lineage；0051 在不回写已部署 0040 的前提下，把 legacy evaluation
的精确三人 frozen panel 原子推进为 claimed → pinned → consumed。0052 不
回写 0038，以独立、可重放的 catalog-pinned ENABLE ALWAYS ingress guard
拒绝全零 preparation digest/raw hash、非 canonical Chain ID，以及任何
relational/JSON 投影漂移。该 guard 还在 source seal 之前拒绝 PostgreSQL
可解析但非 Chrono serde `DateTime<Utc>` AutoSi/Z 的时间别名，以及
preparation、binding、optional supersession/appeal 和已存在 rework lineage 中
任何非小写、带连字符的 canonical UUID 字符串，防止合法 cast
别名被不可变行封存后又被 Rust 原始 JSON parity 永久拒绝。
因 PostgreSQL `timestamptz` 只保留微秒，Chrono AutoSi 中必然携带
非零亚微秒的 9 位小数时间也在入库前 fail closed；`created_at`
只允许 00–23 时、00–59 分和 00–59 秒（不接受 PostgreSQL 会归一化的
`24:00:00`/闰秒别名），且必须精确等于 final checkpoint 毫秒。
0052 同时锁定 preparation/
binding 的完整 key set 和 JSON scalar type：所有不使用
`skip_serializing_if` 的 optional 键必须以字符串或 JSON `null`
显式存在，而缺省 rework lineage 必须完全缺席（不得用
JSON `null`）；已存在 lineage 则必须是精确 13-key 对象。
0053 不回写已部署的 0033，只把 artifact manifest 的 binding authority
精确扩展到旧 Author schema 与 Review-ready schema 两个固定值；运行时启动
会核对全局唯一约束、目标表、约束类型、验证/继承标志、列集合和完整定义。
0054 不回写已部署的 0049，只在尚未产生旧格式 activation record 时前向增加
strict Review Chain proof manifest/fileset 与 terminal bundle 的不可空绑定，并重建
完整 evidence digest 约束和 relational/JSON parity guard；重复应用会核对列类型、
not-null/default 目录形状；任何旧格式记录、带不兼容记录的部分迁移或 bundle
schema 漂移均失败关闭。

CEX P0 production-baseline 的 canonical migration head 当前为
`0088_enforce_provider_terminal_evidence_binding.sql`。0065 为 Ledger effect
增加不可变 operation provenance；0066 建立 Invocation 与 reserve/consume/refund
的精确 durable contract。P0-N5 的 Execution 终态结算按职责拆成唯一编号的
0067–0072：schema/fingerprint、数据库 guard 与 append-only transition evidence、
exact enqueue 与 shadow promotion、短事务 claim/lease 和 verified receipt、
worker outcome persistence，以及 operator acknowledgement/requeue/status projection。
0075–0077 提供 exact account opening 与 inventory parity；0078、0081、0082、0084
收口 provider dispatch 与 unknown-outcome reconciliation；0085 强化 normalized
receipt projection 的 append-only/非负金额约束，0086 增加 TRNM native receipt 的
不可变事件证据，0087 增加 League/World Term Exchange receipt 的 hold→final
append-only history，0088 将 provider command immutable binding 与 terminal authority
下沉到 PostgreSQL：live worker 成功必须绑定 immutable target/model/done/output/hash，
operator reconciliation 成功必须绑定同 attempt 的 append-only `confirmed_executed`
artifact/result/hash；两类证据不可互相替代，并由 PostgreSQL transition/evidence guards 阻断旁路。
这些迁移必须按编号完整执行；不得把已拆分、已废弃的
`0067_add_execution_ledger_settlement_commands.sql` 重新加入链中。

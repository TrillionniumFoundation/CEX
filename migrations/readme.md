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
`0048_add_hepta_consumer_finality_v2.sql`。0048 对已有 V1 finality
projection 明确失败关闭，不从其他表猜测丢失的 reproduction/resolution
绑定；此类记录必须由运维执行显式重新验证。

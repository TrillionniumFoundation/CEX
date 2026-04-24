# Migrations

本目录用于存放数据库迁移文件。

当前策略：
- 先保留 `deploy/sql/schema-v1.sql` 作为一次性全量草案
- 再逐步拆分为按时间戳排序的 migration 文件

建议命名：
- `0001_init_core_tables.sql`
- `0002_add_policy_tables.sql`
- `0003_add_provider_configs.sql`

当前下一步：
1. 先将 schema-v1 拆成第一条初始化 migration
2. 后续变更只通过 migration 增量追加

# Migration-First Workflow v1

## 1. 原则

从现在开始，数据库结构演进以 `migrations/` 为主，`deploy/sql/schema-v1.sql` 作为参考快照，不再长期作为唯一真实来源。

## 2. 建议流程

### 新增结构时
1. 新建 migration 文件
2. 描述变更目的
3. 审查对现有 repository / service 的影响
4. 再修改代码

### 不建议
- 先改代码，最后补 migration
- 在多个 SQL 文件里手动同步同一结构

## 3. 当前状态
- 已新增 `migrations/0001_init_core_tables.sql`
- 后续新表、索引、约束都应通过 migration 追加

## 4. 下一步建议
- 引入 SQLx migrations 或统一迁移执行脚本
- 在 README 中增加数据库初始化步骤

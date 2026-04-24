# SQLx Adoption Plan v1

## 1. 当前目标

让 ledger-service 从 repository placeholder 进入到可连接 PostgreSQL 的第一阶段。

## 2. 当前新增内容

- workspace 增加 `sqlx`
- ledger-service 增加 `sqlx` 依赖
- PostgresLedgerRepository 增加：
  - `connect_from_env()`
  - `create_account()`
  - `get_account()`
  - `append_entry()`
  - `find_by_idempotency_key()`

## 3. 当前说明

这仍然属于 **第一批 repository 骨架实现**，不是完整持久化切换。

原因：
- main.rs 当前仍然使用 placeholder 构造
- API 层当前仍主要依赖内存态 state
- balance / reserved 还没有从数据库聚合
- 事务与并发控制还没接入

## 4. 价值

即使如此，这一步已经把以下最难的门打开了：
- 依赖引入
- repository 结构稳定
- SQL 入口位置稳定
- PostgreSQL 连接路径明确

## 5. 下一步建议

1. 让 main.rs 优先尝试 `connect_from_env()`，失败再回退 placeholder
2. 让 create_account 同时写 repository
3. 让 idempotency 检查逐步迁移到数据库层
4. 让 reserve / consume / refund 进入事务化改造

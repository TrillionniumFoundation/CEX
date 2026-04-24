# Ledger Repository Plan v1

## 1. 当前目标

为 ledger-service 引入从“内存实现”向“持久化实现”演进的中间层。

## 2. 当前新增内容

已引入：
- `LedgerRepository` trait
- `LedgerRepositoryHandle`
- `PostgresLedgerRepository` placeholder

## 3. 当前意义

这一步的价值在于：
- service 层不再默认永久绑定内存实现
- 后续接 SQLx / PostgreSQL 有清晰承接点
- 可以逐步把 API 层对内存状态的依赖替换掉

## 4. 当前状态

目前 postgres repository 还是 placeholder：
- 能读取 `DATABASE_URL`
- 但还没有执行真实 SQL
- 还没有事务与连接池

## 5. 下一步建议

1. 引入 `sqlx`
2. 为 accounts / ledger_entries 建 repository 方法实现
3. 把 create_account / get_account 先切到 repository 层
4. 再逐步改造 reserve / consume / refund

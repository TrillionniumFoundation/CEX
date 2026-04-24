# Ledger DB Cutover v1

## 1. 当前新增行为

ledger-service 现在开始进入“数据库优先尝试，内存回退”的过渡阶段。

### 启动阶段
- 服务启动时优先尝试 `PostgresLedgerRepository::connect_from_env()`
- 如果连接失败，则回退到 placeholder repository

### create_account
- 当前会先调用 repository `create_account()`
- 然后仍然写内存 state
- 这属于过渡期的“尝试落库 + 内存保底”模式

### get_account
- 当前会先尝试 repository `get_account()`
- 如果数据库层返回空或失败，再回退查内存 state

### append_entry
- 当前 reserve / consume / refund 时会尝试 `append_entry()`
- 同时仍保留内存 entries

## 2. 当前意义

这一步把项目从：
- “只有数据库骨架”
推进到：
- “数据库接入路径已经进入运行流程”

## 3. 当前限制

- create_account 的 repo 失败不会阻断 API
- account 余额仍来自内存态
- reserve / consume / refund 仍未事务化
- get_account 目前数据库返回的 balance/reserved 仍是占位值

## 4. 下一步建议

1. 明确 repo 写失败时是否要 fail fast
2. 把 accounts 的余额语义设计清楚
3. 让 reserve / consume 逐步进入数据库事务
4. 为 API 层增加 observability / error reporting

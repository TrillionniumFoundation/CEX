# Ledger Persistence Design v1

## 1. 目标

把 ledger-service 从内存存根逐步过渡到 PostgreSQL 持久化实现。

## 2. 先持久化哪些对象

### 2.1 accounts
需要持久化：
- account_id
- org_id
- account_type
- currency_unit
- status

注意：
当前内存版 `balance` / `reserved` 是聚合态。数据库版可以有两种方案：

#### 方案 A：账户表存聚合余额
优点：读快  
缺点：一致性维护更严格

#### 方案 B：账户表存基本信息，余额从 ledger_entries 聚合
优点：账本语义更纯  
缺点：读时更重

### 当前建议
MVP 阶段先采用 **折中方案**：
- `accounts` 存基础信息
- 余额可先由应用层根据 ledger_entries 聚合或额外缓存

### 2.2 ledger_entries
必须持久化：
- entry_id
- account_id
- direction
- amount
- reason
- reference_type
- reference_id
- idempotency_key
- created_at

## 3. repository 方向

建议先定义：
- `LedgerRepository`
- `AccountRepository`

### 示例职责
- create_account
- get_account
- append_ledger_entry
- find_by_idempotency_key
- list_entries_by_account

## 4. 事务要求

未来数据库实现至少要保证：
- reserve 的检查与写入在同一事务中
- consume 的检查与写入在同一事务中
- idempotency 检查与 entry 写入具有一致性

## 5. 当前限制
- 还未写具体 SQLx repository
- 还未处理并发竞争
- 还未引入事务边界抽象

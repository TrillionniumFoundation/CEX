# Database Persistence Plan v1

## 1. 当前目标

项目已经完成内存版骨架，下一阶段目标是逐步进入持久化。

优先原则：
1. 先让 schema 管理正规化（migrations）
2. 先持久化 ledger-service
3. 再持久化 gateway / execution

## 2. 为什么先做 ledger-service

因为 ledger 是最不适合继续停留在内存态的服务。原因：
- 涉及余额
- 涉及 reserve / consume / refund
- 涉及 idempotency
- 后续与 billing / audit 强关联

## 3. 建议持久化顺序

### Phase A
- accounts
- ledger_entries
- idempotency key 约束（先复用 ledger_entries）

### Phase B
- invocations
- executions

### Phase C
- approvals
- audit_events
- capabilities

## 4. 技术建议

### ORM / 数据访问
建议优先：
- `sqlx`

原因：
- 对 Rust 后台更可控
- 适合账本与严肃 schema
- 类型安全更强

### 当前路线
- 先写 repository trait
- 再写 postgres repository stub
- 再逐步把内存状态替换为数据库存储

## 5. 不建议的做法
- 一口气把所有服务都改成落库
- 在 repository 抽象还没想清楚时就到处写 SQL
- 把 schema.sql 和 migrations 并行维护太久

## 6. 当前下一步
- 为 ledger-service 增加 repository 设计
- 增加 postgres repository 草案
- 增加 migration-first 说明

# Schema Notes v1

## 1. 设计重点

当前 schema-v1 的目标不是一步到位，而是保证 MVP 阶段至少具备：
- 组织与用户
- credits 账户与账本流水
- capability 注册
- invocation / execution
- approval
- audit events

## 2. 当前取舍

### 简化点
- 没有拆 billing account 与 usage account
- 没有做复杂分润表
- 没有做 provider credential 管理
- 没有做 policy rule 表结构细化

### 刻意保留的约束
- 所有关键主键使用 UUID
- ledger_entries 支持 idempotency_key
- invocation / execution / audit 使用 trace_id 关联

## 3. 下一版建议
- 增加 policy_sets / policy_rules
- 增加 provider_configs
- 增加 execution_attempts
- 增加 capability_versions
- 增加 usage_metrics 聚合表

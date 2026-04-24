# Ledger Cutover Checklist v1

## 切库前必须明确

- [ ] 是否启用 `LEDGER_FAIL_FAST`
- [ ] 数据库 migration 是否已执行
- [ ] `accounts` / `ledger_entries` 表是否可写
- [ ] `DATABASE_URL` 是否正确
- [ ] create_account 是否已成功落库验证
- [ ] append_entry 是否已成功落库验证
- [ ] idempotency key 是否已至少具备数据库校验能力

## reserve / consume 改造前

- [ ] account summary 模型是否明确
- [ ] reserved 的持久化语义是否明确
- [ ] 并发场景下的校验策略是否明确
- [ ] 事务边界是否明确

## 切换后观察点

- [ ] create_account 失败率
- [ ] reserve 冲突率
- [ ] duplicate idempotency key 命中率
- [ ] 数据库连接错误率
- [ ] API 层是否还有内存回退路径

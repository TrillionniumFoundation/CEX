# Ledger Fail-Fast Code Cut v1

## 当前新增代码行为

ledger-service 现在已经真正读取：
- `LEDGER_FAIL_FAST`

## 当前规则

### 当 `LEDGER_FAIL_FAST=false`
- repository 写失败时，允许继续走内存回退路径
- 适合本地开发、数据库未完全接好时

### 当 `LEDGER_FAIL_FAST=true`
- `create_account` 的 repository 写失败会直接返回 500
- `append_entry` 的 repository 写失败会直接返回 500
- 不再默默以内存成功掩盖数据库失败

## 当前价值

这一步把 fail-fast 从“文档设计”推进到“真实代码行为”。

## 当前限制

- `get_account` 仍是优先数据库、失败回退内存
- reserve / consume / refund 仍未事务化
- balance / reserved 仍以内存模型为主

## 下一步建议

1. 把 `get_account` 的行为也分模式化
2. 把 transaction skeleton 接到 reserve / consume 真路径
3. 逐步减少内存保底路径

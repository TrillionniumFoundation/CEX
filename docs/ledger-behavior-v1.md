# Ledger Behavior v1

## 当前新增能力

ledger-service 现在已经具备以下更像账本的行为：

### 1. reserve 校验
- reserve 前检查 `available = balance - reserved`
- 如果可用余额不足，则返回错误

### 2. consume 校验
- consume 前检查 `reserved >= amount`
- 不允许消费未预留的额度

### 3. refund 行为
- refund 会增加 `balance`

### 4. idempotency key 骨架
- `reserve / consume / refund` 请求支持 `idempotency_key`
- 当前采用内存集合去重
- 如果重复则返回冲突错误

### 5. 基础错误模型
当前至少返回：
- amount must be positive
- account not found
- duplicate idempotency key
- insufficient available balance for reserve
- insufficient reserved balance for consume

## 当前限制
- 幂等键仍然是内存态
- 没有持久化事务
- 没有双重记账
- 没有 release 动作
- 没有跨服务回滚编排

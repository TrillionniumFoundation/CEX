# Ledger Transaction Design v1

## 1. 当前目标

为 reserve / consume / refund 的数据库事务化改造准备设计边界。

## 2. 为什么事务化很关键

账本动作不是普通 CRUD。至少要保证：
- 余额检查与写入一致
- 幂等检查与写入一致
- entry 追加与状态变化一致

如果没有事务，很容易出现：
- 重复扣费
- 只写 entry 没更新状态
- 余额检查通过但并发写穿透

## 3. 建议事务单元

### reserve
一个事务中完成：
1. 检查 idempotency key
2. 锁定 account 相关状态或余额来源
3. 校验 available >= amount
4. 写入 reserve 类型 ledger entry
5. 提交事务

### consume
一个事务中完成：
1. 检查 idempotency key
2. 校验 reserved >= amount
3. 写入 consume 类型 ledger entry
4. 提交事务

### refund
一个事务中完成：
1. 检查 idempotency key
2. 写入 refund 类型 ledger entry
3. 提交事务

## 4. MVP 阶段简化方案

在真正引入完整双重记账前，可以先采用：
- account summary + ledger_entries 并存
- reserve / consume / refund 在单事务里更新 summary + append entry

## 5. 后续演进

后续可以进一步走向：
- 双重记账
- append-only ledger
- 余额由投影/快照维护

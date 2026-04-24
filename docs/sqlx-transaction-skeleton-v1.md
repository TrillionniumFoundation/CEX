# SQLx Transaction Skeleton v1

## 当前新增内容

在 `PostgresLedgerRepository` 中新增：
- `reserve_transaction_skeleton()`
- `consume_transaction_skeleton()`

## 目的

不是现在就实现完整事务化账本，而是把事务入口和未来责任边界先固定下来。

## reserve skeleton 未来应完成
1. begin transaction
2. idempotency check
3. account lock / summary load
4. available balance validation
5. append reserve entry
6. commit

## consume skeleton 未来应完成
1. begin transaction
2. idempotency check
3. reserved balance validation
4. append consume entry
5. commit

## 当前价值

- SQLx transaction 入口位置明确
- 后续不需要再大幅重构 repository 结构
- 团队可以围绕这个 skeleton 逐步填充真正的账本事务语义

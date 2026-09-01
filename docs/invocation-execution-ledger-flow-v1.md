# Invocation -> Ledger -> Execution Flow v1

## 1. 当前目标

打通一个最小业务闭环：
- 创建 invocation
- 记录 trace_id
- 标记 reserve 意图
- 创建 execution stub

## 2. 当前实现状态

### gateway-service
在 `POST /v1/invocations` 中：
- 生成 `invocation_id`
- 生成 `trace_id`
- 如果请求中带 `account_id` 和 `reserve_amount`，则标记 `ledger_reserved = true`
- 生成 `execution_id` stub

### ledger-service
当前提供：
- account 创建
- reserve / consume / refund 接口

### execution-service
当前提供：
- execution 创建
- execution 查询
- execution 状态机说明

## 3. 当前说明

这是 **stub 级闭环**，不是分布式真实调用闭环。

也就是说：
- gateway 当前还没有通过 HTTP/gRPC 真正调用 ledger-service
- gateway 当前也没有真正调用 execution-service
- 当前先把领域闭环、接口形状和返回结构定下来

## 4. 下一步升级方向

1. gateway 调用 ledger-service 的 reserve API
2. gateway 调用 execution-service 创建 execution
3. 将 execution_id 从 execution-service 返回值中获取
4. 增加失败回滚与 refund 逻辑
5. 引入数据库与幂等键

## 5. 新增行为（2026-04-07）
- gateway 已不再把 reserve 失败伪装成正常 invocation 创建。
- 如 reserve 成功但 execution 创建失败，gateway 会主动调用 ledger `refund` 作为补偿动作。
- invocation 记录现包含：
  - `ledger_reserved`
  - `ledger_refunded`
  - `failure_reason`
- invocation 成功交给 execution-service 后，状态提升为 `Queued`；补偿成功则标记为 `Refunded`。

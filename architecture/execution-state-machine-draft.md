# Execution State Machine Draft

## 1. 状态目标

Execution 必须可观测、可恢复、可审计，并支持审批中断。

## 2. 建议状态

- created
- queued
- policy_check_pending
- policy_blocked
- awaiting_approval
- approved
- dispatching
- running
- succeeded
- failed
- cancelled
- timed_out
- refund_pending
- refunded

## 3. 状态流转示意

1. created -> queued
2. queued -> policy_check_pending
3. policy_check_pending -> policy_blocked | awaiting_approval | approved
4. approved -> dispatching -> running
5. running -> succeeded | failed | timed_out | cancelled
6. failed / timed_out / cancelled -> refund_pending -> refunded（若适用）

## 4. 设计要求

- 每次状态变更必须记录 audit event。
- 关键状态需要 trace id 关联。
- 与 ledger 关联的状态切换必须可回放。
- approval 流转必须与 execution 解耦，但可关联。

## 5. 后续补充
- 每个状态的进入条件
- 每个状态的退出条件
- 重试策略与次数限制
- provider error taxonomy

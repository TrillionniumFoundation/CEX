# ADR-003: Ledger Model

- 状态：Accepted
- 日期：2026-04-07

## Context
AI-native 平台需要一个稳定、可审计、可退款、可分账的 credits / billing 底座。

## Decision
采用“独立账本服务 + 账户模型 + 流水模型”的设计，不沿用传统交易所资产语义。

核心约束：
1. 所有余额变化必须通过账本流水表达。
2. 所有扣费相关接口必须支持 idempotency key。
3. invocation / execution 与账本之间通过 reference_type/reference_id 关联。
4. 支持 reserve -> consume -> refund 的基础闭环。

## Ledger 基础动作
- credit
- reserve
- release
- consume
- refund
- adjust

## Rationale
这套模型更适合：
- AI 调用预算控制
- 失败退款
- 审计追踪
- 后续收益分配

## Consequences
优点：
- 可审计性强
- 适合 credits 与 usage billing
- 易于与 execution 关联

代价：
- 前期需要更严格的数据模型纪律
- 对幂等设计要求较高

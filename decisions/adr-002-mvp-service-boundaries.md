# ADR-002: MVP 服务边界

- 状态：Accepted
- 日期：2026-04-07

## Context
项目需要在 MVP 阶段平衡：
- 可快速落地
- 边界清晰
- 后续可扩展

## Decision
MVP 阶段优先建设以下服务：
1. identity-service
2. ledger-service
3. gateway-service
4. execution-service
5. audit-service

以下服务在第二阶段进入：
- policy-risk-service
- capability-service

以下服务在第三阶段进入：
- marketplace-service

## Rationale
- identity、ledger、gateway、execution、audit 构成最小闭环。
- policy 和 capability 虽然重要，但初期可通过简化配置支持。
- marketplace 不应阻塞核心平台闭环。

## Consequences
优点：
- 有利于快速形成可运行产品
- 有利于缩小 MVP 范围
- 有利于先验证调用、扣费、审计闭环

缺点：
- 初期部分规则可能通过配置而非独立服务承载
- capability 管理能力前期会比较简化

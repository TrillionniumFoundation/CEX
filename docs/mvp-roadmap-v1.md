# MVP Roadmap v1

## 1. 目标

在 3~6 个月内做出一个可用的 Rust AI-native 平台 MVP，重点验证：
- 多租户
- credits / 预算扣费
- AI gateway
- invocation / execution
- policy / approval
- audit trace

## 2. 分阶段路线

### Phase 0（第 1~2 周）：架构定稿
目标：统一术语与边界

交付物：
- 领域模型初稿
- 服务边界确认
- Cargo workspace 设计
- ADR-002 / ADR-003
- 本地开发环境方案

### Phase 1（第 3~6 周）：核心骨架
目标：建立最小可运行骨架

优先服务：
- identity-service
- ledger-service
- gateway-service
- execution-service
- audit-service（简版）

关键能力：
- 用户/组织/API key
- credits 账户
- invocation 创建
- execution 状态流转
- 基础 trace 与日志

### Phase 2（第 7~10 周）：可用 MVP
目标：跑通真实调用闭环

新增能力：
- capability registry
- provider adapter
- routing policy
- approval checkpoint
- budget guardrail

至少打通：
- 1 个云模型提供商
- 1 个 tool 调用场景
- 1 条人工审批流程

### Phase 3（第 11~16 周）：平台化增强
目标：从 demo 变成平台

新增能力：
- capability 管理后台
- 组织级策略
- execution 查询与追踪
- billing trail
- 初步 marketplace 结构

## 3. MVP 成功标准

### 必须达成
- 一个组织内可创建多个用户
- 用户可通过统一网关发起 invocation
- invocation 可进入 execution 流程
- execution 可调用至少一个 AI provider
- credits 可预留、扣减、失败退款
- 敏感动作可被 policy 拦截或进入审批
- 每次执行都有 trace id 和审计记录

### 暂不要求
- 完整 marketplace
- 复杂分润
- 多区域部署
- 高级推荐系统
- 完整可视化运营后台

## 4. 风险点
- 过早微服务化
- 过早追求全自治 Agent
- 账本模型定义不清
- provider 适配层抽象过早复杂化
- policy 体系做得太弱或太重

## 5. 推荐节奏
- 先跑通调用和扣费闭环
- 再做审批和风控
- 再做平台化目录与市场层

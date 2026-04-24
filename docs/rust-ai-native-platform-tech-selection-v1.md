# Rust AI-Native 平台技术选型清单 v1

## 1. 项目目标

本项目目标不是改造为传统加密货币交易所，而是借鉴开源中心化交易所的工程能力，构建一个 **AI 原生平台**。

核心能力：
- 多租户账户体系
- Credits / 用量 / 预算控制
- 模型、Agent、工具、工作流统一接入
- 统一调用网关与路由
- 可审计执行链路
- 风控、审批、权限治理
- Marketplace 扩展能力

主开发语言：**Rust**  
项目根目录：`E:\CEX`

## 2. 总体选型结论

不建议直接继承完整开源 CEX，建议采用：

**Rust 自研主干 + 参考 CEX 架构 + 借鉴账本/工作流/策略引擎设计**

原因：
1. 完整开源 CEX 大多不是 Rust 主栈。
2. 完整 CEX 的交易语义过重。
3. AI 平台的核心难点是执行编排、权限治理、成本计费、审计追责、多模型路由。
4. Rust 适合作为高并发控制面、执行面、账本与网关的主语言。

## 3. 目标架构原则

### 保留的交易所级能力
- 账户体系
- 账本与余额
- 风控与限额
- 审计日志
- API 网关
- 异步任务状态机
- 后台治理

### 丢弃的交易所业务语义
- 交易对（symbol）
- 币种钱包充提
- 订单簿
- 撮合引擎业务语义
- 行情/K线
- 合约/杠杆/爆仓逻辑

### 新增的 AI-native 核心语义
- Capability（模型 / Agent / Tool / Workflow）
- Invocation（调用请求）
- Execution（执行实例）
- Policy（策略）
- Approval（审批）
- Credit / Budget（额度 / 预算）
- Settlement（结算）
- Trace（执行链路）

## 4. 推荐技术栈
- API / 控制面：Axum
- 异步运行时：Tokio
- 内部服务通信：tonic（gRPC）+ HTTP/JSON
- 主数据库：PostgreSQL
- DB Access：SQLx
- 缓存与限流：Redis
- 事件总线：NATS（MVP），后期评估 Kafka
- 工作流：Temporal（优先作为外围 durable execution 能力）
- 可观测性：tracing + OpenTelemetry + Prometheus + Grafana
- 策略引擎：RBAC + ABAC + Casbin/Oso/自研轻量策略层
- 审计分析：ClickHouse 或 OpenSearch

## 5. 推荐服务划分
- identity-service
- ledger-service
- capability-service
- gateway-service
- execution-service
- policy-risk-service
- audit-service
- marketplace-service（第二阶段）

## 6. 模块优先级

### Phase 0
- identity-service
- ledger-service
- gateway-service
- execution-service（简版）
- audit logging

### Phase 1
- policy-risk-service
- capability registry
- approval flow
- provider routing

### Phase 2
- marketplace-service
- revenue split
- enterprise governance
- advanced analytics

## 7. 当前最优策略
1. 参考完整 CEX 的模块边界，而不是继承整套代码。
2. 用 Rust 自建控制面、执行面、账本与治理主干。
3. 将工作流、审计、计费、策略当作一等公民。
4. 从一开始就采用 AI-native 语义，而不是沿用交易所术语。

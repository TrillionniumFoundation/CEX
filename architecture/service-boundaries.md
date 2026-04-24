# Service Boundaries

## 1. 设计目标
服务边界应围绕 **AI-native 平台能力** 划分，而不是围绕传统交易所术语划分。

## 2. 核心服务

### identity-service
职责：用户身份、组织/租户、API keys、基础权限模型

### ledger-service
职责：credits 账户、余额变动、冻结/预留、消费/退款、结算事件

### capability-service
职责：模型/Agent/Tool/Workflow 注册、版本管理、元数据、定价配置

### gateway-service
职责：统一 API、鉴权前置、请求规范化、路由入口、限流、trace 入口

### execution-service
职责：invocation 生命周期、异步任务、重试、timeout、执行状态机、approval checkpoint 对接

### policy-risk-service
职责：权限策略、预算/成本策略、敏感工具控制、审批规则、安全策略

### audit-service
职责：trace persistence、operator audit、compliance export、billing trail

### marketplace-service
职责：capability 发布、搜索发现、订阅购买、收益分配

## 3. 推荐边界原则
1. Identity 与 Ledger 必须分离
2. Execution 与 Policy 分离，但通过清晰判定接口衔接
3. Gateway 是入口，不承载复杂业务规则
4. Audit 是横切关注点，但要有独立归档能力
5. Marketplace 后置，不应阻塞核心平台上线

## 4. MVP 建议
优先落地：identity-service、ledger-service、gateway-service、execution-service、audit logging
第二阶段：policy-risk-service、capability-service
第三阶段：marketplace-service

# System Context

> **Superseded for product direction by** `hepta-nakama-trnm-battle-platform-v1.md`
> and `../decisions/adr-004-three-module-external-agent-battle-platform.md`.
> The platform is now an external-Agent research battle platform with exactly
> three top-level modules: Hepta Research League, Nakama, and TRNM. This file
> remains only as historical context for reusable CEX implementation pieces.

## 1. 平台定位

本平台是一个 **Rust AI-native platform**，借鉴中心化交易所的工程能力，但不继承其金融交易业务语义。

平台核心目标：
- 提供统一的 AI capability 接入层
- 提供统一的调用、计费、审计、治理与执行系统
- 为企业和开发者提供模型、Agent、工具、工作流的可管理平台

## 2. 核心参与方
- End User
- Organization / Tenant
- Capability Provider
- Platform Operator

## 3. 外部系统
- 模型提供商
- 外部工具/API 系统
- 支付/Billing 系统
- 通知系统
- 对象存储与日志分析系统

## 4. 平台内部关键上下文
- Identity & Tenant
- Credits / Ledger
- Capability Registry
- Invocation Gateway
- Execution Runtime
- Policy & Risk Control
- Audit & Trace
- Marketplace（后续）

## 5. 核心请求路径
1. 用户发起 Invocation
2. Gateway 验证身份、租户、配额
3. Policy 判断是否允许执行
4. Ledger 进行预算预留或扣费预估
5. Execution Runtime 调度 capability
6. Audit 记录全链路 trace
7. 返回结果并完成最终结算

## 6. 非目标
- 传统交易所撮合
- 行情/K线系统
- 钱包充提语义
- 合约/杠杆/清算引擎

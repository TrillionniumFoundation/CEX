# Open Source Candidates

## 1. 结论摘要
针对“以 Rust 为主语言构建 AI-native 平台，并借鉴开源中心化交易所工程能力”的目标，不建议寻找“完整 Rust 开源 CEX”直接继承。

更优策略是：
- 参考完整 CEX 学习模块边界
- 参考 Rust 交易/执行内核学习高性能架构
- 参考账本/支付/工作流系统构建真正的平台主干

## 2. 候选类别

### A. 完整开源 CEX（适合看模块全景）
- Peatio / OpenDAX：适合参考用户/账户体系、后台运营逻辑、资产与订单模块边界；但非 Rust、交易所语义重、AI 改造成本高。
- HollaEx：适合参考平台化产品思路、商户/租户/后台组织方式；但非 Rust，更像交易所产品套件。

### B. Rust 交易/执行内核（适合学高性能内核）
候选方向：Rust 订单簿项目、Rust 撮合引擎项目、Rust 事件驱动交易系统。  
可借鉴：事件驱动模型、状态机、低延迟执行路径、快照与回放设计。

### C. 账本 / 计费 / 结算类项目（优先级很高）
应重点寻找：double-entry ledger、billing / usage metering、settlement / reconciliation。  
原因：AI 平台真正需要的是 credits、预算、扣费、退款、分账与审计，而不是交易对和订单簿。

### D. Workflow / Policy / Audit 类项目
重点能力：durable execution、approval pause / resume、policy-as-code、audit trail。

## 3. 推荐优先级
第一优先级：账本 / credits / settlement、Rust 高性能执行内核、workflow / durable execution、policy / risk / audit。  
第二优先级：Peatio / OpenDAX、HollaEx。  
第三优先级：各类小型 Rust exchange demo。

## 4. 当前建议
当前项目不应继续寻找“完美的 Rust 开源 CEX”，而应转向：建立 Rust 平台骨架、确立领域模型、形成候选组件选型、再决定哪些外部项目作为参考代码源。

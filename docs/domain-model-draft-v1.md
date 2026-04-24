# Domain Model Draft v1

## 1. 设计原则

领域模型必须采用 AI-native 语义，不沿用传统交易所术语。

## 2. 核心实体

### 2.1 Organization
表示租户或团队边界。

关键字段：
- org_id
- name
- status
- plan
- policy_set_id
- billing_account_id

### 2.2 User
表示组织内成员或独立用户。

关键字段：
- user_id
- org_id
- role
- status
- auth_subject

### 2.3 Account
表示 credits / billing / budget 账户。

关键字段：
- account_id
- org_id
- account_type
- currency_unit
- status

### 2.4 LedgerEntry
表示账本流水。

关键字段：
- entry_id
- account_id
- direction
- amount
- reason
- reference_type
- reference_id
- idempotency_key

### 2.5 Capability
表示模型、Agent、Tool、Workflow 等可调用能力。

关键字段：
- capability_id
- capability_type
- provider
- version
- pricing_model
- policy_tags
- status

### 2.6 Invocation
表示用户发起的一次调用请求。

关键字段：
- invocation_id
- org_id
- actor_id
- capability_id
- request_payload
- route_hint
- created_at

### 2.7 Execution
表示 invocation 的实际执行实例。

关键字段：
- execution_id
- invocation_id
- status
- provider_target
- started_at
- ended_at
- result_ref
- trace_id

### 2.8 Policy
表示可执行治理规则。

关键字段：
- policy_id
- scope
- rule_type
- condition
- action
- priority

### 2.9 Approval
表示人工审批节点。

关键字段：
- approval_id
- execution_id
- status
- requested_to
- requested_at
- resolved_at
- resolution

### 2.10 AuditEvent
表示审计事件。

关键字段：
- event_id
- trace_id
- actor_type
- actor_id
- event_type
- payload
- created_at

## 3. 关键关系

- Organization 1..n User
- Organization 1..n Account
- Organization 1..n Capability
- Invocation 1..n Execution
- Execution 0..n Approval
- Invocation / Execution 1..n AuditEvent
- Account 1..n LedgerEntry

## 4. 明确禁止的命名

以下术语不应进入主领域模型：
- symbol
- orderbook
- trade pair
- deposit
- withdraw
- market order
- liquidation

## 5. 后续要补的内容
- Execution 状态机
- Ledger reason taxonomy
- Policy rule schema
- Capability pricing schema

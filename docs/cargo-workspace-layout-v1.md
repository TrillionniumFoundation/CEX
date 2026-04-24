# Cargo Workspace Layout v1

## 1. 目标

构建一个适合 Rust AI-native 平台的 monorepo / workspace 结构，兼顾：
- 共享类型与协议
- 服务边界清晰
- 未来可拆分部署
- 便于本地开发与 CI/CD

## 2. 推荐目录结构

```text
E:\CEX
├─ README.md
├─ Cargo.toml
├─ Cargo.lock
├─ .env.example
├─ docs/
├─ architecture/
├─ decisions/
├─ research/
├─ proto/
│  ├─ common/
│  ├─ identity/
│  ├─ ledger/
│  ├─ execution/
│  └─ audit/
├─ crates/
│  ├─ shared-types/
│  ├─ shared-errors/
│  ├─ shared-tracing/
│  ├─ shared-auth/
│  ├─ shared-config/
│  └─ shared-events/
├─ services/
│  ├─ identity-service/
│  ├─ ledger-service/
│  ├─ gateway-service/
│  ├─ execution-service/
│  ├─ policy-risk-service/
│  ├─ audit-service/
│  └─ capability-service/
├─ apps/
│  └─ admin-console/
├─ deploy/
│  ├─ docker/
│  ├─ compose/
│  └─ k8s/
├─ scripts/
└─ tests/
```

## 3. 目录职责

### proto/
存放 gRPC / internal API 协议定义。

### crates/
存放共享基础库，禁止堆业务逻辑。

### services/
每个服务一个 crate，独立拥有：
- application
- domain
- infrastructure
- interfaces

### apps/
用户界面或运营端应用。

### deploy/
部署脚本、docker compose、K8s 清单。

### tests/
跨服务集成测试、契约测试、端到端测试。

## 4. Workspace 原则

1. 核心共享放到 crates/，不要让 services 互相直接依赖实现。
2. 共享的是协议、类型、工具，不共享具体业务流程。
3. execution / ledger / identity 要保持较强边界。
4. gateway-service 可以依赖协议与客户端，不应反向侵入其他服务内部。

## 5. 初期建议

MVP 初期可以只初始化：
- crates/shared-types
- crates/shared-errors
- crates/shared-tracing
- services/identity-service
- services/ledger-service
- services/gateway-service
- services/execution-service
- services/audit-service

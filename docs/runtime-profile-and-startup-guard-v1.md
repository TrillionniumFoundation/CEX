# Runtime Profile and Startup Guard v1

## 1. 目的

该规范定义 CEX 核心服务的统一启动安全基线，解决以下问题：

- 未配置 `APP_ENV` 时隐式进入 dev；
- production 配置遗漏后使用固定开发 token/secret；
- 数据库不可达时服务先监听再降级；
- 不同服务对 fail-fast 的默认值不一致；
- 配置冲突只能在运行中被发现。

实现入口为 `crates/runtime-guard`。

## 2. Profile

支持：

- `test`
- `local`
- `dev`
- `beta`
- `staging`
- `production`（兼容 `prod`）

`trnm-economy`（以及下划线形式 `trnm_economy`）是运行管理器的 lane 名称，
解析时等价于 `production`，因此仍强制执行 production-like 的数据库、凭据和
fail-fast 启动护栏；它不会形成一个可绕过护栏的独立 profile。

解析顺序：

1. `CEX_RUNTIME_PROFILE`
2. `APP_ENV`

当两者都存在时必须表达同一 profile，否则启动失败。

当两者都不存在时默认启动失败。只有显式设置 `CEX_ALLOW_IMPLICIT_DEV_PROFILE=1` 才允许临时兼容；该开关不得用于共享环境。

`beta`、`staging`、`production` 统称 production-like。

## 3. Production-like 基线

### 3.1 数据库

所有核心服务启动前执行 PostgreSQL 预检：

- `DATABASE_URL` 必须存在且非空；
- 建立独立短连接；
- 执行 `select 1`；
- 在预检失败时以配置错误退出，不开始监听。

超时由 `CEX_STARTUP_DATABASE_TIMEOUT_SECONDS` 控制，默认 5 秒，允许范围 1–60 秒。

### 3.2 Fail-fast

下列服务必须显式配置为 true：

- gateway：`GATEWAY_FAIL_FAST=true`
- identity：`IDENTITY_FAIL_FAST=true`
- ledger：`LEDGER_FAIL_FAST=true`
- execution：`EXECUTION_FAIL_FAST=true`
- audit：`AUDIT_FAIL_FAST=true`

`IDENTITY_FAIL_FAST` 目前由 startup guard 执行数据库和配置预检；后续 identity repository 改造后应下沉到服务状态构造器。

### 3.3 禁止值

production-like 会拒绝关键配置中出现以下已知不安全片段：

- `local-dev-key`
- `local-dev-admin-token`
- `local-development-`
- `change-me`
- `changeme`
- `replace-me`
- `REPLACE_`

检查范围包括：

- API key 配置；
- identity/audit/execution/ledger admin token；
- worker token；
- ingress/session/service token；
- game authority、session 和 entitlement signing secret；
- JSON token bundle。

### 3.4 Identity static fallback 过渡措施

production-like 禁止显式配置 `IDENTITY_STATIC_API_KEYS_JSON`。

在彻底从 identity-service 删除静态回退逻辑前，startup guard 会在进程内注入一个随机、不会输出到日志的 deny-only sink key。其目标是避免数据库异常时公开的 `local-dev-key` 恢复有效。

这只是过渡措施，不是最终 authority 模型。最终要求：

- production 构建中 static key code path 不可达；
- DB 查询错误直接返回稳定的 backend-unavailable；
- revoked/expired key 不可因降级重新有效。

## 4. 启动顺序

每个核心 service binary 必须：

1. 初始化 tracing；
2. 调用 `runtime_guard::enforce(ServiceKind::...)`；
3. guard 成功后构造 service state；
4. 构造 router；
5. bind/listen。

失败统一退出码：`78`（configuration error）。

## 5. 本地开发

`.env.example` 使用 `APP_ENV=dev`，因此本地 helper 不受 production-like 限制。

建议逐步增加：

```env
CEX_RUNTIME_PROFILE=dev
APP_ENV=dev
```

两者应保持一致。

本地需要测试 production posture 时，使用 `.env.production.example` 或 `scripts/bootstrap-local-production-env.sh` 生成 ignored 配置，不要在共享 `.env` 填真实 secret。

## 6. 配置示例

```env
APP_ENV=production
CEX_RUNTIME_PROFILE=production
GATEWAY_FAIL_FAST=true
IDENTITY_FAIL_FAST=true
LEDGER_FAIL_FAST=true
EXECUTION_FAIL_FAST=true
AUDIT_FAIL_FAST=true
DATABASE_URL=postgres://...
```

生产配置不得包含固定开发凭据或 placeholder。

## 7. 观测

后续每个 service 的 `/ready` 和 `/metrics` 应暴露：

- runtime profile；
- startup guard version；
- database preflight 状态和时间；
- fail-fast effective value；
- unsafe-default rejection count；
- authority mode。

不得暴露 token、secret、完整连接串或随机 sink key。

## 8. 测试

`runtime-guard` 单元测试覆盖：

- profile alias；
- profile 冲突；
- 缺 profile；
- implicit dev escape hatch；
- production-like 判定；
- weak credential 检测；
- fail-fast 解析；
- timeout 边界。

数据库预检在 integration gate 中使用临时 PostgreSQL 覆盖成功、拒绝和超时路径。

## 9. Rollback

如果 guard 导致 shared environment 无法启动：

1. 不要直接删除 guard；
2. 检查错误信息定位缺失/冲突配置；
3. 修正 profile、fail-fast、DATABASE_URL 或弱凭据；
4. 仅在 local/dev 临时使用 `CEX_ALLOW_IMPLICIT_DEV_PROFILE=1`；
5. production-like 不允许用该开关绕过。

代码回滚必须恢复到上一个已验证 commit，并记录为什么配置无法满足基线。

## 10. 后续版本

v2 计划：

- typed config schema；
- secret reference 类型；
- config fingerprint；
- `/ready` 统一实现；
- workload identity preflight；
- policy bundle revision approval；
- 自动生成环境变量 reference。

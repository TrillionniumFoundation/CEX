# ADR-005: Exact Money Minor Units

- 状态：Proposed / implementation in expand phase
- 日期：2026-08-27
- Owner：Ledger / Economy
- 关联计划：`docs/CEX-DEVELOPMENT-PLAN-2026-08-27-v2.md`

## Context

当前 ledger API、内存状态和 repository bridge 使用 `f64`，PostgreSQL 使用 `numeric(20,6)`。这种混合表示会造成：

- 浮点边界和序列化不一致；
- 不同服务对 scale/rounding 的理解不一致；
- JavaScript 客户端无法安全表示大整数；
- projection rebuild 和跨系统 receipt 难以做逐位比较；
- 价值系统故障排查依赖近似容差。

## Decision

采用 signed 64-bit minor units 作为 CEX credits/economy 金额的下一代规范表示。

Money v2 envelope：

```json
{
  "currency": "credit",
  "scale": 6,
  "minor_units": "12340000"
}
```

约束：

1. `minor_units` 是 JSON decimal string，避免 IEEE-754 客户端截断。
2. 初始最大 scale 为 6，与现有 `numeric(20,6)` 兼容。
3. 算术只允许相同 currency 和 scale。
4. 解析禁止科学计数法和隐式舍入。
5. 溢出必须失败，不能 wrap 或 saturate。
6. account balance/reserved 非负，且 reserved 不得超过 balance。
7. ledger entry amount 为正；direction/action 表达符号语义。

## Database migration

采用 expand/migrate/cutover/contract：

### Expand

`0056_expand_exact_money_minor_units.sql` 添加：

- `accounts.currency_scale`
- `accounts.balance_minor`
- `accounts.reserved_minor`
- `ledger_entries.currency_scale`
- `ledger_entries.amount_minor`

迁移先验证当前数据能精确映射到 bigint minor units，再 backfill。过渡 trigger 保持 numeric 与 minor 表示双向一致。

### Migrate

后续代码：

1. API 增加 Money v2 字段；
2. repository 使用 minor columns 进行锁内算术；
3. 双写偏差进入 metrics/reconciliation；
4. 所有调用方切换到 Money v2。

### Cutover

- minor columns 成为读写 authority；
- numeric columns 仅作为兼容 projection；
- 禁止从 `f64` 构造金额。

### Contract

稳定周期后移除：

- `f64` API 字段；
- numeric-first trigger 分支；
- 近似比较容差；
- 旧客户端 contract。

## Range

scale=6 时，`i64` 支持约 ±9.22 万亿主单位。迁移会拒绝超过该范围的数据。

扩大范围必须通过新 ADR；不能静默改为浮点或无界 JSON number。

## Consequences

优点：

- 算术和幂等 receipt 可逐位比较；
- DB、Rust 和 API 共享同一语义；
- 不依赖舍入容差；
- 便于 property/model-based testing。

代价：

- 需要 versioned API 和双写期；
- scale 变更是显式数据迁移；
- 第三方 SDK 必须按字符串解码 minor units；
- 旧 numeric 列在 contract 阶段前仍需维护。

## Alternatives rejected

### Continue with `f64`

不满足价值系统精确性和可重放要求。

### JSON decimal number

大值会被部分客户端转为 IEEE-754 number，仍有精度风险。

### Arbitrary precision decimal everywhere

语义正确但会引入更复杂依赖、序列化和范围治理；当前 credits 业务不需要无界范围。未来需要时另立 ADR。

## Validation requirements

- decimal parser unit/property tests；
- JSON string round-trip；
- overflow、scale mismatch 和 currency mismatch tests；
- migration fresh/upgrade tests；
- numeric/minor trigger parity tests；
- concurrent reserve/consume tests；
- full reconciliation before cutover。

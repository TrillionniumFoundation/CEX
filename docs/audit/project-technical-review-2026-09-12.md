# CEX 项目技术审查（只读）

审查对象为整合前选定候选树（HEAD `64955ab`；最终整合提交见分支合并记录），2026-09-12）及最近的整合分支内容。未修改源代码、迁移、工作流或远程分支。

## 文档覆盖结论

模块文档覆盖已经形成机器可验证闭环：`docs/module-catalog-v1.json` 登记 23 个 Cargo workspace 成员，`docs/modules/index.md` 为入口，且每个成员均有 `docs/modules/*.md`。`python3 scripts/check-module-documentation.py` 返回 `status=ok`，`python3 scripts/check-development-docs.py` 返回 `status=ok`。因此“是否每个模块都有文档”的答案是：**有，且结构完整**。

深度仍不均衡：共享库和 vendored 协议文档主要是边界/禁止事项，缺少 API 示例、版本兼容矩阵、故障注入案例和性能预算。例如 `docs/modules/trnm-protocol.md:50-59` 仅列出 cargo 验证命令，并明确当前 authoring environment 未执行这些命令；`docs/modules/shared-tracing.md:30-55` 也主要是约束性文字。建议为每个模块增加“公开类型/路由表、状态机、错误码、数据表/索引、SLO、运行手册、契约测试 fixture”小节，并让 catalog checker 验证这些字段，而不仅验证标题存在。

## 主要问题与风险

### P1：精确金额切换仍未完成

`services/ledger-service/src/state.rs:17-35` 的 `AccountRecord`、`LedgerEntryRecord` 仍以 `f64` 保存余额、预留额和 entry amount；`services/ledger-service/src/api.rs:35-49` 的创建账户和动作请求也仍接受 `f64`。这与 Ledger 模块文档宣称的“exact minor-unit authority”（`docs/modules/ledger-service.md:20-24,73-77`）存在实现落差。调用方切换文档也明确指出 Invocation 的 `reserve_amount` 仍是兼容 `f64` 且 `cutover_ready=false`（`docs/ledger-caller-cutover-v1.md:30-33,81-89`）。虽然 v1 写入路径已默认 fail-closed，当前仍不能称为完成的精确金额实现；任何绕过 v2 的内部调用、projection 或旧客户端都可能重新引入浮点舍入问题。

建议：完成 exact ingress（`amount_minor` 字符串、currency/scale），让 PostgreSQL minor 列成为唯一读写权威；将 `f64` 限制在明确标记的兼容读取层并添加静态检查，最后在 `require_v2` 下覆盖所有服务和 SDK 的端到端并发 reserve/consume/refund 测试。

### P1：聚合发布门只由两个路径触发

`.github/workflows/p0-release-candidate-gate.yml:3-8` 的 `push` 触发器只监听 `docs/release-evidence/p0-candidate-trigger.json` 和该 workflow 自身。普通源代码、迁移、配置或模块文档合并到 main 时不会自动运行聚合候选资格门；必须额外更新 trigger 文件。虽然 `rust-service-gate` 会在每次 push 运行，但它不能替代包含 PostgreSQL、迁移、发布证据汇总的聚合门，容易出现“main 已变更但没有对应候选证据”的窗口。

建议：保留昂贵门的手动/共享 trigger 机制，同时新增一个只读一致性 job，强制检查 `HEAD` 的源变更是否伴随 trigger 更新，或由主分支保护要求同一 SHA 的 aggregate workflow 成功后才可合并。

### P2：工具链可复现性依赖 CI 安装完整 Cargo

仓库 pin `rust-toolchain.toml` 为 Rust 1.98.1。当前执行 `python3 scripts/check-cargo-workspace-authority.py` 失败，原因是本机 1.98.1 toolchain 没有 cargo component（`cargo` 报 “cargo ... is not applicable”）；这不是源码编译错误，但说明仅按 rust-toolchain 文件安装可能得到不可用环境。CI 使用 `dtolnay/rust-toolchain` 会补齐组件，本地开发者/外部审阅者则会被阻断。

建议：在 bootstrap 文档和 preflight 脚本中显式执行 `rustup component add cargo rustfmt clippy --toolchain 1.98.1`（或提供可验证的安装命令），并让 `check-cargo-workspace-authority.py` 输出清晰的安装提示。

### P2：文档门验证“形状”多于技术充分性

模块 checker 能验证 23 个成员、入口文件和必需标题，但无法判断文档是否覆盖实际路由、SQL 表、迁移、错误码和运行时依赖。当前 catalog 已登记大型模块（例如 consumer-entry-api、paper-raid-bff、hepta-research-league），但缺少自动化的 source-to-doc route/table 差异检查。建议从 Rust 路由提取器、Cargo target inventory 和 migration parser 生成事实，与模块文档中的受控清单比较；新 route/table/target 未同步文档时直接阻断。

## 验证记录

- `python3 scripts/check-development-docs.py`：通过（migration head `0088_enforce_provider_terminal_evidence_binding.sql`，18 条 v12 requirement）。
- `python3 scripts/check-module-documentation.py`：通过（23 workspace members，所有模块文档和必需章节存在）。
- `cargo test --workspace --all-targets --no-fail-fast`：在整合前树上完成，测试通过但产生 dead-code/unfulfilled-lint-expectation 警告；整合后因本机 pinned 1.98.1 缺少 Cargo component 无法重跑。
- `python3 scripts/check-cargo-workspace-authority.py`：被本地 toolchain 缺少 cargo component 阻断，不能作为编译通过证据。

当前文档和 CI 均保留 `production_authorization=not_granted`，外部 provider、真实部署回滚、凭据托管、长期 soak、独立安全/财务审查和最终人工 go/no-go 仍是上游阻塞项。

## 分支卫生快照

审查时远端仍可见约 92 个 `origin/*` refs，其中包括 `tmp/*`、`probe/*`、`__schema_probe__`、`automation/*`、`backup/*` 等临时/验证分支。它们不影响当前 workspace 文档 checker，但与“只保留一棵 main tree”的目标不一致。合并完成后应先生成合并清单和保护分支白名单，再批量删除已合并及临时远端分支，并复核 `origin/HEAD -> origin/main`、保护规则和开放 PR；删除远程分支属于不可逆治理操作，应由仓库管理员在最终验收步骤执行。

# CEX 开发文档与模块覆盖审查（Sequence 54 候选）

审查基线：`origin/remediation/cex-audit-acceptance-20260912`（提交 `81bd52e5e0ee19722089b32b2cbe2ce4698b6f76`），而不是旧的 `origin/integration/cex-v12-sequence54-20260908`。该分支包含 2026-09-12 的模块目录、Sequence 54 集成计划及 0088 migration head。结论只评价文档和仓库结构，不把本地读取或脚本定义当成 hosted/external 证据。

## 结论

- **模块清单层面已覆盖**：`Cargo.toml` 声明 23 个 workspace members（18 个一方 crate/service/app + 5 个 vendor crate），`docs/module-catalog-v1.json` 逐一登记，`docs/modules/index.md:24-50` 逐一链接 23 份专属契约；`scripts/check-module-documentation.py` 在该候选树实际返回 `status=ok`、`workspace_member_count=23`、`external_component_count=7`、`problems=[]`。
- **格式契约层面齐全**：addendum Block K 要求每份模块文档包含 Purpose/non-goals、Authority/state、Source/entrypoints、Interfaces、Persistence/concurrency/recovery、Configuration/secrets、Security、Verification、Deployment/operations、Compatibility/change protocol（`docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md:49-64`）。23 份契约均有这 10 个二级标题；因此不能再说“模块完全没有文档”。
- **技术深度仍不均衡**：核心状态/资金/消息链路有较多专文和测试指令，但若干模块的接口字段、数据模型/状态转移、错误与幂等表、SLO/告警阈值、部署拓扑和回滚步骤只以原则性句子描述。结构检查通过不等于可直接交付开发、运维或外部审计。
- **生产尚未获授权**：权威文档保持 `production_authorization=not_granted`；v12 计划明确 repository candidate 不是 production-ready，X1–X8 仍需独立外部证据（`docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md:188-203,229-246`；`docs/development-doc-authority-v1.json:20-28,80-95`）。

## 覆盖矩阵

评级含义：**详细** = 模块契约之外还有可定位的协议/状态机/运行手册/测试或迁移材料，开发者可据此实现主要路径；**部分** = 10 节齐全但关键字段、状态/错误/运维参数仍需从源码推断或依赖跨文档拼接；**缺失** = 没有独立契约或未纳入权威目录。

| 模块 | 结构契约 | 需求/边界 | 接口与数据模型 | 状态、失败、幂等 | 安全 | 测试/部署/可观测 | 评级与证据、主要缺口 |
|---|---|---|---|---|---|---|---|
| shared-types | 有 | 清楚 | 类型文件有入口，字段兼容规则在 `shared-types.md:40-57` | 有 exact money/replay 原则，但无完整迁移表 | 有 | cargo 验证；运行时 SLO 由使用方负责 | **部分偏详细**；跨服务 schema 版本/兼容向量仍应集中成表 |
| shared-errors | 有 | 清楚 | 稳定错误 code 原则（`shared-errors.md:30-36`） | 仅语义约束，未列各服务 HTTP/retry 映射 | 有 | 仅 crate 测试，观测继承服务 | **部分**；补 error code→HTTP/status/retry/审计字段矩阵 |
| shared-tracing | 有 | 清楚 | 仅 helper/字段约定（`shared-tracing.md:24-36`） | 无状态机；丢 span 的边界有说明 | 有脱敏规则 | 未定义 exporter、采样、SLO/告警阈值 | **部分**；补 trace context、采样、队列/backpressure、保留期和告警 |
| hepta-paper-raid-contracts | 有 | 协议边界清楚 | 版本化 signed-byte、golden vectors；但字段/错误枚举需查代码 | 重放/冲突原则，缺完整转换图 | Ed25519/hash/bounds 有 | cargo/golden tests；部署不适用 | **详细（协议）**；补 OpenAPI/跨语言向量和版本迁移表 |
| identity-service | 有 | `identity-service.md:14-26` | 路由未列方法、request/response schema、错误码 | 有 replay/collision/revoke 测试焦点（`:79-85`），缺并发状态图和 token rotation 时序 | 有 | 有启动 fail-closed；无容量/SLO/告警阈值 | **部分**；补 API schema、租户/密钥状态机、审计事件字段、回滚演练 |
| ledger-service | 有 | exact Ledger authority 清楚 | `/v2/accounts`、`/v2/ledger/effects`，详细规则分散于 ledger-* 文档 | reserve/consume/refund/receipt/replay 有原则，缺统一状态转移和 SQL 锁语义表 | exact minor units/receipt 绑定明确 | migration/Rust gate；缺余额/延迟 SLO 与告警 | **详细但分散**；将 operation/effect/receipt schema 与并发不变量合并索引 |
| trnm-economy-service | 有 | `trnm-economy-service.md:14-26` | 签名 intent/receipt 路由有描述，未给完整 schema/错误码 | bytes immutable、response-loss/recovery 有（`:52-62,86-90`），缺 settlement 状态图和超时参数 | issuer/audience/key separation 有 | build-evidence 脚本说明充分；缺真实 Chain 拓扑/回滚 runbook | **部分偏详细**；补 Chain adapter contract、状态/重试矩阵、可观测 SLO |
| gateway-service | 有 | exact reserve 边界清楚 | 路由和 worker 分层，字段 schema 主要在 `gateway-exact-reserve-v1.md` | claim/lease/reconcile/requeue 焦点（`gateway-service.md:98-102`），缺统一时序图 | 双金额排除、principal binding、body bounds 有 | hosted gate 命令齐；缺限流配额和端到端 SLO | **详细但跨文档**；补 canonical API/OpenAPI、限流/错误及指标表 |
| execution-service | 有 | Agent-only 边界及 settlement 责任清楚 | source entrypoints 多，接口契约未列完整 payload schema | terminal settlement/provider reconciliation 原则强，但缺全状态图、lease/timeout 数值和 operator 命令表 | 有 unknown-outcome、无本地执行约束 | 多 gate 命令；缺 worker 容量、队列 SLO、告警/回滚 | **详细但需整合**；把 `execution-ledger-settlement*`、provider、ADR-004 汇成一份状态/错误矩阵 |
| audit-service | 有 | hash-chain/outbox authority 清楚 | `/v2/audit/events` 等有概述，字段 schema 在 audit-* 文档分散 | replay/collision/outbox claim/ACK 有测试焦点，缺端到端时序和 retention/backfill 参数 | writer auth/redaction 有 | baseline/postgres gate；缺 delivery SLO、告警阈值和容量模型 | **部分偏详细**；补 outbox 状态机、租约参数、数据保留/导出接口 |
| capability-service | 有 | external-Agent-only 非目标清楚 | 4 个路由和严格字段限制明确（`capability-service.md:40-56`） | 只读快照，替换需重启；无版本发布/回滚状态机、幂等语义 | 本地模型发现禁止，边界清楚 | readiness/metrics 原则；缺指标名、阈值和 registry 签名/来源证明 | **部分**；补 registry schema、签名/摘要供应链、发布回滚流程 |
| consumer-entry-api | 有 | edge-only 边界清楚 | 入口文件多，路由/投影契约跨 Matrix 文档，缺统一 OpenAPI | replay/identity binding 有描述，缺 task projection 状态和错误/幂等表 | CSRF/session/tenant 边界有 | 有测试入口；缺浏览器/API 兼容、性能 SLO 和告警 | **详细但跨文档**；补用户旅程时序和 API schema |
| hepta-research-league | 有 | research facts/review/finality 责任清楚 | 依赖多版本协议，HTTP/SDK 字段需跨 `hepta-agent-protocol`/OpenAPI 查找 | PostgreSQL locks/immutable evidence 有，状态机主文档较完整；缺每命令失败矩阵和重试预算 | Agent proof、Nakama/Chain 信任边界有 | strict postgres/lint gate；缺生产拓扑、容量/SLO | **详细**；补命令级状态/错误/幂等表与 operator playbook |
| paper-raid-bff | 有 | browser edge 责任清楚 | 路由/CLI 在 README 与代码，模块契约只概述 | session/CSRF/replay 有，缺 Alpha/invite 状态图、恢复和幂等清单 | cookie/OIDC/CSRF 有 | cargo tests；部署段短，缺扩缩容、备份恢复和观测指标 | **部分**；补 BFF OpenAPI、会话/邀请状态机、runbook |
| matrix-entry-adapter | 有 | Matrix ingress/normalization 清楚 | 多路由、v3 binding 在专文，字段较详细 | inbox/history/poison/replay/lease 细节丰富，但需统一 v3 状态图 | homeserver/token/redirect/body bounds 有 | SQL runner/多脚本；缺跨服务 SLO/告警目录 | **详细**；补故障注入与容量界限 |
| matrix-bot-relay | 有 | durable delivery authority 清楚 | inbound route 和 delivery binding 有 | claim/ACK/retry/dead-letter/recovery 细，缺统一错误码和参数基线 | ingress auth/redaction 有 | 验证命令多；缺 outbox lag SLO 与报警阈值 | **详细**；补 API schema、指标和重放操作手册 |
| matrix-bot-poller | 有 | cursor/lease/source observation 清楚 | `/sync`、filter/wire contracts 细，未形成公开 schema | gap/replay/lease/recovery 细，缺跨组件幂等矩阵 | token/homeserver trust 有 | 多轮补丁专节；缺生产部署拓扑、容量/SLO | **详细**；补同步速率/限流、告警、恢复演练 |
| vendor trnm-economy-protocol | 有 | 合约边界清楚 | 类型描述，字段表和跨语言向量不足 | 无持久状态，仅调用方承诺 | 签名/issuer 约束有 | cargo tests；无消费方集成门禁汇总 | **部分（库）**；补 schema registry、版本兼容和消费者清单 |
| vendor trnm-finality-types | 有 | proof/receipt 类型边界清楚 | re-export 描述，字段/验证失败表不足 | 无持久状态；恢复责任交给 verifier | 算法/长度边界原则有 | cargo tests；缺跨 Chain 集成向量 | **部分（库）**；补 proof schema、拒绝原因和版本矩阵 |
| vendor trnm-finality-verifier | 有 | verifier/Unix CLI 边界清楚 | CLI 与 verify 函数描述较具体 | 文件证据恢复有，缺并发/幂等操作约定 | trust anchor/key custody 有 | provenance + cargo；Windows CLI 限制已说明 | **详细（工具库）**；补 CLI exit-code、证据目录锁和运维 runbook |
| vendor trnm-protocol | 有 | canonical tx/applied record 清楚 | canonical bytes/字段 bounds 细 | nonce/replay 由调用方负责，缺状态/错误到 Chain 的映射 | canonical parsing/trust 分离有 | cargo/golden 依赖；缺真实 Chain 集成证据 | **详细（协议）**；补跨语言 schema/版本弃用策略 |
| vendor trnm-research-protocol | 有 | signed research command/state 清楚 | deterministic CBOR、authority set、state apply 描述细 | Idempotent/AlteredReplay、snapshot recovery 有 | role/DID/key binding 有 | cargo/golden；缺外部 Chain/Nakama 执行证据 | **详细（协议）**；补命令状态/错误矩阵和消费者升级指南 |

**独立工具缺口**：`tools/paper-raid-agent-bridge` 是 Node.js CLI/daemon，拥有安装、配对、工作流、签名、systemd 用户服务和大量测试（最新树目录可见；入口由 `scripts/check-paper-raid-alpha-candidate.sh` 调用），但它不是 Cargo member，也没有 `docs/modules/` 专属契约。若其属于交付面，应新增 tool catalog（运行时、输入/输出、密钥、升级/回滚、观测和安全边界）；若明确不属于 CEX 候选，应在 `docs/modules/index.md` 和发布清单中写出排除理由，避免“23 成员=全部模块”的误读。

## 文档权威、版本与分支问题

1. `docs/index.md:9-20,22-35` 已定义 ADR→v12→Sequence 54 的优先级，且把模块目录列为规范入口；但 `docs/development-doc-authority-v1.json:36-45` 的 `canonical_documents` 没有列出 `docs/module-catalog-v1.json`、`docs/modules/index.md` 或 23 份契约。建议把 module catalog/index 及其 checker 明确加入 machine authority 的 canonical set，并在完整性哈希中逐项绑定。
2. 父计划 v12 在最新 remediation 候选已更新到 migration head `0088_enforce_provider_terminal_evidence_binding.sql`（`docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md:3-8,62-79`）；旧 integration/早期 authority 文档仍可能显示 0084。合并后必须只保留一个 active head，并让 authority、traceability、README/脚本和 manifest 一致，否则会出现“文档通过但迁移链不同”的资格歧义。
3. 仓库保留 `docs/CEX-DEVELOPMENT-PLAN-*` v2…v12 历史计划。`docs/index.md:11-18` 虽声明历史优先级最低，但建议在每个旧计划首行加 `Status: historical; superseded by v12`，并在 checker 中拒绝旧文档出现 active/current/production-ready 声明。
4. `docs/archive/branch-consolidation-2026-08-29.md:1-10` 声称“41 branches、only main remains”，与当前远端约 90 个 `origin/*` heads（`git branch -a` 可复现）直接冲突。该文件只能作为历史操作记录，必须标注“当时快照/不代表当前远端”；分支合并和删除需以最新 exact SHA、GitHub API 读回结果和保护规则证据为准。
5. `architecture/system-context.md:3-7` 明确已被 ADR-004/Hepta-Nakama-TRNM 取代，但仍保留旧 AI-native/marketplace/policy-risk 叙述（`:9-47`）。建议将旧上下文拆到 `architecture/archive/` 并在当前图中删除容易误导的 Policy/Marketplace 入口，避免开发者实现被废弃方向。
6. `architecture/service-boundaries.md:23-30,39-42` 仍列 `policy-risk-service`、`marketplace-service`，仓库没有对应 workspace member；同时未列 Matrix adapter/poller/relay、Hepta League、TRNM protocol 等现存模块。应标注 historical 或重写为当前 3-domain/23-member 架构。
7. 根 `readme.md` 在最新树中基本为空（仅导航/占位），而 index 将其排除权威链（`docs/index.md:7`）。建议至少放一页指向 `docs/index.md`、候选状态、模块目录和本地验证入口，减少新开发者误读。

## 可执行深化清单

- 生成 `docs/contracts/`（或在模块契约中内嵌表格）统一记录：HTTP/SDK/OpenAPI schema、版本、错误码、鉴权、幂等键范围、状态转移、超时/租约/重试上限、数据库约束和事件字段；优先 identity/gateway/execution/audit/BFF/capability。
- 为 Ledger/Gateway/Execution/Audit/TRNM/Matrix/Hepta 各画一张跨服务时序图，标出“提交 claim → 网络 I/O → outcome commit”、未知结果、重放、死信、人工确认和回滚边界，并链接对应 SQL 函数/测试。
- 为每个 deployable 模块补统一运维表：镜像/二进制与配置 digest、依赖、readiness/liveness、关键 SLI/SLO（队列年龄、重试率、reconcile backlog、hash-chain lag）、告警阈值、扩缩容、备份恢复、回滚命令和责任人。
- 为 23 个模块增加契约级 contract tests：schema round-trip、拒绝未知字段/越权/跨租户、幂等 replay/collision、lease crash/recovery；把 Node agent bridge 也纳入相同矩阵或明确排除。
- 将 `development-doc-authority-v1.json` 的 canonical set 与 module catalog、external-component contracts、Sequence 54 文档保持机器可验证的一致性；要求任何 active 文档变更更新 authority/traceability 并触发 shared candidate trigger。
- 清理/标记历史计划与旧架构文档，建立 supersession 链和单一 migration head；禁止 archive 记录被脚本当作当前分支/生产证据。
- 合并 branches 后重新生成 exact-tree integrity、模块 checker、五个 authoritative gates 与 aggregate candidate manifest；所有 hosted/external 证据必须绑定最终 `main` SHA，不能沿用分支 tip 或旧 archive 声明。

## 可复核命令与证据

```text
git show origin/remediation/cex-audit-acceptance-20260912:Cargo.toml
git show origin/remediation/cex-audit-acceptance-20260912:docs/module-catalog-v1.json
git show origin/remediation/cex-audit-acceptance-20260912:docs/modules/index.md
python3 scripts/check-module-documentation.py
```

在该候选树上模块 checker 输出：`status=ok`, `problems=[]`, `workspace_member_count=23`, `external_component_count=7`, `production_authorization=not_granted`。这证明目录与结构约束，不证明 hosted PostgreSQL、真实 Agent/Nakama/Chain、部署回滚或生产审批已完成。

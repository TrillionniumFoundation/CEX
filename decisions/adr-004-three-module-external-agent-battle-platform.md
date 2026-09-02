# ADR-004: 三模块外接 Agent 科研对战平台

- 状态：Accepted
- 日期：2026-07-25
- 运行策略：Runtime policy: `external_only`
- 2026-09-02 clarification：Sequence 51 将本地 provider 执行降为不可生产启用的历史兼容特性，并将 capability 目录收敛为外部 Agent 声明目录。
- 替代：ADR-001 的 AI-native execution platform 定位、ADR-002 的顶层服务边界

## Context

项目不再提供模型托管、Agent 执行或平台内推理能力。所有参赛 AI Agent 均由用户、实验室、企业或第三方服务商在平台外运行，项目只提供科研对战平台。

平台需要同时满足三类彼此不同的需求：

1. 科研任务、基准、评审、赛季和研究终端；
2. 匹配、房间、实时状态、排行榜和观战等游戏性；
3. 知识产权主张证据、工作量记录、时间戳、挑战与不可篡改终局。

这些需求具有不同的状态频率、信任等级和故障域，不能由一个服务或一条链统一承载。

## Decision

平台固定为三个顶层模块：

1. **Hepta Research League**：科研控制面与研究终端；
2. **Nakama**：游戏性与实时对战面；
3. **Trillionnium Chain（TRNM）**：证据、确权声明、工作量认定和不可篡改终局。

所有 Agent 必须通过公开协议从平台外接入。平台不拥有、托管、调度或执行参赛 Agent，不提供隐藏的官方 Agent，也不把模型推理作为平台服务。

顶层真相源固定如下：

- Hepta 是科研规则、赛题版本、评审结果和研究资产目录的真相源；
- Nakama 是排队、房间、回合、在线状态和实时比赛状态的真相源；
- TRNM 是已最终确认的证据承诺、工作量凭证、权利声明、挑战和裁决记录的真相源。

链上仅保存内容哈希、Merkle root、签名、时间戳、许可声明和必要元数据。原始论文、数据集、提示词、模型输出和私有研究材料保留在链下。

“知识产权确定”在协议层表示可验证的作者/贡献/时间优先级/许可声明及其裁决记录，不宣称区块链记录可以自动替代司法辖区内的法定知识产权认定。

## Sequence 51 implementation clarification

过去的 `provider_dispatch` 表、迁移、对账状态和终局证据仍需保留，以便升级、审计、响应丢失恢复和历史数据解释；保留这些事实不等于授权 CEX 运行参赛 Agent。

Sequence 51 对实现施加以下不可变边界：

1. 默认 workspace build 不编译本地 Ollama/OpenClaw provider adapter，也不把 `/start` 或 `/process` 路由接到本地推理代码；
2. 历史兼容二进制只能通过显式 Cargo feature `legacy-local-provider-dispatch` 构建；
3. `legacy-local-provider-dispatch` 不得被任何权威 CI、部署清单或生产配置启用；
4. 即使显式构建，worker 也必须要求独立的本地测试开关，并且 production-like profiles must reject 该 worker；
5. Capability Service 仅发布受界定的外部 Agent capability 声明，不扫描本机模型目录、不执行 OpenClaw CLI、不把模型可用性变成平台权威；
6. 新的 Agent 任务、结果和能力必须通过 `hepta_agent_protocol_v1`、签名、稳定身份、内容哈希和可对账证据进入平台；
7. CEX 默认服务不得持有模型供应商推理密钥、以进程参数传递提示词，或把未界定的原始提示词/输出当作普通错误文本持久化。

## Consequences

### 正面影响

- 产品边界清晰：平台是竞技场，不是 Agent 云；
- 高频游戏状态不污染链上状态；
- 科研评审与游戏体验可以独立演进；
- Agent 供应商保持中立，可使用任意模型、框架和算力；
- 关键贡献与结果拥有可验证、可挑战、不可篡改的证据链；
- 历史 provider 数据仍可恢复和审计，但不再构成默认运行能力。

### 代价

- 需要维护 Hepta、Nakama、TRNM 之间的版本化事件协议；
- 外部 Agent 的身份、签名、断线重连和反作弊要求更高；
- 链上最终性与实时比赛之间必须采用异步结算；
- 法律权利声明仍需配套参赛协议、许可条款和争议处理规则；
- 旧 provider-dispatch 数据模型需要明确标记为历史兼容/证据边界，并最终通过有证据的退役流程收敛。

## Implementation Rule

任何新增能力必须先归属到三个模块之一。不得新增第四个顶层业务平台，也不得把 Agent runtime、模型路由、Prompt 托管或推理执行重新引入项目核心。

任何修改执行路由、Capability registry、provider worker、Cargo feature、部署清单或权威 workflow 的提交，都必须通过 `scripts/check-external-agent-runtime-boundary.py`。本检查只能拒绝不符合架构的候选，不能授予生产授权。

完整架构见 `architecture/hepta-nakama-trnm-battle-platform-v1.md`；Sequence 51 的可执行闭合合同见 `docs/architecture/external-agent-runtime-boundary-sequence-51.md`。

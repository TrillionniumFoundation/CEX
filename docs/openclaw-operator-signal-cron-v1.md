# OpenClaw Operator Signal Cron v1

## Goal

把当前 CEX 的最小 operator monitoring 链路接到 **OpenClaw 内建 cron**，而不是再额外造一层 OS-level 定时器编排。

当前推荐形态不是“直接从 cron 里重写监控逻辑”，而是：

- repo 内保留检查逻辑：`scripts/check-operator-signals.sh`
- repo 内保留 cron-ready wrapper：`scripts/run-operator-signal-check.sh`
- OpenClaw cron 只负责定时触发 repo-local wrapper

---

## Recommended shape

建议配置：

- exactly **one** recurring cron job
- cadence: every 5 minutes
- session target: `isolated`
- allowed tools: `exec,read`
- delivery mode: `none`
- job body: run `./scripts/run-operator-signal-check.sh --compact`

为什么当前先推荐 `delivery = none`：

- 这条巡检默认会高频运行
- repo 内 wrapper 已经负责：
  - 输出 machine-readable JSON
  - 落盘 `run/operator-signals/last.json`
  - 可通过 `OPERATOR_SIGNAL_NOTIFY_COMMAND` 触发外部通知
- 先避免把每次正常 tick 都直接变成聊天消息噪音

如果后续要升级成“只在 warn/critical 时发 chat 通知”，建议在 host 环境里先把 `OPERATOR_SIGNAL_NOTIFY_COMMAND` 接到受控 notifier，再决定是否打开额外 delivery。

---

## Repo-local helper

当前仓库已提供 helper：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1
```

```bash
./scripts/register-openclaw-operator-signal-cron.sh
```

如果你只想要 Linux 上最短的一条默认安装命令，可直接用：

```bash
./install-operator-signal-cron.sh
./install-operator-signal-cron.sh --status
./install-operator-signal-cron.sh --status-json
./install-operator-signal-cron.sh --examples
./install-operator-signal-cron.sh --doctor
./install-operator-signal-cron.sh --doctor-json
./install-operator-signal-cron.sh --help-json
./install-operator-signal-cron.sh --schema
```

`scripts/install-operator-signal-cron.sh` 仍保留兼容，但 repo-root alias 现在是更推荐的默认入口。它只是一个薄转发层，最终仍调用 bash helper，因此不会引入第二套安装逻辑；另外还补了更顺手的顶层 shortcuts：`--status`（等价于 `--show`）、`--status-json`（机器可读当前 cron 状态）、`--examples`、`--doctor`（快速环境/状态摘要）、`--doctor-json`（机器可读诊断输出）、`--help-json`（机器可读 help/quickstart 输出）、以及 `--schema`（机器可读 schema catalog）。当前 `--doctor-json` 还带稳定字段 `kind`、`schemaVersion`、`defaultPolicyProfile`、`recommendedCommands`、`effectiveRunCommand` 与 `profileCommands`，以及轻量 `recommendedConsumption` 指引，便于外部脚本长期依赖；`--help-json` 也会给出稳定的 `defaults`、`quickstart`、`shortcuts`、`examples`、`profiles` 与同样的 `recommendedConsumption` 入口；`--schema` 会显式列出 `helpJson` / `statusJson` / `doctorJson` 三个 JSON surface 的字段集合与版本信息，并通过 `related.catalogJson` 暴露 shared catalog reader 及其 `helpJson` / `schema` / `catalogJson` surfaces。当前 `related.catalogJson` 里还额外带一个小型 `consumptionGuide`，直接指出 `evolution.stabilityLevel`、`evolution.stabilityNotes`、`evolution.compatibilityStatus`、`evolution.breakingChanges`、`evolution.migrationHints`、`evolution.consumerWarnings`、`evolution.parsingPolicy`、`evolution.strictnessLevels`、`evolution.integrationProfiles`、`evolution.consumerExamples`、`evolution.consumerProfiles` 与 `evolution.recommendedConsumptionOrder` 这些最值得先看的路径。现在它还会额外给出 `contractPaths`，直接指向 shared catalog 顶层 `contracts.recommendedConsumption.{sharedCore,repoRootInstall,catalogReader,powershellRegister}`，给出 `metaPaths`，直接指向各 CLI 自身的 `recommendedConsumptionMeta`，并再给出 `metaContractPath=contracts.recommendedConsumptionMeta`、`summaryMetaPaths` 与统一的 `summaryContractPath=contracts.recommendedConsumptionSummary`。对于不想一上来就展开整份 schema 的消费者，repo-root `--help-json` / `--doctor-json` 现在也都带 `recommendedConsumption`，可以先拿最小入口命令、推荐 strictness/profile、example consumer ids 和 parsing hints。现在这两个 surface 以及 catalog reader `--help-json` 还会额外镜像 `catalogSummaryDiscoverabilityPath=summaryDiscoverability.recommendedConsumption` 与对应对象本身，因此轻量 JSON 消费者也能直接发现 top-level summary 入口，而不必先读取 schema。这个 block 现在还会额外带 `summaryDisplay`、`compactSummary`、`firstFieldsDisplay`、`exampleConsumersDisplay` 四个更短的共享摘要字段，以及一个更轻的 `summaryDiscoverability` 入口，用来直接指出 `summaryDiscoverabilityContractPath`、`summaryContractPath`、`summaryMetaPath`、schema 命令、以及推荐优先看的短摘要字段。它现在也直接带 `metaPaths` 与 `metaContractPath=contracts.recommendedConsumptionMeta`，因此轻量 surface 也能直接发现 per-CLI self-description 层。除此之外，repo-root `--help-json` / `--doctor-json` 与 catalog reader `--help-json` 现在还会直接镜像各自的 `recommendedConsumptionMetaPath`、统一的 `recommendedConsumptionMetaContractPath=contracts.recommendedConsumptionMeta`，以及 focused `recommendedConsumptionMetaContract` 对象本身，让调用方不必先展开 `recommendedConsumption.metaPaths` 才能定位当前 surface 的 self-description block。现在这几个轻量 JSON surface 还会进一步镜像 `recommendedConsumptionMetaDiscoverabilityPath`、`recommendedConsumptionMetaDiscoverabilityContractPath=contracts.recommendedConsumptionMetaDiscoverability` 与 `recommendedConsumptionMetaDiscoverability` 对象本身，让调用方可以直接从更小的 per-CLI meta discoverability block 起步，再按推荐 lookup 顺序回到 `recommendedConsumptionMeta`、当前 instance contract 与相邻 summary-layer 路径。repo-root 普通 `--help` 与 `--doctor` 现在都会直接渲染这套 shared compact summary，并额外打印 `Per-CLI self-description layer:` 与 `Top-level summary layer:` 两段提示，前者现在会直接给出当前 `recommendedConsumptionMeta` path / contract / meta discoverability path / contract / current instance / current contract，后者继续给出 `summaryDiscoverability.recommendedConsumption`、`contracts.summaryDiscoverability`、`summarySurfaceGuide.recommendedConsumption`、`contracts.summarySurfaceGuide` 与 `entry -> contract -> surfaceGuide -> summaryMeta` 这条推荐阅读顺序。catalog reader 自己的 `--help-json` 现在也带同类 `recommendedConsumption`，而普通 `--help` 文本里也会直接打印同一套短摘要，并同样附带这组 top-level summary-layer hints 和 self-description-layer hints。与此同时，shared catalog 的 `cli.powershellRegister` 现在也补了镜像版 `recommendedConsumption`，会直接指向推荐 PowerShell 安装命令、repo-root schema/catalog 命令，以及 `coverage.powershellRegister` 这条必须先看的边界提示，并带同样的 compact summary fields、`metaPaths`、`metaContractPath` 和 `summaryDiscoverability` 轻入口。它的 `cli.powershellRegister.recommendedConsumptionMeta` 现在也和 bash-backed CLI 一样，继续镜像 `metaDiscoverabilityPath`、`metaDiscoverabilityContractPath=contracts.recommendedConsumptionMetaDiscoverability` 与 `metaDiscoverability` 对象本身，因此如果外部消费者只想要 PowerShell mirror 的轻量 per-CLI self-description 入口，也可以直接从 `cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverability` 起步，然后再用 `coverage.powershellRegister` 判断当前宿主是否只是 docs/schema mirror。为了减少调用方自己猜“下一步该看哪个 field”，PowerShell mirror 现在还额外给出 `metaDiscoverabilityStartPath`、`notesPath`、`discoveryCommands.{meta,coverage,notes}` 与 `recommendedDocsMirrorReadOrder`，明确建议先看 meta-discoverability，再看 coverage，再看 notes，最后才展开更完整的 `cli.powershellRegister.recommendedConsumption`。这组 PowerShell mirror guidance 现在也被镜像进更窄的 schema-first surfaces：repo-root `--schema.related.catalogJson.consumptionGuide` 会显式给 `powershellMirrorGuidancePaths` 与 `powershellMirrorConsumerExampleId`，catalog reader `--schema.outputs.catalogJson.recommendedConsumption` 也会给同名两项，因此调用方即使只停留在 consumption-guide 层，也能直接发现 PowerShell docs/schema-mirror 的推荐读取路径与对应 consumer example。并且 shared catalog 顶层现已新增显式 `contracts.recommendedConsumption`，把 `sharedCore`、`repoRootInstall`、`catalogReader`、`powershellRegister` 四类 recommendedConsumption shape 直接做成可发现 contract；同时又新增了 `contracts.recommendedConsumptionMeta`，专门描述 per-CLI `recommendedConsumptionMeta` 自描述层，再新增更窄的 `contracts.recommendedConsumptionSummary`，专门描述短摘要层，以及 `contracts.recommendedConsumptionSummaryDiscoverability`，专门描述 light-entry `summaryDiscoverability` block 自身。现在 catalog reader 自己的 `--schema` 也会显式给出这层 `recommendedConsumption.contractPaths`、`recommendedConsumption.metaPaths`、`recommendedConsumption.summaryMetaPaths`、`recommendedConsumption.summaryDiscoverabilityPaths`、`recommendedConsumption.summaryContractPaths`、`recommendedConsumption.metaContractPath`、`recommendedConsumption.summaryContractPath`、`recommendedConsumption.summaryDiscoverabilityContractPath`，以及镜像 `recommendedConsumption.contract` / `recommendedConsumption.metaContract` / `recommendedConsumption.summaryContract` / `recommendedConsumption.summaryDiscoverabilityContract`，不必先完整展开 catalog 实例再猜 contract。repo-root schema 与 reader schema 还会额外镜像 `summaryDiscoverabilityPath=summaryDiscoverability.recommendedConsumption` 以及对应的 top-level summary block 本身，方便消费者直接从顶层 summary entry point 起步。进一步地，`cli.repoRootInstall`、`cli.catalogReader`、`cli.powershellRegister` 现在也各自带有 `recommendedConsumptionMeta`，直接镜像本 CLI 的 `instancePath`、`contractPath`、`metaContractPath`、`summaryMetaPath`、`summaryDiscoverabilityPath`、`summarySurfaceGuidePath`、`summaryContractPath`、`summaryDiscoverabilityContractPath`、`summarySurfaceGuideContractPath` 和 focused contract 摘要；现在这层 self-description block 里还继续镜像了更轻的 `metaDiscoverabilityPath`、`metaDiscoverabilityContractPath=contracts.recommendedConsumptionMetaDiscoverability` 与 `metaDiscoverability` 对象本身。shared catalog 也同步补了 `contracts.recommendedConsumptionMetaDiscoverability`、schema `consumptionGuide.metaDiscoverabilityPaths`、以及 reader schema 里的 `recommendedConsumption.metaDiscoverabilityPaths` / `metaDiscoverabilityContractPath` / mirrored object hints。与此同时，每个 `recommendedConsumption` 对象本身也都有 `summaryMeta` 和更轻的 `summaryDiscoverability`，分别服务于完整短摘要元信息与轻入口发现。这样 per-CLI self-description、per-CLI meta discoverability、compact summary discoverability、short-summary focused contract、以及轻入口 summary discoverability 都已经闭环。现在又进一步补了一层 top-level `summaryDiscoverability.recommendedConsumption` 聚合块，所以如果外部消费者只关心短摘要层入口，也可以直接从 catalog 顶层起步，而不是必须先钻进 `cli.*.recommendedConsumption.*`。在此基础上，shared catalog 现在还补了 `contracts.summaryDiscoverability` 与更具体的 `contracts.summarySurfaceGuide`。前者给 top-level summary entry object 本身一个稳定 contract，后者则专门描述 `summarySurfaceGuide.recommendedConsumption` 这个 focused surface-guide block，用来告诉消费者：`summaryDiscoverability.recommendedConsumption` 这个顶层 summary 入口分别会镜像到哪些 compact、schema、help-json、doctor-json surface，以及推荐先读哪一类 surface。现在 `summaryDiscoverability.recommendedConsumption` 自身也额外带 `preferredReadOrder`，把“先读 entry object，再看 focused contract，再跳 surface guide，最后按需展开 summary meta”这条建议链做成了 machine-readable 字段，而不是只留在文档解释里。repo-root `--help-json` / `--doctor-json`、catalog reader `--help-json` 现在都额外镜像 `catalogSummarySurfaceGuidePath=summarySurfaceGuide.recommendedConsumption`、`catalogSummarySurfaceGuideContractPath=contracts.summarySurfaceGuide` 与对应对象本身；repo-root `--schema`、catalog reader `--schema` 和 shared catalog `--compact` 也都直接暴露 `summarySurfaceGuidePath`、`summarySurfaceGuideContractPath`、`summarySurfaceGuide`、`summarySurfaceGuideContract`。同时，为了让 schema-first 消费面更对称，repo-root `related.catalogJson.consumptionGuide` 与 catalog reader `outputs.catalogJson.recommendedConsumption` 现在还额外镜像 `topLevelSummaryPaths={discoverability,surfaceGuide}`、`topLevelSummaryContractPaths={discoverability,surfaceGuide}`，以及单独的 `summarySurfaceGuideContractPath`，这样消费者即使只读较窄的 consumption-guide block，也能同时发现 top-level summary entry 与 top-level surface guide 的 path/contract 对。继续往前收了一刀之后，三类 per-CLI `recommendedConsumption` 实例本身现在也直接带同样三项，而 `contracts.recommendedConsumption.{sharedCore,repoRootInstall,catalogReader,powershellRegister}` 也已显式声明这些字段类型，所以 top-level summary path maps 现在不再只是外围 schema wrapper 的附加提示，而是已经进入更窄、更稳定的 per-CLI instance contract 层。这让 top-level summary layer 不再只是“有入口 path”，而是已经有一份更稳定、显式、machine-readable 的 surface map，供 schema-first、catalog-first、help-first 三类消费者统一发现和读取。仓库现在还提供 `./scripts/read-operator-signal-cron-catalog.sh`，可读出 `kind=operator-signal-cron-cli-catalog` 的 shared catalog，并支持 `--help-json` 与 `--schema` 自描述；这个 catalog 现已同时覆盖 repo-root alias、bash helpers、catalog reader 本身，以及 PowerShell helper 的 docs/schema/capabilities 元数据，并额外提供 top-level `provenance` / `coverage` / `evolution` / `capabilities` / `surfaces` / `contracts` 聚合块，方便外部消费者直接拿到高层 provenance/coverage truth、manifest evolution history、capability manifest、surface inventory 与 contract pairs。其中 `evolution` 现已进一步细化到 `historyVersion`、`compatibilityPolicy`、`compatibilityStatus`、`stabilityLevel`、顶层 `stabilityNotes` / `compatibilityNotes` / `breakingChanges` / `migrationHints` / `consumerWarnings`、`parsingPolicy`、`strictnessLevels`、`integrationProfiles`、`consumerExamples`、`consumerProfiles`、`recommendedConsumptionOrder`、`latestChangedFields`，以及每个 milestone 的 `changedFields` / `compatibilityNotes`，因此不只是“记录发生过哪些阶段”，而是开始变成最小可机读的 schema evolution record + consumption guide。显式传入 `--policy-profile deploy` / `identity` 等参数时，repo-root alias 现在也会正确覆盖默认 `default`，不再把两者叠加进同一条 runCommand。

支持动作：

- `-Action install`
- `-Action show`
- `-Action remove`
- `-Action run-now`
- `-Recreate`
- `-UseEntryIdentityPolicyExample`
- `-UseMonitoringDeployPolicyExample`
- `-PolicyBundle entry-identity|monitoring-deploy|baseline`
- `-PolicyProfile default|identity|deploy`

示例：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -PolicyProfile default
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -UseEntryIdentityPolicyExample
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -UseMonitoringDeployPolicyExample
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -UseEntryIdentityPolicyExample -UseMonitoringDeployPolicyExample
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -PolicyBundle baseline
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action show
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action run-now
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action remove
```

```bash
./scripts/register-openclaw-operator-signal-cron.sh
./scripts/register-openclaw-operator-signal-cron.sh --dry-run
./scripts/register-openclaw-operator-signal-cron.sh --print-run-command
./scripts/register-openclaw-operator-signal-cron.sh --print-message
./scripts/register-openclaw-operator-signal-cron.sh --action show
./scripts/register-openclaw-operator-signal-cron.sh --policy-profile default
```

其中 bash helper 在未显式传 policy flags 时会默认走 `policy-profile=default`，并支持 `--dry-run` 先看生成的 cron payload，而不直接改动 OpenClaw cron 状态；若只想看最终 repo-local command 或最终 OpenClaw message body，可改用 `--print-run-command` / `--print-message`。

加上 policy example 开关、`-PolicyBundle` 或 `-PolicyProfile` 时，helper 会把 cron body 改成直接设置高层 env。当前推荐：

```bash
OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE="default" ./scripts/run-operator-signal-check.sh --compact
OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE="identity" ./scripts/run-operator-signal-check.sh --compact
OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE="deploy" ./scripts/run-operator-signal-check.sh --compact
```

兼容的 bundle form 仍支持：

```bash
OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE="entry-identity" ./scripts/run-operator-signal-check.sh --compact
OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE="monitoring-deploy" ./scripts/run-operator-signal-check.sh --compact
OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE="baseline" ./scripts/run-operator-signal-check.sh --compact
OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE="entry-identity,monitoring-deploy" ./scripts/run-operator-signal-check.sh --compact
```

wrapper 会在运行时通过 `scripts/render-operator-signal-policy-bundle.sh` 把这些 profile/bundle 名解析成最终 `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON`，这样 cron body 不必再手工包 command substitution。

---

## What the cron job is expected to do

OpenClaw cron job body should normally only do this:

```bash
./scripts/run-operator-signal-check.sh --compact
```

如果你已经决定把 repo-local notify policy 一起带进 cron body，也可以改成：

```bash
OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE="default" ./scripts/run-operator-signal-check.sh --compact
OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE="identity" ./scripts/run-operator-signal-check.sh --compact
OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE="deploy" ./scripts/run-operator-signal-check.sh --compact
```

约束：

- 不编辑源码
- 不修改 repo 配置
- 不尝试在 cron body 里重写 signal 判定逻辑
- 仅执行 repo-local wrapper，并在失败时给出简短结果

---

## State written by the wrapper

当前 wrapper 会把结果落到：

- `run/operator-signals/last.json`
- `run/operator-signals/last.status`
- `run/operator-signals/result-<timestamp>.json`
- `run/operator-signals/last-notify-state.json`（最近一次已发送通知的归一化状态）

这意味着即使 OpenClaw cron 本身不做 delivery，也仍然有：

- 最新状态文件
- 最近历史结果
- shell-friendly exit code

适合后续继续接：

- chat notifier
- OS cron / systemd timer
- dashboard collector
- 更正式的 exporter

---

## Notification strategy

当前建议分两层：

### Layer 1, always on

OpenClaw cron 定时跑 wrapper，持续更新 `run/operator-signals/*.json`。

### Layer 2, optional escalation

在 host 环境设置：

- `OPERATOR_SIGNAL_NOTIFY_ON=critical` 或 `warn`
- `OPERATOR_SIGNAL_NOTIFY_COMMAND=<your notifier>`（通用兜底）
- `OPERATOR_SIGNAL_NOTIFY_WARN_COMMAND=<your warn notifier>`
- `OPERATOR_SIGNAL_NOTIFY_CRITICAL_COMMAND=<your critical notifier>`
- `OPERATOR_SIGNAL_NOTIFY_RECOVERY=1`
- `OPERATOR_SIGNAL_NOTIFY_RECOVERY_COMMAND=<your recovery notifier>`
- `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON=[{"name":"rule","matchAll":["service:name"],"command":"/path/to/command"}]`

仓库里现在还附了两个可直接复用的示例策略：

```bash
export OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(cat ./scripts/operator-signal-policy-entry-identity.example.json)"
export OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(cat ./scripts/operator-signal-policy-monitoring-deploy.example.json)"
export OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(jq -cs add ./scripts/operator-signal-policy-entry-identity.example.json ./scripts/operator-signal-policy-monitoring-deploy.example.json)"
```

其中 entry-identity 样板把 consumer-entry 的 identity governance 告警分成两层：

- `entry-identity-hard`
  - `consumer_entry:identity_binding_not_loaded`
  - `consumer_entry:identity_registry_not_loaded`
  - `consumer_entry:identity_ref_integrity_not_ok`
  - 直接走 `policy:entry-identity:page`
- `entry-identity-soft`
  - `consumer_entry:identity_governance_invalid`
  - `consumer_entry:identity_actor_gate_invalid`
  - `consumer_entry:identity_approval_source_invalid`
  - `consumer_entry:identity_approval_coverage_invalid`
  - 先走 `policy:entry-identity:chat`
  - 同一 soft policy 在窗口内累计 3 次后自动升级到同一个 page route

这样值班侧能区分“治理面需要修，但 live identity 还没断”与“binding/registry/ref-integrity 已经真出问题”这两类 incident。

monitoring-deploy 样板也分两层：

- `monitoring-deploy-hard`
  - `monitoring_deploy:metadata_unreadable`
  - 直接走 `policy:monitoring-deploy:page`
- `monitoring-deploy-soft`
  - `monitoring_deploy:post_action_status`
  - 先走 `policy:monitoring-deploy:chat`
  - 在窗口内连续命中 3 次后升级到同一个 `policy:monitoring-deploy:page`

规则还支持两层可选控制字段。

基础匹配/排序/门槛/抑制层：

- `priority`，显式优先级，数值越大越优先；用于避免 policy 选择完全依赖规则书写顺序
- `groupKey`，把多条 policy 归到同一个 family/group 下做共享计数
- `groupMinOccurrences`，该 family 在窗口内至少累计出现多少次后，当前 policy 才 eligible
- `groupMinActiveSeconds`，该 family 至少持续多少秒后，当前 policy 才 eligible
- `groupOccurrenceWindowSeconds`，family 级 occurrence 窗口；未配置时默认继承 policy 自己的 `occurrenceWindowSeconds`
- `groupMaxGapSeconds`，family 级 gap reset；未配置时默认继承 policy 自己的 `maxGapSeconds`
- `matchAny`，候选 signal 中任意命中其一即可
- `matchNone`，这些 signal 里只要命中任意一个，当前 policy 就不会 eligible
- `minOccurrences`，至少命中多少次后才升级为 policy route
- `minActiveSeconds`，至少持续多少秒后才升级为 policy route
- `occurrenceWindowSeconds`，只统计最近多少秒内的命中次数，适合表达“X 分钟内连续 / 累计出现 N 次”
- `maxGapSeconds`，若两次命中间隔超过该值，就把 streak / 活跃期重置，适合表达“连续出现”而不是“很久以前也算一次”
- `suppressedByPolicies`，当这些更高阶 policy 中任意一条已 eligible 时，当前 policy 虽然命中也不会抢路由
- `suppressedByGroups`，当这些 family/group 中任意一组已有 eligible member 时，当前 policy 虽然命中也不会抢路由

升级层：

- `escalateAfterOccurrences`，这条 policy 自己累计出现到多少次后，切到 escalation 路由
- `escalateAfterSeconds`，这条 policy 自己持续多少秒后，切到 escalation 路由
- `escalationCommand`，升级后优先改走的新命令；未配置时沿用原 command
- `escalationRoute`，升级后的 route 名；未配置时默认是原 route 后面追加 `:escalated`
- `groupEscalateAfterOccurrences`，整个 family/group 累计出现到多少次后，切到 group escalation 路由
- `groupEscalateAfterSeconds`，整个 family/group 持续多少秒后，切到 group escalation 路由
- `groupEscalationCommand`，group escalation 后优先改走的新命令；未配置时沿用已有 command
- `groupEscalationRoute`，group escalation 后的 route 名；未配置时默认是原 route 后面追加 `:group-escalated`
- `OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON={"service:name":"/path/to/command"}`

仓库里现在也提供了一个最小 notifier 模板：

```bash
./scripts/notify-operator-signals-example.sh
```

它的行为是：

- 从 stdin 读取 wrapper 输出的 JSON
- 若 wrapper 已提供 `notify.policy_summary` / `OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY`，优先把这段更短的人话摘要写进通知内容
- 也支持三档长度：`notify.policy_summary_levels.ultra_short|short|full`，模板可通过 `OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_LEVEL` 选择更短或更全的版本
- notifier 模板现在不只是替换 `notify.policy_summary` 这一行，而是会连整条通知正文一起按 `ultra_short|short|full` 切换，方便同一条 incident 在标题式提醒、普通聊天通知、值班排障记录之间复用
- 现在还会额外生成一个单行 `family_brief`，例如 `critical{refund=1} | warn{audit=1,entry-abuse=2,upstream=1}`，适合放标题位或频道 topic
- full 模式本地日志现在也会附带一个紧凑的 `family_grouped_alerts:` 多行块，以及一个单行 `summary_views_json:` 机器可读块，方便直接在机器上看 family 聚合视图或做 grep/脚本消费
- 若走 `OPERATOR_SIGNAL_NOTIFY_WEBHOOK_MODE=json`，模板现在还会把最终选中的正文形态直接打进 webhook payload，例如 `selected_level`、`rendered_text`、`alerts_brief`、`family_brief`、结构化 `family_grouped_alerts` 以及 `summary{...}`；同时新增稳定容器 `render.summary_views` / `summary.summary_views`，方便外部路由器/值班系统一次性消费所有摘要视图。该容器当前带 `version: 1` 作为 schema/version 标记
- `alerts_brief` 现在按 critical-first 压缩，并会先按 family（如 `refund`、`audit`、`upstream`、`worker`、`entry-abuse`）再按 service 收口，短通知和 webhook 标题位能更快看出“先哪类故障、再哪些服务在响”；若默认 family heuristics 不合适，也可用 `OPERATOR_SIGNAL_NOTIFY_ALERT_FAMILY_RULES_JSON` 自定义映射
- 默认写本地日志到 `run/operator-signals/notifications.log`
- 可选通过 `OPERATOR_SIGNAL_NOTIFY_WEBHOOK_URL` 发 webhook
- 默认不往 stdout 打印内容，避免污染 wrapper 的 JSON 输出
- 如需本地调试可设 `OPERATOR_SIGNAL_NOTIFY_ECHO=1`

最小 `summary_views` schema（当前 `version: 1`）可按下面理解：

```json
{
  "version": 1,
  "policy_summary": "critical incident new state: ...",
  "family_brief": "critical{refund=1} | warn{audit=1,upstream=1}",
  "alerts_brief": "critical refund(execution:refund_failures) | warn upstream(gateway:invocation_create_upstream_failures)",
  "family_grouped_alerts": [
    {
      "severity": "critical",
      "severity_rank": 0,
      "signal_count": 1,
      "family_count": 1,
      "families": [
        {
          "family": "refund",
          "signal_count": 1,
          "service_count": 1,
          "services": [
            {
              "service": "execution",
              "signal_count": 1,
              "names": ["refund_failures"]
            }
          ]
        }
      ]
    }
  ]
}
```

其中：

- `policy_summary` 适合直接贴人类通知正文
- `family_brief` 适合标题 / topic / 副标题
- `alerts_brief` 适合一行 incident headline
- `family_grouped_alerts` 适合下游脚本、值班系统或 webhook 消费方继续加工

### summary_views evolution / compatibility notes

当前建议把 `render.summary_views` / `summary.summary_views` 视为 notifier 的稳定摘要接口，兼容约定如下：

- 先看 `version`，当前固定为 `1`
- 新接入方优先读 `summary_views` 容器，不要依赖同级平铺字段长期不变
- 当前仍保留平铺字段（如 `alerts_brief`、`family_brief`、`family_grouped_alerts`）用于兼容老消费方，但后续新增摘要视图会优先进入 `summary_views`
- `summary_views_json:` 本地日志行与 webhook JSON 中的 `summary.summary_views` / `render.summary_views` 应视为同一份摘要模型，只是承载位置不同
- 若未来要扩字段，应优先做“只增不删”，并在必要时提升 `version`
- 若外部脚本只需要一个最小稳定接口，当前推荐最少读取：`version`、`policy_summary`、`family_brief`、`alerts_brief`

一个简单演进视角可以这样理解：

- v1 首批稳定字段：`policy_summary`
- v1 可读摘要增强：`family_brief`、`alerts_brief`
- v1 结构化视图增强：`family_grouped_alerts`
- v1 统一容器化：`render.summary_views` / `summary.summary_views` + 本地 `summary_views_json:`

### summary_views consumer checklist

给新接入方的最短建议只有 5 条：

1. 先读 `version`，当前按 `1` 处理
2. 优先消费 `summary.summary_views`（或 `render.summary_views`），不要先绑定平铺字段
3. 最少先取 `policy_summary`、`family_brief`、`alerts_brief`
4. 需要结构化分组时，再读 `family_grouped_alerts`
5. 若消费本地日志，优先抓 `summary_views_json:` 这一行，而不是自己从整段文本里重新解析

### summary_views changelog / evolution timeline

一个简短时间线可按下面理解：

- v1.0：稳定 `policy_summary`，作为最小的人类可读摘要
- v1.1：补 `family_brief`、`alerts_brief`，覆盖标题位与单行 incident headline
- v1.2：补 `family_grouped_alerts`，提供按 severity/family/service 分层的结构化视图
- v1.3：把上述字段收进 `render.summary_views` / `summary.summary_views`，同时保留平铺字段兼容
- v1.4：full 本地日志补 `summary_views_json:` 单行机器可读块，方便 grep / shell 二次消费
- v1.5：为 `summary_views` 与 `summary_views_json` 补 `version: 1`，明确 schema 边界

注意，这里的 `v1.x` 是文档化的演进脉络，不代表独立协议大版本。真正的兼容判断仍以 `summary_views.version` 为准，当前固定为 `1`。

### summary_views recommended consumption order

如果你是在接 webhook、日志解析、值班机器人或外部路由器，当前推荐按这个顺序消费：

1. 先读 `summary.summary_views`
2. 若上面不存在，再回退读 `render.summary_views`
3. 先检查 `version`，当前按 `1` 处理
4. 先消费 `policy_summary`、`family_brief`、`alerts_brief` 这三个最常用视图
5. 只有在需要结构化 drill-down 时，再读 `family_grouped_alerts`
6. 如果来源是本地日志，优先抓 `summary_views_json:` 这一行，而不是反向解析整段人类文本
7. 只有在兼容旧接入方时，才回退读平铺字段（如顶层 `alerts_brief` / `family_brief`）

### summary_views consumer snippet

最小 jq / shell 风格消费示例：

```bash
summary_views_json=$(jq -c '.summary.summary_views // .render.summary_views // {}' webhook.json)
version=$(jq -r '.version // 0' <<<"$summary_views_json")
if [ "$version" != "1" ]; then
  echo "unsupported summary_views version: $version" >&2
  exit 1
fi

policy_summary=$(jq -r '.policy_summary // ""' <<<"$summary_views_json")
family_brief=$(jq -r '.family_brief // ""' <<<"$summary_views_json")
alerts_brief=$(jq -r '.alerts_brief // ""' <<<"$summary_views_json")

printf 'policy=%s\nfamily=%s\nalerts=%s\n' \
  "$policy_summary" "$family_brief" "$alerts_brief"
```

如果消费的是本地 `notifications.log`，可先抓 `summary_views_json:` 这一行：

```bash
summary_views_json=$(sed -n 's/^summary_views_json: //p' notifications.log | tail -n 1)
```

### summary_views sample payload

下面是一份更接近真实 webhook 的简化样例，方便直接对照字段接入：

```json
{
  "selected_level": "ultra_short",
  "rendered_text": "[CEX operator signals]\nchecked_at: 2026-04-19T04:39:00+08:00\noverall: critical\nnotify_policy_summary: critical: refund-chain -> policy:refund-chain:page (policy-escalation)\nfamily_brief: critical{refund=1} | warn{audit=1,entry-abuse=2,upstream=1}\nalerts_brief: critical refund(execution:refund_failures) | warn audit(execution:audit_failures); entry-abuse(consumer_entry:rate_limited_requests,matrix_entry:duplicate_events); upstream(gateway:invocation_create_upstream_failures)",
  "summary": {
    "overall": "critical",
    "notify_reason": "direct",
    "notify_severity": "critical",
    "summary_views": {
      "version": 1,
      "policy_summary": "critical: refund-chain -> policy:refund-chain:page (policy-escalation)",
      "family_brief": "critical{refund=1} | warn{audit=1,entry-abuse=2,upstream=1}",
      "alerts_brief": "critical refund(execution:refund_failures) | warn audit(execution:audit_failures); entry-abuse(consumer_entry:rate_limited_requests,matrix_entry:duplicate_events); upstream(gateway:invocation_create_upstream_failures)",
      "family_grouped_alerts": [
        {
          "severity": "critical",
          "families": [
            {
              "family": "refund",
              "services": [
                { "service": "execution", "names": ["refund_failures"] }
              ]
            }
          ]
        },
        {
          "severity": "warn",
          "families": [
            {
              "family": "audit",
              "services": [
                { "service": "execution", "names": ["audit_failures"] }
              ]
            },
            {
              "family": "entry-abuse",
              "services": [
                { "service": "consumer_entry", "names": ["rate_limited_requests"] },
                { "service": "matrix_entry", "names": ["duplicate_events"] }
              ]
            },
            {
              "family": "upstream",
              "services": [
                { "service": "gateway", "names": ["invocation_create_upstream_failures"] }
              ]
            }
          ]
        }
      ]
    }
  }
}
```

如果后续想把 OpenClaw cron + repo-local wrapper 再接到 Prometheus，而不重写一套 exporter，当前仓库还补了一个最小 bridge：

```bash
./scripts/render-operator-signals-prometheus.sh run/operator-signals/last.json
```

或者直接串起来：

```bash
./scripts/check-operator-signals.sh --compact | ./scripts/render-operator-signals-prometheus.sh
```

它会把 wrapper 的统一 JSON 渲染成 Prometheus text exposition，适合作为 node_exporter textfile collector 一类最小接线的上游。配套 example 规则当前见：

- `ops/monitoring/prometheus/core-runtime-operator-signals-from-wrapper.example.yml`
- `ops/monitoring/alertmanager/core-runtime-operator-signals-from-wrapper-routing.example.yml`
- `ops/monitoring/prometheus/product-edge-operator-signals-from-wrapper.example.yml`
- `ops/monitoring/alertmanager/product-edge-operator-signals-from-wrapper-routing.example.yml`
- `ops/monitoring/prometheus/monitoring-deploy-operator-signals-from-wrapper.example.yml`
- `ops/monitoring/alertmanager/monitoring-deploy-operator-signals-from-wrapper-routing.example.yml`
- `ops/monitoring/prometheus/minimal-wrapper-monitoring-bundle.example.yml`
- `ops/monitoring/alertmanager/minimal-wrapper-monitoring-bundle.example.yml`
- `ops/monitoring/monitoring-bundle-manifest.example.yml`
- `scripts/assemble-monitoring-bundles.sh`
- `scripts/assemble-monitoring-bundles.sh --check`
- `scripts/export-monitoring-bundles.sh`
- `scripts/export-monitoring-bundles.sh --output-dir /tmp/cex-monitoring-export --include-focused`
- `scripts/install-monitoring-bundles.sh`
- `scripts/install-monitoring-bundles.sh --install-dir /tmp/cex-monitoring-install --include-focused`
- `scripts/overlay-monitoring-bundles.sh`
- `scripts/overlay-monitoring-bundles.sh --target-root /tmp/cex-monitoring-overlay --include-focused`
- `scripts/deploy-monitoring-bundles.sh`
- `scripts/deploy-monitoring-bundles.sh --deploy-root /tmp/cex-monitoring-live-target --include-focused`
- `scripts/deploy-monitoring-bundles.sh --from-overlay-root /tmp/cex-monitoring-overlay --mode copy`
- `scripts/deploy-monitoring-bundles.sh --reload --reload-dry-run`
- `scripts/deploy-monitoring-bundles.sh --verify --verify-attempts 5 --verify-delay-secs 2`
- `scripts/reload-monitoring-targets.sh`
- `scripts/reload-monitoring-targets.sh --mode command --prometheus-command 'systemctl reload prometheus' --alertmanager-command 'systemctl reload alertmanager'`
- `scripts/reload-monitoring-targets.sh --mode command --failure-policy restart --prometheus-command 'systemctl reload prometheus' --prometheus-restart-command 'systemctl restart prometheus'`
- `scripts/verify-monitoring-targets.sh`
- `scripts/verify-monitoring-targets.sh --attempts 5 --delay-secs 2`

另外，`run-operator-signal-check.sh` 现在默认启用：

- `OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY=1`
- `OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS=1800`

也就是：

- 告警状态相对“上一次已发送通知”发生变化时，会立即再次触发 notifier
- 若状态一直没变，则不会每个 cron tick 都重复通知
- 但若同一状态持续超过 1800 秒，wrapper 会再发一条 reminder（`notify.reason=reminder_interval_elapsed`）

若同时配置了 batch policy、signal-specific、severity-specific 与通用 command，wrapper 当前优先级是：

1. `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON` 命中的 batch/correlation rule
2. `OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON` 中命中的 `service:name -> command`
3. severity / phase 路由
   - warn -> `OPERATOR_SIGNAL_NOTIFY_WARN_COMMAND`
   - critical -> `OPERATOR_SIGNAL_NOTIFY_CRITICAL_COMMAND`
   - recovery/resolved -> `OPERATOR_SIGNAL_NOTIFY_RECOVERY_COMMAND`
4. 通用 `OPERATOR_SIGNAL_NOTIFY_COMMAND`

batch/correlation rule 适合这种场景：

- `gateway:invocation_create_upstream_failures + execution:refund_failures` 作为一个 incident 单独走最重路径
- `approval_backlog + queued_worker_lease_expired` 组合时走 worker/operator 联合排障路径
- 入口层和核心层同时异常时，优先走 incident 合流命令而不是各自散发
- 某组合必须连续出现 2 次以上，或持续 10 分钟以上，才值得升级成 incident 路由
- 某告警要在 5 分钟窗口里出现 3 次以上才算真实抖动，而不是偶发一次
- 某组合虽然出现过两次，但两次间隔太久，不该还算同一波 incident
- 某条 incident route 先发到普通值班频道，但如果持续 30 分钟还没恢复，就自动切到 escalation route
- 某条 policy 只在出现 gateway upstream failures 且没有 refund failures 时才成立，此时可用 `matchNone:["execution:refund_failures"]` 排除更严重场景
- 多条 policy 都 eligible 时，可给更重要的那条显式更高 `priority`；如果没配 priority，当前实现会优先更具体的规则，再回退到原始规则顺序
- 多条不同 policy 若都属于同一个 incident family，可给它们同一个 `groupKey`，再用 `groupMinOccurrences` 表达“这个家族 10 分钟内累计出现 3 次才算真正进入 incident 路由”
- 若这个 family 进入 incident 后还持续升级，可继续叠加 `groupEscalateAfterOccurrences` / `groupEscalateAfterSeconds`，让整组 policy 统一切到更重 route
- 若某一整组 family 成立后应压制另一整组 family，可给后者配置 `suppressedByGroups:["winning-family"]`

signal-specific 路由适合这种场景：

- `execution:refund_failures` 单独走最重路径
- `matrix_entry:duplicate_events` 单独走轻路径
- `consumer_entry:ingress_auth_failures` 走入口/安全排障路径

当某条已通知过的异常回落到阈值以下时，wrapper 默认会发一条 recovery 通知：

- `notify.reason=recovered_below_threshold`
- `notify.severity=ok`（或当前低于阈值但未完全 ok 的等级）
- `notify.previous_severity` 表示它是从哪个等级恢复下来的

输出 JSON 的 `notify.route` 也会显式标明本次走的是 `policy:<name>`、`signal:<service:name>`、`warn`、`critical`、`recovery` 还是 `default` 路由；`notify.policy_name`、`notify.signal_key` / `notify.signal_keys`、`notify.previous_signal_key` 也会直接给出关联信号信息。若 policy 使用了时间窗/连续出现门槛，输出里还会附带：

- `notify.policy_occurrences`
- `notify.policy_active_seconds`
- `notify.policy_escalated`
- `notify.policy_group_escalated`
- `notify.policy_candidates[]`，列出当前命中的 policy 候选规则、其 occurrences / active_seconds / window / gap / eligible 状态，方便排查“为什么这条 policy 还没升级”
- `notify.policy_selection_trace`，把最终候选排序和 winner/loser 原因收口成更适合人读的 explain 视图
- `notify.policy_summary`，对同一轮 policy 决策给出一段更短的人话摘要，适合直接塞进通知消息或值班摘要；当前会按 warn / critical / reminder / routing_changed / recovery 等场景自动换更贴合的措辞
- `notify.policy_summary_levels.ultra_short|short|full`，同一份摘要的三档密度版本，适合给短信式提醒、普通聊天通知、值班排障摘要分别选用
- `notify.policy_candidates[].priority` / `specificity_score` / `rule_index`，用于排查“为什么最终是这条 policy 赢了 route”
- `notify.policy_selection_trace.candidates[].decision_summary` / `decision_detail`，用于直接看“是被 suppress、被 matchNone 挡住、被 group 门槛卡住，还是只是输给了更高优先级/更具体的候选”
- `notify.policy_selection_trace.summary_lines[]` / `notify.policy_summary`，用于直接拿一段更短的 operator-facing 人话摘要
- `notify.policy_candidates[].group_key` / `group_occurrences` / `group_active_seconds` / `group_min_occurrences` / `group_min_active_seconds` / `group_eligible`，用于排查“为什么这条 policy 在自己的门槛满足后，仍然被 family/group 级门槛卡住”
- `notify.policy_candidates[].group_escalated` / `group_escalate_after_occurrences` / `group_escalate_after_seconds` / `group_escalation_route`，用于排查“为什么这条 policy 已被 family/group 级升级接管”
- `notify.policy_candidates[].match_none_blocked` / `blocked_by_signal_keys[]`，用于排查“为什么这条 policy 被负条件挡住了”
- `notify.policy_candidates[].suppressed` / `suppressed_by[]` / `suppressed_by_groups_matched[]`，用于排查“为什么这条 policy 明明 eligible 了却没有抢到 route”
- `notify.policy_candidates[].escalated` / `escalate_after_occurrences` / `escalate_after_seconds` / `escalation_route`，用于排查“为什么这条 policy 已经开始升级”

当同样的 alerts 没变，但路由从普通 `warn/critical` 升级成某条 policy route，或同一条 policy route 进一步升级成 policy escalation / group escalation route 时，`notify.reason` 会写成 `routing_changed`，避免被 unchanged suppression 吞掉。

若确实需要每次 `warn/critical` 都通知，可把 `OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY` 显式设为 `0`；若不想要 reminder，可把 `OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS=0`；若不想发恢复通知，可把 `OPERATOR_SIGNAL_NOTIFY_RECOVERY=0`。

这样真正的 notify 逻辑仍在 repo-local wrapper 外围，不会把 OpenClaw cron job 自身绑死到某一种消息渠道。

---

## Why not just use OS cron directly

当前更推荐 OpenClaw cron 的原因：

- scheduler 归 OpenClaw Gateway 管
- job 可见、可列、可 run-now、可 remove
- 后续若要升级成 agent-side summarize / routing，更平滑
- 与 repo 里已有 `register-openclaw-autopilot-cron.ps1` 风格一致

---

## Current limitation

这个 cron 接线仍然是最小版，当前还没覆盖：

- 对 `consumer-entry-api` / `matrix-entry-adapter` 做统一 signal 汇总
- 更完整的 policy DSL（例如抑制关系、跨时间窗聚合、按来源分层权重）
- 正式 Prometheus / Alertmanager 接线

所以它现在的定位是：

- **repo-local OpenClaw cron integration template**
- 不是最终生产监控编排

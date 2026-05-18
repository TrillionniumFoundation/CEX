# Scripts

## Preferred entrypoints

Day-to-day regression validation on Windows should use:

```powershell
powershell -ExecutionPolicy Bypass -File .\gate-local.ps1
```

or CI-safe/service-local mode:

```powershell
powershell -ExecutionPolicy Bypass -File .\gate-local.ps1 -ServiceLocalOnly
```

The PowerShell helper also honors `CEX_ENV_FILE`, then `.env`, then `.env.example`; this lets PowerShell Core run service-local and full core-runtime gates on Linux without requiring a checked-in `.env`. On Linux, PowerShell Docker discovery also falls back to a repo-local `sudo -n docker` shim when direct Docker socket access is denied.

On Linux, use the repo-local equivalent full gate:

```bash
./scripts/gate-local-linux.sh
```

Useful Linux variants:

```bash
./scripts/gate-local-linux.sh --service-local-only
./scripts/gate-local-linux.sh --skip-db-bootstrap
./scripts/gate-local-linux.sh --with-trillionnium-ui-audit
```

Set `CEX_LINUX_GATE_TRILLIONNIUM_UI_AUDIT=1` to enable the same Trillionnium UI audit from env-driven gate runs.

For DB bootstrap, the Linux helpers prefer local `psql`, then Docker Postgres. If the Docker socket is not directly accessible but passwordless `sudo docker` works, they automatically use `sudo -n docker`; set `CEX_DOCKER_USE_SUDO=1` to force that path.

The lower-level Windows orchestrator is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\rust-regression-check.ps1
```

The Linux gate relies on:

```bash
./scripts/runtime-manager-linux.sh
./scripts/seed-local-dev.sh
```

The detached Linux runtime now also starts a repo-local queued-worker loop and the product-edge `consumer-entry-api` / `matrix-entry-adapter` surfaces by default, and it rebuilds the Rust service binaries before launch so a restart does not silently boot stale code. It writes a local identity binding/registry/approved-revision bundle under `run/linux-runtime/entry-config/` when those entry governance paths are not supplied, so operator-signal checks exercise the real identity governance path instead of treating missing bindings as healthy. If you intentionally want to reuse already-built binaries, set `CEX_RUNTIME_SKIP_BUILD=1`. Useful commands:

```bash
./scripts/runtime-manager-linux.sh restart
./scripts/execution-queued-worker.sh status
./scripts/execution-queued-worker.sh once
```

For the Trillionnium `/app`, `/world`, and `/league` browser surfaces, run the dedicated UI regression audit after starting the local-production runtime:

```bash
CEX_ENV_FILE=run/local-production/.env ./scripts/check-trillionnium-ui-audit.sh
```

The audit checks English-mode visible CJK leaks, actionable horizontal overflow, first-viewport tapability, and mobile/tablet/desktop ordering/height budgets for the current first-playable UI. It writes JSON plus first-viewport screenshots under `run/trillionnium-ui-audit/`.

For the hard Trillionnium playability scorecard gate, run:

```bash
CEX_ENV_FILE=run/local-production/.env ./scripts/check-trillionnium-world-playability-scorecard.sh
```

The scorecard reads `consumer-entry-api /health` plus `/metrics` and requires the five user-facing product metrics to be `10.0/10`: technical reliability, first playable completeness, real-player comprehension cost, long-term replayability, and economy/social strategy depth. It also keeps 10 diagnostic sub-axes (onboarding, intent mapping, quest clarity, scoring/reward explainability, feedback/recovery, economy balance, social/co-op, retention/progression, surface feedback, observability) at `10.0/10`. It writes JSON under `run/playability-scorecard/`. The real-user-beta and public-commercial wrappers also require this scorecard before they pass.

Set `CEX_ENABLE_QUEUED_WORKER=0` when you need deterministic gate/debug behavior without background queue consumption. Set `CEX_ENABLE_ENTRY_SERVICES=0` only when you explicitly want the older core-only local runtime.

If you want CEX to use a repo-local isolated OpenClaw scope instead of the default `~/.openclaw` / `main` agent, bootstrap it once with:

```bash
./scripts/bootstrap-openclaw-cex.sh
```

That writes an isolated config under `run/openclaw-cex/`; `runtime-manager-linux.sh` will auto-detect that default location and export `OPENCLAW_STATE_DIR`, `OPENCLAW_CONFIG_PATH`, `OPENCLAW_AGENT_DIR`, and `CAPABILITY_OPENCLAW_MODELS_JSON_PATH` before starting services and the Linux queued worker.

Database backup/restore production drill:

```bash
./scripts/drill-db-backup-restore.sh
```

The drill writes a custom-format `pg_dump`, restores it into a temporary Postgres database, compares core table counts, writes a JSON summary under `run/drills/`, and drops the temporary restore database by default. It uses the same Docker discovery as the Linux gate, including passwordless `sudo -n docker` fallback.

For a machine-readable operator snapshot without running the full gate:

```bash
./scripts/check-operator-signals.sh
```

If you want to turn that unified JSON into Prometheus text exposition, use:

```bash
./scripts/check-operator-signals.sh --compact | ./scripts/render-operator-signals-prometheus.sh
# or
./scripts/render-operator-signals-prometheus.sh run/operator-signals/last.json
```

For a direct runtime smoke of the native core and entry Prometheus endpoints:

```bash
./scripts/smoke-runtime-metrics.sh
```

For a live OpenClaw provider smoke probe through the repo-local CEX scope:

```bash
./scripts/probe-openclaw-provider.sh --model minimax/MiniMax-M2.5
```

For a stricter pre-production verdict that combines runtime health, native metrics, unified operator signals, provider dead-letter blockers, and an optional-but-required-by-default live provider probe:

```bash
CEX_READINESS_MODE=local CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash ./scripts/check-production-readiness.sh
```

A non-zero result is expected while provider failures, dead letters, external billing/quota blockers, or production-posture checks remain. The script defaults to `CEX_READINESS_MODE=production`, which rejects local-dev keys, missing ingress tokens, missing session-auth enforcement, non-durable edge replay/rate-limit stores, and live entry runtimes whose `/health` still shows those protections disabled. It also calls `scripts/check-trillionnium-game-account-auth.sh` in read-only mode to gate the `/account` client contract, register/login/session/logout advertisement, password-auth posture, auth rate-limit configuration, aggregate auth metrics exposure, no-secret observability boundary, and public-launch boundary. Use `CEX_READINESS_MODE=local` for the Linux/local smoke path only; production signoff should leave provider probing required and set `CEX_PROVIDER_PROBE_MODEL` to the provider/model intended for launch. To avoid repeated wrapper/signoff runs burning through short provider rate windows, readiness accepts a fresh successful provider-probe readiness log for the same model for `CEX_PROVIDER_PROBE_SUCCESS_MAX_AGE_SECONDS` seconds (default `21600`) before making another live call. The Trillionnium real-user-beta and public-commercial wrappers inherit `CEX_PROVIDER_PROBE_MODEL` / `CEX_PROVIDER_PROBE_TRANSPORT` from the loaded env unless their wrapper-specific overrides are set.

For an explicit game-account gate outside full production readiness:

```bash
./scripts/check-trillionnium-game-account-auth.sh
CEX_GAME_ACCOUNT_AUTH_MUTATING_SMOKE=1 ./scripts/check-trillionnium-game-account-auth.sh
```

The default account gate is read-only. It also requires the aggregate account-auth counters to be visible on `/metrics` and the account readiness JSON to declare `trillionnium_game_account_auth_observability_v1` with no password/token/cookie logging. The mutating smoke requires password auth to be enabled and verifies register/session/logout, Argon2id registry storage, plaintext absence, and bad-login rate limiting.

For a bounded runtime soak that repeats runtime status, metrics smoke, operator signals, and worker-queue checks, while probing the live provider at the beginning and end when configured:

```bash
CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash   CEX_SOAK_DURATION_SECONDS=300   CEX_SOAK_INTERVAL_SECONDS=30   ./scripts/soak-runtime.sh
```

The soak writes JSONL plus a summary under `run/soak/` and exits non-zero on any failed tick or provider probe.

The current readiness evidence matrix and known non-100% blockers are tracked in `docs/production-readiness-evidence-v1.md`. The final scoped Linux self-hosted signoff entrypoint is `scripts/check-production-signoff.sh`; it requires a clean repo, production readiness, and fresh 2h+ soak evidence. In production mode, readiness also requires the Trillionnium five user-facing playability metrics at 10/10 plus a fresh successful `scripts/drill-db-backup-restore.sh` summary by default; set `CEX_DB_BACKUP_RESTORE_DRILL_REQUIRED=0` only for local smoke/debug runs.

A production-posture env skeleton is available at `.env.production.example`; it enumerates the non-default keys, ingress/session-auth controls, durable edge stores, identity governance files, and provider probe settings that the default production readiness mode expects. `scripts/bootstrap-local-production-env.sh` writes owner-only local env and policy files; production readiness rejects a `CEX_ENV_FILE` that is group/other readable. Set `CEX_REQUIRED_BLOCK_CAPABILITY_PREFIXES` to require the live execution policy bundle to block known non-launch providers before signoff. Production readiness also requires a fresh monitoring deploy verification metadata file by default (`CEX_MONITORING_DEPLOY_METADATA_PATH`).

For a local generated production-posture profile (high-entropy local secrets, durable edge files under `run/local-production/`, and a repo-local OpenClaw CEX scope), use:

```bash
./scripts/bootstrap-local-production-env.sh --force
CEX_ENV_FILE=run/local-production/.env ./scripts/runtime-manager-linux.sh restart
CEX_ENV_FILE=run/local-production/.env CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash ./scripts/check-production-readiness.sh
```

`CEX_ENV_FILE` is honored by the shared shell helpers and lets local signoff avoid mutating `.env`.

It now aggregates:

- core surfaces: `gateway /v1/info`, `execution /v1/info`
- supporting product-edge surfaces: `consumer-entry-api /health`, `matrix-entry-adapter /health`

Provider-backed execution failures can be drilled from the execution service with `GET /v1/executions/provider-failures`, and dead-letter-only incidents with `GET /v1/executions/provider-dead-letters`; acknowledged historical items remain queryable with `include_acknowledged=true` / `acknowledged_only=true`, while active signals ignore acked items. A single item can be acknowledged with `POST /v1/executions/:id/provider-failure/ack` (or the dead-letter alias) after the root cause is linked to an external incident or manually closed. `check-operator-signals.sh` treats active `execution:provider_dead_letters` and `execution:provider_retry_budget_exhausted` as critical by default. Execution-service also exposes a native `GET /metrics` Prometheus text endpoint for runtime counters, lifecycle status gauges, queued-worker gauges, provider failure gauges, and operator signal gauges; gateway-service exposes native `GET /metrics` for gateway runtime counters and operator signal gauges; identity/ledger/audit/capability expose minimal native `GET /metrics` up/config/count gauges.

It also promotes a small set of entry-surface metrics into warn signals via env-driven thresholds:

- `ALERT_CONSUMER_ENTRY_RATE_LIMITED_THRESHOLD` (default `20`)
- `ALERT_CONSUMER_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD` (default `5`)
- `ALERT_MATRIX_ENTRY_RATE_LIMITED_THRESHOLD` (default `20`)
- `ALERT_MATRIX_ENTRY_INGRESS_AUTH_FAILURE_THRESHOLD` (default `5`)
- `ALERT_MATRIX_ENTRY_DUPLICATE_EVENT_THRESHOLD` (default `25`)

For a cron-ready wrapper that also persists the latest result under `run/operator-signals/`, can invoke a notification command, defaults to suppressing unchanged repeat notifications, and now exposes `notify.policy_candidates[]` plus `notify.policy_selection_trace` so you can see why a policy route is or is not yet eligible, suppressed, policy-escalated, group-escalated, or selected by priority/specificity:

```bash
./scripts/run-operator-signal-check.sh
```

Relevant env:

- `OPERATOR_SIGNAL_NOTIFY_ON=never|critical|warn|always`
- `OPERATOR_SIGNAL_NOTIFY_COMMAND='...'` (generic fallback)
- `OPERATOR_SIGNAL_NOTIFY_WARN_COMMAND='...'` (preferred route for warn)
- `OPERATOR_SIGNAL_NOTIFY_CRITICAL_COMMAND='...'` (preferred route for critical)
- `OPERATOR_SIGNAL_NOTIFY_RECOVERY=0|1` (default `1`)
- `OPERATOR_SIGNAL_NOTIFY_RECOVERY_COMMAND='...'` (preferred route for recovery/resolved notifications)
- `OPERATOR_SIGNAL_NOTIFY_POLICY_JSON='[{"name":"rule","matchAll":["service:name"],"command":"/path/to/command"}]'` (highest-priority batch/correlation routing, supports optional `priority`, `groupKey`, `groupMinOccurrences`, `groupMinActiveSeconds`, `groupOccurrenceWindowSeconds`, `groupMaxGapSeconds`, `groupEscalateAfterOccurrences`, `groupEscalateAfterSeconds`, `groupEscalationCommand`, `groupEscalationRoute`, `matchAny`, `matchNone`, `minOccurrences`, `minActiveSeconds`, `occurrenceWindowSeconds`, `maxGapSeconds`, `suppressedByPolicies`, `suppressedByGroups`, `escalateAfterOccurrences`, `escalateAfterSeconds`, `escalationCommand`, and `escalationRoute`)

Repo-local policy examples now live at:

```bash
scripts/operator-signal-policy-entry-identity.example.json
scripts/operator-signal-policy-monitoring-deploy.example.json
```

If you want a stable bundle/profile entrypoint instead of remembering file paths, use:

```bash
./scripts/render-operator-signal-policy-bundle.sh --list
./scripts/render-operator-signal-policy-bundle.sh --profile default
./scripts/render-operator-signal-policy-bundle.sh --bundle baseline
./scripts/render-operator-signal-policy-bundle.sh --bundle entry-identity --bundle monitoring-deploy
```

The preferred high-level form is now profile-based:

```bash
export OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE=default
```

If you explicitly want the rendered JSON, the preferred bundle form is:

```bash
export OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(./scripts/render-operator-signal-policy-bundle.sh --profile default)"
```

Equivalent raw-file forms are:

```bash
export OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(cat ./scripts/operator-signal-policy-entry-identity.example.json)"
# or
export OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(cat ./scripts/operator-signal-policy-monitoring-deploy.example.json)"
# or combine both
export OPERATOR_SIGNAL_NOTIFY_POLICY_JSON="$(jq -cs add ./scripts/operator-signal-policy-entry-identity.example.json ./scripts/operator-signal-policy-monitoring-deploy.example.json)"
```

The entry-identity sample intentionally splits routing into:

- `entry-identity-hard` → immediate `policy:entry-identity:page`
  - `identity_binding_not_loaded`
  - `identity_registry_not_loaded`
  - `identity_ref_integrity_not_ok`
- `entry-identity-soft` → initial `policy:entry-identity:chat`
  - `identity_governance_invalid`
  - `identity_actor_gate_invalid`
  - `identity_approval_source_invalid`
  - `identity_approval_coverage_invalid`

and lets the soft policy escalate to the same page route after repeated hits in the configured window.

The monitoring-deploy sample intentionally splits routing into:

- `monitoring-deploy-hard` → immediate `policy:monitoring-deploy:page`
  - `metadata_unreadable`
- `monitoring-deploy-soft` → initial `policy:monitoring-deploy:chat`
  - `post_action_status`

and lets repeated deploy-verdict failures escalate to the same page route after repeated hits in the configured window.
- `OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON='{"service:name":"/path/to/command"}'` (next-priority per-signal routing)
- `OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY=0|1` (default `1`)
- `OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS=<seconds>` (default `1800`, set `0` to disable reminder notifications for unchanged alerts)

For coordinated session-auth runtime authority actions across `consumer-entry-api` and `matrix-entry-adapter`, use:

```bash
./scripts/reload-session-auth-runtime.sh --action status --service both
./scripts/reload-session-auth-runtime.sh --action validate --service both --compact
./scripts/reload-session-auth-runtime.sh --action reload --service both \
  --consumer-token "$CONSUMER_ENTRY_INGRESS_TOKEN" \
  --matrix-token "$MATRIX_ENTRY_INGRESS_TOKEN"
```

The helper intentionally runs **consumer first, matrix second** for `--service both --action reload`, so the verifier can accept the next revision before the signer starts issuing it. Coordinated reload results now also include a `postStatus` block with each side's live revision plus `liveRevisionMatch`, so you can immediately see whether both services converged on the same runtime authority revision.

If you want a repo-local activation wrapper that also copies a candidate registry file into the live path and automatically rolls the file back when coordinated reload fails, use:

```bash
./activate-session-auth-runtime.sh \
  --candidate-file next.json \
  --consumer-token "$CONSUMER_ENTRY_INGRESS_TOKEN" \
  --matrix-token "$MATRIX_ENTRY_INGRESS_TOKEN"
```

By default that wrapper now uses this repo-local contract:

```text
run/local-runtime/session-auth-candidates/<candidate>
run/local-runtime/session-auth-issuer-registry.json
run/local-runtime/session-auth-issuer-registry-approved-revisions.json
run/session-auth-runtime-backups/
run/session-auth-runtime-activation/last.json
```

So `--candidate-file next.json` will resolve under `run/local-runtime/session-auth-candidates/` when you pass a basename, `live` defaults to `run/local-runtime/session-auth-issuer-registry.json`, and the approved-revisions file is auto-attached when the default file exists. You can inspect the resolved defaults with:

```bash
./activate-session-auth-runtime.sh --print-defaults
./activate-session-auth-runtime.sh --print-defaults --compact
```

That activation helper writes the candidate file into the live path, runs coordinated reload, and if the reload/convergence check fails, restores the pre-activation live file and re-runs coordinated reload to bring verifier and signer back to the previous live revision. It now also writes a default summary file to `run/session-auth-runtime-activation/last.json` (unless you override `--summary-file` or `SESSION_AUTH_RUNTIME_SUMMARY_FILE`).

If you need an explicit rollback later, use:

```bash
./rollback-session-auth-runtime.sh
./rollback-session-auth-runtime.sh --backup-file ./run/session-auth-runtime-backups/session-auth-issuer-registry.json.pre-activate.1234567890.bak.json
./rollback-session-auth-runtime.sh --print-defaults
```

That rollback helper restores either the latest matching pre-activate backup or the explicit backup file you choose, then runs coordinated reload and writes its own summary to `run/session-auth-runtime-rollback/last.json` by default.

Both activation and rollback now also append history events to `run/session-auth-runtime-history/history.jsonl` by default. To inspect that history, use:

```bash
./read-session-auth-runtime-history.sh
./read-session-auth-runtime-history.sh --compact
./read-session-auth-runtime-history.sh --latest
./read-session-auth-runtime-history.sh --action activation --status activated --limit 5
./read-session-auth-runtime-history.sh --summary
./read-session-auth-runtime-history.sh --summary --compact
./read-session-auth-runtime-history.sh --summary --require-latest-converged
```

If you want one unified operator entrypoint instead of remembering separate helpers, use:

```bash
./session-auth-runtime.sh status --service both --compact
./session-auth-runtime.sh validate --service both --compact
./session-auth-runtime.sh reload --service both --consumer-token "$CONSUMER_ENTRY_INGRESS_TOKEN" --matrix-token "$MATRIX_ENTRY_INGRESS_TOKEN"
./session-auth-runtime.sh activate --candidate-file next.json --consumer-token "$CONSUMER_ENTRY_INGRESS_TOKEN" --matrix-token "$MATRIX_ENTRY_INGRESS_TOKEN"
./session-auth-runtime.sh rollback --backup-file ./run/session-auth-runtime-backups/<file>
./session-auth-runtime.sh history --summary --compact
./session-auth-runtime.sh last --require-converged
./session-auth-runtime.sh --examples
./session-auth-runtime.sh --print-run-command
./session-auth-runtime.sh --help-json
./session-auth-runtime.sh --summary-json
./session-auth-runtime.sh --summary-compact
./session-auth-runtime.sh --summary-field overall.surfaceCount
./session-auth-runtime.sh --schema
# help-json / summary-json now also expose machine-readable summarySurfaceGuide, metaDiscoverability, catalogEntry, surfaceCapabilities, surfaceProfiles, consumerProfiles, profileSelectionGuide, profileSelectionTrace, lifecycle, maturity, stabilityPolicy, and related contract paths
# lifecycle / maturity now make the expected stability tier and additive evolution policy explicit for consumers
# compatibilityPolicy now makes additive compatibility rules and deprecation behavior machine-readable for long-lived consumers
# contractGovernance now aggregates lifecycle/maturity/compatibility/stability into one top-level governance block for readers and operators
# surfaceLifecycleMatrix / contractStatusMatrix now give a machine-readable glance view of per-surface lifecycle state and per-contract status
./session-auth-runtime.sh --status-json
./session-auth-runtime.sh --status-compact
./session-auth-runtime.sh --status-field overall.lastKnownStatus
./session-auth-runtime.sh --doctor
./session-auth-runtime.sh --doctor-json
./session-auth-runtime.sh --doctor-compact
./session-auth-runtime.sh --doctor-field overall.status
```

To read that latest activation summary in a smaller operator-friendly shape, use:

```bash
./read-session-auth-runtime-activation-status.sh
./read-session-auth-runtime-activation-status.sh --compact
./read-session-auth-runtime-activation-status.sh --field overall.status
./read-session-auth-runtime-activation-status.sh --require-converged
```

For an OpenClaw cron registration helper around that wrapper:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1
```

If you want the shortest Linux one-liner, use:

```bash
./install-operator-signal-cron.sh
./install-operator-signal-cron.sh --dry-run
./install-operator-signal-cron.sh --status
./install-operator-signal-cron.sh --status-json
./install-operator-signal-cron.sh --examples
./install-operator-signal-cron.sh --doctor
./install-operator-signal-cron.sh --doctor-json
./install-operator-signal-cron.sh --help-json
./install-operator-signal-cron.sh --schema
./install-operator-signal-cron.sh --print-run-command
./install-operator-signal-cron.sh --print-message
./scripts/read-operator-signal-cron-catalog.sh
./scripts/read-operator-signal-cron-catalog.sh --compact
./scripts/read-operator-signal-cron-catalog.sh --field defaults.policyProfile
./scripts/read-operator-signal-cron-catalog.sh --help-json
./scripts/read-operator-signal-cron-catalog.sh --schema
```

The older `./scripts/install-operator-signal-cron.sh` path still works, but the repo-root alias is now the preferred default. It is only a thin forwarder to the script helper, so the generated default profile/runCommand behavior stays identical. The alias now also adds a slightly nicer top-level UX with `--status` (maps to `--show`), `--status-json` for machine-readable current cron state, `--examples`, `--doctor` (quick environment/status snapshot), `--doctor-json` for machine-readable diagnostics, `--help-json` for a machine-readable quickstart/help surface, and `--schema` for a machine-readable schema catalog of those JSON outputs. `--doctor-json` now also carries a stable `kind`, `schemaVersion`, `defaultPolicyProfile`, `recommendedCommands.*`, `effectiveRunCommand`, `profileCommands.{default,identity,deploy}`, and a lightweight `recommendedConsumption` block so external scripts do not need to guess either the preferred next command, the profile-specific variants, or the recommended catalog entry path. When you pass explicit policy flags like `--policy-profile deploy`, that explicit choice now overrides the repo-root default instead of being appended to it. If you need a single machine-readable source for the shared policy/defaults/examples catalog behind the bash CLIs and the documented PowerShell helper, use `./scripts/read-operator-signal-cron-catalog.sh`; it emits `kind=operator-signal-cron-cli-catalog`, supports `--compact`, can read a subfield with `--field <dotted.path>`, and now also exposes both `--help-json` and `--schema` contracts. The catalog-reader `--schema` surface now also explicitly exposes `recommendedConsumption.contractPaths`, `recommendedConsumption.metaPaths`, `recommendedConsumption.summaryMetaPaths`, `recommendedConsumption.summaryDiscoverabilityPaths`, `recommendedConsumption.summaryContractPaths`, `recommendedConsumption.metaContractPath`, `recommendedConsumption.summaryContractPath`, `recommendedConsumption.summaryDiscoverabilityContractPath`, plus mirrored `recommendedConsumption.contract`, `recommendedConsumption.metaContract`, `recommendedConsumption.summaryContract`, and `recommendedConsumption.summaryDiscoverabilityContract`, so callers can discover both `contracts.recommendedConsumption.*`, the dedicated `contracts.recommendedConsumptionMeta`, and the narrower compact-summary / light-entry contracts without first reading the full catalog instance. The catalog-reader `--help-json` surface now also carries a lightweight `recommendedConsumption` block with repo-root schema command, reader schema/compact commands, recommended first fields, preferred strictness/profile, example consumer ids, parsing hints, and shorter shared display fields like `summaryDisplay`, `compactSummary`, `firstFieldsDisplay`, and `exampleConsumersDisplay`. That shared catalog now also includes top-level `provenance`, `coverage`, `evolution`, `capabilities`, `surfaces`, and `contracts` aggregates, so consumers can discover which blocks are runtime-backed vs docs/schema mirrors, how the manifest grew over time, high-level capability flags, the repo-root alias surfaces, catalog-reader surfaces, and their `kind/schemaVersion` pairs without digging through each CLI block separately. The `evolution` block now also carries `historyVersion`, `compatibilityPolicy`, `compatibilityStatus`, `stabilityLevel`, top-level `stabilityNotes` / `compatibilityNotes` / `breakingChanges` / `migrationHints` / `consumerWarnings`, `parsingPolicy`, `strictnessLevels`, `integrationProfiles`, `consumerExamples`, `consumerProfiles`, `recommendedConsumptionOrder`, `latestChangedFields`, and per-milestone `changedFields` / `compatibilityNotes`, so downstream readers can treat it as a minimal schema-evolution record plus a small consumption guide rather than a plain milestone list. The repo-root `./install-operator-signal-cron.sh --schema` output now also advertises this reader under `related.catalogJson`, including the reader's own `helpJson` / `schema` / `catalogJson` surface metadata, plus a small `consumptionGuide` block that points to `evolution.stabilityLevel`, `evolution.stabilityNotes`, `evolution.compatibilityStatus`, `evolution.breakingChanges`, `evolution.migrationHints`, `evolution.consumerWarnings`, `evolution.parsingPolicy`, `evolution.strictnessLevels`, `evolution.integrationProfiles`, `evolution.consumerExamples`, `evolution.consumerProfiles`, and `evolution.recommendedConsumptionOrder`, so machine-readable consumers can discover both the shared catalog and its consumption/contract entry points from the repo-root surface. That same repo-root schema now also exposes `contractPaths`, `metaPaths`, `metaContractPath`, plus mirrored `recommendedConsumptionContract` and `recommendedConsumptionMetaContract`, pointing at the shared catalog's top-level `contracts.recommendedConsumption.{sharedCore,repoRootInstall,catalogReader,powershellRegister}` blocks and `contracts.recommendedConsumptionMeta`. For lighter-weight entry points, both `./install-operator-signal-cron.sh --help-json` and `--doctor-json` now also expose a `recommendedConsumption` block with schema/catalog commands, recommended first fields, preferred strictness/profile, example consumer ids, parsing hints, shared compact-display fields (`summaryDisplay`, `compactSummary`, `firstFieldsDisplay`, `exampleConsumersDisplay`), the per-surface `metaPaths` map, `metaContractPath=contracts.recommendedConsumptionMeta`, and a focused `summaryDiscoverability` light-entry block pointing at `summaryContractPath`, `summaryMetaPath`, schema commands, and the preferred short-summary fields. Those lightweight repo-root JSON surfaces now also mirror `recommendedConsumptionMetaPath=cli.repoRootInstall.recommendedConsumptionMeta`, `recommendedConsumptionMetaContractPath=contracts.recommendedConsumptionMeta`, and the focused `recommendedConsumptionMetaContract` object itself, so callers can discover the self-description layer directly without first expanding `recommendedConsumption.metaPaths`. They now also expose `recommendedConsumptionMetaDiscoverabilityPath=cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverability`, `recommendedConsumptionMetaDiscoverabilityContractPath=contracts.recommendedConsumptionMetaDiscoverability`, and the lightweight `recommendedConsumptionMetaDiscoverability` object, so callers can jump straight into the smaller per-CLI meta-discovery block without first loading the full self-description object. Plain repo-root `./install-operator-signal-cron.sh --help` and `--doctor` now render the same shorter summary from that shared helper instead of each carrying their own hand-written mini summary, and now also print a dedicated `Per-CLI self-description layer:` section alongside the existing `Top-level summary layer:` hints. The catalog reader joins that lighter-weight path too: `./scripts/read-operator-signal-cron-catalog.sh --help-json` now exposes the same style of block, and plain `./scripts/read-operator-signal-cron-catalog.sh --help` renders the same shorter summary inline plus the same top-level summary-layer hints and a matching self-description-layer hint block. The documented PowerShell helper now also carries a mirrored `cli.powershellRegister.recommendedConsumption` block inside the shared catalog, including the preferred PowerShell install command plus an explicit docs-mirror/coverage reminder for hosts without `pwsh`, along with the same shared compact-display fields, `metaPaths`, `metaContractPath`, and `summaryDiscoverability` light-entry metadata. Its `cli.powershellRegister.recommendedConsumptionMeta` mirror now also carries the same `metaDiscoverabilityPath` / `metaDiscoverabilityContractPath` / `metaDiscoverability` trio as the bash-backed CLIs, so consumers can start from `cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverability` when they want the lightweight per-CLI discovery layer, then immediately gate that result with `coverage.powershellRegister` before assuming runtime backing. To make that docs/schema-only flow more explicit, the PowerShell mirror now also exposes `metaDiscoverabilityStartPath`, `notesPath`, `discoveryCommands.{meta,coverage,notes}`, and `recommendedDocsMirrorReadOrder`, so schema-first consumers can follow a small machine-readable sequence instead of reverse-engineering which field to read next. That same PowerShell-specific path set is now also mirrored into the narrower consumption-guide surfaces as `related.catalogJson.consumptionGuide.powershellMirrorGuidancePaths` / `powershellMirrorConsumerExampleId` and `outputs.catalogJson.recommendedConsumption.powershellMirrorGuidancePaths` / `powershellMirrorConsumerExampleId`, so consumers that start from repo-root schema or reader schema do not need to expand the full PowerShell instance just to find the docs-mirror read order. The shared catalog now also exposes an explicit `contracts.recommendedConsumption` subtree, a dedicated `contracts.recommendedConsumptionMeta` subtree for the per-CLI self-description layer, plus a narrower `contracts.recommendedConsumptionSummary` focused on the short-summary layer and `contracts.recommendedConsumptionSummaryDiscoverability` focused on the light-entry discovery block. Repo-root `--schema` and catalog-reader `--schema` now both surface those contract paths directly, and now also expose `metaPaths`, `metaContractPath`, `summaryMetaPaths`, `summaryDiscoverabilityPaths`, `summaryContractPaths`, `summaryContractPath`, and `summaryDiscoverabilityContractPath` for the recommended-consumption/self-description layer. On top of that, the shared catalog now carries a top-level `summaryDiscoverability.recommendedConsumption` aggregate, so callers who only want the summary-layer entry point can start from one top-level block instead of first walking into `cli.*.recommendedConsumption.*`. That top-level layer now also has its own focused contract `contracts.summaryDiscoverability`, and the catalog further mirrors a stable per-surface guide at `summarySurfaceGuide.recommendedConsumption` with `contracts.summarySurfaceGuide`, so consumers can discover not just the top-level entry object but also which compact, schema, and lightweight JSON surfaces mirror it. Repo-root `--help-json` / `--doctor-json` and catalog-reader `--help-json` now mirror both `catalogSummaryDiscoverability*` and `catalogSummarySurfaceGuide*`, while repo-root `--schema`, catalog-reader `--schema`, and the shared catalog `--compact` output now expose `summarySurfaceGuidePath`, `summarySurfaceGuideContractPath`, `summarySurfaceGuide`, and `summarySurfaceGuideContract` directly. The per-CLI detail blocks themselves now also carry `recommendedConsumptionMeta`, which mirrors each CLI's own `instancePath`, `contractPath`, `metaContractPath`, `summaryMetaPath`, `summaryDiscoverabilityPath`, `summarySurfaceGuidePath`, `summaryContractPath`, `summaryDiscoverabilityContractPath`, `summarySurfaceGuideContractPath`, and focused contract excerpt so consumers can stay inside `cli.repoRootInstall`, `cli.catalogReader`, or `cli.powershellRegister` without reassembling those links by hand. Each `recommendedConsumptionMeta` block now also carries a smaller `metaDiscoverability` object plus `metaDiscoverabilityPath` / `metaDiscoverabilityContractPath`, and the shared catalog exposes `contracts.recommendedConsumptionMetaDiscoverability`, `recommendedConsumption.metaDiscoverabilityPaths`, and `recommendedConsumption.metaDiscoverabilityContractPath` so schema-first and help-first consumers can discover that lighter per-CLI meta layer explicitly. The `recommendedConsumption` objects themselves also now carry both a `summaryMeta` sub-object and a lighter `summaryDiscoverability` sub-object, so consumers can choose between full short-summary metadata and the smaller light-entry discovery block. That light-entry block is now also self-describing via `summaryDiscoverabilityContractPath`, so a caller can start from the object itself and then jump straight to `contracts.recommendedConsumptionSummaryDiscoverability` without first reconstructing the contract path elsewhere. For callers that prefer a single catalog-level landing zone, the shared catalog also mirrors the same discovery guidance at `summaryDiscoverability.recommendedConsumption`, and the schema surfaces now advertise that with `summaryDiscoverabilityPath=summaryDiscoverability.recommendedConsumption`. The new `summarySurfaceGuide.recommendedConsumption` layer then shows where that top-level summary entry is mirrored across `--compact`, repo-root `--schema`, repo-root `--help-json` / `--doctor-json`, and catalog-reader `--help-json` / `--schema`, so surface-first consumers can stay additive and machine-readable without reverse-engineering paths by hand. The top-level `summaryDiscoverability.recommendedConsumption` entry itself now also carries a `preferredReadOrder`, so a consumer can start from the entry block and still learn the intended progression from entry object, to focused contract, to surface guide, to fuller summary metadata without needing out-of-band docs. To make the schema-first path more symmetric, repo-root `related.catalogJson.consumptionGuide` and catalog-reader `outputs.catalogJson.recommendedConsumption` now also mirror `topLevelSummaryPaths={discoverability,surfaceGuide}`, `topLevelSummaryContractPaths={discoverability,surfaceGuide}`, and `summarySurfaceGuideContractPath`, so consumers can discover both top-level summary objects and both top-level contracts directly from the narrower consumption-guide blocks. The per-CLI `recommendedConsumption` instances themselves now mirror the same three hints too, and `contracts.recommendedConsumption.{sharedCore,repoRootInstall,catalogReader,powershellRegister}` explicitly declare them, so these top-level summary path maps are no longer only documented in surrounding schema wrappers, but also part of the narrower instance contract layer.

If you want the installed cron job to carry repo-local sample policies in its body, use either helper:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -PolicyProfile default
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -PolicyBundle baseline
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -UseEntryIdentityPolicyExample
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -UseMonitoringDeployPolicyExample
powershell -ExecutionPolicy Bypass -File .\scripts\register-openclaw-operator-signal-cron.ps1 -Action install -UseEntryIdentityPolicyExample -UseMonitoringDeployPolicyExample
```

```bash
./scripts/register-openclaw-operator-signal-cron.sh
./scripts/register-openclaw-operator-signal-cron.sh --dry-run
./scripts/register-openclaw-operator-signal-cron.sh --action show
./scripts/register-openclaw-operator-signal-cron.sh --policy-profile default
```

The bash helper defaults to `--policy-profile default` on install when you do not pass any explicit policy flags, `--dry-run` prints the exact generated job payload without mutating cron state, and `--print-run-command` / `--print-message` expose just the final repo-local command or final OpenClaw message body for lighter inspection.

The preferred shell-level form is now profile-based, not raw JSON substitution:

```bash
OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE=default ./scripts/run-operator-signal-check.sh --compact
```

For a repo-local notification template that reads the wrapper JSON from stdin, writes a local notification log, and can optionally POST to a webhook. It now prefers the wrapper-provided `notify.policy_summary` / `OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY` when present, so policy-routing reasons show up directly in the notification text. The wrapper also now emits `notify.policy_summary_levels.{ultra_short,short,full}` plus matching env vars, and the notifier can select them via `OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_LEVEL=ultra_short|short|full`. The body format now also changes with that level, not just the policy-summary line. Its `alerts_brief` summary is now critical-first and grouped by severity plus configurable family/service buckets like `refund`, `audit`, `upstream`, `worker`, and `entry-abuse`; override those buckets with `OPERATOR_SIGNAL_NOTIFY_ALERT_FAMILY_RULES_JSON='[{"name":"family","matchAny":["regex1","regex2"]}]'`. It now also emits a compact single-line `family_brief` such as `critical{refund=1} | warn{audit=1,entry-abuse=2,upstream=1}`, which sits between terse titles and full family breakdowns. In full local-log mode, it now also prints both a compact multiline `family_grouped_alerts:` block and a single-line `summary_views_json:` block, so on-host troubleshooting and grep-friendly automation do not require inspecting webhook payloads. In `OPERATOR_SIGNAL_NOTIFY_WEBHOOK_MODE=json`, the notifier now also sends pre-rendered fields like `selected_level`, `rendered_text`, `alerts_brief`, `family_brief`, structured `family_grouped_alerts`, and a stable `render.summary_views` / `summary.summary_views` container that groups these views together for downstream consumers. That container now includes `version: 1` so downstream parsers can evolve safely.

Quick `summary_views` shape:

```json
{
  "version": 1,
  "policy_summary": "critical incident new state: ...",
  "family_brief": "critical{refund=1} | warn{upstream=1}",
  "alerts_brief": "critical refund(execution:refund_failures) | warn upstream(gateway:invocation_create_upstream_failures)",
  "family_grouped_alerts": [ ... ]
}
```

For the fuller schema, field-usage notes, summary_views evolution/compatibility guidance, a short consumer checklist, a recommended consumption order, a copy/paste consumer snippet, a sample payload, and a changelog-style evolution timeline, see `docs/openclaw-operator-signal-cron-v1.md`.

If you want focused Prometheus / Alertmanager examples, start with:

- consumer-entry identity governance:
  - `ops/monitoring/prometheus/consumer-entry-identity-governance-alerts.example.yml`
  - `ops/monitoring/alertmanager/consumer-entry-identity-governance-routing.example.yml`
- cross-service core runtime signals from the wrapper bridge:
  - `ops/monitoring/prometheus/core-runtime-operator-signals-from-wrapper.example.yml`
  - `ops/monitoring/alertmanager/core-runtime-operator-signals-from-wrapper-routing.example.yml`
- product-edge signals from the wrapper bridge:
  - `ops/monitoring/prometheus/product-edge-operator-signals-from-wrapper.example.yml`
  - `ops/monitoring/alertmanager/product-edge-operator-signals-from-wrapper-routing.example.yml`
- monitoring deploy verdict signals from the wrapper bridge:
  - `ops/monitoring/prometheus/monitoring-deploy-operator-signals-from-wrapper.example.yml`
  - `ops/monitoring/alertmanager/monitoring-deploy-operator-signals-from-wrapper-routing.example.yml`
- combined starter bundle + manifest:
  - `ops/monitoring/prometheus/minimal-wrapper-monitoring-bundle.example.yml`
  - `ops/monitoring/alertmanager/minimal-wrapper-monitoring-bundle.example.yml`
  - `ops/monitoring/monitoring-bundle-manifest.example.yml`
  - regenerate/check them with:
    - `./scripts/assemble-monitoring-bundles.sh`
    - `./scripts/assemble-monitoring-bundles.sh --check`
  - export an install-friendly directory with:
    - `./scripts/export-monitoring-bundles.sh`
    - `./scripts/export-monitoring-bundles.sh --output-dir /tmp/cex-monitoring-export --include-focused`
  - install it into a repo-local target layout with:
    - `./scripts/install-monitoring-bundles.sh`
    - `./scripts/install-monitoring-bundles.sh --install-dir /tmp/cex-monitoring-install --include-focused`
  - build a symlink-based overlay with:
    - `./scripts/overlay-monitoring-bundles.sh`
    - `./scripts/overlay-monitoring-bundles.sh --target-root /tmp/cex-monitoring-overlay --include-focused`
  - deploy into a live-target layout with:
    - `./scripts/deploy-monitoring-bundles.sh`
    - `./scripts/deploy-monitoring-bundles.sh --deploy-root /tmp/cex-monitoring-live-target --include-focused`
    - `./scripts/deploy-monitoring-bundles.sh --from-overlay-root /tmp/cex-monitoring-overlay --mode copy`
    - `./scripts/deploy-monitoring-bundles.sh --reload --reload-dry-run`
  - trigger only the post-deploy reload hook with:
    - `./scripts/reload-monitoring-targets.sh`
    - `./scripts/reload-monitoring-targets.sh --mode command --prometheus-command 'systemctl reload prometheus' --alertmanager-command 'systemctl reload alertmanager'`
    - `./scripts/reload-monitoring-targets.sh --mode command --failure-policy restart --prometheus-command 'systemctl reload prometheus' --prometheus-restart-command 'systemctl restart prometheus'`
  - verify health after deploy/reload with:
    - `./scripts/verify-monitoring-targets.sh`
    - `./scripts/verify-monitoring-targets.sh --attempts 5 --delay-secs 2`
    - `./scripts/deploy-monitoring-bundles.sh --verify --verify-attempts 5 --verify-delay-secs 2`
  - write structured helper results with `--summary-file <path>` when running `reload-monitoring-targets.sh` or `verify-monitoring-targets.sh`; when you trigger them via `deploy-monitoring-bundles.sh`, the deploy metadata file now records them under `postDeployActions.reload` / `postDeployActions.verify`, plus a short `postDeployActions.overall` verdict for operator glance (`deploy_only`, `success`, `reload_failed`, `verify_failed`, `incomplete`), a minimal severity layer (`ok|warn|error`) via `overall.severity` / `overall.requiresAttention` / `overall.operatorDisplay`, and a next-step hint via `overall.nextActionHint`
  - read the latest deploy verdict back out with:
    - `./scripts/read-monitoring-deploy-status.sh`
    - `./scripts/read-monitoring-deploy-status.sh --compact`
    - `./scripts/read-monitoring-deploy-status.sh --json`
    - `./scripts/read-monitoring-deploy-status.sh --field overall.operatorDisplay`
    - `./scripts/read-monitoring-deploy-status.sh --compact --fail-on-severity warn`
  - feed that deploy verdict into the operator signal chain via:
    - `MONITORING_DEPLOY_METADATA_FILE=./run/monitoring-live-target/metadata/monitoring-deploy-metadata.yml ./scripts/check-operator-signals.sh --compact`
    - optional: `MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY=1` if you want unfinished deploy-only runs to raise a soft warning instead of staying supporting-only

```bash
./scripts/notify-operator-signals-example.sh
```

Exit codes:

- `0` = ok
- `1` = warn
- `2` = critical

## Deprecated regression scripts

The following PowerShell scripts are now **covered by Rust tests** and are retained only as compatibility/manual probes:

- `smoke-test-local-runtime.ps1`
- `api-regression-check.ps1`
- `approval-regression-check.ps1`
- `reject-refund-regression-check.ps1`
- `settlement-regression-check.ps1`
- `lifecycle-regression-check.ps1`
- `state-machine-regression-check.ps1`
- `workflow-persistence-regression-check.ps1`
- `audit-persistence-regression-check.ps1`

Their original names under `scripts\` are now compatibility shims that forward to the real legacy bodies in `scripts\legacy\`. This preserves old entrypoints without keeping the real logic at the top level.

Coverage mapping and retirement rationale live in:

- `docs\REGRESSION-COVERAGE-MATRIX.md`
- `docs\CI-GATE.md`

Admin-token precedence and split-admin rules live in:

- `docs\admin-token-model.md`

That includes:
- identity key-management scopes (`api_keys:manage`, `api_keys:read`)
- audit trace-read scope (`audit:read`)
- execution lifecycle scopes (`executions:manage`, `executions:read`)
- ledger/account scopes (`ledger:manage`, `ledger:read`)

## When these old scripts are still useful

They are still fine for:

- manual shell-first debugging
- step-by-step operator walkthroughs
- quick one-off probes without compiling or waiting for the full Rust gate

But they are **not** the default regression authority anymore.

When they do inspect gateway invocation reads, the retained compatibility/manual scripts now also emit a standardized nested execution snapshot object (for example `execution_snapshot`, `initial_execution_snapshot`, `final_execution_snapshot`, or `<flow>_execution_snapshot_*`) via the shared `Get-CexInvocationExecutionSnapshotObject` helper in `_dev-helpers.ps1`. Their top-level JSON result envelopes are also now normalized through the shared `New-CexLegacyResultObject` helper, so the common `ok=true` + ordered payload shape no longer has to be reassembled in each script. On top of that, the repeated nested account and invocation/execution state fragments are now starting to converge on shared helpers like `Get-CexAccountStateObject`, `Get-CexInvocationExecutionStateObject`, `Get-CexFlowCheckpointObject`, and `Get-CexFlowCheckpointState`; repeated health / audit-event outputs can now use `Get-CexHealthStateObject` and `Get-CexAuditEventTypeList`; approval DB row fragments can now converge on `Get-CexApprovalDbStateObject` / `Get-CexApprovalDbStateFromPsqlLine`; and repeated operational steps like creating ledger accounts, submitting gateway invocations, or restarting the detached local runtime can now use `New-CexLedgerAccount`, `New-CexGatewayInvocation`, and `Restart-CexDetachedRuntime` instead of each script hand-rolling the same POST bodies, process invocations, and error-handling shims.

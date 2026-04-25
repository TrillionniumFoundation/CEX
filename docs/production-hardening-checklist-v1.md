# Production Hardening Checklist v1

## Current state

CEX is now back to a green workspace test baseline on this branch.

That removes the immediate compile blocker, but it does **not** mean production readiness. The remaining gaps are mainly in ingress hardening, production operations, and stronger policy / abuse controls.

## P0 before any closed beta

### 1. Protect product-facing ingress

Required:

- require shared ingress token or equivalent authenticated edge in front of:
  - `consumer-entry-api`
  - `matrix-entry-adapter`
- reject empty and oversized text payloads
- stop relying on open localhost-first assumptions as the only control

Current repo status:

- `consumer-entry-api` now supports optional `CONSUMER_ENTRY_INGRESS_TOKEN`
- `matrix-entry-adapter` now supports optional `MATRIX_ENTRY_INGRESS_TOKEN`
- both entry layers now reject empty / oversized text payloads
- `matrix-bot-relay` now forwards adapter ingress token when configured
- `matrix-entry-adapter` now supports optional `MATRIX_ENTRY_RECENT_EVENT_STORE_PATH`, so duplicate-event cache can survive process restart
- `consumer-entry-api` now supports optional `CONSUMER_ENTRY_REPLAY_STORE_PATH` (with legacy `CONSUMER_ENTRY_MATRIX_EVENT_STORE_PATH` alias), so request replay state can survive process restart and return the prior accepted task response for duplicate Matrix `event_id` or chat `idempotency_key`
- `consumer-entry-api` now also supports optional `CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH`, so quota buckets can survive process restart instead of resetting to a clean slate on every redeploy/restart
- `consumer-entry-api` now also supports optional local identity binding resolution via `CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH` plus `CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING`, allowing caller-supplied `org_id` / `account_id` to be filled or rejected against a trusted local binding map before forwarding; binding files now support a versioned `version/revision` document shape, can embed a repo-local `product_users` registry for `external identity -> product_user_id -> org/account` closure, and can now also load that registry from a separate `CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH` file so binding membership and canonical product-user data are no longer forced into the same document; source metadata is exposed via `/health` and forwarded `source.identity_resolution`, reloads still run in-process through `POST /v1/admin/identity-bindings/reload`, startup/reload can append JSONL audit events through `CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH`, and the same minimal reload governance layer (`require_revision`, `reject_same_revision`, `allow_legacy_format`, `require_approved_revision`, `allow_rollback`) plus local approved-revision source still protects the live binding set; reload calls can now also enforce an actor allow-list via `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR` / `CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS`. The focused admin surface now also includes registry / approval / actor / governance endpoints, `/health.identity_governance_overview`, and governance-related `/metrics` gauges, so operator glance no longer needs to infer identity governance from raw file metadata alone.
- `identity binding reload` endpoint now returns an explicit `identity_binding_audit` block (`path/last_status/last_policy_*`), and `last_status` is covered in endpoint-level tests (`written` / `write_error` / `serialize_error` / `disabled`), improving reload observability and audit-write troubleshooting.

Still missing:

- fully shared/service-backed real user/session auth and session issuance. The first product-edge slice is now in place: `consumer-entry-api` can require signed user/session assertions, and `matrix-entry-adapter` can mint/forward those downstream assertions when calling `consumer-entry-api`. This has now been tightened further with explicit issuer allow-list + expected audience checks on the consumer side, a default `audience=consumer-entry-api` on the matrix side, a request-bound `request_fingerprint` so a signed assertion is harder to replay against a different payload inside its TTL, an issuer-specific secret path (`CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON` on the consumer side, `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET` on the matrix side), a rotation-friendly `(issuer,key_id)` path (`CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON` plus `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_KEY_ID`), and now also a repo-local shared issuer registry path (`CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH` plus `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH`) so the boundary is no longer forced to be one global secret, one secret per issuer, or per-service duplicated key selection. The repo-local registry now also carries revision-aware metadata into `/health` and `/metrics`, has focused consumer admin status/validate surfaces, supports an optional approved-revision source (`CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH` / `MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH`) plus enforcement flags on both sides, and exposes matrix-side selection plus approval-governance status so operators can see whether a signer is using an explicit secret or a registry-selected key and whether the live registry revision is actually approved. That closes the old "shared ingress token + caller-supplied user/session/org" gap for create endpoints more cleanly, but it is still a repo-local verification baseline, not yet a dedicated auth/session authority.
- replay protection at the product edge is still incomplete (broader product-session paths still lack durable replay state and the current user/org/session identity closure, while now splittable into binding + registry files, is still repo-local file state rather than a shared service-backed authority)
- persisted dedupe is partially closed today for Matrix `event_id` and chat `idempotency_key`, but not yet uniform across all entry surfaces or all client behaviors
- per-user / per-room / per-session / per-org rate-limit knobs now exist in `consumer-entry-api`, and their recent bucket state can now be snapshotted locally through `CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH`, but they are still single-node/local-file state, not yet a durable distributed policy/quota system
- tenant-aware rate limits now have a better local identity base because `consumer-entry-api` can resolve through a repo-local `product_users` registry plus governed binding revisions, but this is still not a formally shared org identity layer with centralized audit trail or distributed policy enforcement

### 2. Make production observability non-optional

Required:

- service-level metrics endpoint or exporter strategy
- alertable counters for:
  - invocation create failures
  - ledger reserve/refund failures
  - execution retry-budget exhaustion
  - approval backlog
  - queue depth / lease expiry churn
  - audit write failures
- trace correlation documented end-to-end
- operator runbook for restart / degraded upstream / partial outage

Still missing in repo shape:

- consistent metrics surface across core services (consumer-entry / matrix-entry 现在已有 `/metrics` Prometheus 文本面，外加 `/health` JSON；其中 consumer-entry 还已开始暴露 identity governance gauge，例如 binding loaded / registry loaded / ref-integrity / actor gate / approval source / approval coverage；execution `/v1/info` 现在已补到更接近值班面的 runtime/operator snapshot，包含 queued-worker backlog/lease/retry 信号，以及 provider failure 分类汇总与阈值信号（billing / timeout / auth / rate_limited / unavailable / unknown / dead_letter / retry_budget_exhausted）；execution-service 也已提供原生 `/metrics` Prometheus 文本面，导出 runtime counters、status gauges、queued-worker gauges、provider failure gauges 与 operator signal gauges；gateway-service 也已提供原生 `/metrics`，导出 gateway runtime counters 与 operator signal gauges；identity/ledger/audit/capability 也已补最小 native `/metrics` up/config/count gauges，当前 core services 已都有 Prometheus 文本面)
- alerting rules
- SLO / error-budget policy
- rollback and incident playbooks

Current operator doc baseline:

- `docs/operator-runbook-v1.md` 已收口当前最小排障路径，覆盖 `/health` + `/v1/info` + worker queue summary 的一线判断与 restart/degraded upstream/partial outage 处理顺序，但仍不是完整 on-call 手册
- `docs/alert-rules-draft-v1.md` 已把当前最小 signal 面收成告警草案，明确哪些字段可直接触发 high/critical；仓库现在还额外补了 focused example files：consumer-entry identity governance（`ops/monitoring/prometheus/consumer-entry-identity-governance-alerts.example.yml`、`ops/monitoring/alertmanager/consumer-entry-identity-governance-routing.example.yml`）、跨服务 core runtime wrapper line（`ops/monitoring/prometheus/core-runtime-operator-signals-from-wrapper.example.yml`、`ops/monitoring/alertmanager/core-runtime-operator-signals-from-wrapper-routing.example.yml`）、product-edge wrapper line（`ops/monitoring/prometheus/product-edge-operator-signals-from-wrapper.example.yml`、`ops/monitoring/alertmanager/product-edge-operator-signals-from-wrapper-routing.example.yml`）以及 monitoring-deploy wrapper line（`ops/monitoring/prometheus/monitoring-deploy-operator-signals-from-wrapper.example.yml`、`ops/monitoring/alertmanager/monitoring-deploy-operator-signals-from-wrapper-routing.example.yml`）。同时还新增了 combined starter bundle（`ops/monitoring/prometheus/minimal-wrapper-monitoring-bundle.example.yml`、`ops/monitoring/alertmanager/minimal-wrapper-monitoring-bundle.example.yml`）、machine-readable inventory（`ops/monitoring/monitoring-bundle-manifest.example.yml`）、repo-local assemble helper（`scripts/assemble-monitoring-bundles.sh` / `--check`）、export helper（`scripts/export-monitoring-bundles.sh`）、install helper（`scripts/install-monitoring-bundles.sh`）、symlink/overlay helper（`scripts/overlay-monitoring-bundles.sh`）、live-target deploy helper（`scripts/deploy-monitoring-bundles.sh`）、post-deploy reload helper（`scripts/reload-monitoring-targets.sh`，现支持 `failure-policy=restart` + service-specific restart commands）和 post-deploy health verification helper（`scripts/verify-monitoring-targets.sh`，支持 attempts/delay retry），以及 repo-local bridge `scripts/render-operator-signals-prometheus.sh`，可把 unified wrapper JSON 渲染成 Prometheus text exposition。不过它们仍只是起步模板，不是覆盖全仓的正式 exporter + Prometheus/Alertmanager 规则库
- `docs/openclaw-operator-signal-cron-v1.md` 已给出把 repo-local signal wrapper 接到 OpenClaw cron 的模板与 helper，但还没做成正式内置监控产品面

### 3. Strengthen policy and abuse guardrails

Required:

- configurable sensitive-action policy, not only fixed keyword match
- explicit budget guardrails per org / account / capability
- edge throttling for spam / flooding / retry storms
- safer defaults for externally reachable entry services

Current repo status:

- execution retry budget exists; queued-worker provider failures now also distinguish retryable vs non-retryable classes, so timeout / rate-limit / provider-unavailable failures can auto-requeue with bounded exponential backoff while billing/auth-style failures still terminate and refund rather than loop
- approval threshold exists
- execution policy now supports configurable approval/block keywords, capability-prefix rules, and optional hard reserve reject threshold
- policy still remains service-local and env-driven, not yet a full standalone policy/risk layer

Still missing:

- org-aware policy bundles
- DB-backed or centrally managed policy versioning
- user / room / org quotas
- abuse event audit taxonomy

## P1 for a serious closed beta

- persistent task / session layer for consumer entry
- durable identity mapping beyond repo-local files: `matrix_user_id -> product user -> org/account`
- approval / retry / cancel controls exposed safely in product surface
- richer operator dashboards for worker queue, approvals, refunds, failures
- configuration profiles for local-dev vs beta vs production (entry 层现在已有最小 `runtime_profile` 启动护栏，但还没有全仓统一 profile/overlay 体系)

## P2 for public launch

- real commercial billing / subscription / entitlements
- stronger tenant isolation and secret management review
- disaster recovery drill and migration rollback rehearsal
- external abuse review and threat model
- on-call ownership and incident lifecycle

## Recommended next implementation order

1. finish profile-aware edge auth baselines across every externally reachable entry surface
2. add per-surface durable rate limit / dedupe strategy
3. add cross-service metrics + queue / refund / approval alerts
4. replace MVP policy with configurable policy bundles
5. lift the current repo-local product-user mapping into a shared persistent identity/session model

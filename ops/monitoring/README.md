# CEX Monitoring Examples

This folder now contains three layers of repo-local monitoring examples:

1. **Focused direct metrics rules**
   - `prometheus/consumer-entry-identity-governance-alerts.example.yml`
   - `prometheus/consumer-entry-trillionnium-route-runner-handoff-alerts.example.yml`
   - `alertmanager/consumer-entry-identity-governance-routing.example.yml`
2. **Focused wrapper-derived rules**
   - `prometheus/core-runtime-operator-signals-from-wrapper.example.yml`
   - `alertmanager/core-runtime-operator-signals-from-wrapper-routing.example.yml`
   - `prometheus/product-edge-operator-signals-from-wrapper.example.yml`
   - `alertmanager/product-edge-operator-signals-from-wrapper-routing.example.yml`
   - `prometheus/monitoring-deploy-operator-signals-from-wrapper.example.yml`
   - `alertmanager/monitoring-deploy-operator-signals-from-wrapper-routing.example.yml`
3. **Combined bundle + manifest**
   - `prometheus/minimal-wrapper-monitoring-bundle.example.yml`
   - `alertmanager/minimal-wrapper-monitoring-bundle.example.yml`
   - `monitoring-bundle-manifest.example.yml`
4. **Dashboard examples**
   - `grafana/trillionnium-route-runner-handoff-dashboard.example.json`

## How to read this layout

- **consumer-entry identity governance** uses direct service metrics from `consumer-entry-api /metrics`
- **Trillionnium route-runner handoff** uses direct service metrics from `consumer-entry-api /metrics`
- **core runtime** and **product-edge** currently rely on the wrapper bridge:
  - `scripts/check-operator-signals.sh`
  - `scripts/render-operator-signals-prometheus.sh`

## Recommended starting point

If you just want one Prometheus file and one Alertmanager file to start from, use:

- `prometheus/minimal-wrapper-monitoring-bundle.example.yml`
- `alertmanager/minimal-wrapper-monitoring-bundle.example.yml`

If you want the machine-readable inventory of which focused files feed that bundle, use:

- `monitoring-bundle-manifest.example.yml`

If you want a starter Grafana view for the route-runner reward/next-route handoff posture, import:

- `grafana/trillionnium-route-runner-handoff-dashboard.example.json`

It tracks the same direct gauges used by the `CexTrillionniumRouteRunnerHandoff*` Prometheus alerts:

- all-gates-green
- playability / closed-beta / real-user beta / public-commercial gates
- feed source count
- runner count
- reward-claim and next-route action counts

Those alerts also carry `component=trillionnium-route-runner-handoff` and `owner=product-ops`, and the example Alertmanager product-edge routing file matches that component before the generic product-edge route.

If you want to validate just this focused handoff monitoring contract without running full production readiness/signoff, use:

- `scripts/check-trillionnium-route-runner-handoff-monitoring.sh`
- `scripts/check-trillionnium-route-runner-handoff-monitoring.sh --summary-file run/monitoring-route-runner-handoff-summary.json`

The check validates live-target metadata freshness, Prometheus alert metric/label coverage, Alertmanager route order, and the dashboard metric set. Production readiness and production signoff also call this script so the standalone contract cannot drift from launch gates.

If you want to regenerate the combined bundles from the manifest, use:

- `scripts/assemble-monitoring-bundles.sh`
- `scripts/assemble-monitoring-bundles.sh --check`

If you want to export an install-friendly bundle directory, use:

- `scripts/export-monitoring-bundles.sh`
- `scripts/export-monitoring-bundles.sh --output-dir /tmp/cex-monitoring-export --include-focused`

If you want to install the exported results into a repo-local install layout, use:

- `scripts/install-monitoring-bundles.sh`
- `scripts/install-monitoring-bundles.sh --install-dir /tmp/cex-monitoring-install --include-focused`

If you want a symlink-based overlay rooted at another directory, use:

- `scripts/overlay-monitoring-bundles.sh`
- `scripts/overlay-monitoring-bundles.sh --target-root /tmp/cex-monitoring-overlay --include-focused`

If you want a live-target deploy layout that maps into Prometheus rules.d / Alertmanager conf.d style directories, use:

- `scripts/deploy-monitoring-bundles.sh`
- `scripts/deploy-monitoring-bundles.sh --deploy-root /tmp/cex-monitoring-live-target --include-focused`
- `scripts/deploy-monitoring-bundles.sh --from-overlay-root /tmp/cex-monitoring-overlay --mode copy`
- `scripts/deploy-monitoring-bundles.sh --reload --reload-dry-run`

If you only want the post-deploy reload hook, use:

- `scripts/reload-monitoring-targets.sh`
- `scripts/reload-monitoring-targets.sh --mode command --prometheus-command 'systemctl reload prometheus' --alertmanager-command 'systemctl reload alertmanager'`
- `scripts/reload-monitoring-targets.sh --mode command --failure-policy restart --prometheus-command 'systemctl reload prometheus' --prometheus-restart-command 'systemctl restart prometheus'`

If you want a post-deploy health verification step, use:

- `scripts/verify-monitoring-targets.sh`
- `scripts/verify-monitoring-targets.sh --attempts 5 --delay-secs 2`
- `scripts/deploy-monitoring-bundles.sh --verify --verify-attempts 5 --verify-delay-secs 2`

Both `reload-monitoring-targets.sh` and `verify-monitoring-targets.sh` now also support `--summary-file <path>`, and `deploy-monitoring-bundles.sh` writes those structured results back into `metadata/monitoring-deploy-metadata.yml` under `postDeployActions.reload` / `postDeployActions.verify`. It also adds `postDeployActions.overall`, a thin operator-facing verdict with `status`, `severity`, `requiresAttention`, `nextActionHint`, `successful`, `requestedActions`, `failedActions`, `summaryDisplay`, and `operatorDisplay` (for example `warn/deploy_only, deploy only`, `ok/success, reload ok`, or `error/verify_failed, reload ok, verify failed`). If you want to read that verdict without manually opening YAML, use `scripts/read-monitoring-deploy-status.sh` in text, `--compact`, `--json`, or `--field overall.operatorDisplay` mode; for automation, it also supports `--fail-on-severity warn|error` so callers can turn the verdict into an exit code. The repo-local operator signal check now also knows how to read that metadata through `MONITORING_DEPLOY_METADATA_FILE`; when present, `check-operator-signals.sh` will always expose it under `supporting.monitoring_deploy`, and will raise a soft `monitoring_deploy:post_action_status` warning when the latest deploy verdict is unhealthy (by default it does not alert on plain `deploy_only` unless `MONITORING_DEPLOY_ALERT_ON_DEPLOY_ONLY=1`).

There is now also a focused wrapper-derived monitoring-deploy slice (`ops/monitoring/prometheus/monitoring-deploy-operator-signals-from-wrapper.example.yml` + `ops/monitoring/alertmanager/monitoring-deploy-operator-signals-from-wrapper-routing.example.yml`) for cases where the latest deploy verdict itself should surface into Prometheus/Alertmanager. For OpenClaw cron installation, the repo now ships both a PowerShell helper (`scripts/register-openclaw-operator-signal-cron.ps1`) and a Linux/bash helper (`scripts/register-openclaw-operator-signal-cron.sh`); the bash helper defaults to the recommended `default` policy profile on install, so `./scripts/register-openclaw-operator-signal-cron.sh --dry-run` is now the quickest local way to inspect the generated job payload.

## Current limitation

These are still **repo-local example bundles**, not a complete production monitoring package.
They do not yet provide:

- full native `/metrics` coverage for gateway/execution
- full matrix/product policy coverage
- silence policy / ownership / escalation tree beyond the focused route-runner component labels/routing example
- a single blessed production deployment layout
- a complete dashboard pack beyond the focused route-runner handoff starter

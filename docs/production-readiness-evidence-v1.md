# Production Readiness Evidence v1

Last updated: 2026-04-25

This page records evidence for production-readiness claims. It is intentionally conservative: a green local gate is evidence, not a launch approval.

## Current verified evidence

| Area | Evidence | Status |
| --- | --- | --- |
| Workspace tests | `cargo test -p execution-service` after provider failure ack + policy bundle changes | PASS |
| Linux full runtime gate | `scripts/gate-local-linux.sh --skip-workspace --skip-db-bootstrap` with core + entry services and metrics smoke | PASS |
| Docker-backed Linux full gate | `scripts/gate-local-linux.sh` with Docker Postgres migrations/seeding via passwordless `sudo docker`, core + entry services, ignored runtime suites, and metrics smoke | PASS |
| PowerShell service-local gate on Linux | `pwsh -NoLogo -NoProfile -File ./gate-local.ps1 -ServiceLocalOnly` after installing PowerShell 7.6.1; cargo service-local slices passed and runtime was restored with local-production profile | PASS |
| PowerShell full gate on Linux | `pwsh -NoLogo -NoProfile -File ./gate-local.ps1` after cross-platform Docker/PowerShell/runtime fixes; service-local slices, detached runtime restart, audit/identity/gateway ignored runtime suites, and approval probe all passed | PASS |
| Local readiness smoke | `CEX_READINESS_MODE=local CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/check-production-readiness.sh` | PASS |
| Local production-posture smoke | `CEX_ENV_FILE=run/local-production/.env CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/check-production-readiness.sh` | PASS |
| Live provider probe | `scripts/probe-openclaw-provider.sh --model google/gemini-2.5-flash` | PASS |
| Gateway → queued worker → OpenClaw provider success | `cap.openclaw.model.google.gemini-2-5-flash` invocation completed with `output_text=CEX_PROVIDER_PROBE_OK` | PASS |
| Short local soak | `CEX_SOAK_DURATION_SECONDS=60 CEX_SOAK_INTERVAL_SECONDS=15 CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/soak-runtime.sh` | PASS |
| 10-minute local soak | `CEX_READINESS_MODE=local CEX_SOAK_DURATION_SECONDS=600 CEX_SOAK_INTERVAL_SECONDS=60 CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/soak-runtime.sh` | PASS |
| Local production-posture soak | `CEX_ENV_FILE=run/local-production/.env CEX_SOAK_DURATION_SECONDS=300 CEX_SOAK_INTERVAL_SECONDS=60 CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/soak-runtime.sh` (summary `run/soak/soak-20260425T055657Z-248676.summary.json`) | PASS |
| 1-hour local production-posture soak | `CEX_ENV_FILE=run/local-production/.env CEX_SOAK_DURATION_SECONDS=3600 CEX_SOAK_INTERVAL_SECONDS=60 CEX_SOAK_SKIP_PROVIDER_PROBE=1 scripts/soak-runtime.sh` (summary `run/soak/soak-20260425T064637Z-302074.summary.json`) | PASS |
| 2-hour local production-posture soak | `CEX_ENV_FILE=run/local-production/.env CEX_SOAK_DURATION_SECONDS=7200 CEX_SOAK_INTERVAL_SECONDS=120 CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/soak-runtime.sh` (summary `run/soak/soak-20260425T081221Z-356435.summary.json`) | PASS |
| DB backup/restore drill | `scripts/drill-db-backup-restore.sh` using Docker Postgres backup, temporary restore DB, and core table count comparison (summary `run/drills/db-backup-restore-20260425T063804Z-289267.summary.json`) | PASS |
| Latest production readiness signoff smoke | `CEX_ENV_FILE=run/local-production/.env CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/check-production-readiness.sh` after 2-hour soak, fresh DB restore drill, and verified monitoring deploy metadata | PASS |
| Monitoring deploy verification | `scripts/deploy-monitoring-bundles.sh --mode copy --include-focused --force --verify --verify-mode command ...` deployed Prometheus/Alertmanager bundles to `run/monitoring-live-target` and verified both targets via command mode | PASS |
| Provider launch policy posture | Production policy blocks known non-launch provider prefixes `cap.openclaw.model.minimax.` and `cap.openclaw.model.openai-codex.` while Google Gemini is the validated launch provider | PASS |
| Local production secret-file posture | `run/local-production/.env` and generated policy bundle chmod `600`; production readiness fails if `CEX_ENV_FILE` has group/other permissions | PASS |

## Active blockers to a truthful 100% claim

- PowerShell 7.6.1 is now installed and the full `gate-local.ps1` passes under Linux PowerShell Core, including runtime blackbox/probe suites; a true Windows-host-native run is still not available on this machine.
- Direct non-sudo Docker socket access for user `qian` is still denied, but passwordless `sudo docker` is available and the Docker-backed Linux gate now passes through that path.
- The current strict production profile pass and soak use generated owner-only local secrets under `run/local-production/.env`; they validate posture mechanics but are not a real external secret-management environment.
- Formal production deployment to a non-local host and migration rollback strategy beyond restore-from-backup still need real environment evidence. A local Docker Postgres backup/restore drill, 2-hour local-production soak, and local monitoring bundle deploy/verify now pass.
- MiniMax remains provider-account blocked by billing/credit errors; OpenAI Codex remains quota/plan blocked. Google Gemini is the currently validated successful external provider path, and production policy now blocks the known non-launch MiniMax/OpenAI-Codex capability prefixes until those account states are resolved.
- The latest production readiness rerun after the 2-hour soak passed again with the required live Google provider probe. Production mode now also requires fresh DB backup/restore drill and monitoring deploy verification summaries by default.

## Signoff commands

Local smoke:

```bash
CEX_READINESS_MODE=local \
  CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash \
  ./scripts/check-production-readiness.sh
```

Local production-posture smoke:

```bash
./scripts/bootstrap-local-production-env.sh --force
CEX_ENV_FILE=run/local-production/.env ./scripts/runtime-manager-linux.sh restart
CEX_ENV_FILE=run/local-production/.env \
  CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash \
  ./scripts/check-production-readiness.sh
```

Backup/restore drill:

```bash
./scripts/drill-db-backup-restore.sh
```

Soak:

```bash
CEX_ENV_FILE=run/local-production/.env \
  CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash \
  CEX_SOAK_DURATION_SECONDS=3600 \
  CEX_SOAK_INTERVAL_SECONDS=60 \
  ./scripts/soak-runtime.sh
```

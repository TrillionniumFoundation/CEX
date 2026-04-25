# Production Readiness Evidence v1

Last updated: 2026-04-25

This page records evidence for production-readiness claims. It is intentionally conservative: a green local gate is evidence, not a launch approval.

## Current verified evidence

| Area | Evidence | Status |
| --- | --- | --- |
| Workspace tests | `cargo test -p execution-service` after provider failure ack + policy bundle changes | PASS |
| Linux full runtime gate | `scripts/gate-local-linux.sh --skip-workspace --skip-db-bootstrap` with core + entry services and metrics smoke | PASS |
| Local readiness smoke | `CEX_READINESS_MODE=local CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/check-production-readiness.sh` | PASS |
| Local production-posture smoke | `CEX_ENV_FILE=run/local-production/.env CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/check-production-readiness.sh` | PASS |
| Live provider probe | `scripts/probe-openclaw-provider.sh --model google/gemini-2.5-flash` | PASS |
| Gateway → queued worker → OpenClaw provider success | `cap.openclaw.model.google.gemini-2-5-flash` invocation completed with `output_text=CEX_PROVIDER_PROBE_OK` | PASS |
| Short local soak | `CEX_SOAK_DURATION_SECONDS=60 CEX_SOAK_INTERVAL_SECONDS=15 CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/soak-runtime.sh` | PASS |
| 10-minute local soak | `CEX_READINESS_MODE=local CEX_SOAK_DURATION_SECONDS=600 CEX_SOAK_INTERVAL_SECONDS=60 CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/soak-runtime.sh` | PASS |
| Local production-posture soak | `CEX_ENV_FILE=run/local-production/.env CEX_SOAK_DURATION_SECONDS=300 CEX_SOAK_INTERVAL_SECONDS=60 CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash scripts/soak-runtime.sh` (summary `run/soak/soak-20260425T055657Z-248676.summary.json`) | PASS |

## Active blockers to a truthful 100% claim

- Windows-native `gate-local.ps1` still has not run on this host because PowerShell is unavailable.
- Docker-backed bootstrap / migration path still has not run in-session because Docker socket access is unavailable without elevation.
- The current strict production profile pass and soak use generated local secrets under `run/local-production/.env`; they validate posture mechanics but are not a real secret-management or deployment environment.
- Formal production deployment, backup/restore drills, migration rollback rehearsal, and longer soak windows still need real environment evidence.
- MiniMax remains provider-account blocked by billing/credit errors; OpenAI Codex remains quota/plan blocked. Google Gemini is the currently validated successful external provider path.

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

Soak:

```bash
CEX_ENV_FILE=run/local-production/.env \
  CEX_PROVIDER_PROBE_MODEL=google/gemini-2.5-flash \
  CEX_SOAK_DURATION_SECONDS=3600 \
  CEX_SOAK_INTERVAL_SECONDS=60 \
  ./scripts/soak-runtime.sh
```

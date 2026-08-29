#!/usr/bin/env bash
set -euo pipefail

MODE=full
EVIDENCE=''
while (($#)); do
  case "$1" in
    --mode)
      MODE="${2:-}"
      shift
      ;;
    --evidence)
      EVIDENCE="${2:-}"
      shift
      ;;
    -h|--help)
      echo 'usage: check-hepta-postgres-integration.sh [--mode full|recovery-only] [--evidence PATH]'
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 64
      ;;
  esac
  shift
done

case "$MODE" in
  full|recovery-only) ;;
  *) echo "invalid mode: $MODE" >&2; exit 64 ;;
esac

: "${HEPTA_TEST_DATABASE_URL:?HEPTA_TEST_DATABASE_URL is required}"
if [[ "${HEPTA_REQUIRE_POSTGRES_TESTS:-}" != '1' ]]; then
  echo 'HEPTA_REQUIRE_POSTGRES_TESTS=1 is required for strict hosted evidence' >&2
  exit 1
fi
case "$HEPTA_TEST_DATABASE_URL" in
  postgres://*|postgresql://*) ;;
  *) echo 'HEPTA_TEST_DATABASE_URL must be a PostgreSQL URL' >&2; exit 1 ;;
esac

if [[ "$MODE" == 'recovery-only' ]]; then
  cargo test --locked -p hepta-research-league --test postgres_recovery -- --test-threads=1
else
  cargo test --locked -p hepta-research-league --all-targets --no-fail-fast -- --test-threads=1
  cargo clippy --locked -p hepta-research-league --all-targets -- -D warnings
fi

if [[ -n "$EVIDENCE" ]]; then
  mkdir -p "$(dirname "$EVIDENCE")"
  python3 - "$EVIDENCE" "${GITHUB_SHA:-unknown}" "$MODE" <<'PY'
import json
from datetime import datetime, timezone
from pathlib import Path
import sys

Path(sys.argv[1]).write_text(json.dumps({
    "schema": "cex.hepta-postgres-integration-evidence.v1",
    "status": "ok",
    "ok": True,
    "commit_sha": sys.argv[2],
    "mode": sys.argv[3],
    "postgres_required": True,
    "completed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    "checks": [
        "restart-persistence",
        "multi-instance-disjoint-outbox-claim",
        "wrong-owner-ack-rejection",
        "expired-lease-recovery",
        "readiness-and-metrics",
    ],
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
fi

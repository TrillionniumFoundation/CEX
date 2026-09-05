#!/usr/bin/env bash
set -euo pipefail
set +x
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Both public entrypoints run the full guarded chain; never a 0001-only reset.
exec python3 "$ROOT/scripts/matrix_postgres_regression.py" "$@"

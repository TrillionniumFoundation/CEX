#!/usr/bin/env bash
set -euo pipefail
set +x
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The Python runner applies the complete migration chain before preserving every
# original check-matrix-transport-postgres.sh SQL assertion. It never evals a URL.
exec python3 "$ROOT/scripts/matrix_postgres_regression.py" "$@"

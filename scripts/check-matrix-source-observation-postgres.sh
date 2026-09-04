#!/usr/bin/env bash
set -euo pipefail
set +x
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
: "${MATRIX_TEST_DATABASE_URL:?MATRIX_TEST_DATABASE_URL must name a disposable test database}"
[[ "${MATRIX_TEST_ALLOW_SCHEMA_RESET:-}" == 1 ]] || { echo "MATRIX_TEST_ALLOW_SCHEMA_RESET=1 is required for this destructive disposable-database test" >&2; exit 64; }
command -v psql >/dev/null || { echo 'psql is required' >&2; exit 69; }
command -v python3 >/dev/null || { echo 'python3 is required' >&2; exit 69; }
# Capture stdout only, check the parser's exit status before eval, and quote
# every value. In particular, parser diagnostics must never become shell code.
if ! pg_environment="$(python3 - <<'PY'
import os
import shlex
from urllib.parse import parse_qs, unquote, urlsplit
try:
    value = urlsplit(os.environ['MATRIX_TEST_DATABASE_URL'])
    if value.scheme not in {'postgres', 'postgresql'} or not value.hostname:
        raise ValueError('invalid database endpoint')
    database = unquote(value.path.removeprefix('/'))
    if not database or value.fragment:
        raise ValueError('invalid database name or fragment')
    fields = {
        'PGHOST': value.hostname,
        'PGPORT': str(value.port or 5432),
        'PGDATABASE': database,
        'PGUSER': unquote(value.username or ''),
        'PGPASSWORD': unquote(value.password or ''),
        'PGSSLMODE': parse_qs(value.query).get('sslmode', ['prefer'])[-1],
    }
    if any('\x00' in field for field in fields.values()):
        raise ValueError('invalid database field')
    for key, field in fields.items():
        print(f'export {key}={shlex.quote(field)}')
except (ValueError, KeyError):
    raise SystemExit('invalid MATRIX_TEST_DATABASE_URL')
PY
)"; then
    echo 'database configuration rejected' >&2
    exit 65
fi
# Validate first, then retain the original complete durability regression.
# It resets transport tables and must run only against a disposable test DB.
bash "$ROOT/scripts/check-matrix-transport-postgres.sh"
unset PGSERVICE PGSERVICEFILE
# shellcheck disable=SC1090
eval "$pg_environment"
unset pg_environment MATRIX_TEST_DATABASE_URL
MIGRATION="$ROOT/services/matrix-entry-adapter/migrations/0002_source_observation_replay.sql"
psql -X -q -v ON_ERROR_STOP=1 -f "$MIGRATION"
# Re-apply to verify additive migration/backfill idempotency.
psql -X -q -v ON_ERROR_STOP=1 -f "$MIGRATION"
psql -X -q -v ON_ERROR_STOP=1 -f "$ROOT/scripts/test-matrix-source-observation-replay.sql"
printf '%s\n' '{"schema":"cex.matrix-source-observation-check.v1","status":"ok","production_authorization":"not_granted"}'

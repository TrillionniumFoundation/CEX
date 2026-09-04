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
# Only an explicitly consented disposable test database reaches this point.
unset PGSERVICE PGSERVICEFILE
# shellcheck disable=SC1090
eval "$pg_environment"
unset pg_environment
# Clean extension-only histories before the original suite reuses test identities.
psql -X -q -v ON_ERROR_STOP=1 <<'SQL'
do $cleanup$
declare t text;
begin
    foreach t in array array['matrix_transport_source_observations',
        'matrix_transport_poison_payloads', 'matrix_transport_cursor_history',
        'matrix_transport_send_bindings', 'matrix_transport_send_receipts'] loop
        if to_regclass('public.' || t) is not null then
            execute format('truncate table public.%I restart identity', t);
        end if;
    end loop;
end;
$cleanup$;
SQL
# Retain the complete original transport behavior regression.
bash "$ROOT/scripts/check-matrix-transport-postgres.sh"
unset MATRIX_TEST_DATABASE_URL
for NAME in 0002_source_observation_replay.sql 0003_sync_recovery_and_send_receipts.sql; do
  MIGRATION="$ROOT/services/matrix-entry-adapter/migrations/$NAME"
  psql -X -q -v ON_ERROR_STOP=1 -f "$MIGRATION"
  psql -X -q -v ON_ERROR_STOP=1 -f "$MIGRATION"
done
psql -X -q -v ON_ERROR_STOP=1 -f "$ROOT/scripts/test-matrix-source-observation-replay.sql"
psql -X -q -v ON_ERROR_STOP=1 -f "$ROOT/scripts/test-matrix-sync-recovery-postgres.sql"
printf '%s\n' '{"schema":"cex.matrix-recovery-postgres-check.v2","status":"ok","production_authorization":"not_granted"}'

#!/usr/bin/env bash
set -Eeuo pipefail

# Exercise the supported upgrade boundary for the normalized receipt tables.
#
# The check deliberately builds a database at the 0084 schema, copies that
# database for each case, and then applies 0085/0086/0087 in order.  Keeping the
# cases in cloned databases means a failed migration can be inspected without
# poisoning the next case, while the caller's DATABASE_URL database is never
# reset or otherwise modified.

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
# Capture credentials supplied by the caller before `_dev-helpers.sh` installs
# its convenience default (`postgres`).  A DATABASE_URL commonly carries the
# real password itself; forcing that default into PGPASSWORD would override the
# URI and make the hosted cex/cex_ci gate fail authentication.  An explicitly
# supplied PGPASSWORD remains the conventional override; a separately supplied
# CEX_POSTGRES_PASSWORD is only used when the URI has no password component.
RECEIPT_CALLER_PGPASSWORD="${PGPASSWORD-}"
RECEIPT_CALLER_PGPASSWORD_SET=0
if [[ ${PGPASSWORD+x} ]]; then
  RECEIPT_CALLER_PGPASSWORD_SET=1
fi
RECEIPT_CALLER_CEX_PASSWORD="${CEX_POSTGRES_PASSWORD-}"
RECEIPT_CALLER_CEX_PASSWORD_SET=0
if [[ ${CEX_POSTGRES_PASSWORD+x} ]]; then
  RECEIPT_CALLER_CEX_PASSWORD_SET=1
fi
RECEIPT_PSQL_PASSWORD=""
RECEIPT_PSQL_PASSWORD_SET=0
RECEIPT_DOCKER_PSQL_PASSWORD=""
# Keep an explicit caller URL authoritative while still importing the rest of
# the repository's local/container defaults from `.env`.
RECEIPT_CALLER_DATABASE_URL="${DATABASE_URL-}"
RECEIPT_CALLER_DATABASE_URL_SET=0
if [[ ${DATABASE_URL+x} ]]; then
  RECEIPT_CALLER_DATABASE_URL_SET=1
fi
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"

# Do not let an explicitly supplied URL get replaced by a repository .env file.
# When no URL was supplied, retain the normal local-helper discovery behavior.
cex_load_env
if [[ "$RECEIPT_CALLER_DATABASE_URL_SET" == "1" ]]; then
  export DATABASE_URL="$RECEIPT_CALLER_DATABASE_URL"
fi
: "${DATABASE_URL:?DATABASE_URL is required}"

BASE_URL="$(cex_effective_database_url)"
MIGRATION_85="$PROJECT_ROOT/migrations/0085_harden_term_exchange_receipt_projections.sql"
MIGRATION_86="$PROJECT_ROOT/migrations/0086_add_trnm_native_receipt_evidence.sql"
MIGRATION_87="$PROJECT_ROOT/migrations/0087_add_term_exchange_receipt_event_history.sql"

# Keep the complete PostgreSQL URI (including query/fragment options) when
# swapping only its database component.  The Docker fallback also needs the
# URI's user/password rather than silently reverting to the helper defaults.
# Parse once so the hundreds of migration/test calls below do not each spawn a
# parser.  Reject control characters in decoded components before they can be
# passed to a command or environment variable.
if ! command -v python3 >/dev/null 2>&1; then
  echo "receipt partial-upgrade check requires python3 for PostgreSQL URI handling" >&2
  exit 1
fi
if ! RECEIPT_DB_URI_PARTS="$(python3 - 3<<<"$BASE_URL" <<'PY'
from urllib.parse import parse_qsl, unquote, urlsplit
import os
import sys

raw_url = os.fdopen(3, encoding="utf-8").read().rstrip("\n")
if any(ord(char) < 0x20 or ord(char) == 0x7f for char in raw_url):
    raise SystemExit("DATABASE_URL contains a control character")
source = urlsplit(raw_url)
if source.scheme not in {"postgres", "postgresql"} or not source.netloc or not source.hostname:
    raise SystemExit("DATABASE_URL must be a postgres URL with a host")
if source.fragment:
    raise SystemExit("DATABASE_URL fragments are not supported")
try:
    # PostgreSQL's default port is 5432 when the URI omits one.  Preserve that
    # effective target so the Docker socket guard cannot mistake an omitted
    # port for an unknown/non-matching server.
    port = source.port or 5432
except ValueError as error:
    raise SystemExit(f"DATABASE_URL has an invalid port: {error}") from error

user = unquote(source.username or "")
password = unquote(source.password or "")
host = source.hostname or ""
database = unquote(source.path.lstrip("/"))
if not user:
    raise SystemExit("DATABASE_URL must include a user for Docker/local parity")
for label, value in (("user", user), ("password", password), ("database", database), ("host", host)):
    if any(ord(char) < 0x20 or ord(char) == 0x7f or char == "\t" for char in value):
        raise SystemExit(f"DATABASE_URL {label} contains a control character")
dangerous_query_keys = {
    "dbname", "database", "host", "hostaddr", "port", "user", "password",
    "service", "options", "replication", "target_session_attrs",
    "load_balance_hosts", "load_balance_host_type", "passfile",
    "sslcert", "sslkey", "sslrootcert", "sslcrl", "sslcrldir",
    "sslpassword", "gssencmode", "channel_binding",
}
for key, value in parse_qsl(source.query, keep_blank_values=True, strict_parsing=True):
    key = unquote(key).lower()
    value = unquote(value)
    if key in dangerous_query_keys:
        raise SystemExit(f"DATABASE_URL query parameter is not allowed: {key}")
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in key + value):
        raise SystemExit("DATABASE_URL query contains a control character")

# Preserve the raw username/host-port spelling while removing only the
# password component.  This value is used for every psql argument below; the
# decoded password is carried separately through PGPASSWORD/env-file.
safe_netloc = source.netloc
if "@" in safe_netloc:
    userinfo, hostport = safe_netloc.rsplit("@", 1)
    safe_netloc = f"{userinfo.split(':', 1)[0]}@{hostport}"

# Unit separator preserves empty optional user/password fields when Bash reads
# the result; tabs are treated as whitespace and would collapse them.
print("\x1f".join((source.scheme, safe_netloc, source.query, user, password,
                   "1" if source.password is not None else "0",
                   host, str(port))))
PY
)"; then
  echo "cannot parse DATABASE_URL for receipt partial-upgrade check" >&2
  exit 1
fi
IFS=$'\x1f' read -r RECEIPT_DB_URI_SCHEME RECEIPT_DB_URI_NETLOC \
  RECEIPT_DB_URI_QUERY RECEIPT_DB_URI_USER RECEIPT_DB_URI_PASSWORD \
  RECEIPT_DB_URI_PASSWORD_PRESENT \
  RECEIPT_DB_URI_HOST RECEIPT_DB_URI_PORT <<<"$RECEIPT_DB_URI_PARTS"
RECEIPT_DB_URI_SAFE_NETLOC="$RECEIPT_DB_URI_NETLOC"
if [[ -z "$RECEIPT_DB_URI_SCHEME" || -z "$RECEIPT_DB_URI_NETLOC" || -z "$RECEIPT_DB_URI_HOST" ]]; then
  echo "DATABASE_URL URI parsing returned incomplete connection details" >&2
  exit 1
fi
export CEX_DATABASE_URL_SYNCED=1
export CEX_DATABASE_URL_PASSWORD_PRESENT="$RECEIPT_DB_URI_PASSWORD_PRESENT"
export CEX_POSTGRES_HOST="$RECEIPT_DB_URI_HOST"
export CEX_POSTGRES_PORT="$RECEIPT_DB_URI_PORT"
# libpq gives PGPASSWORD precedence over a URI password.  Restore an explicit
# caller override after `.env` loading, but clear an env-file value when the
# selected DATABASE_URL already contains credentials.
if [[ "$RECEIPT_CALLER_PGPASSWORD_SET" == "1" ]]; then
  export PGPASSWORD="$RECEIPT_CALLER_PGPASSWORD"
elif [[ "$RECEIPT_CALLER_CEX_PASSWORD_SET" == "1" \
        || "$RECEIPT_DB_URI_PASSWORD_PRESENT" == "1" ]]; then
  unset PGPASSWORD
fi
# Prefer an explicit caller PGPASSWORD.  Otherwise a password embedded in the
# URL is authoritative and must not be replaced by a stale value imported from
# `.env`; both local libpq and Docker receive the decoded value separately from
# the stripped URI.  With no URL password, retain an explicitly supplied
# CEX_POSTGRES_PASSWORD or a password loaded from the env file.  Presence is
# tracked separately so an intentionally empty password is still passed.
if [[ "$RECEIPT_CALLER_PGPASSWORD_SET" == "1" ]]; then
  RECEIPT_PSQL_PASSWORD="$RECEIPT_CALLER_PGPASSWORD"
  RECEIPT_PSQL_PASSWORD_SET=1
  RECEIPT_DOCKER_PSQL_PASSWORD="$RECEIPT_CALLER_PGPASSWORD"
elif [[ "$RECEIPT_DB_URI_PASSWORD_PRESENT" == "1" ]]; then
  RECEIPT_PSQL_PASSWORD="$RECEIPT_DB_URI_PASSWORD"
  RECEIPT_PSQL_PASSWORD_SET=1
  RECEIPT_DOCKER_PSQL_PASSWORD="$RECEIPT_DB_URI_PASSWORD"
elif [[ ${PGPASSWORD+x} ]]; then
  # With a passwordless URI, an explicitly loaded environment PGPASSWORD is
  # still the selected libpq credential.  Preserve it for both local psql and
  # the Docker fallback; otherwise the latter silently drops the env-file
  # password and authenticates with an unintended socket default.
  RECEIPT_PSQL_PASSWORD="$PGPASSWORD"
  RECEIPT_PSQL_PASSWORD_SET=1
  RECEIPT_DOCKER_PSQL_PASSWORD="$PGPASSWORD"
elif [[ "$RECEIPT_CALLER_CEX_PASSWORD_SET" == "1" ]]; then
  RECEIPT_PSQL_PASSWORD="$RECEIPT_CALLER_CEX_PASSWORD"
  RECEIPT_PSQL_PASSWORD_SET=1
  RECEIPT_DOCKER_PSQL_PASSWORD="$RECEIPT_CALLER_CEX_PASSWORD"
elif [[ "${CEX_POSTGRES_PASSWORD_EXPLICIT:-0}" == "1" ]]; then
  RECEIPT_PSQL_PASSWORD="${CEX_POSTGRES_PASSWORD:-}"
  RECEIPT_PSQL_PASSWORD_SET=1
  RECEIPT_DOCKER_PSQL_PASSWORD="$RECEIPT_PSQL_PASSWORD"
fi
RECEIPT_DOCKER_PSQL_USER="${RECEIPT_DB_URI_USER:-${CEX_POSTGRES_USER:-postgres}}"

for migration in "$MIGRATION_85" "$MIGRATION_86" "$MIGRATION_87"; do
  [[ -f "$migration" ]] || {
    echo "receipt partial-upgrade migration is missing: $migration" >&2
    exit 1
  }
done

if ! cex_has_local_psql && ! cex_can_use_docker_postgres; then
  echo "receipt partial-upgrade check requires psql or a usable Docker Postgres container" >&2
  exit 1
fi

# This is a destructive-looking test only inside databases that this script
# creates.  Refuse a remote URL unless an operator explicitly opts in.
if [[ "$RECEIPT_DB_URI_HOST" != "127.0.0.1" \
      && "$RECEIPT_DB_URI_HOST" != "localhost" \
      && "$RECEIPT_DB_URI_HOST" != "::1" \
      && "${CEX_ALLOW_NONLOCAL_RECEIPT_PARTIAL_UPGRADE_CHECK:-0}" != "1" ]]; then
  echo "refusing non-local DATABASE_URL for receipt partial-upgrade check" >&2
  echo "set CEX_ALLOW_NONLOCAL_RECEIPT_PARTIAL_UPGRADE_CHECK=1 only for an isolated disposable server" >&2
  exit 1
fi

run_token_source="${CEX_RECEIPT_PARTIAL_UPGRADE_RUN_ID:-$(date -u +%Y%m%d%H%M%S)_$$}"
if [[ ! "$run_token_source" =~ ^[A-Za-z0-9_]+$ ]]; then
  echo "unsafe receipt partial-upgrade run id" >&2
  exit 2
fi
# PostgreSQL limits identifiers to 63 bytes.  The fixture deliberately uses
# descriptive case labels, so an operator-provided run id cannot be allowed to
# make a later case fail halfway through the matrix.  Keep a readable prefix
# and an 8-byte digest when the supplied token is unusually long; the digest
# preserves practical uniqueness without echoing an unbounded value into SQL
# identifiers.
run_token="$run_token_source"
if (( ${#run_token} > 24 )); then
  run_token="${run_token:0:15}_$(printf '%s' "$run_token" | sha256sum | cut -c1-8)"
fi

case_database_name() {
  local label="$1"
  local prefix="cex_receipt_pu_"
  local raw="${prefix}${label}_${run_token}"
  if (( ${#raw} <= 63 )); then
    printf '%s\n' "$raw"
    return 0
  fi
  # Keep enough of the label for operators to identify the case, then append
  # a digest of the full label so two long labels cannot collapse to one DB.
  local digest
  digest="$(printf '%s' "$label" | sha256sum | cut -c1-8)"
  local fixed_length=$(( ${#prefix} + 1 + ${#digest} + 1 + ${#run_token} ))
  local label_length=$((63 - fixed_length))
  if (( label_length < 1 )); then
    echo "receipt partial-upgrade run id leaves no room for a case database name" >&2
    return 2
  fi
  printf '%s\n' "${prefix}${label:0:label_length}_${digest}_${run_token}"
}

base_db="cex_receipt_pu_base_${run_token}"
if [[ ! "$base_db" =~ ^[A-Za-z_][A-Za-z0-9_]*$ || ${#base_db} -gt 63 ]]; then
  # The generated default is intentionally short; this branch also catches a
  # user-supplied run id before it can become an SQL identifier.
  echo "receipt partial-upgrade database name is invalid or too long: $base_db" >&2
  exit 2
fi

db_url() {
  local database="$1"
  if [[ ! "$database" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
    echo "unsafe database name: $database" >&2
    return 2
  fi
  # The scheme/netloc/query were parsed and validated above.  Assemble the
  # path structurally so a literal `__CEX_DATABASE__` in credentials or query
  # data can never be rewritten accidentally.
  local rewritten="${RECEIPT_DB_URI_SCHEME}://${RECEIPT_DB_URI_SAFE_NETLOC}/${database}"
  if [[ -n "$RECEIPT_DB_URI_QUERY" ]]; then
    rewritten+="?${RECEIPT_DB_URI_QUERY}"
  fi
  printf '%s\n' "$rewritten"
}

docker_psql() {
  local database="$1"
  shift
  local -a args=(psql -U "$RECEIPT_DOCKER_PSQL_USER" -d "$database" -X -v ON_ERROR_STOP=1)
  if [[ -n "$RECEIPT_DB_URI_HOST" && "$RECEIPT_DB_URI_HOST" != "localhost" \
        && "$RECEIPT_DB_URI_HOST" != "127.0.0.1" && "$RECEIPT_DB_URI_HOST" != "::1" ]]; then
    # A Docker fallback may target a service hostname or a remote disposable
    # PostgreSQL endpoint.  In that case use the full URI so host/port and
    # query options are honored instead of assuming the container socket.
    args=(psql "$(db_url "$database")" -X -v ON_ERROR_STOP=1)
  elif ! cex_postgres_docker_socket_is_target; then
    echo "Docker PostgreSQL container port does not match DATABASE_URL; refusing socket fallback" >&2
    return 2
  fi
  if [[ -n "$RECEIPT_DOCKER_PSQL_PASSWORD" \
        || "$RECEIPT_DB_URI_PASSWORD_PRESENT" == "1" \
        || "$RECEIPT_CALLER_PGPASSWORD_SET" == "1" \
        || "$RECEIPT_CALLER_CEX_PASSWORD_SET" == "1" \
        || "$RECEIPT_PSQL_PASSWORD_SET" == "1" \
        || "${CEX_POSTGRES_PASSWORD_EXPLICIT:-0}" == "1" ]]; then
    cex_docker_exec_with_password "$RECEIPT_DOCKER_PSQL_PASSWORD" \
      "$CEX_POSTGRES_CONTAINER_NAME" "${args[@]}" "$@"
  else
    cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" "${args[@]}" "$@"
  fi
}

docker_psql_stdin() {
  local database="$1"
  shift
  local -a args=(psql -U "$RECEIPT_DOCKER_PSQL_USER" -d "$database" -X -v ON_ERROR_STOP=1)
  if [[ -n "$RECEIPT_DB_URI_HOST" && "$RECEIPT_DB_URI_HOST" != "localhost" \
        && "$RECEIPT_DB_URI_HOST" != "127.0.0.1" && "$RECEIPT_DB_URI_HOST" != "::1" ]]; then
    args=(psql "$(db_url "$database")" -X -v ON_ERROR_STOP=1)
  elif ! cex_postgres_docker_socket_is_target; then
    echo "Docker PostgreSQL container port does not match DATABASE_URL; refusing socket fallback" >&2
    return 2
  fi
  if [[ -n "$RECEIPT_DOCKER_PSQL_PASSWORD" \
        || "$RECEIPT_DB_URI_PASSWORD_PRESENT" == "1" \
        || "$RECEIPT_CALLER_PGPASSWORD_SET" == "1" \
        || "$RECEIPT_CALLER_CEX_PASSWORD_SET" == "1" \
        || "$RECEIPT_PSQL_PASSWORD_SET" == "1" \
        || "${CEX_POSTGRES_PASSWORD_EXPLICIT:-0}" == "1" ]]; then
    cex_docker_exec_with_password_stdin "$RECEIPT_DOCKER_PSQL_PASSWORD" \
      "$CEX_POSTGRES_CONTAINER_NAME" "${args[@]}" "$@"
  else
    cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" "${args[@]}" "$@"
  fi
}

run_admin_sql() {
  local sql="$1"
  if cex_has_local_psql; then
    if [[ "$RECEIPT_PSQL_PASSWORD_SET" == "1" ]]; then
      PGPASSWORD="$RECEIPT_PSQL_PASSWORD" \
        psql "$(db_url postgres)" -X -v ON_ERROR_STOP=1 -c "$sql"
    else
      psql "$(db_url postgres)" -X -v ON_ERROR_STOP=1 -c "$sql"
    fi
    return $?
  fi
  if cex_can_use_docker_postgres; then
    docker_psql postgres -c "$sql"
    return $?
  fi
  echo "no usable postgres client found" >&2
  return 1
}

run_db_stdin() {
  local database="$1"
  shift
  if cex_has_local_psql; then
    if [[ "$RECEIPT_PSQL_PASSWORD_SET" == "1" ]]; then
      PGPASSWORD="$RECEIPT_PSQL_PASSWORD" \
        psql "$(db_url "$database")" -X -v ON_ERROR_STOP=1 "$@"
    else
      psql "$(db_url "$database")" -X -v ON_ERROR_STOP=1 "$@"
    fi
    return $?
  fi
  if cex_can_use_docker_postgres; then
    docker_psql_stdin "$database" "$@"
    return $?
  fi
  echo "no usable postgres client found" >&2
  return 1
}

run_db_file() {
  local database="$1"
  local file="$2"
  run_db_stdin "$database" -f - < "$file"
}

drop_db() {
  local database="$1"
  # PostgreSQL 13+ supports WITH (force).  Retain a fallback for older local
  # images, where no client can remain connected during this short-lived test.
  run_admin_sql "drop database if exists \"$database\" with (force);" >/dev/null 2>&1 || \
    run_admin_sql "drop database if exists \"$database\";" >/dev/null 2>&1 || true
}

created_databases=()
cleanup() {
  local status=$?
  set +e
  local index
  for ((index=${#created_databases[@]}-1; index>=0; index--)); do
    drop_db "${created_databases[index]}"
  done
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

run_admin_sql 'select 1' >/dev/null
drop_db "$base_db"
run_admin_sql "create database \"$base_db\";" >/dev/null
created_databases+=("$base_db")

echo "==> building pre-0085 receipt schema in $base_db"
while IFS= read -r migration; do
  [[ -f "$migration" ]] || continue
  migration_name="$(basename "$migration")"
  migration_number="${migration_name%%_*}"
  if ((10#$migration_number < 85)); then
    echo "    applying $migration_name"
    run_db_file "$base_db" "$migration" >/dev/null
  fi
done < <(find "$PROJECT_ROOT/migrations" -maxdepth 1 -type f \
  -name '[0-9][0-9][0-9][0-9]_*.sql' | sort)

echo "==> seeding legacy (pre-0085) receipt rows"
run_db_stdin "$base_db" <<'SQL'
-- Native receipts: one amount comes from immutable intent evidence, one is an
-- audit-only intent whose legacy evidence supplies the compatibility amount,
-- one malformed/negative intent proves the fail-closed zero path, and one
-- explicit JSON-null amount proves null cannot fall back to positive evidence.
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values
(
    'pu-intent-native-happy', 'term_exchange_protocol_v1', 'pu-scope',
    'native-happy', repeat('a', 64),
    '{"intent_id":"pu-intent-native-happy","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-happy","amount_credits":37,"kind":"reward"}'::jsonb,
    'accepted'
),
(
    'pu-intent-native-fallback', 'term_exchange_protocol_v1', 'pu-scope',
    'native-fallback', repeat('b', 64),
    '{"intent_id":"pu-intent-native-fallback","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-fallback","kind":"audit"}'::jsonb,
    'accepted'
),
(
    'pu-intent-native-invalid', 'term_exchange_protocol_v1', 'pu-scope',
    'native-invalid', repeat('c', 64),
    '{"intent_id":"pu-intent-native-invalid","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-invalid","amount_credits":-7,"kind":"audit"}'::jsonb,
    'accepted'
),
(
    'pu-intent-native-null', 'term_exchange_protocol_v1', 'pu-scope',
    'native-null', repeat('d', 64),
    '{"intent_id":"pu-intent-native-null","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-null","amount_credits":null,"kind":"audit"}'::jsonb,
    'accepted'
);

insert into public.trnm_economic_receipts (
    receipt_id, intent_id, protocol_version, idempotency_scope,
    idempotency_key, progression_class, status, receipt_json, finalized_at
) values
(
    'pu-receipt-native-happy', 'pu-intent-native-happy',
    'term_exchange_protocol_v1', 'pu-scope', 'native-happy',
    'progression_allowed', 'settled',
    -- The legacy evidence deliberately says 99; 0086 must use the immutable
    -- intent amount (37), rewrite the evidence amount, and rebuild its hash.
    '{"intent_id":"pu-intent-native-happy","receipt_id":"pu-receipt-native-happy","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":99}}'::jsonb,
    '2026-01-01T00:00:00Z'
),
(
    'pu-receipt-native-fallback', 'pu-intent-native-fallback',
    'term_exchange_protocol_v1', 'pu-scope', 'native-fallback',
    'progression_allowed', 'settled',
    '{"intent_id":"pu-intent-native-fallback","receipt_id":"pu-receipt-native-fallback","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":9}}'::jsonb,
    '2026-01-01T00:00:01Z'
),
(
    'pu-receipt-native-invalid', 'pu-intent-native-invalid',
    'term_exchange_protocol_v1', 'pu-scope', 'native-invalid',
    'progression_allowed', 'settled',
    '{"intent_id":"pu-intent-native-invalid","receipt_id":"pu-receipt-native-invalid","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":11}}'::jsonb,
    '2026-01-01T00:00:02Z'
),
(
    'pu-receipt-native-null', 'pu-intent-native-null',
    'term_exchange_protocol_v1', 'pu-scope', 'native-null',
    'progression_allowed', 'settled',
    -- The writer serializes Option::None as JSON null and emits zero.  A
    -- stale positive compatibility evidence value must not become authority.
    '{"intent_id":"pu-intent-native-null","receipt_id":"pu-receipt-native-null","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":13}}'::jsonb,
    '2026-01-01T00:00:03Z'
);

-- These rows have the exact 0026 shape: there is intentionally no
-- amount_credits column until 0085 adds it.
insert into public.league_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    finalized_at
) values
(
    'pu-league-known', 'term_exchange_protocol_v1', 'pu-intent-league-known',
    'pu-term', 'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-league-known', 'pu-ledger-league-known', null,
    '2026-01-01T00:01:00Z'
),
(
    'pu-league-unknown', 'term_exchange_protocol_v1', 'pu-intent-league-unknown',
    'pu-term', 'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-league-unknown', 'pu-ledger-league-unknown', null,
    '2026-01-01T00:01:01Z'
);

insert into public.world_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    finalized_at
) values
(
    'pu-world-known', 'term_exchange_protocol_v1', 'pu-intent-world-known',
    'pu-term', 'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-world-known', 'pu-ledger-world-known', null,
    '2026-01-01T00:02:00Z'
),
(
    'pu-world-unknown', 'term_exchange_protocol_v1', 'pu-intent-world-unknown',
    'pu-term', 'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-world-unknown', 'pu-ledger-world-unknown', null,
    '2026-01-01T00:02:01Z'
);
SQL

new_case() {
  local label="$1"
  local database
  database="$(case_database_name "$label")"
  if [[ ! "$database" =~ ^[A-Za-z_][A-Za-z0-9_]*$ || ${#database} -gt 63 ]]; then
    echo "generated case database name is invalid or too long: $database" >&2
    return 2
  fi
  drop_db "$database"
  run_admin_sql "create database \"$database\" template \"$base_db\";" >/dev/null
  created_databases+=("$database")
  CASE_DB="$database"
}

expect_migration_failure() {
  local database="$1"
  local label="$2"
  local migration="$3"
  local expected_text="$4"
  local output
  local status
  set +e
  output="$(run_db_file "$database" "$migration" 2>&1)"
  status=$?
  set -e
  if ((status == 0)); then
    echo "ERROR: $label unexpectedly accepted a conflicting upgrade" >&2
    return 1
  fi
  if [[ "$output" != *"$expected_text"* ]]; then
    echo "ERROR: $label failed with an unexpected error (wanted '$expected_text')" >&2
    printf '%s\n' "$output" >&2
    return 1
  fi
  echo "    rejected $label (status=$status)"
}

assert_db() {
  local database="$1"
  run_db_stdin "$database"
}

create_preexisting_league_history_table() {
  local database="$1"
  # Keep this fixture deliberately free of the 0087 checks/triggers.  It models
  # the deployment runner that committed a table and one history row before the
  # migration's hardening statements ran; 0087 must validate the row before it
  # can become a predecessor for a new append.
  run_db_stdin "$database" <<'SQL'
create table public.league_term_exchange_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    receipt_id text not null,
    event_sequence bigint not null,
    event_kind text not null default 'initial',
    previous_receipt_hash text,
    protocol_version text not null,
    intent_id text not null,
    term_id text not null,
    backend_id text not null,
    backend_kind text not null,
    status text not null,
    progression_class text not null,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    amount_credits bigint,
    finalized_at timestamptz not null,
    receipt_json jsonb not null,
    receipt_hash text not null,
    created_at timestamptz not null default now(),
    unique (receipt_id, event_sequence)
);
SQL
}

create_partial_term_history_tables() {
  local database="$1"
  # Model a runner that committed the history table DDL without the
  # event_id identity/default, primary key, or receipt-sequence uniqueness.
  # League keeps a plain bigint column; World omits event_id entirely.  The
  # 0087 schema guards must repair both shapes before backfill.
  run_db_stdin "$database" <<'SQL'
create table public.league_term_exchange_receipt_events_v1 (
    event_id bigint,
    receipt_id text,
    event_sequence bigint,
    event_kind text,
    previous_receipt_hash text,
    protocol_version text,
    intent_id text,
    term_id text,
    backend_id text,
    backend_kind text,
    status text,
    progression_class text,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    amount_credits bigint,
    finalized_at timestamptz,
    receipt_json jsonb,
    receipt_hash text,
    created_at timestamptz
);
create table public.world_term_exchange_receipt_events_v1 (
    receipt_id text,
    event_sequence bigint,
    event_kind text,
    previous_receipt_hash text,
    protocol_version text,
    intent_id text,
    term_id text,
    backend_id text,
    backend_kind text,
    status text,
    progression_class text,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    amount_credits bigint,
    finalized_at timestamptz,
    receipt_json jsonb,
    receipt_hash text,
    created_at timestamptz
);
SQL
}

echo "==> conflict: normalized amount column has a fractional numeric shape"
new_case normalized_amount_type
normalized_amount_type_db="$CASE_DB"
run_db_stdin "$normalized_amount_type_db" <<'SQL'
-- A same-named numeric column must not be accepted as exact credits.
alter table public.league_term_exchange_receipts
    add column amount_credits numeric;
alter table public.world_term_exchange_receipts
    add column amount_credits numeric;
update public.league_term_exchange_receipts
   set amount_credits = 1.5
 where receipt_id = 'pu-league-known';
update public.world_term_exchange_receipts
   set amount_credits = 2.5
 where receipt_id = 'pu-world-known';
SQL
expect_migration_failure "$normalized_amount_type_db" \
  "normalized fractional amount column type" "$MIGRATION_85" \
  'normalized league receipt amount_credits must be bigint'
assert_db "$normalized_amount_type_db" <<'SQL'
do $check$
declare
    league_type text;
    world_type text;
    league_amount numeric;
    world_amount numeric;
begin
    select udt_name into league_type
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'league_term_exchange_receipts'
       and column_name = 'amount_credits';
    select udt_name into world_type
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'world_term_exchange_receipts'
       and column_name = 'amount_credits';
    select amount_credits into league_amount
      from public.league_term_exchange_receipts
     where receipt_id = 'pu-league-known';
    select amount_credits into world_amount
      from public.world_term_exchange_receipts
     where receipt_id = 'pu-world-known';
    if league_type is distinct from 'numeric'
       or world_type is distinct from 'numeric'
       or league_amount is distinct from 1.5
       or world_amount is distinct from 2.5 then
        raise exception 'fractional amount fixture changed during rollback';
    end if;
end
$check$;
SQL
echo "    normalized fractional amount shape conflict passed"

echo "==> conflict: normalized amount constraint has a weak predicate"
new_case normalized_amount_constraint
normalized_amount_constraint_db="$CASE_DB"
run_db_stdin "$normalized_amount_constraint_db" <<'SQL'
alter table public.league_term_exchange_receipts
    add column amount_credits bigint;
alter table public.world_term_exchange_receipts
    add column amount_credits bigint;
alter table public.league_term_exchange_receipts
    add constraint league_term_exchange_receipts_amount_credits_nonnegative_v1
    check (amount_credits >= -999999);
alter table public.world_term_exchange_receipts
    add constraint world_term_exchange_receipts_amount_credits_nonnegative_v1
    check (amount_credits is null or amount_credits >= 0);
SQL
expect_migration_failure "$normalized_amount_constraint_db" \
  "normalized weak amount constraint" "$MIGRATION_85" \
  'normalized league receipt amount constraint is not canonical'
assert_db "$normalized_amount_constraint_db" <<'SQL'
do $check$
declare
    league_expression text;
    world_expression text;
begin
    select regexp_replace(
               lower(coalesce(pg_get_expr(c.conbin, c.conrelid), '')),
               '[[:space:]]+', '', 'g'
           )
      into league_expression
      from pg_catalog.pg_constraint c
     where c.conrelid = 'public.league_term_exchange_receipts'::regclass
       and c.conname = 'league_term_exchange_receipts_amount_credits_nonnegative_v1';
    select regexp_replace(
               lower(coalesce(pg_get_expr(c.conbin, c.conrelid), '')),
               '[[:space:]]+', '', 'g'
           )
      into world_expression
      from pg_catalog.pg_constraint c
     where c.conrelid = 'public.world_term_exchange_receipts'::regclass
       and c.conname = 'world_term_exchange_receipts_amount_credits_nonnegative_v1';
    if position('-999999' in coalesce(league_expression, '')) = 0
       or world_expression is distinct from '((amount_creditsisnull)or(amount_credits>=0))' then
        raise exception 'weak amount constraint changed during rollback';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid in (
             'public.league_term_exchange_receipts'::regclass,
             'public.world_term_exchange_receipts'::regclass
         )
           and tgname like 'trg_cex_%term_exchange_receipt_%'
    ) then
        raise exception 'weak amount rejection installed projection triggers';
    end if;
end
$check$;
SQL
echo "    normalized weak amount constraint conflict passed"

echo "==> conflict: normalized amount column has generated/default drift"
new_case normalized_amount_column_shape
normalized_amount_column_shape_db="$CASE_DB"
run_db_stdin "$normalized_amount_column_shape_db" <<'SQL'
-- The 0085 contract is a nullable, plain bigint column with no default.  A
-- same-named default/generated column must not be mistaken for that shape.
alter table public.league_term_exchange_receipts
    add column amount_credits bigint default 7;
alter table public.world_term_exchange_receipts
    add column amount_credits bigint generated always as (0) stored;
SQL
expect_migration_failure "$normalized_amount_column_shape_db" \
  "normalized amount generated/default column shape" "$MIGRATION_85" \
  'normalized league receipt amount_credits column has incompatible nullability/default/generated shape'
assert_db "$normalized_amount_column_shape_db" <<'SQL'
do $check$
declare
    league_default text;
    world_generated text;
begin
    select column_default
      into league_default
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'league_term_exchange_receipts'
       and column_name = 'amount_credits';
    select a.attgenerated::text
      into world_generated
      from pg_catalog.pg_attribute a
     where a.attrelid = 'public.world_term_exchange_receipts'::regclass
       and a.attname = 'amount_credits';
    if league_default is null
       or position('7' in league_default) = 0
       or world_generated is distinct from 's' then
        raise exception 'normalized amount generated/default shape changed during rollback';
    end if;
end
$check$;
SQL
echo "    normalized generated/default amount shape conflict passed"

echo "==> happy partial upgrade (including an interrupted 0086 table)"
new_case happy
happy_db="$CASE_DB"

# Also model an 0085 attempt that committed its expand-only columns before the
# constraint/trigger portion ran.  The real migration must treat these columns
# as already present and finish the hardening idempotently.
run_db_stdin "$happy_db" <<'SQL'
alter table public.league_term_exchange_receipts
    add column amount_credits bigint;
alter table public.world_term_exchange_receipts
    add column amount_credits bigint;
SQL
run_db_file "$happy_db" "$MIGRATION_85" >/dev/null

# Simulate a rollout that created the event table and its default-zero amount
# column, but stopped before the 0086 evidence repair and trigger installation.
run_db_stdin "$happy_db" <<'SQL'
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    intent_id text not null,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
) values (
    'pu-intent-native-happy', 1, repeat('a', 64),
    'pu-receipt-native-happy', 'term_exchange_protocol_v1', 'pu-scope',
    'native-happy', 'progression_allowed', 'settled', 0,
    '{"intent_id":"pu-intent-native-happy","receipt_id":"pu-receipt-native-happy","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed"}'::jsonb,
    'stale-partial-hash', 'initial', '2026-01-01T00:00:00Z'
), (
    -- A native-looking row that predates completion of 0086 must be repaired
    -- but may not receive the compatibility marker: its provenance is not
    -- provably the migration's legacy-projection INSERT path.
    'pu-intent-native-fallback', 1, repeat('b', 64),
    'pu-receipt-native-fallback', 'term_exchange_protocol_v1', 'pu-scope',
    'native-fallback', 'progression_allowed', 'settled', 0,
    '{"intent_id":"pu-intent-native-fallback","receipt_id":"pu-receipt-native-fallback","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":9}}'::jsonb,
    'stale-partial-fallback-hash', 'initial', '2026-01-01T00:00:01Z'
);
SQL

# A known amount on a projection models an application that wrote the new
# nullable column before the history migration; the second row remains the
# legacy unknown amount and must stay NULL/fail-closed.
run_db_stdin "$happy_db" <<'SQL'
alter table public.league_term_exchange_receipts disable trigger all;
alter table public.world_term_exchange_receipts disable trigger all;
update public.league_term_exchange_receipts
   set amount_credits = 23
 where receipt_id = 'pu-league-known';
update public.world_term_exchange_receipts
   set amount_credits = 31
 where receipt_id = 'pu-world-known';
alter table public.league_term_exchange_receipts
    enable always trigger trg_cex_league_term_exchange_receipt_mutation_v1;
alter table public.league_term_exchange_receipts
    enable always trigger trg_cex_league_term_exchange_receipt_truncate_v1;
alter table public.world_term_exchange_receipts
    enable always trigger trg_cex_world_term_exchange_receipt_mutation_v1;
alter table public.world_term_exchange_receipts
    enable always trigger trg_cex_world_term_exchange_receipt_truncate_v1;
SQL

run_db_file "$happy_db" "$MIGRATION_86" >/dev/null
run_db_file "$happy_db" "$MIGRATION_87" >/dev/null

# Re-apply all three migrations to prove an interrupted deployment can be
# retried without duplicate history or evidence rows.
run_db_file "$happy_db" "$MIGRATION_85" >/dev/null
run_db_file "$happy_db" "$MIGRATION_86" >/dev/null
run_db_file "$happy_db" "$MIGRATION_87" >/dev/null

assert_db "$happy_db" <<'SQL'
do $test$
declare
    amount_value bigint;
    evidence_value jsonb;
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 4 then
        raise exception 'native receipt event backfill count mismatch';
    end if;
    if not exists (
        select 1
          from pg_catalog.pg_constraint c
         where c.conrelid = 'public.trnm_economic_receipt_events_v1'::regclass
           and c.contype = 'f'
           and c.confrelid = 'public.trnm_economic_intents'::regclass
           and c.confdeltype = 'r'
           and c.confupdtype = 'a'
           and c.convalidated
           and not c.condeferrable
           and not c.condeferred
    ) then
        raise exception 'partial native event table did not regain canonical intent foreign key';
    end if;

    select amount_credits, receipt_json -> 'evidence'
      into amount_value, evidence_value
      from public.trnm_economic_receipt_events_v1
     where intent_id = 'pu-intent-native-happy';
    if amount_value is distinct from 37
       or evidence_value #>> '{amount_credits}' is distinct from '37'
       or evidence_value #>> '{payload_hash}' is distinct from repeat('a', 64)
       or evidence_value ? '_cex_legacy_amount_fallback' then
        raise exception 'immutable native amount/evidence backfill mismatch';
    end if;

    select amount_credits, receipt_json -> 'evidence'
      into amount_value, evidence_value
      from public.trnm_economic_receipt_events_v1
     where intent_id = 'pu-intent-native-fallback';
    if amount_value is distinct from 9
       or evidence_value #>> '{amount_credits}' is distinct from '9'
       or evidence_value ? '_cex_legacy_amount_fallback' then
        raise exception 'pre-existing native fallback row received migration authority marker';
    end if;

    select amount_credits
      into amount_value
      from public.trnm_economic_receipt_events_v1
     where intent_id = 'pu-intent-native-invalid';
    if amount_value is distinct from 0 then
        raise exception 'invalid immutable amount did not fail closed to zero';
    end if;

    select amount_credits, receipt_json -> 'evidence'
      into amount_value, evidence_value
      from public.trnm_economic_receipt_events_v1
     where intent_id = 'pu-intent-native-null';
    if amount_value is distinct from 0
       or evidence_value #>> '{amount_credits}' is distinct from '0'
       or evidence_value #>> '{payload_hash}' is distinct from repeat('d', 64) then
        raise exception 'explicit NULL immutable amount did not fail closed to zero';
    end if;

    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 2
       or (select count(*) from public.world_term_exchange_receipt_events_v1) <> 2 then
        raise exception 'term receipt history backfill count mismatch';
    end if;
    if (select amount_credits from public.league_term_exchange_receipt_events_v1
         where receipt_id = 'pu-league-known' and event_sequence = 1) is distinct from 23
       or (select amount_credits from public.world_term_exchange_receipt_events_v1
         where receipt_id = 'pu-world-known' and event_sequence = 1) is distinct from 31 then
        raise exception 'known projection amount was not copied to history';
    end if;
    if (select amount_credits from public.league_term_exchange_receipt_events_v1
         where receipt_id = 'pu-league-unknown' and event_sequence = 1) is not null
       or (select amount_credits from public.world_term_exchange_receipt_events_v1
         where receipt_id = 'pu-world-unknown' and event_sequence = 1) is not null then
        raise exception 'legacy unknown projection amount was fabricated';
    end if;
    if exists (
        select 1
          from public.league_term_exchange_receipt_events_v1
         where event_sequence <> 1 or event_kind <> 'backfill'
    ) or exists (
        select 1
          from public.world_term_exchange_receipt_events_v1
         where event_sequence <> 1 or event_kind <> 'backfill'
    ) then
        raise exception 'history backfill sequence/kind mismatch';
    end if;
    if exists (
        select 1
          from public.trnm_economic_receipt_events_v1
         where receipt_hash <> encode(digest(receipt_json::text, 'sha256'), 'hex')
    ) or exists (
        select 1
          from public.league_term_exchange_receipt_events_v1
         where receipt_hash <> 'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
    ) or exists (
        select 1
          from public.world_term_exchange_receipt_events_v1
         where receipt_hash <> 'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
    ) then
        raise exception 'receipt hash backfill mismatch';
    end if;
end
$test$;

do $mutation$
declare
    rejected boolean := false;
begin
    begin
        update public.trnm_economic_receipt_events_v1
           set status = 'tampered'
         where intent_id = 'pu-intent-native-happy';
    exception when others then
        rejected := position('append-only' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'native event mutation guard did not reject update';
    end if;

    rejected := false;
    begin
        delete from public.league_term_exchange_receipt_events_v1
         where receipt_id = 'pu-league-known';
    exception when others then
        rejected := position('append-only' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'term history mutation guard did not reject delete';
    end if;
end
$mutation$;
SQL

# Exercise the live 0087 INSERT trigger directly after the happy upgrade.  The
# projection rows are deliberately inserted first because history is required
# to bind to an existing compatibility receipt; each attempted event is then
# wrapped in a PL/pgSQL subtransaction so a rejected row cannot poison the
# remainder of this case.  These probes cover the three authority bypasses that
# a service-level shadow write would otherwise hide: orphan receipt, swapped
# immutable identity, and both directions of nullable amount drift.
run_db_stdin "$happy_db" <<'SQL'
insert into public.league_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at
) values
(
    'pu-direct-swap', 'term_exchange_protocol_v1',
    'pu-intent-direct-authority', 'pu-term', 'pu-backend', 'cex', 'settled',
    'progression_allowed', 'pu-direct-swap-settle', 'pu-direct-swap-ledger',
    null, null, '2026-01-01T00:07:00Z'
),
(
    'pu-direct-null-amount', 'term_exchange_protocol_v1',
    'pu-intent-direct-null', 'pu-term', 'pu-backend', 'cex', 'settled',
    'progression_allowed', 'pu-direct-null-settle', 'pu-direct-null-ledger',
    null, null, '2026-01-01T00:07:01Z'
),
(
    'pu-direct-known-amount', 'term_exchange_protocol_v1',
    'pu-intent-direct-known', 'pu-term', 'pu-backend', 'cex', 'settled',
    'progression_allowed', 'pu-direct-known-settle', 'pu-direct-known-ledger',
    null, 17, '2026-01-01T00:07:02Z'
);

-- Native receipt IDs are intent-level identities.  Give the trigger a second
-- valid intent so the live cross-intent claim below exercises the event-stream
-- authority rather than failing at the parent foreign key.
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-native-cross', 'term_exchange_protocol_v1', 'pu-scope',
    'native-cross', repeat('f', 64),
    '{"intent_id":"pu-intent-native-cross","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-cross"}'::jsonb,
    'accepted'
);
-- Keep the top-level intent identity malformed while retaining a valid term;
-- the native INSERT trigger must reject this before it can mint a receipt.
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-native-bad-json-runtime', 'term_exchange_protocol_v1', 'pu-scope',
    'native-bad-json-runtime', repeat('e', 64),
    '{"intent_id":123,"protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-bad-json-runtime"}'::jsonb,
    'accepted'
);

do $direct_insert_guards$
declare
    rejected boolean;
begin
    -- The migration-only evidence marker cannot be self-authored by a live
    -- native writer, even when every other envelope field is internally
    -- consistent.  This probe runs after 0086/0087 install the runtime INSERT
    -- trigger and must fail before a row is committed.
    rejected := false;
    begin
        with payload as (
            select jsonb_build_object(
                'intent_id', 'pu-intent-native-cross',
                'receipt_id', 'pu-native-forged-marker',
                'protocol_version', 'term_exchange_protocol_v1',
                'term_id', 'pu-term-native-cross',
                'backend_id', 'cex-settlement-backend',
                'backend_kind', 'cex',
                'status', 'settled',
                'progression_class', 'progression_allowed',
                'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:07:01Z')::bigint,
                'evidence', jsonb_build_object(
                    'payload_hash', repeat('f', 64),
                    'amount_credits', 0,
                    '_cex_legacy_amount_fallback', true
                )
            ) as receipt_json
        )
        insert into public.trnm_economic_receipt_events_v1 (
            intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
            idempotency_scope, idempotency_key, progression_class, status,
            amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
        )
        select 'pu-intent-native-cross', 1, repeat('f', 64),
               'pu-native-forged-marker', 'term_exchange_protocol_v1',
               'pu-scope', 'native-cross', 'progression_allowed', 'settled',
               0, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
               'initial', '2026-01-01T00:07:01Z'
          from payload;
    exception when others then
        rejected := position('reserved for migration backfill' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'native INSERT accepted the migration-only fallback marker';
    end if;

    -- A receipt already bound to pu-intent-native-happy cannot be claimed by
    -- another intent, even when that intent carries a self-consistent hash and
    -- JSON snapshot.
    rejected := false;
    begin
        with payload as (
            select jsonb_build_object(
                'intent_id', 'pu-intent-native-cross',
                'receipt_id', 'pu-receipt-native-happy',
                'protocol_version', 'term_exchange_protocol_v1',
                'term_id', 'pu-term-native-cross',
                'backend_id', 'cex-settlement-backend',
                'backend_kind', 'cex',
                'status', 'settled',
                'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:07:02Z')::bigint,
                'progression_class', 'progression_allowed',
                'evidence', jsonb_build_object(
                    'payload_hash', repeat('f', 64),
                    'amount_credits', 0
                )
            ) as receipt_json
        )
        insert into public.trnm_economic_receipt_events_v1 (
            intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
            idempotency_scope, idempotency_key, progression_class, status,
            amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
        )
        select 'pu-intent-native-cross', 1, repeat('f', 64),
               'pu-receipt-native-happy', 'term_exchange_protocol_v1',
               'pu-scope', 'native-cross', 'progression_allowed', 'settled',
               0, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
               'initial', '2026-01-01T00:07:02Z'
          from payload;
    exception when others then
        rejected := position('already bound to a different intent' in lower(sqlerrm)) > 0
                    or position('conflicts with compatibility receipt intent' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'native cross-intent receipt INSERT was accepted';
    end if;

    -- A malformed immutable intent JSON must not be rescued by the matching
    -- SQL columns: serde would reject the numeric intent_id before lookup.
    rejected := false;
    begin
        with payload as (
            select jsonb_build_object(
                'intent_id', 'pu-intent-native-bad-json-runtime',
                'receipt_id', 'pu-native-bad-json-runtime-receipt',
                'protocol_version', 'term_exchange_protocol_v1',
                'term_id', 'pu-term-native-bad-json-runtime',
                'backend_id', 'cex-settlement-backend',
                'backend_kind', 'cex',
                'status', 'settled',
                'progression_class', 'progression_allowed',
                'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:07:02Z')::bigint,
                'evidence', jsonb_build_object(
                    'payload_hash', repeat('e', 64),
                    'amount_credits', 0
                )
            ) as receipt_json
        )
        insert into public.trnm_economic_receipt_events_v1 (
            intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
            idempotency_scope, idempotency_key, progression_class, status,
            amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
        )
        select 'pu-intent-native-bad-json-runtime', 1, repeat('e', 64),
               'pu-native-bad-json-runtime-receipt', 'term_exchange_protocol_v1',
               'pu-scope', 'native-bad-json-runtime', 'progression_allowed', 'settled',
               0, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
               'initial', '2026-01-01T00:07:02Z'
          from payload;
    exception when others then
        rejected := position('immutable intent json identity is invalid' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'malformed native intent JSON INSERT was accepted';
    end if;

    -- An explicit JSON null is how the Rust Option<i64>::None field is
    -- serialized.  It is an invalid value authority, not an invitation to
    -- supply a positive event amount from outside the immutable intent.
    rejected := false;
    begin
        with payload as (
            select jsonb_build_object(
                'intent_id', 'pu-intent-native-null',
                'receipt_id', 'pu-native-null-runtime-receipt',
                'protocol_version', 'term_exchange_protocol_v1',
                'term_id', 'pu-term-native-null',
                'backend_id', 'cex-settlement-backend',
                'backend_kind', 'cex',
                'status', 'settled',
                'progression_class', 'progression_allowed',
                'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:07:02Z')::bigint,
                'evidence', jsonb_build_object(
                    'payload_hash', repeat('d', 64),
                    'amount_credits', 7
                )
            ) as receipt_json
        )
        insert into public.trnm_economic_receipt_events_v1 (
            intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
            idempotency_scope, idempotency_key, progression_class, status,
            amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
        )
        select 'pu-intent-native-null', 1, repeat('d', 64),
               'pu-native-null-runtime-receipt', 'term_exchange_protocol_v1',
               'pu-scope', 'native-null', 'progression_allowed', 'settled',
               7, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
               'initial', '2026-01-01T00:07:02Z'
          from payload;
    exception when others then
        rejected := position('invalid immutable intent amount' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'explicit NULL native intent amount INSERT was accepted';
    end if;

    -- 1. No projection authority for the supplied receipt id.  The old
    -- intent-keyed lookup accepted this self-authenticated event.
    rejected := false;
    begin
        with payload as (
            select jsonb_build_object(
                'protocol_version', 'term_exchange_protocol_v1',
                'receipt_id', 'pu-direct-orphan',
                'intent_id', 'pu-intent-direct-orphan',
                'term_id', 'pu-term',
                'backend_id', 'pu-backend',
                'backend_kind', 'cex',
                'status', 'settled',
                'progression_class', 'progression_allowed',
                'settlement_reference', null,
                'ledger_entry_id', null,
                'reason', null,
                'amount_credits', null,
                'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:07:03Z')::bigint
            ) as receipt_json
        )
        insert into public.league_term_exchange_receipt_events_v1 (
            receipt_id, event_sequence, event_kind, previous_receipt_hash,
            protocol_version, intent_id, term_id, backend_id, backend_kind, status,
            progression_class, settlement_reference, ledger_entry_id, reason,
            amount_credits, finalized_at, receipt_json, receipt_hash
        )
        select 'pu-direct-orphan', 1, 'initial', null,
               'term_exchange_protocol_v1', 'pu-intent-direct-orphan', 'pu-term',
               'pu-backend', 'cex', 'settled', 'progression_allowed', null, null,
               null, null, '2026-01-01T00:07:03Z', receipt_json,
               'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
          from payload;
    exception when others then
        rejected := position('projection authority is missing' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'direct orphan history INSERT was accepted';
    end if;

    -- 2. The receipt projection exists, but the event presents a different
    -- immutable intent.  Resolving the projection by intent_id would miss it.
    rejected := false;
    begin
        with payload as (
            select jsonb_build_object(
                'protocol_version', 'term_exchange_protocol_v1',
                'receipt_id', 'pu-direct-swap',
                'intent_id', 'pu-intent-direct-forged',
                'term_id', 'pu-term',
                'backend_id', 'pu-backend',
                'backend_kind', 'cex',
                'status', 'settled',
                'progression_class', 'progression_allowed',
                'settlement_reference', 'pu-direct-swap-settle',
                'ledger_entry_id', 'pu-direct-swap-ledger',
                'reason', null,
                'amount_credits', null,
                'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:07:04Z')::bigint
            ) as receipt_json
        )
        insert into public.league_term_exchange_receipt_events_v1 (
            receipt_id, event_sequence, event_kind, previous_receipt_hash,
            protocol_version, intent_id, term_id, backend_id, backend_kind, status,
            progression_class, settlement_reference, ledger_entry_id, reason,
            amount_credits, finalized_at, receipt_json, receipt_hash
        )
        select 'pu-direct-swap', 1, 'initial', null,
               'term_exchange_protocol_v1', 'pu-intent-direct-forged', 'pu-term',
               'pu-backend', 'cex', 'settled', 'progression_allowed',
               'pu-direct-swap-settle', 'pu-direct-swap-ledger', null,
               null, '2026-01-01T00:07:04Z', receipt_json,
               'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
          from payload;
    exception when others then
        rejected := position('immutable projection identity' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'direct swapped-intent history INSERT was accepted';
    end if;

    -- 3a. A legacy NULL projection amount is still exact authority; it may not
    -- be upgraded by a positive event amount.
    rejected := false;
    begin
        with payload as (
            select jsonb_build_object(
                'protocol_version', 'term_exchange_protocol_v1',
                'receipt_id', 'pu-direct-null-amount',
                'intent_id', 'pu-intent-direct-null',
                'term_id', 'pu-term',
                'backend_id', 'pu-backend',
                'backend_kind', 'cex',
                'status', 'settled',
                'progression_class', 'progression_allowed',
                'settlement_reference', 'pu-direct-null-settle',
                'ledger_entry_id', 'pu-direct-null-ledger',
                'reason', null,
                'amount_credits', 7,
                'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:07:05Z')::bigint
            ) as receipt_json
        )
        insert into public.league_term_exchange_receipt_events_v1 (
            receipt_id, event_sequence, event_kind, previous_receipt_hash,
            protocol_version, intent_id, term_id, backend_id, backend_kind, status,
            progression_class, settlement_reference, ledger_entry_id, reason,
            amount_credits, finalized_at, receipt_json, receipt_hash
        )
        select 'pu-direct-null-amount', 1, 'initial', null,
               'term_exchange_protocol_v1', 'pu-intent-direct-null', 'pu-term',
               'pu-backend', 'cex', 'settled', 'progression_allowed',
               'pu-direct-null-settle', 'pu-direct-null-ledger', null,
               7, '2026-01-01T00:07:05Z', receipt_json,
               'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
          from payload;
    exception when others then
        rejected := position('immutable projection amount' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'direct NULL-to-known amount history INSERT was accepted';
    end if;

    -- 3b. The reverse transition is rejected as well: a known projection may
    -- not be erased by an event carrying SQL/JSON NULL.
    rejected := false;
    begin
        with payload as (
            select jsonb_build_object(
                'protocol_version', 'term_exchange_protocol_v1',
                'receipt_id', 'pu-direct-known-amount',
                'intent_id', 'pu-intent-direct-known',
                'term_id', 'pu-term',
                'backend_id', 'pu-backend',
                'backend_kind', 'cex',
                'status', 'settled',
                'progression_class', 'progression_allowed',
                'settlement_reference', 'pu-direct-known-settle',
                'ledger_entry_id', 'pu-direct-known-ledger',
                'reason', null,
                'amount_credits', null,
                'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:07:06Z')::bigint
            ) as receipt_json
        )
        insert into public.league_term_exchange_receipt_events_v1 (
            receipt_id, event_sequence, event_kind, previous_receipt_hash,
            protocol_version, intent_id, term_id, backend_id, backend_kind, status,
            progression_class, settlement_reference, ledger_entry_id, reason,
            amount_credits, finalized_at, receipt_json, receipt_hash
        )
        select 'pu-direct-known-amount', 1, 'initial', null,
               'term_exchange_protocol_v1', 'pu-intent-direct-known', 'pu-term',
               'pu-backend', 'cex', 'settled', 'progression_allowed',
               'pu-direct-known-settle', 'pu-direct-known-ledger', null,
               null, '2026-01-01T00:07:06Z', receipt_json,
               'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
          from payload;
    exception when others then
        rejected := position('immutable projection amount' in lower(sqlerrm)) > 0;
    end;
    if not rejected then
        raise exception 'direct known-to-NULL amount history INSERT was accepted';
    end if;

    if exists (
        select 1
          from public.league_term_exchange_receipt_events_v1
         where receipt_id in (
             'pu-direct-orphan', 'pu-direct-swap',
             'pu-direct-null-amount', 'pu-direct-known-amount'
         )
    ) then
        raise exception 'a rejected direct history INSERT left an event row behind';
    end if;
end
$direct_insert_guards$;
SQL
echo "    happy partial upgrade passed"

echo "==> legacy projection INSERT provenance marker"
new_case legacy_backfill_marker
legacy_backfill_marker_db="$CASE_DB"
run_db_file "$legacy_backfill_marker_db" "$MIGRATION_85" >/dev/null
# Isolate this case from the native fixture rows copied from base_db.  The
# target below is the only audit-only legacy receipt, so exactly one event may
# carry the migration-only marker.  No native event is pre-seeded for it.
run_db_stdin "$legacy_backfill_marker_db" <<'SQL'
delete from public.trnm_economic_receipts
 where intent_id like 'pu-intent-native-%';
delete from public.trnm_economic_intents
 where intent_id like 'pu-intent-native-%';
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-legacy-marker', 'term_exchange_protocol_v1', 'pu-scope',
    'legacy-marker', repeat('9', 64),
    '{"intent_id":"pu-intent-legacy-marker","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-legacy-marker","kind":"audit"}'::jsonb,
    'accepted'
);
insert into public.trnm_economic_receipts (
    receipt_id, intent_id, protocol_version, idempotency_scope,
    idempotency_key, progression_class, status, receipt_json, finalized_at
) values (
    'pu-receipt-legacy-marker', 'pu-intent-legacy-marker',
    'term_exchange_protocol_v1', 'pu-scope', 'legacy-marker',
    'progression_allowed', 'settled',
    '{"intent_id":"pu-intent-legacy-marker","receipt_id":"pu-receipt-legacy-marker","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"amount_credits":42}}'::jsonb,
    '2026-01-01T00:09:00Z'
);
SQL
run_db_file "$legacy_backfill_marker_db" "$MIGRATION_86" >/dev/null
assert_db "$legacy_backfill_marker_db" <<'SQL'
do $marker_check$
declare
    marker_value jsonb;
    amount_value bigint;
    receipt_hash_value text;
begin
    if (select count(*)
          from public.trnm_economic_receipt_events_v1
         where receipt_json #> '{evidence,_cex_legacy_amount_fallback}' = 'true'::jsonb) <> 1 then
        raise exception 'legacy fallback marker was not unique to the inserted batch';
    end if;
    select amount_credits,
           receipt_json #> '{evidence,_cex_legacy_amount_fallback}',
           receipt_hash
      into amount_value, marker_value, receipt_hash_value
      from public.trnm_economic_receipt_events_v1
     where intent_id = 'pu-intent-legacy-marker';
    if amount_value is distinct from 42
       or marker_value is distinct from 'true'::jsonb
       or receipt_hash_value is distinct from
          encode(digest((select receipt_json::text
                           from public.trnm_economic_receipt_events_v1
                          where intent_id = 'pu-intent-legacy-marker'), 'sha256'), 'hex') then
        raise exception 'legacy fallback marker amount/hash provenance mismatch';
    end if;
end
$marker_check$;
SQL
echo "    legacy projection marker provenance passed"
# A retry after a successful backfill must not duplicate the authorization row
# or re-mark an unrelated native event.  Reapply 0086 against the same case and
# verify the durable provenance ledger remains one-to-one.
run_db_file "$legacy_backfill_marker_db" "$MIGRATION_86" >/dev/null
assert_db "$legacy_backfill_marker_db" <<'SQL'
do $marker_retry_check$
declare
    marker_count bigint;
    provenance_count bigint;
begin
    select count(*) into marker_count
      from public.trnm_economic_receipt_events_v1
     where receipt_json #> '{evidence,_cex_legacy_amount_fallback}' = 'true'::jsonb;
    select count(*) into provenance_count
      from public.trnm_economic_receipt_legacy_fallback_provenance_v1;
    if marker_count <> 1 or provenance_count <> 1 then
        raise exception 'legacy fallback provenance retry duplicated authorization';
    end if;
end
$marker_retry_check$;
SQL
echo "    legacy projection marker retry idempotence passed"

echo "==> conflict: normalized projection backed by explicit-null native intent"
new_case normalized_native_null
normalized_native_null_db="$CASE_DB"
run_db_file "$normalized_native_null_db" "$MIGRATION_85" >/dev/null
# A native EconomicIntent serializes Option::None as an explicit JSON null.
# That value is present-but-invalid for the native amount authority, so a
# normalized projection carrying a positive amount must not be allowed to seed
# a history event during 0087 backfill.  Disable only the legacy projection
# mutation triggers while constructing this hostile partial-upgrade fixture;
# restore them before invoking the migration so the failure is attributable to
# the normalized-history validator itself.
run_db_stdin "$normalized_native_null_db" <<'SQL'
alter table public.league_term_exchange_receipts disable trigger all;
insert into public.league_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at
) values (
    'pu-league-native-null', 'term_exchange_protocol_v1',
    'pu-intent-native-null', 'pu-term-native-null', 'pu-backend', 'cex',
    'settled', 'progression_allowed', 'pu-settle-native-null',
    'pu-ledger-native-null', null, 7, '2026-01-01T00:03:04Z'
);
alter table public.league_term_exchange_receipts
    enable always trigger trg_cex_league_term_exchange_receipt_mutation_v1;
alter table public.league_term_exchange_receipts
    enable always trigger trg_cex_league_term_exchange_receipt_truncate_v1;
SQL
expect_migration_failure "$normalized_native_null_db" \
  "normalized projection positive amount with explicit-null native intent" \
  "$MIGRATION_87" \
  'normalized receipt amount is not authorized by an invalid immutable intent amount'
assert_db "$normalized_native_null_db" <<'SQL'
do $check$
begin
    if to_regclass('public.league_term_exchange_receipt_events_v1') is not null
       or to_regclass('public.world_term_exchange_receipt_events_v1') is not null then
        raise exception 'rejected explicit-null native amount migration left history tables behind';
    end if;
    if (select amount_credits
          from public.league_term_exchange_receipts
         where receipt_id = 'pu-league-native-null') is distinct from 7
       or (select intent_id
             from public.league_term_exchange_receipts
            where receipt_id = 'pu-league-native-null') is distinct from 'pu-intent-native-null'
       or (select intent_json -> 'amount_credits'
             from public.trnm_economic_intents
            where intent_id = 'pu-intent-native-null') is distinct from 'null'::jsonb then
        raise exception 'explicit-null native amount fixture changed during rejected migration';
    end if;
end
$check$;
SQL
echo "    explicit-null native intent authority conflict passed"

echo "==> conflict: native evidence binding"
new_case native_evidence
evidence_db="$CASE_DB"
run_db_file "$evidence_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$evidence_db" <<'SQL'
update public.trnm_economic_receipts
   set receipt_json = jsonb_set(
       receipt_json,
       '{evidence}',
       jsonb_build_object(
           'payload_hash', repeat('d', 64),
           'amount_credits', 37
       ),
       true
   )
 where intent_id = 'pu-intent-native-happy';
SQL
expect_migration_failure "$evidence_db" "native evidence payload-hash conflict" "$MIGRATION_86" \
  'TRNM receipt event JSON binding mismatch'
assert_db "$evidence_db" <<'SQL'
do $check$
begin
    if to_regclass('public.trnm_economic_receipt_events_v1') is not null then
        raise exception 'rejected 0086 left a native history table behind';
    end if;
end
$check$;
SQL
echo "    native evidence conflict passed"

echo "==> conflict: native orphan intent binding"
new_case native_orphan
orphan_db="$CASE_DB"
run_db_file "$orphan_db" "$MIGRATION_85" >/dev/null
# Model a deployment runner that committed only the event table's first
# column before the 0086 transaction was interrupted.  The orphan guard must
# reject this state before the migration can silently skip the row in its
# intent-joined compatibility backfill.
run_db_stdin "$orphan_db" <<'SQL'
create table public.trnm_economic_receipt_events_v1 (
    intent_id text not null
);
insert into public.trnm_economic_receipt_events_v1 (intent_id)
values ('pu-intent-native-orphan');
SQL
expect_migration_failure "$orphan_db" "native orphan intent binding" "$MIGRATION_86" \
  'TRNM receipt event intent binding does not exist'
assert_db "$orphan_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 1
       or (select intent_id from public.trnm_economic_receipt_events_v1 limit 1)
          is distinct from 'pu-intent-native-orphan' then
        raise exception 'orphan fixture changed during rejected migration';
    end if;
    if exists (
        select 1
          from information_schema.columns
         where table_schema = 'public'
           and table_name = 'trnm_economic_receipt_events_v1'
           and column_name = 'amount_credits'
    ) then
        raise exception 'rejected orphan migration left expand column behind';
    end if;
end
$check$;
SQL
echo "    native orphan conflict passed"

echo "==> conflict: native receipt_id claimed by multiple intents before hardening"
new_case native_cross_id
native_cross_db="$CASE_DB"
run_db_file "$native_cross_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_cross_db" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values
(
    'pu-intent-native-cross-a', 'term_exchange_protocol_v1', 'pu-scope',
    'native-cross-a', repeat('d', 64),
    '{"intent_id":"pu-intent-native-cross-a","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-cross-a"}'::jsonb,
    'accepted'
),
(
    'pu-intent-native-cross-b', 'term_exchange_protocol_v1', 'pu-scope',
    'native-cross-b', repeat('e', 64),
    '{"intent_id":"pu-intent-native-cross-b","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-cross-b"}'::jsonb,
    'accepted'
);

-- Model a runner that committed the complete event shape and rows, but stopped
-- before 0086 installed its constraints/indexes.  There is deliberately no
-- amount column: the receipt-id guard must run before the compatibility repair
-- can alter this partial state.
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    intent_id text not null,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
with payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-cross-a',
        'receipt_id', 'pu-native-cross-receipt',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'evidence', jsonb_build_object('payload_hash', repeat('d', 64), 'amount_credits', 0)
    ) as receipt_json
)
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    receipt_json, receipt_hash, event_kind, finalized_at
)
select 'pu-intent-native-cross-a', 1, repeat('d', 64), 'pu-native-cross-receipt',
       'term_exchange_protocol_v1', 'pu-scope', 'native-cross-a',
       'progression_allowed', 'settled', receipt_json,
       encode(digest(receipt_json::text, 'sha256'), 'hex'), 'initial',
       '2026-01-01T00:08:00Z'
  from payload;
with payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-cross-b',
        'receipt_id', 'pu-native-cross-receipt',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'evidence', jsonb_build_object('payload_hash', repeat('e', 64), 'amount_credits', 0)
    ) as receipt_json
)
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    receipt_json, receipt_hash, event_kind, finalized_at
)
select 'pu-intent-native-cross-b', 1, repeat('e', 64), 'pu-native-cross-receipt',
       'term_exchange_protocol_v1', 'pu-scope', 'native-cross-b',
       'progression_allowed', 'settled', receipt_json,
       encode(digest(receipt_json::text, 'sha256'), 'hex'), 'initial',
       '2026-01-01T00:08:01Z'
  from payload;
SQL
expect_migration_failure "$native_cross_db" "native cross-intent receipt_id fork" "$MIGRATION_86" \
  'TRNM native receipt history contains multiple intents for one receipt_id'
assert_db "$native_cross_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 2 then
        raise exception 'native cross-intent conflict changed partial history during rollback';
    end if;
    if exists (
        select 1
          from information_schema.columns
         where table_schema = 'public'
           and table_name = 'trnm_economic_receipt_events_v1'
           and column_name = 'amount_credits'
    ) then
        raise exception 'rejected cross-intent migration left expand column behind';
    end if;
end
$check$;
SQL
echo "    native cross-intent receipt conflict passed"

echo "==> conflict: malformed native intent payload hash"
new_case native_bad_hash
native_bad_hash_db="$CASE_DB"
run_db_file "$native_bad_hash_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_bad_hash_db" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-native-bad-hash', 'term_exchange_protocol_v1', 'pu-scope',
    'native-bad-hash', 'not-a-sha256-digest',
    '{"intent_id":"pu-intent-native-bad-hash","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-bad-hash"}'::jsonb,
    'accepted'
);
SQL
expect_migration_failure "$native_bad_hash_db" "malformed native intent payload hash" "$MIGRATION_86" \
  'TRNM economic intent payload_hash is not canonical'
assert_db "$native_bad_hash_db" <<'SQL'
do $check$
begin
    if to_regclass('public.trnm_economic_receipt_events_v1') is not null then
        raise exception 'rejected malformed-hash migration left a native event table behind';
    end if;
end
$check$;
SQL
echo "    malformed native intent hash conflict passed"

echo "==> repair: prior native event foreign key used immediate NO ACTION"
new_case native_fk_noaction
native_fk_noaction_db="$CASE_DB"
run_db_file "$native_fk_noaction_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_fk_noaction_db" <<'SQL'
-- This is the shape produced by an earlier 0086 attempt, whose omitted
-- `ON DELETE` clause created an immediate NO ACTION foreign key.  It is safe
-- to repair to explicit RESTRICT, and the migration must do so atomically.
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    intent_id text not null references public.trnm_economic_intents(intent_id),
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
SQL
run_db_file "$native_fk_noaction_db" "$MIGRATION_86" >/dev/null
assert_db "$native_fk_noaction_db" <<'SQL'
do $check$
begin
    if not exists (
        select 1
          from pg_catalog.pg_constraint c
         where c.conrelid = 'public.trnm_economic_receipt_events_v1'::regclass
           and c.contype = 'f'
           and c.confrelid = 'public.trnm_economic_intents'::regclass
           and c.confdeltype = 'r'
           and c.confupdtype = 'a'
           and c.convalidated
           and not c.condeferrable
           and not c.condeferred
    ) then
        raise exception 'prior NO ACTION native foreign key was not repaired to RESTRICT';
    end if;
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 4 then
        raise exception 'NO ACTION foreign-key repair changed native backfill count';
    end if;
end
$check$;
SQL
echo "    prior NO ACTION foreign-key repair passed"

echo "==> conflict: native event_id key has no sequence default"
new_case native_event_id_default
native_event_id_default_db="$CASE_DB"
run_db_file "$native_event_id_default_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_event_id_default_db" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-native-no-default', 'term_exchange_protocol_v1', 'pu-scope',
    'native-no-default', repeat('f', 64),
    '{"intent_id":"pu-intent-native-no-default","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-no-default"}'::jsonb,
    'accepted'
);

-- A plain bigint primary key is not append-safe: the native writer omits
-- event_id and relies on an owned identity/serial sequence.  The migration
-- must reject this partial shape instead of installing a trigger that will
-- fail only on the first production append.
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint primary key,
    intent_id text not null,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
with payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-no-default',
        'receipt_id', 'pu-native-no-default-receipt',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'evidence', jsonb_build_object(
            'payload_hash', repeat('f', 64), 'amount_credits', 0
        )
    ) as receipt_json
)
insert into public.trnm_economic_receipt_events_v1 (
    event_id, intent_id, event_sequence, intent_hash, receipt_id,
    protocol_version, idempotency_scope, idempotency_key,
    progression_class, status, amount_credits, receipt_json,
    receipt_hash, event_kind, finalized_at
)
select 1, 'pu-intent-native-no-default', 1, repeat('f', 64),
       'pu-native-no-default-receipt', 'term_exchange_protocol_v1',
       'pu-scope', 'native-no-default', 'progression_allowed', 'settled',
       0, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
       'initial', '2026-01-01T00:09:00Z'
  from payload;
SQL
expect_migration_failure "$native_event_id_default_db" "native event_id sequence default" "$MIGRATION_86" \
  'TRNM native receipt event event_id has no identity or sequence default'
assert_db "$native_event_id_default_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 1 then
        raise exception 'event_id default conflict changed native history during rollback';
    end if;
    if exists (
        select 1 from pg_catalog.pg_trigger
         where tgrelid = 'public.trnm_economic_receipt_events_v1'::regclass
           and tgname = 'trg_cex_validate_trnm_economic_receipt_event_v1'
    ) then
        raise exception 'rejected event_id default migration installed a trigger';
    end if;
end
$check$;
SQL
echo "    native event_id sequence-default conflict passed"

echo "==> conflict: native event_id sequence increment headroom"
new_case native_sequence_headroom
native_sequence_headroom_db="$CASE_DB"
run_db_file "$native_sequence_headroom_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_sequence_headroom_db" <<'SQL'
-- A custom owned sequence can be below MAXVALUE while its next increment
-- would already overflow.  0086 must reject that state before installing any
-- trigger or backfilling rows.  The first attempt exercises last_value
-- headroom; the second (after rollback) exercises max(event_id) headroom.
create sequence public.pu_native_headroom_event_id_seq
    as bigint
    increment by 2
    minvalue 1
    maxvalue 10
    no cycle;
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint primary key
        default nextval('public.pu_native_headroom_event_id_seq'::regclass),
    intent_id text not null
        references public.trnm_economic_intents(intent_id)
        on delete restrict,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
alter sequence public.pu_native_headroom_event_id_seq
    owned by public.trnm_economic_receipt_events_v1.event_id;
select setval('public.pu_native_headroom_event_id_seq'::regclass, 9, true);
SQL
expect_migration_failure "$native_sequence_headroom_db" \
  "native event_id sequence last_value headroom" "$MIGRATION_86" \
  'TRNM native receipt event event_id sequence is exhausted'

# Keep the partial table, move the sequence back below the limit, and place a
# manually supplied event_id at the same unsafe boundary.  This second run
# must fail on max(event_id) even though last_value itself has room.
run_db_stdin "$native_sequence_headroom_db" <<'SQL'
select setval('public.pu_native_headroom_event_id_seq'::regclass, 1, false);
insert into public.trnm_economic_receipt_events_v1 (
    event_id, intent_id, event_sequence, intent_hash, receipt_id,
    protocol_version, idempotency_scope, idempotency_key,
    progression_class, status, amount_credits, receipt_json,
    receipt_hash, event_kind, finalized_at
) values (
    9, 'pu-intent-native-happy', 1, repeat('a', 64),
    'pu-receipt-native-happy', 'term_exchange_protocol_v1', 'pu-scope',
    'native-happy', 'progression_allowed', 'settled', 37,
    '{"intent_id":"pu-intent-native-happy","receipt_id":"pu-receipt-native-happy","protocol_version":"term_exchange_protocol_v1","status":"settled","progression_class":"progression_allowed","evidence":{"payload_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","amount_credits":37}}'::jsonb,
    repeat('0', 64), 'initial', '2026-01-01T00:00:00Z'
);
SQL
expect_migration_failure "$native_sequence_headroom_db" \
  "native event_id sequence max-event headroom" "$MIGRATION_86" \
  'TRNM native receipt event event_id sequence is exhausted'
assert_db "$native_sequence_headroom_db" <<'SQL'
do $check$
declare
    sequence_increment_value bigint;
    sequence_max_value bigint;
    sequence_last_value bigint;
    sequence_called_value boolean;
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 1
       or (select event_id from public.trnm_economic_receipt_events_v1 limit 1) <> 9 then
        raise exception 'native sequence-headroom rollback changed the partial row';
    end if;
    select s.seqincrement, s.seqmax
      into sequence_increment_value, sequence_max_value
      from pg_catalog.pg_sequence s
      join pg_catalog.pg_class c on c.oid = s.seqrelid
      join pg_catalog.pg_namespace n on n.oid = c.relnamespace
     where n.nspname = 'public'
       and c.relname = 'pu_native_headroom_event_id_seq';
    select last_value, is_called
      into sequence_last_value, sequence_called_value
      from public.pu_native_headroom_event_id_seq;
    if sequence_increment_value is distinct from 2
       or sequence_max_value is distinct from 10
       or sequence_last_value is distinct from 1
       or sequence_called_value then
        raise exception 'native sequence-headroom fixture state changed during rollback';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid = 'public.trnm_economic_receipt_events_v1'::regclass
           and tgname = 'trg_cex_validate_trnm_economic_receipt_event_v1'
    ) then
        raise exception 'native sequence-headroom rejection installed a trigger';
    end if;
end
$check$;
SQL
echo "    native event_id sequence-headroom conflict passed"

echo "==> conflict: native event_id sequence is shared by another default"
new_case native_shared_sequence
native_shared_sequence_db="$CASE_DB"
run_db_file "$native_shared_sequence_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_shared_sequence_db" <<'SQL'
-- Even when the native event_id sequence is already owned by its canonical
-- column, a second DEFAULT nextval() consumer makes the generator unsafe.
create sequence public.pu_native_shared_event_id_seq
    as bigint
    increment by 1
    minvalue 1
    maxvalue 9223372036854775807
    no cycle;
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint primary key
        default nextval('public.pu_native_shared_event_id_seq'::regclass),
    intent_id text not null
        references public.trnm_economic_intents(intent_id)
        on delete restrict,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
alter sequence public.pu_native_shared_event_id_seq
    owned by public.trnm_economic_receipt_events_v1.event_id;
create table public.pu_native_shared_sequence_consumer (
    id bigint default nextval('public.pu_native_shared_event_id_seq'::regclass)
);
SQL
expect_migration_failure "$native_shared_sequence_db" \
  "native event_id shared sequence" "$MIGRATION_86" \
  'TRNM native receipt event event_id sequence is already used by another column/default'
assert_db "$native_shared_sequence_db" <<'SQL'
do $check$
begin
    if to_regclass('public.trnm_economic_receipt_events_v1') is null
       or to_regclass('public.pu_native_shared_sequence_consumer') is null then
        raise exception 'native shared-sequence migration rollback changed partial objects';
    end if;
    if pg_get_serial_sequence(
           'public.trnm_economic_receipt_events_v1', 'event_id'
       ) is distinct from 'public.pu_native_shared_event_id_seq'
       or not exists (
           select 1
             from pg_catalog.pg_depend d
            where d.classid = 'pg_catalog.pg_class'::regclass
              and d.objid = 'public.pu_native_shared_event_id_seq'::regclass
              and d.refclassid = 'pg_catalog.pg_class'::regclass
              and d.refobjid = 'public.trnm_economic_receipt_events_v1'::regclass
              and d.refobjsubid = (
                  select attnum
                    from pg_catalog.pg_attribute
                   where attrelid = 'public.trnm_economic_receipt_events_v1'::regclass
                     and attname = 'event_id'
              )
              and d.deptype in ('a', 'i')
       )
       or not exists (
           select 1
             from pg_catalog.pg_depend d
             join pg_catalog.pg_attrdef ad on ad.oid = d.objid
            where d.classid = 'pg_catalog.pg_attrdef'::regclass
              and d.refclassid = 'pg_catalog.pg_class'::regclass
              and d.refobjid = 'public.pu_native_shared_event_id_seq'::regclass
              and ad.adrelid = 'public.pu_native_shared_sequence_consumer'::regclass
       ) then
        raise exception 'native shared sequence rollback changed dependency boundaries';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid = 'public.trnm_economic_receipt_events_v1'::regclass
           and tgname = 'trg_cex_validate_trnm_economic_receipt_event_v1'
    ) then
        raise exception 'native shared-sequence rejection installed a trigger';
    end if;
end
$check$;
SQL
echo "    native event_id shared-sequence conflict passed"

echo "==> conflict: native initial receipt_id index belongs to another table"
new_case native_initial_index_wrong_table
native_initial_index_wrong_table_db="$CASE_DB"
run_db_file "$native_initial_index_wrong_table_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_initial_index_wrong_table_db" <<'SQL'
-- PostgreSQL index names are schema-scoped, not table-scoped.  A separately
-- committed/hand-authored rollout can therefore occupy the canonical name on
-- an unrelated table.  Match the native receipt_id attribute number and
-- predicate exactly so a name-only catalog lookup would incorrectly accept it.
create table public.pu_native_initial_index_decoy (
    dummy_1 text,
    dummy_2 text,
    dummy_3 text,
    dummy_4 text,
    receipt_id text not null,
    event_sequence bigint not null
);
create unique index idx_trnm_receipt_events_receipt_id_initial_v1
    on public.pu_native_initial_index_decoy(receipt_id)
    where event_sequence = 1;
SQL
expect_migration_failure "$native_initial_index_wrong_table_db" \
  "native initial receipt_id index wrong-table ownership" "$MIGRATION_86" \
  'TRNM native receipt initial receipt_id index is not canonical'
assert_db "$native_initial_index_wrong_table_db" <<'SQL'
do $check$
begin
    if to_regclass('public.trnm_economic_receipt_events_v1') is not null then
        raise exception 'wrong-table index rejection left a native event table behind';
    end if;
    if not exists (
        select 1
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_namespace index_namespace
            on index_namespace.oid = index_class.relnamespace
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
         where index_namespace.nspname = 'public'
           and index_class.relname = 'idx_trnm_receipt_events_receipt_id_initial_v1'
           and index_meta.indrelid = 'public.pu_native_initial_index_decoy'::regclass
           and index_meta.indisunique
           and index_meta.indpred is not null
    ) then
        raise exception 'wrong-table index rejection changed or removed the decoy index';
    end if;
    if to_regclass('public.pu_native_initial_index_decoy') is null then
        raise exception 'wrong-table index rejection removed the decoy table';
    end if;
end
$check$;
SQL
echo "    native wrong-table initial index conflict passed"

echo "==> conflict: native performance indexes belong to another table"
new_case native_performance_index_wrong_table
native_performance_index_wrong_table_db="$CASE_DB"
run_db_file "$native_performance_index_wrong_table_db" "$MIGRATION_85" >/dev/null
run_db_file "$native_performance_index_wrong_table_db" "$MIGRATION_86" >/dev/null
run_db_stdin "$native_performance_index_wrong_table_db" <<'SQL'
-- Re-run 0086 against a completed native table after moving both published
-- performance names to an unrelated table.  The decoys use the exact key and
-- order so only the index owner distinguishes them from the canonical objects.
drop index public.idx_trnm_receipt_events_latest_v1;
drop index public.idx_trnm_receipt_events_finalized_v1;
create table public.pu_native_performance_index_decoy (
    event_id bigint,
    intent_id text,
    event_sequence bigint,
    finalized_at timestamptz
);
create index idx_trnm_receipt_events_latest_v1
    on public.pu_native_performance_index_decoy(intent_id, event_sequence desc, event_id desc);
create index idx_trnm_receipt_events_finalized_v1
    on public.pu_native_performance_index_decoy(finalized_at desc, event_id desc);
SQL
expect_migration_failure "$native_performance_index_wrong_table_db" \
  "native performance indexes wrong-table ownership" "$MIGRATION_86" \
  'TRNM native receipt history has an incompatible performance index'
assert_db "$native_performance_index_wrong_table_db" <<'SQL'
do $check$
begin
    if to_regclass('public.trnm_economic_receipt_events_v1') is null
       or to_regclass('public.pu_native_performance_index_decoy') is null then
        raise exception 'native performance-index rejection removed fixture objects';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_index
         where indexrelid in (
             'public.idx_trnm_receipt_events_latest_v1'::regclass,
             'public.idx_trnm_receipt_events_finalized_v1'::regclass
         )
           and indrelid = 'public.trnm_economic_receipt_events_v1'::regclass
    ) then
        raise exception 'native performance-index rejection recreated indexes on history';
    end if;
    if (
        select count(*)
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_namespace index_namespace
            on index_namespace.oid = index_class.relnamespace
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
         where index_namespace.nspname = 'public'
           and index_class.relname in (
               'idx_trnm_receipt_events_latest_v1',
               'idx_trnm_receipt_events_finalized_v1'
           )
           and index_class.relkind = 'i'
           and index_meta.indrelid = 'public.pu_native_performance_index_decoy'::regclass
    ) <> 2 then
        raise exception 'native performance-index rejection changed the decoy catalog';
    end if;
end
$check$;
SQL
echo "    native wrong-table performance index conflict passed"

echo "==> conflict: native latest index uses a non-default operator class"
new_case native_performance_index_opclass
native_performance_index_opclass_db="$CASE_DB"
run_db_file "$native_performance_index_opclass_db" "$MIGRATION_85" >/dev/null
run_db_file "$native_performance_index_opclass_db" "$MIGRATION_86" >/dev/null
run_db_stdin "$native_performance_index_opclass_db" <<'SQL'
-- text_pattern_ops changes the operator class while leaving the visible
-- column/order list unchanged.  It is not the canonical equality/sort index
-- used by the native latest-receipt lookup.
drop index public.idx_trnm_receipt_events_latest_v1;
create index idx_trnm_receipt_events_latest_v1
    on public.trnm_economic_receipt_events_v1(
        intent_id text_pattern_ops,
        event_sequence desc,
        event_id desc
    );
SQL
expect_migration_failure "$native_performance_index_opclass_db" \
  "native latest index operator class" "$MIGRATION_86" \
  'TRNM native receipt history has an incompatible performance index idx_trnm_receipt_events_latest_v1'
assert_db "$native_performance_index_opclass_db" <<'SQL'
do $check$
begin
    if not exists (
        select 1
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
          join pg_catalog.pg_opclass opclass_meta
            on opclass_meta.oid = index_meta.indclass[0]
         where index_class.relname = 'idx_trnm_receipt_events_latest_v1'
           and index_meta.indrelid = 'public.trnm_economic_receipt_events_v1'::regclass
           and opclass_meta.opcname = 'text_pattern_ops'
    ) then
        raise exception 'native non-default-opclass fixture changed during rollback';
    end if;
end
$check$;
SQL
echo "    native latest non-default operator-class conflict passed"

echo "==> conflict: native identity keys have INCLUDE columns"
new_case native_key_index_include
native_key_index_include_db="$CASE_DB"
run_db_file "$native_key_index_include_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_key_index_include_db" <<'SQL'
-- INCLUDE columns are not part of a unique index's key.  A name/column-only
-- catalog test would therefore accept these indexes even though they are not
-- the canonical key shape required by the native history contract.
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity,
    intent_id text not null
        references public.trnm_economic_intents(intent_id)
        on delete restrict,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now()
);
create unique index pu_native_event_id_include_idx
    on public.trnm_economic_receipt_events_v1(event_id)
    include (receipt_id);
create unique index pu_native_intent_sequence_include_idx
    on public.trnm_economic_receipt_events_v1(intent_id, event_sequence)
    include (receipt_id);
SQL
expect_migration_failure "$native_key_index_include_db" \
  "native identity key INCLUDE shape" "$MIGRATION_86" \
  'TRNM native receipt event_id key is missing a valid unique index'
assert_db "$native_key_index_include_db" <<'SQL'
do $check$
begin
    if (
        select count(*)
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
         where index_class.relname in (
                   'pu_native_event_id_include_idx',
                   'pu_native_intent_sequence_include_idx'
               )
           and index_meta.indrelid = 'public.trnm_economic_receipt_events_v1'::regclass
           and index_meta.indnatts > index_meta.indnkeyatts
    ) <> 2 then
        raise exception 'native INCLUDE-index fixture changed during rollback';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid = 'public.trnm_economic_receipt_events_v1'::regclass
           and tgname = 'trg_cex_validate_trnm_economic_receipt_event_v1'
    ) then
        raise exception 'rejected native INCLUDE key migration installed a trigger';
    end if;
end
$check$;
SQL
echo "    native identity key INCLUDE conflict passed"

echo "==> conflict: native provenance source index has INCLUDE columns"
new_case native_provenance_source_index_include
native_provenance_source_index_include_db="$CASE_DB"
run_db_file "$native_provenance_source_index_include_db" "$MIGRATION_85" >/dev/null
run_db_file "$native_provenance_source_index_include_db" "$MIGRATION_86" >/dev/null
run_db_stdin "$native_provenance_source_index_include_db" <<'SQL'
-- The provenance source uniqueness index is a published one-column key.  An
-- INCLUDE payload must not be mistaken for that exact catalog shape.
drop index public.uq_trnm_receipt_legacy_fallback_provenance_legacy_receipt_id_v1;
create unique index uq_trnm_receipt_legacy_fallback_provenance_legacy_receipt_id_v1
    on public.trnm_economic_receipt_legacy_fallback_provenance_v1(legacy_receipt_id)
    include (intent_hash);
SQL
expect_migration_failure "$native_provenance_source_index_include_db" \
  "native provenance source INCLUDE shape" "$MIGRATION_86" \
  'TRNM legacy fallback provenance source index is not canonical'
assert_db "$native_provenance_source_index_include_db" <<'SQL'
do $check$
begin
    if not exists (
        select 1
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
         where index_class.relname =
                   'uq_trnm_receipt_legacy_fallback_provenance_legacy_receipt_id_v1'
           and index_meta.indrelid =
                   'public.trnm_economic_receipt_legacy_fallback_provenance_v1'::regclass
           and index_meta.indnatts > index_meta.indnkeyatts
    ) then
        raise exception 'native provenance INCLUDE index changed during rollback';
    end if;
    if to_regclass('public.trnm_economic_receipt_legacy_fallback_provenance_v1') is null then
        raise exception 'native provenance INCLUDE rejection removed the ledger';
    end if;
end
$check$;
SQL
echo "    native provenance source INCLUDE conflict passed"

echo "==> conflict: native history amount column type drift"
new_case native_history_amount_type
native_history_amount_type_db="$CASE_DB"
run_db_file "$native_history_amount_type_db" "$MIGRATION_85" >/dev/null
run_db_file "$native_history_amount_type_db" "$MIGRATION_86" >/dev/null
run_db_stdin "$native_history_amount_type_db" <<'SQL'
-- A completed rollout can still be followed by an independently committed
-- catalog edit.  Re-running 0086 must reject an integer amount rather than
-- treating the same-named column as the bigint authority.
alter table public.trnm_economic_receipt_events_v1
    alter column amount_credits type integer
    using amount_credits::integer;
SQL
expect_migration_failure "$native_history_amount_type_db" \
  "native history amount column type drift" "$MIGRATION_86" \
  'TRNM native receipt event column amount_credits has incompatible type'
assert_db "$native_history_amount_type_db" <<'SQL'
do $check$
begin
    if (select udt_name
          from information_schema.columns
         where table_schema = 'public'
           and table_name = 'trnm_economic_receipt_events_v1'
           and column_name = 'amount_credits') is distinct from 'int4' then
        raise exception 'native amount type drift fixture changed during rollback';
    end if;
end
$check$;
SQL
echo "    native amount type drift was rejected and rolled back"

echo "==> conflict: native history timestamp column type drift"
new_case native_history_timestamp_type
native_history_timestamp_type_db="$CASE_DB"
run_db_file "$native_history_timestamp_type_db" "$MIGRATION_85" >/dev/null
run_db_file "$native_history_timestamp_type_db" "$MIGRATION_86" >/dev/null
run_db_stdin "$native_history_timestamp_type_db" <<'SQL'
alter table public.trnm_economic_receipt_events_v1
    alter column finalized_at type timestamp without time zone
    using finalized_at at time zone 'UTC';
SQL
expect_migration_failure "$native_history_timestamp_type_db" \
  "native history timestamp column type drift" "$MIGRATION_86" \
  'TRNM native receipt event column finalized_at has incompatible type'
assert_db "$native_history_timestamp_type_db" <<'SQL'
do $check$
begin
    if (select udt_name
          from information_schema.columns
         where table_schema = 'public'
           and table_name = 'trnm_economic_receipt_events_v1'
           and column_name = 'finalized_at') is distinct from 'timestamp' then
        raise exception 'native timestamp type drift fixture changed during rollback';
    end if;
end
$check$;
SQL
echo "    native timestamp type drift was rejected and rolled back"

echo "==> repair: native history omitted-column defaults"
new_case native_history_default_repair
native_history_default_repair_db="$CASE_DB"
run_db_file "$native_history_default_repair_db" "$MIGRATION_85" >/dev/null
run_db_file "$native_history_default_repair_db" "$MIGRATION_86" >/dev/null
run_db_stdin "$native_history_default_repair_db" <<'SQL'
alter table public.trnm_economic_receipt_events_v1
    alter column amount_credits drop default,
    alter column event_kind drop default,
    alter column created_at drop default;
SQL
run_db_file "$native_history_default_repair_db" "$MIGRATION_86" >/dev/null
assert_db "$native_history_default_repair_db" <<'SQL'
do $check$
declare
    amount_default text;
    event_kind_default text;
    created_at_default text;
begin
    select max(column_default) filter (where column_name = 'amount_credits'),
           max(column_default) filter (where column_name = 'event_kind'),
           max(column_default) filter (where column_name = 'created_at')
      into amount_default, event_kind_default, created_at_default
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'trnm_economic_receipt_events_v1';
    if amount_default is distinct from '0'
       or event_kind_default is distinct from '''initial''::text'
       or created_at_default is distinct from 'now()' then
        raise exception 'native omitted-column defaults were not repaired';
    end if;
end
$check$;
SQL
echo "    native omitted-column defaults were repaired"

echo "==> repair: native same-named weak shape constraint"
new_case native_shape_guard
native_shape_guard_db="$CASE_DB"
run_db_file "$native_shape_guard_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_shape_guard_db" <<'SQL'
-- Keep the canonical fragments in an unreachable OR branch.  A name/validation
-- check must not mistake this deliberately weak object for the real shape
-- authority, especially on an otherwise empty partial table.
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    intent_id text not null references public.trnm_economic_intents(intent_id),
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence),
    constraint trnm_receipt_events_native_shape_v1 check (
        event_id > 0
        or event_id <= 0
        or (
            event_sequence > 0
            and btrim(intent_id) <> ''
            and intent_hash ~ '^[0-9a-f]{64}$'
            and btrim(receipt_id) <> ''
            and btrim(protocol_version) <> ''
            and btrim(idempotency_scope) <> ''
            and btrim(idempotency_key) <> ''
            and progression_class in ('progression_allowed', 'recoverable_hold', 'terminal_skip', 'hard_fail')
            and length(btrim(status)) between 1 and 128
            and amount_credits >= 0
            and event_kind in ('initial', 'recoverable_hold_retry', 'progression')
            and jsonb_typeof(receipt_json) = 'object'
            and receipt_hash ~ '^[0-9a-f]{64}$'
        )
    )
);
SQL
run_db_file "$native_shape_guard_db" "$MIGRATION_86" >/dev/null
assert_db "$native_shape_guard_db" <<'SQL'
do $check$
declare
    expression text;
    compact_expression text;
begin
    select pg_get_constraintdef(c.oid)
      into expression
      from pg_catalog.pg_constraint c
     where c.conrelid = 'public.trnm_economic_receipt_events_v1'::regclass
       and c.conname = 'trnm_receipt_events_native_shape_v1'
       and c.contype = 'c';
    compact_expression := regexp_replace(lower(coalesce(expression, '')), '[[:space:]]+', '', 'g');
    if expression is null
       or position('event_id>0' in compact_expression) = 0
       or position('event_id>0)or' in compact_expression) > 0 then
        raise exception 'native weak shape constraint was not replaced by the canonical predicate';
    end if;
end
$check$;
alter table public.trnm_economic_receipt_events_v1 disable trigger all;
do $insert_check$
begin
    begin
        insert into public.trnm_economic_receipt_events_v1 (
            event_id, intent_id, event_sequence, intent_hash, receipt_id,
            protocol_version, idempotency_scope, idempotency_key,
            progression_class, status, amount_credits, receipt_json,
            receipt_hash, event_kind, finalized_at
        )
        select 0, intent_id, 99, intent_hash, receipt_id || '-shape',
               protocol_version, idempotency_scope, idempotency_key,
               progression_class, status, amount_credits, receipt_json,
               receipt_hash, 'progression', finalized_at
          from public.trnm_economic_receipt_events_v1
         limit 1;
        raise exception 'native canonical shape constraint accepted event_id zero';
    exception when check_violation then
        null;
    end;
end
$insert_check$;
SQL
echo "    native weak shape constraint was replaced and enforced"

echo "==> conflict: native pre-existing event sequence gap"
new_case native_sequence_gap
native_sequence_gap_db="$CASE_DB"
run_db_file "$native_sequence_gap_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_sequence_gap_db" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-native-sequence-gap', 'term_exchange_protocol_v1', 'pu-scope',
    'native-sequence-gap', repeat('f', 64),
    '{"intent_id":"pu-intent-native-sequence-gap","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-sequence-gap"}'::jsonb,
    'accepted'
);
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    intent_id text not null,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
with first_payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-sequence-gap',
        'receipt_id', 'pu-native-sequence-gap-receipt',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'held_review',
        'progression_class', 'recoverable_hold',
        'evidence', jsonb_build_object('payload_hash', repeat('f', 64), 'amount_credits', 0)
    ) as receipt_json
), second_payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-sequence-gap',
        'receipt_id', 'pu-native-sequence-gap-receipt',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'evidence', jsonb_build_object('payload_hash', repeat('f', 64), 'amount_credits', 0)
    ) as receipt_json
)
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
)
select 'pu-intent-native-sequence-gap', 1, repeat('f', 64),
       'pu-native-sequence-gap-receipt', 'term_exchange_protocol_v1',
       'pu-scope', 'native-sequence-gap', 'recoverable_hold', 'held_review',
       0, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
       'initial', '2026-01-01T00:10:00Z'
  from first_payload;
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
)
with second_payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-sequence-gap',
        'receipt_id', 'pu-native-sequence-gap-receipt',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'evidence', jsonb_build_object('payload_hash', repeat('f', 64), 'amount_credits', 0)
    ) as receipt_json
)
select 'pu-intent-native-sequence-gap', 3, repeat('f', 64),
       'pu-native-sequence-gap-receipt', 'term_exchange_protocol_v1',
       'pu-scope', 'native-sequence-gap', 'progression_allowed', 'settled',
       0, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
       'recoverable_hold_retry', '2026-01-01T00:10:01Z'
  from second_payload;
SQL
expect_migration_failure "$native_sequence_gap_db" "native event sequence gap" "$MIGRATION_86" \
  'TRNM native receipt history failed pre-existing-row validation: event sequence is not contiguous'
assert_db "$native_sequence_gap_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 2 then
        raise exception 'sequence-gap conflict changed native history during rollback';
    end if;
end
$check$;
SQL
echo "    native event sequence-gap conflict passed"

echo "==> conflict: malformed native backend JSON field"
new_case native_bad_backend
native_bad_backend_db="$CASE_DB"
run_db_file "$native_bad_backend_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_bad_backend_db" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-native-bad-backend', 'term_exchange_protocol_v1', 'pu-scope',
    'native-bad-backend', repeat('f', 64),
    '{"intent_id":"pu-intent-native-bad-backend","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-native-bad-backend"}'::jsonb,
    'accepted'
);
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    intent_id text not null,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
with payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-bad-backend',
        'receipt_id', 'pu-native-bad-backend-receipt',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'backend_kind', 'not-a-backend',
        'evidence', jsonb_build_object('payload_hash', repeat('f', 64), 'amount_credits', 0)
    ) as receipt_json
)
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
)
select 'pu-intent-native-bad-backend', 1, repeat('f', 64),
       'pu-native-bad-backend-receipt', 'term_exchange_protocol_v1',
       'pu-scope', 'native-bad-backend', 'progression_allowed', 'settled',
       0, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
       'initial', '2026-01-01T00:11:00Z'
  from payload;
SQL
expect_migration_failure "$native_bad_backend_db" "malformed native backend JSON" "$MIGRATION_86" \
  'TRNM native receipt history failed pre-existing-row validation: backend_kind is unknown'
assert_db "$native_bad_backend_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 1 then
        raise exception 'backend conflict changed native history during rollback';
    end if;
end
$check$;
SQL
echo "    malformed native backend conflict passed"

echo "==> conflict: malformed pre-existing native intent JSON term_id type"
new_case native_bad_intent_json
native_bad_intent_json_db="$CASE_DB"
run_db_file "$native_bad_intent_json_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$native_bad_intent_json_db" <<'SQL'
-- The INSERT trigger catches this shape for new rows, but an event committed
-- before 0086's trigger existed would otherwise be accepted by the
-- pre-existing-row audit because ->> stringifies the numeric term_id.
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-native-bad-intent-json', 'term_exchange_protocol_v1', 'pu-scope',
    'native-bad-intent-json', repeat('f', 64),
    '{"intent_id":"pu-intent-native-bad-intent-json","protocol_version":"term_exchange_protocol_v1","term_id":123}'::jsonb,
    'accepted'
);
create table public.trnm_economic_receipt_events_v1 (
    event_id bigint generated by default as identity primary key,
    intent_id text not null,
    event_sequence bigint not null,
    intent_hash text not null,
    receipt_id text not null,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    amount_credits bigint not null default 0,
    receipt_json jsonb not null,
    receipt_hash text not null,
    event_kind text not null default 'initial',
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    unique (intent_id, event_sequence)
);
with payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-bad-intent-json',
        'receipt_id', 'pu-native-bad-intent-json-receipt',
        'protocol_version', 'term_exchange_protocol_v1',
        -- Keep the receipt text self-consistent with ->>; only the intent
        -- authority's JSON type is malformed in this fixture.
        'term_id', '123',
        'backend_id', 'cex-settlement-backend',
        'backend_kind', 'cex',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:12:00Z')::bigint,
        'evidence', jsonb_build_object(
            'payload_hash', repeat('f', 64),
            'amount_credits', 0
        )
    ) as receipt_json
)
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
)
select 'pu-intent-native-bad-intent-json', 1, repeat('f', 64),
       'pu-native-bad-intent-json-receipt', 'term_exchange_protocol_v1',
       'pu-scope', 'native-bad-intent-json', 'progression_allowed', 'settled',
       0, receipt_json, encode(digest(receipt_json::text, 'sha256'), 'hex'),
       'initial', '2026-01-01T00:12:00Z'
  from payload;
SQL
expect_migration_failure "$native_bad_intent_json_db" "malformed native intent JSON term_id" "$MIGRATION_86" \
  'TRNM native receipt history failed pre-existing-row validation: immutable intent term_id is invalid'
assert_db "$native_bad_intent_json_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 1 then
        raise exception 'malformed intent JSON conflict changed native history during rollback';
    end if;
end
$check$;
SQL
echo "    malformed native intent JSON conflict passed"

echo "==> conflict: native receipt_id fork in partial history"
new_case native_id
native_id_db="$CASE_DB"
run_db_file "$native_id_db" "$MIGRATION_85" >/dev/null
run_db_file "$native_id_db" "$MIGRATION_86" >/dev/null
run_db_stdin "$native_id_db" <<'SQL'
alter table public.trnm_economic_receipt_events_v1
    disable trigger trg_cex_validate_trnm_economic_receipt_event_v1;
with payload as (
    select jsonb_build_object(
        'intent_id', 'pu-intent-native-happy',
        'receipt_id', 'pu-receipt-native-fork',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'held_review',
        'progression_class', 'recoverable_hold',
        'evidence', jsonb_build_object(
            'payload_hash', repeat('a', 64),
            'amount_credits', 37
        )
    ) as receipt_json
)
insert into public.trnm_economic_receipt_events_v1 (
    intent_id, event_sequence, intent_hash, receipt_id, protocol_version,
    idempotency_scope, idempotency_key, progression_class, status,
    amount_credits, receipt_json, receipt_hash, event_kind, finalized_at
)
select 'pu-intent-native-happy', 2, repeat('a', 64), 'pu-receipt-native-fork',
       'term_exchange_protocol_v1', 'pu-scope', 'native-happy',
       'recoverable_hold', 'held_review', 37, receipt_json,
       encode(digest(receipt_json::text, 'sha256'), 'hex'),
       'recoverable_hold_retry', '2026-01-01T00:00:03Z'
  from payload;
alter table public.trnm_economic_receipt_events_v1
    enable always trigger trg_cex_validate_trnm_economic_receipt_event_v1;
SQL
expect_migration_failure "$native_id_db" "native receipt_id fork" "$MIGRATION_86" \
  'TRNM native receipt history contains multiple receipt_ids'
assert_db "$native_id_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.trnm_economic_receipt_events_v1) <> 5 then
        raise exception 'native receipt_id conflict changed partial history during rollback';
    end if;
end
$check$;
SQL
echo "    native receipt_id conflict passed"

echo "==> conflict: immutable amount versus normalized projection"
new_case amount
amount_db="$CASE_DB"
run_db_stdin "$amount_db" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    'pu-intent-amount-conflict', 'term_exchange_protocol_v1', 'pu-scope',
    'amount-conflict', repeat('e', 64),
    '{"intent_id":"pu-intent-amount-conflict","protocol_version":"term_exchange_protocol_v1","term_id":"pu-term-amount-conflict","amount_credits":41}'::jsonb,
    'accepted'
);
insert into public.league_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    finalized_at
) values (
    'pu-league-amount-conflict', 'term_exchange_protocol_v1',
    'pu-intent-amount-conflict', 'pu-term', 'pu-backend', 'cex', 'settled',
    'progression_allowed', 'pu-settle-amount-conflict', 'pu-ledger-amount-conflict',
    null, '2026-01-01T00:03:00Z'
);
SQL
run_db_file "$amount_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$amount_db" <<'SQL'
alter table public.league_term_exchange_receipts disable trigger all;
update public.league_term_exchange_receipts
   set amount_credits = 99
 where receipt_id = 'pu-league-amount-conflict';
SQL
expect_migration_failure "$amount_db" "immutable amount/projection conflict" "$MIGRATION_87" \
  'normalized receipt amount does not match immutable intent amount'
assert_db "$amount_db" <<'SQL'
do $check$
begin
    if to_regclass('public.league_term_exchange_receipt_events_v1') is not null then
        raise exception 'rejected amount conflict left term history behind';
    end if;
end
$check$;
SQL
echo "    immutable amount conflict passed"

echo "==> conflict: term receipt history receipt_id fork"
new_case history
history_db="$CASE_DB"
run_db_file "$history_db" "$MIGRATION_85" >/dev/null
run_db_file "$history_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$history_db" <<'SQL'
alter table public.world_term_exchange_receipt_events_v1
    disable trigger trg_cex_validate_world_term_exchange_receipt_event_v1;
with predecessor as (
    select receipt_hash
      from public.world_term_exchange_receipt_events_v1
     where receipt_id = 'pu-world-unknown'
       and event_sequence = 1
), payload as (
    select jsonb_build_object(
        'protocol_version', 'term_exchange_protocol_v1',
        'receipt_id', 'pu-world-history-fork',
        'intent_id', 'pu-intent-world-unknown',
        'term_id', 'pu-term',
        'backend_id', 'pu-backend',
        'backend_kind', 'cex',
        'status', 'held_review',
        'progression_class', 'recoverable_hold',
        'settlement_reference', null,
        'ledger_entry_id', null,
        'reason', null,
        'amount_credits', null,
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:03:01Z')::bigint
    ) as receipt_json,
    predecessor.receipt_hash
      from predecessor
)
insert into public.world_term_exchange_receipt_events_v1 (
    receipt_id, event_sequence, event_kind, previous_receipt_hash,
    protocol_version, intent_id, term_id, backend_id, backend_kind, status,
    progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at, receipt_json, receipt_hash
)
select 'pu-world-history-fork', 2, 'transition', receipt_hash,
       'term_exchange_protocol_v1', 'pu-intent-world-unknown', 'pu-term',
       'pu-backend', 'cex', 'held_review', 'recoverable_hold', null, null,
       null, null, '2026-01-01T00:03:01Z', receipt_json,
       'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
  from payload;
alter table public.world_term_exchange_receipt_events_v1
    enable always trigger trg_cex_validate_world_term_exchange_receipt_event_v1;
SQL
expect_migration_failure "$history_db" "world receipt history receipt_id fork" "$MIGRATION_87" \
  'normalized world receipt history contains multiple receipt_ids'
assert_db "$history_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.world_term_exchange_receipt_events_v1) <> 3 then
        raise exception 'world history receipt_id conflict changed partial history during rollback';
    end if;
end
$check$;
SQL
echo "    term history conflict passed"

echo "==> pre-existing valid term history (projection authority and NULL amount)"
new_case legacy_history
legacy_history_db="$CASE_DB"
run_db_file "$legacy_history_db" "$MIGRATION_85" >/dev/null
create_preexisting_league_history_table "$legacy_history_db"
run_db_stdin "$legacy_history_db" <<'SQL'
with payload as (
    select jsonb_build_object(
        'protocol_version', 'term_exchange_protocol_v1',
        'receipt_id', 'pu-league-unknown',
        'intent_id', 'pu-intent-league-unknown',
        'term_id', 'pu-term',
        'backend_id', 'pu-backend',
        'backend_kind', 'cex',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'settlement_reference', 'pu-settle-league-unknown',
        'ledger_entry_id', 'pu-ledger-league-unknown',
        'reason', null,
        'amount_credits', null,
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:01:01Z')::bigint
    ) as receipt_json
)
insert into public.league_term_exchange_receipt_events_v1 (
    receipt_id, event_sequence, event_kind, previous_receipt_hash,
    protocol_version, intent_id, term_id, backend_id, backend_kind, status,
    progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at, receipt_json, receipt_hash
)
select 'pu-league-unknown', 1, 'initial', null,
       'term_exchange_protocol_v1', 'pu-intent-league-unknown', 'pu-term',
       'pu-backend', 'cex', 'settled', 'progression_allowed',
       'pu-settle-league-unknown', 'pu-ledger-league-unknown', null,
       null, '2026-01-01T00:01:01Z', receipt_json,
       'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
  from payload;
SQL
run_db_file "$legacy_history_db" "$MIGRATION_87" >/dev/null
# A retry must validate and preserve the pre-existing row rather than treating
# it as a fresh projection seed or fabricating an amount for the NULL legacy
# value.
run_db_file "$legacy_history_db" "$MIGRATION_87" >/dev/null
assert_db "$legacy_history_db" <<'SQL'
do $check$
declare
    amount_value bigint;
    event_kind_value text;
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 2
       or (select count(*) from public.world_term_exchange_receipt_events_v1) <> 2 then
        raise exception 'valid pre-existing history fixture produced the wrong backfill count';
    end if;
    select amount_credits, event_kind
      into amount_value, event_kind_value
      from public.league_term_exchange_receipt_events_v1
     where receipt_id = 'pu-league-unknown' and event_sequence = 1;
    if amount_value is not null or event_kind_value is distinct from 'initial' then
        raise exception 'valid legacy NULL amount/history seed was rewritten';
    end if;
    if not exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
           and tgname = 'trg_cex_validate_league_term_exchange_receipt_event_v1'
    ) then
        raise exception 'pre-existing history validation trigger was not installed';
    end if;
end
$check$;
SQL
echo "    valid pre-existing history passed"

echo "==> partial history schema repair (event_id/default/unique keys)"
new_case partial_history_schema
partial_history_schema_db="$CASE_DB"
run_db_file "$partial_history_schema_db" "$MIGRATION_85" >/dev/null
create_partial_term_history_tables "$partial_history_schema_db"
run_db_file "$partial_history_schema_db" "$MIGRATION_87" >/dev/null
assert_db "$partial_history_schema_db" <<'SQL'
do $schema_check$
declare
    column_default_value text;
    nullable_value text;
    world_identity text;
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 2
       or (select count(*) from public.world_term_exchange_receipt_events_v1) <> 2 then
        raise exception 'partial history schema repair did not backfill both lanes';
    end if;

    select column_default, is_nullable
      into column_default_value, nullable_value
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'league_term_exchange_receipt_events_v1'
       and column_name = 'event_id';
    if column_default_value is null or nullable_value <> 'NO' then
        raise exception 'plain League event_id was not repaired to a generated non-null key';
    end if;

    select column_default, is_nullable
      into column_default_value, nullable_value
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'world_term_exchange_receipt_events_v1'
       and column_name = 'event_id';
    select a.attidentity::text
      into world_identity
      from pg_catalog.pg_attribute a
     where a.attrelid = 'public.world_term_exchange_receipt_events_v1'::regclass
       and a.attname = 'event_id';
    if world_identity is distinct from 'd' or nullable_value <> 'NO' then
        raise exception 'World event_id identity generation was not restored';
    end if;

    if not exists (
        select 1
          from pg_catalog.pg_constraint c
         where c.conrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
           and c.contype = 'p'
    ) or not exists (
        select 1
          from pg_catalog.pg_constraint c
         where c.conrelid = 'public.world_term_exchange_receipt_events_v1'::regclass
           and c.contype = 'p'
    ) then
        raise exception 'partial history event_id primary keys were not restored';
    end if;
    if to_regclass('public.uq_league_term_exchange_receipt_events_v1_receipt_sequence_v1') is null
       or to_regclass('public.uq_world_term_exchange_receipt_events_v1_receipt_sequence_v1') is null then
        raise exception 'partial history receipt/event sequence uniqueness was not restored';
    end if;
end
$schema_check$;
SQL
echo "    partial history schema repair passed"

echo "==> repair: normalized same-named weak shape constraint"
new_case normalized_shape_guard
normalized_shape_guard_db="$CASE_DB"
run_db_file "$normalized_shape_guard_db" "$MIGRATION_85" >/dev/null
create_partial_term_history_tables "$normalized_shape_guard_db"
run_db_stdin "$normalized_shape_guard_db" <<'SQL'
-- Keep the canonical fragments in an unreachable OR branch.  0087 must replace
-- this object rather than trusting its name/validation bit on an empty partial
-- table.
alter table public.league_term_exchange_receipt_events_v1
    add constraint league_term_exchange_receipt_events_v1_shape_v1 check (
        event_id > 0
        or event_id <= 0
        or (
            event_sequence > 0
            and (previous_receipt_hash is null or previous_receipt_hash ~ '^sha256:[0-9a-f]{64}$')
            and receipt_hash ~ '^sha256:[0-9a-f]{64}$'
            and length(btrim(status)) between 1 and 128
            and progression_class in ('progression_allowed', 'recoverable_hold', 'terminal_skip', 'hard_fail')
            and event_kind in ('initial', 'backfill', 'hold', 'final', 'transition')
            and (amount_credits is null or amount_credits >= 0)
            and jsonb_typeof(receipt_json) = 'object'
        )
    );
SQL
run_db_file "$normalized_shape_guard_db" "$MIGRATION_87" >/dev/null
assert_db "$normalized_shape_guard_db" <<'SQL'
do $check$
declare
    expression text;
    compact_expression text;
begin
    select pg_get_constraintdef(c.oid)
      into expression
      from pg_catalog.pg_constraint c
     where c.conrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
       and c.conname = 'league_term_exchange_receipt_events_v1_shape_v1'
       and c.contype = 'c';
    compact_expression := regexp_replace(lower(coalesce(expression, '')), '[[:space:]]+', '', 'g');
    if expression is null
       or position('event_id>0' in compact_expression) = 0
       or position('event_id>0)or' in compact_expression) > 0 then
        raise exception 'normalized weak shape constraint was not replaced by the canonical predicate';
    end if;
end
$check$;
SQL
echo "    normalized weak shape constraint was replaced and enforced"

echo "==> conflict: normalized initial intent index belongs to another table"
new_case normalized_initial_index_wrong_table
normalized_initial_index_wrong_table_db="$CASE_DB"
run_db_file "$normalized_initial_index_wrong_table_db" "$MIGRATION_85" >/dev/null
create_partial_term_history_tables "$normalized_initial_index_wrong_table_db"
run_db_stdin "$normalized_initial_index_wrong_table_db" <<'SQL'
-- The canonical initial-intent index names are schema-scoped.  Keep a valid
-- differently named key on each history table so the migration cannot simply
-- create a replacement after discovering the decoy.  The decoy mirrors the
-- canonical key's attribute number, uniqueness, and predicate, differing only
-- in its owning table; a name-only catalog guard would accept it.
create unique index pu_league_initial_intent_valid_idx
    on public.league_term_exchange_receipt_events_v1(intent_id)
    where event_sequence = 1;
create unique index pu_world_initial_intent_valid_idx
    on public.world_term_exchange_receipt_events_v1(intent_id)
    where event_sequence = 1;
create table public.pu_normalized_initial_index_decoy (
    dummy_1 text,
    dummy_2 text,
    dummy_3 text,
    dummy_4 text,
    dummy_5 text,
    dummy_6 text,
    intent_id text not null,
    event_sequence bigint not null
);
create unique index uq_league_term_exchange_receipt_events_initial_intent_v1
    on public.pu_normalized_initial_index_decoy(intent_id)
    where event_sequence = 1;
SQL
expect_migration_failure "$normalized_initial_index_wrong_table_db" \
  "normalized initial intent index wrong-table ownership" "$MIGRATION_87" \
  'normalized league receipt history has an incompatible initial intent key'
assert_db "$normalized_initial_index_wrong_table_db" <<'SQL'
do $check$
begin
    if to_regclass('public.league_term_exchange_receipt_events_v1') is null
       or to_regclass('public.world_term_exchange_receipt_events_v1') is null
       or to_regclass('public.pu_normalized_initial_index_decoy') is null then
        raise exception 'wrong-table initial-intent rejection removed partial fixture objects';
    end if;
    if not exists (
        select 1
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_namespace index_namespace
            on index_namespace.oid = index_class.relnamespace
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
         where index_namespace.nspname = 'public'
           and index_class.relname = 'uq_league_term_exchange_receipt_events_initial_intent_v1'
           and index_meta.indrelid = 'public.pu_normalized_initial_index_decoy'::regclass
           and index_meta.indisunique
           and index_meta.indpred is not null
    ) then
        raise exception 'wrong-table initial-intent rejection changed or removed the decoy index';
    end if;
    if not exists (
        select 1
          from pg_catalog.pg_index index_meta
         where index_meta.indexrelid = 'public.pu_league_initial_intent_valid_idx'::regclass
           and index_meta.indrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
    ) or not exists (
        select 1
          from pg_catalog.pg_index index_meta
         where index_meta.indexrelid = 'public.pu_world_initial_intent_valid_idx'::regclass
           and index_meta.indrelid = 'public.world_term_exchange_receipt_events_v1'::regclass
    ) then
        raise exception 'wrong-table initial-intent rejection changed valid history indexes';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid in (
             'public.league_term_exchange_receipt_events_v1'::regclass,
             'public.world_term_exchange_receipt_events_v1'::regclass
         )
           and tgname like 'trg_cex_validate_%term_exchange_receipt_event_v1'
    ) then
        raise exception 'wrong-table initial-intent rejection installed history triggers';
    end if;
end
$check$;
SQL
echo "    normalized wrong-table initial-intent conflict passed"

echo "==> conflict: normalized initial intent canonical name belongs to a table"
new_case normalized_initial_index_table_collision
normalized_initial_index_table_collision_db="$CASE_DB"
run_db_file "$normalized_initial_index_table_collision_db" "$MIGRATION_85" >/dev/null
create_partial_term_history_tables "$normalized_initial_index_table_collision_db"
run_db_stdin "$normalized_initial_index_table_collision_db" <<'SQL'
-- A table can occupy an index's schema-scoped canonical name.  Keep a valid
-- alternate key so a guard that filters out non-index relations would proceed
-- after CREATE INDEX IF NOT EXISTS silently skipped the canonical index.
create unique index pu_league_initial_intent_table_collision_valid_idx
    on public.league_term_exchange_receipt_events_v1(intent_id)
    where event_sequence = 1;
create unique index pu_world_initial_intent_table_collision_valid_idx
    on public.world_term_exchange_receipt_events_v1(intent_id)
    where event_sequence = 1;
create table public.uq_league_term_exchange_receipt_events_initial_intent_v1 (
    marker text
);
SQL
expect_migration_failure "$normalized_initial_index_table_collision_db" \
  "normalized initial intent canonical-name table collision" "$MIGRATION_87" \
  'normalized league receipt history has an incompatible initial intent key'
assert_db "$normalized_initial_index_table_collision_db" <<'SQL'
do $check$
begin
    if to_regclass('public.league_term_exchange_receipt_events_v1') is null
       or to_regclass('public.world_term_exchange_receipt_events_v1') is null
       or to_regclass('public.uq_league_term_exchange_receipt_events_initial_intent_v1') is null then
        raise exception 'table-collision rejection removed partial fixture objects';
    end if;
    if not exists (
        select 1
          from pg_catalog.pg_class relation_class
          join pg_catalog.pg_namespace relation_namespace
            on relation_namespace.oid = relation_class.relnamespace
         where relation_namespace.nspname = 'public'
           and relation_class.relname = 'uq_league_term_exchange_receipt_events_initial_intent_v1'
           and relation_class.relkind = 'r'
    ) then
        raise exception 'table-collision rejection changed the canonical-name table';
    end if;
    if not exists (
        select 1
          from pg_catalog.pg_index index_meta
         where index_meta.indexrelid = 'public.pu_league_initial_intent_table_collision_valid_idx'::regclass
           and index_meta.indrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
    ) or not exists (
        select 1
          from pg_catalog.pg_index index_meta
         where index_meta.indexrelid = 'public.pu_world_initial_intent_table_collision_valid_idx'::regclass
           and index_meta.indrelid = 'public.world_term_exchange_receipt_events_v1'::regclass
    ) then
        raise exception 'table-collision rejection changed valid history indexes';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid in (
             'public.league_term_exchange_receipt_events_v1'::regclass,
             'public.world_term_exchange_receipt_events_v1'::regclass
         )
           and tgname like 'trg_cex_validate_%term_exchange_receipt_event_v1'
    ) then
        raise exception 'table-collision rejection installed history triggers';
    end if;
end
$check$;
SQL
echo "    normalized canonical-name table collision passed"

echo "==> conflict: normalized performance indexes belong to another table"
new_case normalized_performance_index_wrong_table
normalized_performance_index_wrong_table_db="$CASE_DB"
run_db_file "$normalized_performance_index_wrong_table_db" "$MIGRATION_85" >/dev/null
create_partial_term_history_tables "$normalized_performance_index_wrong_table_db"
run_db_stdin "$normalized_performance_index_wrong_table_db" <<'SQL'
-- All six canonical performance names are schema-scoped.  Recreate their
-- exact key/order shapes on an unrelated table so a name-only guard would
-- accept every object while leaving the history tables without those indexes.
create table public.pu_normalized_performance_index_decoy (
    event_id bigint,
    receipt_id text,
    event_sequence bigint,
    finalized_at timestamptz,
    intent_id text
);
create index idx_league_term_exchange_receipt_events_latest_v1
    on public.pu_normalized_performance_index_decoy(receipt_id, event_sequence desc, event_id desc);
create index idx_league_term_exchange_receipt_events_finalized_v1
    on public.pu_normalized_performance_index_decoy(finalized_at desc, event_id desc);
create index idx_league_term_exchange_receipt_events_intent_v1
    on public.pu_normalized_performance_index_decoy(intent_id, finalized_at desc, event_id desc);
create index idx_world_term_exchange_receipt_events_latest_v1
    on public.pu_normalized_performance_index_decoy(receipt_id, event_sequence desc, event_id desc);
create index idx_world_term_exchange_receipt_events_finalized_v1
    on public.pu_normalized_performance_index_decoy(finalized_at desc, event_id desc);
create index idx_world_term_exchange_receipt_events_intent_v1
    on public.pu_normalized_performance_index_decoy(intent_id, finalized_at desc, event_id desc);
SQL
expect_migration_failure "$normalized_performance_index_wrong_table_db" \
  "normalized performance indexes wrong-table ownership" "$MIGRATION_87" \
  'normalized league receipt history has an incompatible performance index'
assert_db "$normalized_performance_index_wrong_table_db" <<'SQL'
do $check$
declare
    decoy_oid oid := 'public.pu_normalized_performance_index_decoy'::regclass;
    decoy_owner oid;
    canonical_names text[] := array[
        'idx_league_term_exchange_receipt_events_latest_v1',
        'idx_league_term_exchange_receipt_events_finalized_v1',
        'idx_league_term_exchange_receipt_events_intent_v1',
        'idx_world_term_exchange_receipt_events_latest_v1',
        'idx_world_term_exchange_receipt_events_finalized_v1',
        'idx_world_term_exchange_receipt_events_intent_v1'
    ];
begin
    select relowner into decoy_owner from pg_catalog.pg_class where oid = decoy_oid;
    if decoy_oid is null
       or to_regclass('public.league_term_exchange_receipt_events_v1') is null
       or to_regclass('public.world_term_exchange_receipt_events_v1') is null then
        raise exception 'performance-index rejection removed partial fixture objects';
    end if;
    if (
        select count(*)
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_namespace index_namespace
            on index_namespace.oid = index_class.relnamespace
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
         where index_namespace.nspname = 'public'
           and index_class.relname = any(canonical_names)
           and index_class.relkind = 'i'
           and index_class.relowner = decoy_owner
           and index_meta.indrelid = decoy_oid
    ) <> cardinality(canonical_names) then
        raise exception 'performance-index rejection changed the decoy index catalog';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_namespace index_namespace
            on index_namespace.oid = index_class.relnamespace
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
         where index_namespace.nspname = 'public'
           and index_class.relname = any(canonical_names)
           and index_meta.indrelid in (
               'public.league_term_exchange_receipt_events_v1'::regclass,
               'public.world_term_exchange_receipt_events_v1'::regclass
           )
    ) then
        raise exception 'performance-index rejection left a canonical index on history';
    end if;
end
$check$;
SQL
echo "    normalized performance wrong-table index conflict passed"

echo "==> conflict: normalized projection key uses a non-default operator class"
new_case normalized_projection_key_opclass
normalized_projection_key_opclass_db="$CASE_DB"
run_db_file "$normalized_projection_key_opclass_db" "$MIGRATION_85" >/dev/null
run_db_file "$normalized_projection_key_opclass_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$normalized_projection_key_opclass_db" <<'SQL'
-- Keep the published fallback name but change the text operator class.  A
-- column/uniqueness-only probe would accept this as the projection authority.
alter table public.league_term_exchange_receipts
    drop constraint league_term_exchange_receipts_pkey;
create unique index uq_league_term_exchange_receipts_receipt_id_v1
    on public.league_term_exchange_receipts(receipt_id text_pattern_ops);
SQL
expect_migration_failure "$normalized_projection_key_opclass_db" \
  "normalized projection key operator class" "$MIGRATION_87" \
  'normalized league receipt projection has an incompatible receipt_id key'
assert_db "$normalized_projection_key_opclass_db" <<'SQL'
do $check$
begin
    if not exists (
        select 1
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
          join pg_catalog.pg_opclass opclass_meta
            on opclass_meta.oid = index_meta.indclass[0]
         where index_class.relname = 'uq_league_term_exchange_receipts_receipt_id_v1'
           and index_meta.indrelid = 'public.league_term_exchange_receipts'::regclass
           and opclass_meta.opcname = 'text_pattern_ops'
    ) then
        raise exception 'normalized projection operator-class fixture changed during rollback';
    end if;
end
$check$;
SQL
echo "    normalized projection non-default operator-class conflict passed"

echo "==> conflict: normalized projection canonical key name collides with a decoy relation"
new_case normalized_projection_key_canonical_name_collision
normalized_projection_key_canonical_name_collision_db="$CASE_DB"
run_db_file "$normalized_projection_key_canonical_name_collision_db" "$MIGRATION_85" >/dev/null
run_db_file "$normalized_projection_key_canonical_name_collision_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$normalized_projection_key_canonical_name_collision_db" <<'SQL'
-- The projection primary key is a valid alternate authority.  Occupy the
-- published fallback name with a table anyway; a name-only guard nested under
-- `if not has_projection_unique` would incorrectly skip this conflict.
create table public.uq_league_term_exchange_receipts_receipt_id_v1 (
    marker text
);
SQL
expect_migration_failure "$normalized_projection_key_canonical_name_collision_db" \
  "normalized projection canonical-name decoy relation" "$MIGRATION_87" \
  'normalized league receipt projection has an incompatible receipt_id key'
assert_db "$normalized_projection_key_canonical_name_collision_db" <<'SQL'
do $check$
declare
    relation_kind "char";
begin
    select c.relkind
      into relation_kind
      from pg_catalog.pg_class c
      join pg_catalog.pg_namespace n on n.oid = c.relnamespace
     where n.nspname = 'public'
       and c.relname = 'uq_league_term_exchange_receipts_receipt_id_v1';
    if relation_kind is distinct from 'r'
       or to_regclass('public.league_term_exchange_receipts') is null
       or not exists (
           select 1
             from pg_catalog.pg_index i
            where i.indrelid = 'public.league_term_exchange_receipts'::regclass
              and i.indisunique
              and i.indnkeyatts = 1
              and i.indnatts = 1
       ) then
        raise exception 'projection canonical-name decoy rejection changed valid catalog objects';
    end if;
end
$check$;
SQL
echo "    normalized projection canonical-name decoy conflict passed"

echo "==> conflict: normalized initial-intent key uses a non-default operator class"
new_case normalized_initial_index_opclass
normalized_initial_index_opclass_db="$CASE_DB"
run_db_file "$normalized_initial_index_opclass_db" "$MIGRATION_85" >/dev/null
run_db_file "$normalized_initial_index_opclass_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$normalized_initial_index_opclass_db" <<'SQL'
-- The sequence-one intent key has a published canonical name and predicate;
-- changing only text's operator class must still be rejected.
drop index public.uq_league_term_exchange_receipt_events_initial_intent_v1;
create unique index uq_league_term_exchange_receipt_events_initial_intent_v1
    on public.league_term_exchange_receipt_events_v1(intent_id text_pattern_ops)
    where event_sequence = 1;
SQL
expect_migration_failure "$normalized_initial_index_opclass_db" \
  "normalized initial-intent key operator class" "$MIGRATION_87" \
  'normalized league receipt history has an incompatible initial intent key'
assert_db "$normalized_initial_index_opclass_db" <<'SQL'
do $check$
begin
    if not exists (
        select 1
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
          join pg_catalog.pg_opclass opclass_meta
            on opclass_meta.oid = index_meta.indclass[0]
         where index_class.relname = 'uq_league_term_exchange_receipt_events_initial_intent_v1'
           and index_meta.indrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
           and opclass_meta.opcname = 'text_pattern_ops'
    ) then
        raise exception 'normalized initial-intent operator-class fixture changed during rollback';
    end if;
end
$check$;
SQL
echo "    normalized initial-intent non-default operator-class conflict passed"

echo "==> conflict: normalized event_id canonical key name collides with a decoy relation"
new_case normalized_event_key_canonical_name_collision
normalized_event_key_canonical_name_collision_db="$CASE_DB"
run_db_file "$normalized_event_key_canonical_name_collision_db" "$MIGRATION_85" >/dev/null
run_db_file "$normalized_event_key_canonical_name_collision_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$normalized_event_key_canonical_name_collision_db" <<'SQL'
-- A completed history has a valid primary-key event_id index, so the
-- canonical fallback name is not needed for the functional invariant.  A
-- same-named table must nevertheless be rejected; otherwise a retry would
-- silently preserve catalog/name drift behind that alternate valid key.
create table public.uq_league_term_exchange_receipt_events_v1_event_id_v1 (
    marker text
);
SQL
expect_migration_failure "$normalized_event_key_canonical_name_collision_db" \
  "normalized event_id canonical-name decoy relation" "$MIGRATION_87" \
  'normalized league receipt history has an incompatible event_id key'
assert_db "$normalized_event_key_canonical_name_collision_db" <<'SQL'
do $check$
declare
    relation_kind "char";
begin
    select c.relkind
      into relation_kind
      from pg_catalog.pg_class c
      join pg_catalog.pg_namespace n on n.oid = c.relnamespace
     where n.nspname = 'public'
       and c.relname = 'uq_league_term_exchange_receipt_events_v1_event_id_v1';
    if relation_kind is distinct from 'r'
       or to_regclass('public.league_term_exchange_receipt_events_v1') is null
       or not exists (
           select 1
             from pg_catalog.pg_index i
            where i.indrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
              and i.indisunique
              and i.indnkeyatts = 1
              and i.indnatts = 1
       ) then
        raise exception 'event_id canonical-name decoy rejection changed valid catalog objects';
    end if;
end
$check$;
SQL
echo "    normalized event_id canonical-name decoy conflict passed"

echo "==> conflict: normalized receipt-sequence canonical key name collides with a decoy relation"
new_case normalized_sequence_key_canonical_name_collision
normalized_sequence_key_canonical_name_collision_db="$CASE_DB"
run_db_file "$normalized_sequence_key_canonical_name_collision_db" "$MIGRATION_85" >/dev/null
run_db_file "$normalized_sequence_key_canonical_name_collision_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$normalized_sequence_key_canonical_name_collision_db" <<'SQL'
-- The completed history already has a valid (receipt_id,event_sequence)
-- uniqueness constraint.  Keep that alternate authority in place while
-- occupying the published fallback name with a table; a name-only
-- IF-NOT-EXISTS path must not let this decoy hide.
create table public.uq_league_term_exchange_receipt_events_v1_receipt_sequence_v1 (
    marker text
);
SQL
expect_migration_failure "$normalized_sequence_key_canonical_name_collision_db" \
  "normalized receipt-sequence canonical-name decoy relation" "$MIGRATION_87" \
  'normalized league receipt history has an incompatible receipt sequence key'
assert_db "$normalized_sequence_key_canonical_name_collision_db" <<'SQL'
do $check$
declare
    relation_kind "char";
begin
    select c.relkind
      into relation_kind
      from pg_catalog.pg_class c
      join pg_catalog.pg_namespace n on n.oid = c.relnamespace
     where n.nspname = 'public'
       and c.relname = 'uq_league_term_exchange_receipt_events_v1_receipt_sequence_v1';
    if relation_kind is distinct from 'r'
       or to_regclass('public.league_term_exchange_receipt_events_v1') is null
       or not exists (
           select 1
             from pg_catalog.pg_index i
            where i.indrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
              and i.indisunique
              and i.indnkeyatts = 2
              and i.indnatts = 2
       ) then
        raise exception 'receipt-sequence canonical-name decoy rejection changed valid catalog objects';
    end if;
end
$check$;
SQL
echo "    normalized receipt-sequence canonical-name decoy conflict passed"

echo "==> conflict: normalized performance index uses an explicit non-default collation"
new_case normalized_performance_index_collation
normalized_performance_index_collation_db="$CASE_DB"
run_db_file "$normalized_performance_index_collation_db" "$MIGRATION_85" >/dev/null
run_db_file "$normalized_performance_index_collation_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$normalized_performance_index_collation_db" <<'SQL'
-- An explicit C collation changes the index's catalog collation vector while
-- leaving the visible key/order list unchanged.  It must not be adopted as
-- the default-collation history lookup index.
drop index public.idx_league_term_exchange_receipt_events_latest_v1;
create index idx_league_term_exchange_receipt_events_latest_v1
    on public.league_term_exchange_receipt_events_v1(
        intent_id collate "C",
        event_sequence desc,
        event_id desc
    );
SQL
expect_migration_failure "$normalized_performance_index_collation_db" \
  "normalized performance index collation" "$MIGRATION_87" \
  'normalized league receipt history has an incompatible performance index idx_league_term_exchange_receipt_events_latest_v1'
assert_db "$normalized_performance_index_collation_db" <<'SQL'
do $check$
begin
    if not exists (
        select 1
          from pg_catalog.pg_class index_class
          join pg_catalog.pg_index index_meta
            on index_meta.indexrelid = index_class.oid
          join pg_catalog.pg_collation collation_meta
            on collation_meta.oid = index_meta.indcollation[0]
         where index_class.relname = 'idx_league_term_exchange_receipt_events_latest_v1'
           and index_meta.indrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
           and collation_meta.collname = 'C'
    ) then
        raise exception 'normalized performance collation fixture changed during rollback';
    end if;
end
$check$;
SQL
echo "    normalized performance non-default-collation conflict passed"

echo "==> conflict: normalized history timestamp column type drift"
new_case normalized_history_timestamp_type
normalized_history_timestamp_type_db="$CASE_DB"
run_db_file "$normalized_history_timestamp_type_db" "$MIGRATION_85" >/dev/null
run_db_file "$normalized_history_timestamp_type_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$normalized_history_timestamp_type_db" <<'SQL'
-- A timestamp without time zone is not interchangeable with the published
-- timestamptz history authority; a replay around a DST/offset boundary would
-- otherwise change the finalized instant.
alter table public.league_term_exchange_receipt_events_v1
    alter column finalized_at type timestamp without time zone
    using finalized_at at time zone 'UTC';
SQL
expect_migration_failure "$normalized_history_timestamp_type_db" \
  "normalized history timestamp column type drift" "$MIGRATION_87" \
  'normalized league receipt history column finalized_at has incompatible type'
assert_db "$normalized_history_timestamp_type_db" <<'SQL'
do $check$
begin
    if (select udt_name
          from information_schema.columns
         where table_schema = 'public'
           and table_name = 'league_term_exchange_receipt_events_v1'
           and column_name = 'finalized_at') is distinct from 'timestamp' then
        raise exception 'normalized timestamp type drift fixture changed during rollback';
    end if;
end
$check$;
SQL
echo "    normalized timestamp type drift was rejected and rolled back"

echo "==> repair: normalized history omitted-column defaults"
new_case normalized_history_default_repair
normalized_history_default_repair_db="$CASE_DB"
run_db_file "$normalized_history_default_repair_db" "$MIGRATION_85" >/dev/null
run_db_file "$normalized_history_default_repair_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$normalized_history_default_repair_db" <<'SQL'
alter table public.league_term_exchange_receipt_events_v1
    alter column event_kind drop default,
    alter column created_at drop default;
alter table public.world_term_exchange_receipt_events_v1
    alter column event_kind drop default,
    alter column created_at drop default;
SQL
run_db_file "$normalized_history_default_repair_db" "$MIGRATION_87" >/dev/null
assert_db "$normalized_history_default_repair_db" <<'SQL'
do $check$
declare
    missing_defaults integer;
begin
    select count(*)
      into missing_defaults
      from information_schema.columns
     where table_schema = 'public'
       and table_name in (
           'league_term_exchange_receipt_events_v1',
           'world_term_exchange_receipt_events_v1'
       )
       and (
           (column_name = 'event_kind' and column_default is distinct from '''initial''::text')
           or (column_name = 'created_at' and column_default is distinct from 'now()')
       );
    if missing_defaults <> 0 then
        raise exception 'normalized omitted-column defaults were not repaired';
    end if;
end
$check$;
SQL
echo "    normalized omitted-column defaults were repaired"

echo "==> conflict: normalized history event_id uses an unowned sequence"
new_case partial_history_unowned_sequence
partial_history_unowned_sequence_db="$CASE_DB"
run_db_file "$partial_history_unowned_sequence_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$partial_history_unowned_sequence_db" <<'SQL'
-- A deployment may have committed a plain nextval default without attaching
-- the sequence to this column with ALTER SEQUENCE ... OWNED BY.  0087 must not
-- silently adopt that ambiguous generator as append authority.
create sequence public.pu_unowned_league_event_id_seq;
create table public.league_term_exchange_receipt_events_v1 (
    event_id bigint primary key default nextval('public.pu_unowned_league_event_id_seq'::regclass),
    receipt_id text,
    event_sequence bigint,
    event_kind text,
    previous_receipt_hash text,
    protocol_version text,
    intent_id text,
    term_id text,
    backend_id text,
    backend_kind text,
    status text,
    progression_class text,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    amount_credits bigint,
    finalized_at timestamptz,
    receipt_json jsonb,
    receipt_hash text,
    created_at timestamptz
);
SQL
expect_migration_failure "$partial_history_unowned_sequence_db" \
  "partial history unowned event_id sequence" "$MIGRATION_87" \
  'normalized league receipt history event_id default/identity has no owned sequence'
assert_db "$partial_history_unowned_sequence_db" <<'SQL'
do $check$
begin
    if to_regclass('public.league_term_exchange_receipt_events_v1') is null
       or to_regclass('public.pu_unowned_league_event_id_seq') is null then
        raise exception 'unowned-sequence fixture objects disappeared during rollback';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
           and tgname = 'trg_cex_validate_league_term_exchange_receipt_event_v1'
    ) or to_regclass('public.world_term_exchange_receipt_events_v1') is not null then
        raise exception 'rejected unowned-sequence migration left hardening objects behind';
    end if;
end
$check$;
SQL
echo "    partial history unowned-sequence conflict passed"

echo "==> conflict: normalized history event_id sequence is shared by another default"
new_case partial_history_shared_sequence
partial_history_shared_sequence_db="$CASE_DB"
run_db_file "$partial_history_shared_sequence_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$partial_history_shared_sequence_db" <<'SQL'
-- A plain bigint partial table makes 0087 adopt the canonical sequence name.
-- The same sequence is already referenced by another column's nextval default;
-- adopting it would let ALTER SEQUENCE ... OWNED BY move the ownership boundary
-- away from that consumer.  The migration must reject this before any DDL.
create sequence public.league_term_exchange_receipt_events_v1_event_id_seq
    as bigint
    increment by 1
    minvalue 1
    maxvalue 9223372036854775807
    no cycle;
create table public.pu_shared_sequence_consumer (
    id bigint default nextval(
        'public.league_term_exchange_receipt_events_v1_event_id_seq'::regclass
    )
);
create table public.league_term_exchange_receipt_events_v1 (
    event_id bigint,
    receipt_id text,
    event_sequence bigint,
    event_kind text,
    previous_receipt_hash text,
    protocol_version text,
    intent_id text,
    term_id text,
    backend_id text,
    backend_kind text,
    status text,
    progression_class text,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    amount_credits bigint,
    finalized_at timestamptz,
    receipt_json jsonb,
    receipt_hash text,
    created_at timestamptz
);
SQL
expect_migration_failure "$partial_history_shared_sequence_db" \
  "partial history shared event_id sequence" "$MIGRATION_87" \
  'normalized league receipt history event_id sequence is already used by another column/default'
assert_db "$partial_history_shared_sequence_db" <<'SQL'
do $check$
declare
    sequence_oid oid;
    consumer_default text;
    history_default text;
    dependency_count bigint;
begin
    sequence_oid := 'public.league_term_exchange_receipt_events_v1_event_id_seq'::regclass;
    if to_regclass('public.league_term_exchange_receipt_events_v1') is null
       or to_regclass('public.pu_shared_sequence_consumer') is null
       or to_regclass('public.world_term_exchange_receipt_events_v1') is not null then
        raise exception 'shared-sequence migration rollback changed partial objects';
    end if;
    select column_default
      into consumer_default
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'pu_shared_sequence_consumer'
       and column_name = 'id';
    select column_default
      into history_default
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'league_term_exchange_receipt_events_v1'
       and column_name = 'event_id';
    if consumer_default is null
       or position('nextval' in lower(consumer_default)) = 0
       or history_default is not null
       or pg_get_serial_sequence(
              'public.league_term_exchange_receipt_events_v1', 'event_id'
          ) is not null then
        raise exception 'shared-sequence rollback changed column defaults';
    end if;
    select count(*)
      into dependency_count
      from pg_catalog.pg_depend d
     where d.classid = 'pg_catalog.pg_attrdef'::regclass
       and d.refclassid = 'pg_catalog.pg_class'::regclass
       and d.refobjid = sequence_oid;
    if dependency_count <> 1 then
        raise exception 'shared-sequence consumer dependency was not preserved';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_depend d
         where d.classid = 'pg_catalog.pg_class'::regclass
           and d.objid = sequence_oid
           and d.deptype in ('a', 'i')
    ) then
        raise exception 'rejected shared sequence gained a new owner';
    end if;
end
$check$;
SQL
echo "    partial history shared-sequence conflict passed"

# The first attempt above exercises the plain/no-default adoption branch.  Now
# give the same partial table an owned default and retry: ownership by event_id
# alone must not make a generator shared with another DEFAULT nextval() safe.
run_db_stdin "$partial_history_shared_sequence_db" <<'SQL'
alter table public.league_term_exchange_receipt_events_v1
    alter column event_id set default nextval(
        'public.league_term_exchange_receipt_events_v1_event_id_seq'::regclass
    );
alter sequence public.league_term_exchange_receipt_events_v1_event_id_seq
    owned by public.league_term_exchange_receipt_events_v1.event_id;
SQL
expect_migration_failure "$partial_history_shared_sequence_db" \
  "partial history owned shared event_id sequence" "$MIGRATION_87" \
  'normalized league receipt history event_id sequence is already used by another column/default'
assert_db "$partial_history_shared_sequence_db" <<'SQL'
do $check$
begin
    if pg_get_serial_sequence(
           'public.league_term_exchange_receipt_events_v1', 'event_id'
       ) is distinct from 'public.league_term_exchange_receipt_events_v1_event_id_seq'
       or not exists (
           select 1
             from pg_catalog.pg_depend d
            where d.classid = 'pg_catalog.pg_class'::regclass
              and d.objid = 'public.league_term_exchange_receipt_events_v1_event_id_seq'::regclass
              and d.refclassid = 'pg_catalog.pg_class'::regclass
              and d.refobjid = 'public.league_term_exchange_receipt_events_v1'::regclass
              and d.refobjsubid = (
                  select attnum
                    from pg_catalog.pg_attribute
                   where attrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
                     and attname = 'event_id'
              )
              and d.deptype in ('a', 'i')
       )
       or not exists (
           select 1
             from pg_catalog.pg_depend d
             join pg_catalog.pg_attrdef ad on ad.oid = d.objid
            where d.classid = 'pg_catalog.pg_attrdef'::regclass
              and d.refclassid = 'pg_catalog.pg_class'::regclass
              and d.refobjid = 'public.league_term_exchange_receipt_events_v1_event_id_seq'::regclass
              and ad.adrelid = 'public.pu_shared_sequence_consumer'::regclass
       ) then
        raise exception 'owned shared sequence rollback changed dependency boundaries';
    end if;
    if to_regclass('public.world_term_exchange_receipt_events_v1') is not null then
        raise exception 'owned shared sequence rejection left world history table';
    end if;
end
$check$;
SQL
echo "    partial history owned shared-sequence conflict passed"

echo "==> conflict: normalized history event_id sequence increment headroom"
new_case hist_seq_headroom
partial_history_sequence_headroom_db="$CASE_DB"
run_db_file "$partial_history_sequence_headroom_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$partial_history_sequence_headroom_db" <<'SQL'
-- A custom owned sequence can be below MAXVALUE while its next increment
-- would already overflow.  0087 must reject that state before installing any
-- history trigger or backfilling projection rows.  The first attempt
-- exercises last_value headroom; the second (after rollback) exercises
-- max(event_id) headroom.
create sequence public.pu_normalized_headroom_event_id_seq
    as bigint
    increment by 2
    minvalue 1
    maxvalue 10
    no cycle;
create table public.league_term_exchange_receipt_events_v1 (
    event_id bigint primary key
        default nextval('public.pu_normalized_headroom_event_id_seq'::regclass),
    receipt_id text not null,
    event_sequence bigint not null,
    event_kind text not null default 'initial',
    previous_receipt_hash text,
    protocol_version text not null,
    intent_id text not null,
    term_id text not null,
    backend_id text not null,
    backend_kind text not null,
    status text not null,
    progression_class text not null,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    amount_credits bigint,
    finalized_at timestamptz not null,
    receipt_json jsonb not null,
    receipt_hash text not null,
    created_at timestamptz not null default now(),
    unique (receipt_id, event_sequence)
);
alter sequence public.pu_normalized_headroom_event_id_seq
    owned by public.league_term_exchange_receipt_events_v1.event_id;
select setval('public.pu_normalized_headroom_event_id_seq'::regclass, 9, true);
SQL
expect_migration_failure "$partial_history_sequence_headroom_db" \
  "normalized history event_id sequence last_value headroom" "$MIGRATION_87" \
  'normalized league receipt history event_id sequence is exhausted'

# Keep the partial table, move the sequence back below the limit, and place a
# manually supplied event_id at the same unsafe boundary.  This second run
# must fail on max(event_id) even though last_value itself has room.
run_db_stdin "$partial_history_sequence_headroom_db" <<'SQL'
select setval('public.pu_normalized_headroom_event_id_seq'::regclass, 1, false);
with payload as (
    select jsonb_build_object(
        'protocol_version', 'term_exchange_protocol_v1',
        'receipt_id', 'pu-league-unknown',
        'intent_id', 'pu-intent-league-unknown',
        'term_id', 'pu-term',
        'backend_id', 'pu-backend',
        'backend_kind', 'cex',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'settlement_reference', 'pu-settle-league-unknown',
        'ledger_entry_id', 'pu-ledger-league-unknown',
        'reason', null,
        'amount_credits', null,
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:01:01Z')::bigint
    ) as receipt_json
)
insert into public.league_term_exchange_receipt_events_v1 (
    event_id, receipt_id, event_sequence, event_kind, previous_receipt_hash,
    protocol_version, intent_id, term_id, backend_id, backend_kind, status,
    progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at, receipt_json, receipt_hash
)
select 9, 'pu-league-unknown', 1, 'initial', null,
       'term_exchange_protocol_v1', 'pu-intent-league-unknown', 'pu-term',
       'pu-backend', 'cex', 'settled', 'progression_allowed',
       'pu-settle-league-unknown', 'pu-ledger-league-unknown', null,
       null, '2026-01-01T00:01:01Z', receipt_json,
       'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
  from payload;
SQL
expect_migration_failure "$partial_history_sequence_headroom_db" \
  "normalized history event_id sequence max-event headroom" "$MIGRATION_87" \
  'normalized league receipt history event_id sequence is exhausted'
assert_db "$partial_history_sequence_headroom_db" <<'SQL'
do $check$
declare
    sequence_increment_value bigint;
    sequence_max_value bigint;
    sequence_last_value bigint;
    sequence_called_value boolean;
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 1
       or (select event_id from public.league_term_exchange_receipt_events_v1 limit 1) <> 9 then
        raise exception 'normalized sequence-headroom rollback changed the partial row';
    end if;
    select s.seqincrement, s.seqmax
      into sequence_increment_value, sequence_max_value
      from pg_catalog.pg_sequence s
      join pg_catalog.pg_class c on c.oid = s.seqrelid
      join pg_catalog.pg_namespace n on n.oid = c.relnamespace
     where n.nspname = 'public'
       and c.relname = 'pu_normalized_headroom_event_id_seq';
    select last_value, is_called
      into sequence_last_value, sequence_called_value
      from public.pu_normalized_headroom_event_id_seq;
    if sequence_increment_value is distinct from 2
       or sequence_max_value is distinct from 10
       or sequence_last_value is distinct from 1
       or sequence_called_value then
        raise exception 'normalized sequence-headroom fixture state changed during rollback';
    end if;
    if to_regclass('public.world_term_exchange_receipt_events_v1') is not null
       or exists (
           select 1
             from pg_catalog.pg_trigger
            where tgrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
              and tgname = 'trg_cex_validate_league_term_exchange_receipt_event_v1'
       ) then
        raise exception 'normalized sequence-headroom rejection left hardening objects';
    end if;
end
$check$;
SQL
echo "    normalized history event_id sequence-headroom conflict passed"

echo "==> conflict: normalized plain event_id sequence transactional rollback"
new_case plain_seq_rollback
plain_seq_rollback_db="$CASE_DB"
run_db_file "$plain_seq_rollback_db" "$MIGRATION_85" >/dev/null
run_db_stdin "$plain_seq_rollback_db" <<'SQL'
-- Model a partially-created plain bigint key with the sequence name that
-- 0087 adopts.  The sequence is deliberately unowned and starts at a
-- headroom boundary; the migration must reject it before any state-changing
-- repair operation.
create sequence public.league_term_exchange_receipt_events_v1_event_id_seq
    as bigint
    increment by 2
    minvalue 1
    maxvalue 10
    no cycle;
create table public.league_term_exchange_receipt_events_v1 (
    event_id bigint,
    receipt_id text,
    event_sequence bigint,
    event_kind text,
    previous_receipt_hash text,
    protocol_version text,
    intent_id text,
    term_id text,
    backend_id text,
    backend_kind text,
    status text,
    progression_class text,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    amount_credits bigint,
    finalized_at timestamptz,
    receipt_json jsonb,
    receipt_hash text,
    created_at timestamptz
);
select setval(
    'public.league_term_exchange_receipt_events_v1_event_id_seq'::regclass,
    9,
    true
);
SQL
expect_migration_failure "$plain_seq_rollback_db" \
  "plain normalized sequence headroom" "$MIGRATION_87" \
  'normalized league receipt history event_id sequence is exhausted'
assert_db "$plain_seq_rollback_db" <<'SQL'
do $check$
declare
    sequence_last_value bigint;
    sequence_called_value boolean;
    column_default_value text;
begin
    select last_value, is_called
      into sequence_last_value, sequence_called_value
      from public.league_term_exchange_receipt_events_v1_event_id_seq;
    select column_default
      into column_default_value
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'league_term_exchange_receipt_events_v1'
       and column_name = 'event_id';
    if sequence_last_value is distinct from 9
       or not sequence_called_value
       or column_default_value is not null
       or pg_get_serial_sequence(
              'public.league_term_exchange_receipt_events_v1', 'event_id'
          ) is not null then
        raise exception 'plain sequence headroom failure changed generator state';
    end if;
end
$check$;
SQL

# A safe sequence state reaches the transactional RESTART path, then a later
# row-semantic failure aborts the migration.  The original sequence state,
# plain column shape, and unowned status must all survive that rollback.
run_db_stdin "$plain_seq_rollback_db" <<'SQL'
select setval(
    'public.league_term_exchange_receipt_events_v1_event_id_seq'::regclass,
    1,
    false
);
insert into public.league_term_exchange_receipt_events_v1 (
    event_id, receipt_id, event_sequence, event_kind, previous_receipt_hash,
    protocol_version, intent_id, term_id, backend_id, backend_kind, status,
    progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at, receipt_json, receipt_hash, created_at
) values (
    1, 'pu-league-unknown', 1, 'initial', null,
    'term_exchange_protocol_v1', 'pu-intent-league-unknown', 'pu-term',
    'pu-backend', 'cex', 'settled', 'progression_allowed',
    'pu-settle-league-unknown', 'pu-ledger-league-unknown', null, null,
    '2026-01-01T00:01:01Z',
    '{"protocol_version":"term_exchange_protocol_v1","receipt_id":"pu-league-unknown","intent_id":"pu-intent-league-unknown","term_id":"pu-term","backend_id":"pu-backend","backend_kind":"cex","status":"settled","progression_class":"progression_allowed"}'::jsonb,
    'sha256:' || repeat('0', 63),
    '2026-01-01T00:01:01Z'
);
SQL
expect_migration_failure "$plain_seq_rollback_db" \
  "plain normalized sequence later semantic rollback" "$MIGRATION_87" \
  'normalized league receipt history failed pre-existing-row validation: receipt_hash is invalid'
assert_db "$plain_seq_rollback_db" <<'SQL'
do $check$
declare
    sequence_last_value bigint;
    sequence_called_value boolean;
    column_default_value text;
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 1
       or (select event_id from public.league_term_exchange_receipt_events_v1 limit 1) <> 1
       or (select receipt_hash from public.league_term_exchange_receipt_events_v1 limit 1)
          is distinct from 'sha256:' || repeat('0', 63) then
        raise exception 'plain sequence semantic rollback changed the partial row';
    end if;
    select last_value, is_called
      into sequence_last_value, sequence_called_value
      from public.league_term_exchange_receipt_events_v1_event_id_seq;
    select column_default
      into column_default_value
      from information_schema.columns
     where table_schema = 'public'
       and table_name = 'league_term_exchange_receipt_events_v1'
       and column_name = 'event_id';
    if sequence_last_value is distinct from 1
       or sequence_called_value
       or column_default_value is not null
       or pg_get_serial_sequence(
              'public.league_term_exchange_receipt_events_v1', 'event_id'
          ) is not null then
        raise exception 'transactional sequence RESTART did not roll back';
    end if;
    if to_regclass('public.world_term_exchange_receipt_events_v1') is not null then
        raise exception 'failed plain sequence migration left world history table';
    end if;
end
$check$;
SQL
echo "    normalized plain event_id sequence rollback passed"

echo "==> conflict: forged orphan term history row"
new_case forged_orphan_history
forged_orphan_history_db="$CASE_DB"
run_db_file "$forged_orphan_history_db" "$MIGRATION_85" >/dev/null
create_preexisting_league_history_table "$forged_orphan_history_db"
run_db_stdin "$forged_orphan_history_db" <<'SQL'
with payload as (
    select jsonb_build_object(
        'protocol_version', 'term_exchange_protocol_v1',
        'receipt_id', 'pu-forged-orphan-history',
        'intent_id', 'pu-intent-forged-orphan-history',
        'term_id', 'pu-term',
        'backend_id', 'pu-backend',
        'backend_kind', 'cex',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'settlement_reference', null,
        'ledger_entry_id', null,
        'reason', null,
        'amount_credits', null,
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:04:00Z')::bigint
    ) as receipt_json
)
insert into public.league_term_exchange_receipt_events_v1 (
    receipt_id, event_sequence, event_kind, previous_receipt_hash,
    protocol_version, intent_id, term_id, backend_id, backend_kind, status,
    progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at, receipt_json, receipt_hash
)
select 'pu-forged-orphan-history', 1, 'initial', null,
       'term_exchange_protocol_v1', 'pu-intent-forged-orphan-history', 'pu-term',
       'pu-backend', 'cex', 'settled', 'progression_allowed', null, null, null,
       null, '2026-01-01T00:04:00Z', receipt_json,
       'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
  from payload;
SQL
expect_migration_failure "$forged_orphan_history_db" "forged orphan history row" "$MIGRATION_87" \
  'normalized league receipt history failed pre-existing-row validation: no immutable intent or projection authority'
assert_db "$forged_orphan_history_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 1 then
        raise exception 'forged orphan history row changed during rejected migration';
    end if;
    if exists (
        select 1
          from pg_catalog.pg_trigger
         where tgrelid = 'public.league_term_exchange_receipt_events_v1'::regclass
           and tgname = 'trg_cex_validate_league_term_exchange_receipt_event_v1'
    ) then
        raise exception 'rejected forged orphan migration left a validation trigger behind';
    end if;
end
$check$;
SQL
echo "    forged orphan history rejected"

echo "==> conflict: forged term history hash"
new_case forged_hash_history
forged_hash_history_db="$CASE_DB"
run_db_file "$forged_hash_history_db" "$MIGRATION_85" >/dev/null
create_preexisting_league_history_table "$forged_hash_history_db"
# Bind the forged row to a real projection so this case reaches the digest
# validator rather than being rejected earlier as an orphan authority.
run_db_stdin "$forged_hash_history_db" <<'SQL'
insert into public.league_term_exchange_receipts (
    receipt_id, protocol_version, intent_id, term_id, backend_id, backend_kind,
    status, progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at
)
select 'pu-forged-hash-history', protocol_version, intent_id, term_id, backend_id,
       backend_kind, status, progression_class, settlement_reference,
       ledger_entry_id, reason, amount_credits, finalized_at
  from public.league_term_exchange_receipts
 where receipt_id = 'pu-league-unknown';
SQL
run_db_stdin "$forged_hash_history_db" <<'SQL'
with payload as (
    select jsonb_build_object(
        'protocol_version', 'term_exchange_protocol_v1',
        'receipt_id', 'pu-forged-hash-history',
        'intent_id', 'pu-intent-league-unknown',
        'term_id', 'pu-term',
        'backend_id', 'pu-backend',
        'backend_kind', 'cex',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'settlement_reference', 'pu-settle-league-unknown',
        'ledger_entry_id', 'pu-ledger-league-unknown',
        'reason', null,
        'amount_credits', null,
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:05:00Z')::bigint
    ) as receipt_json
)
insert into public.league_term_exchange_receipt_events_v1 (
    receipt_id, event_sequence, event_kind, previous_receipt_hash,
    protocol_version, intent_id, term_id, backend_id, backend_kind, status,
    progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at, receipt_json, receipt_hash
)
select 'pu-forged-hash-history', 1, 'initial', null,
       'term_exchange_protocol_v1', 'pu-intent-league-unknown', 'pu-term',
       'pu-backend', 'cex', 'settled', 'progression_allowed',
       'pu-settle-league-unknown', 'pu-ledger-league-unknown', null,
       null, '2026-01-01T00:05:00Z', receipt_json,
       'sha256:' || repeat('0', 64)
  from payload;
SQL
expect_migration_failure "$forged_hash_history_db" "forged history hash" "$MIGRATION_87" \
  'normalized league receipt history failed pre-existing-row validation: receipt_hash digest mismatch'
assert_db "$forged_hash_history_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 1
       or (select receipt_hash from public.league_term_exchange_receipt_events_v1 limit 1)
          is distinct from 'sha256:' || repeat('0', 64) then
        raise exception 'forged hash history row changed during rejected migration';
    end if;
end
$check$;
SQL
echo "    forged history hash rejected"

echo "==> conflict: forged pre-existing retry projection parity"
new_case forged_retry_history
forged_retry_history_db="$CASE_DB"
run_db_file "$forged_retry_history_db" "$MIGRATION_85" >/dev/null
create_preexisting_league_history_table "$forged_retry_history_db"
# Give the legacy projection a known amount so the second history event cannot
# introduce a self-authenticated amount that differs from its authority.
run_db_stdin "$forged_retry_history_db" <<'SQL'
alter table public.league_term_exchange_receipts disable trigger all;
update public.league_term_exchange_receipts
   set amount_credits = 23
 where receipt_id = 'pu-league-known';
SQL
run_db_stdin "$forged_retry_history_db" <<'SQL'
with first_payload as (
    select jsonb_build_object(
        'protocol_version', 'term_exchange_protocol_v1',
        'receipt_id', 'pu-league-known',
        'intent_id', 'pu-intent-league-known',
        'term_id', 'pu-term',
        'backend_id', 'pu-backend',
        'backend_kind', 'cex',
        'status', 'held_review',
        'progression_class', 'recoverable_hold',
        'settlement_reference', null,
        'ledger_entry_id', null,
        'reason', null,
        'amount_credits', 23,
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:06:00Z')::bigint
    ) as receipt_json
), first_row as (
    insert into public.league_term_exchange_receipt_events_v1 (
        receipt_id, event_sequence, event_kind, previous_receipt_hash,
        protocol_version, intent_id, term_id, backend_id, backend_kind, status,
        progression_class, settlement_reference, ledger_entry_id, reason,
        amount_credits, finalized_at, receipt_json, receipt_hash
    )
    select 'pu-league-known', 1, 'initial', null,
           'term_exchange_protocol_v1', 'pu-intent-league-known', 'pu-term',
           'pu-backend', 'cex', 'held_review', 'recoverable_hold', null, null,
           null, 23, '2026-01-01T00:06:00Z', receipt_json,
           'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
      from first_payload
    returning receipt_hash
), second_payload as (
    select jsonb_build_object(
        'protocol_version', 'term_exchange_protocol_v1',
        'receipt_id', 'pu-league-known',
        'intent_id', 'pu-intent-league-known',
        'term_id', 'pu-term',
        'backend_id', 'pu-backend',
        'backend_kind', 'cex',
        'status', 'settled',
        'progression_class', 'progression_allowed',
        'settlement_reference', 'pu-settle-league-known',
        'ledger_entry_id', 'pu-ledger-league-known',
        'reason', null,
        'amount_credits', 99,
        'finalized_at_epoch', extract(epoch from timestamptz '2026-01-01T00:06:01Z')::bigint
    ) as receipt_json,
    first_row.receipt_hash
      from first_row
)
insert into public.league_term_exchange_receipt_events_v1 (
    receipt_id, event_sequence, event_kind, previous_receipt_hash,
    protocol_version, intent_id, term_id, backend_id, backend_kind, status,
    progression_class, settlement_reference, ledger_entry_id, reason,
    amount_credits, finalized_at, receipt_json, receipt_hash
)
select 'pu-league-known', 2, 'final', receipt_hash,
       'term_exchange_protocol_v1', 'pu-intent-league-known', 'pu-term',
       'pu-backend', 'cex', 'settled', 'progression_allowed',
       'pu-settle-league-known', 'pu-ledger-league-known', null,
       99, '2026-01-01T00:06:01Z', receipt_json,
       'sha256:' || encode(digest(receipt_json::text, 'sha256'), 'hex')
  from second_payload;
SQL
expect_migration_failure "$forged_retry_history_db" "forged pre-existing retry projection parity" "$MIGRATION_87" \
  'normalized league receipt history failed pre-existing-row validation: projection amount binding mismatch'
assert_db "$forged_retry_history_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 2
       or (select amount_credits from public.league_term_exchange_receipt_events_v1
             where receipt_id = 'pu-league-known' and event_sequence = 2) is distinct from 99 then
        raise exception 'forged retry projection fixture changed during rejected migration';
    end if;
    if (select amount_credits from public.league_term_exchange_receipts
         where receipt_id = 'pu-league-known') is distinct from 23 then
        raise exception 'forged retry projection authority changed during rollback';
    end if;
end
$check$;
SQL
echo "    forged pre-existing retry projection conflict rejected"

echo "==> conflict: projection/history identity and amount"
new_case projection
projection_db="$CASE_DB"
run_db_file "$projection_db" "$MIGRATION_85" >/dev/null
run_db_file "$projection_db" "$MIGRATION_87" >/dev/null
run_db_stdin "$projection_db" <<'SQL'
alter table public.league_term_exchange_receipts disable trigger all;
update public.league_term_exchange_receipts
   set term_id = 'pu-tampered-term', amount_credits = 999
 where receipt_id = 'pu-league-known';
SQL
expect_migration_failure "$projection_db" "league projection/history conflict" "$MIGRATION_87" \
  'normalized league receipt history failed pre-existing-row validation: projection identity binding mismatch'
assert_db "$projection_db" <<'SQL'
do $check$
begin
    if (select count(*) from public.league_term_exchange_receipt_events_v1) <> 2 then
        raise exception 'projection conflict changed existing history during rollback';
    end if;
    if (select term_id from public.league_term_exchange_receipts
         where receipt_id = 'pu-league-known') is distinct from 'pu-tampered-term'
       or (select amount_credits from public.league_term_exchange_receipts
         where receipt_id = 'pu-league-known') is distinct from 999 then
        raise exception 'projection conflict fixture was unexpectedly rewritten';
    end if;
end
$check$;
SQL
echo "    projection conflict passed"

echo "==> malformed omitted-intent evidence amounts fail closed"
new_case native_evidence_amount_shapes
native_evidence_amount_shapes_db="$CASE_DB"
run_db_file "$native_evidence_amount_shapes_db" "$MIGRATION_85" >/dev/null
# These audit-only intents omit amount_credits, so 0086 may consult evidence;
# each evidence value below is deliberately malformed in a different way.  A
# text scalar, fractional number, or negative number must never be coerced into
# a value-bearing amount (nor fall through to a stored positive projection).
run_db_stdin "$native_evidence_amount_shapes_db" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values
(
    'pu-intent-evidence-string', 'term_exchange_protocol_v1', 'pu-shape',
    'evidence-string', repeat('e', 64),
    jsonb_build_object(
        'intent_id', 'pu-intent-evidence-string',
        'protocol_version', 'term_exchange_protocol_v1',
        'term_id', 'pu-term-evidence-string',
        'kind', 'audit'
    ), 'accepted'
),
(
    'pu-intent-evidence-fraction', 'term_exchange_protocol_v1', 'pu-shape',
    'evidence-fraction', repeat('f', 64),
    jsonb_build_object(
        'intent_id', 'pu-intent-evidence-fraction',
        'protocol_version', 'term_exchange_protocol_v1',
        'term_id', 'pu-term-evidence-fraction',
        'kind', 'audit'
    ), 'accepted'
),
(
    'pu-intent-evidence-negative', 'term_exchange_protocol_v1', 'pu-shape',
    'evidence-negative', repeat('0', 64),
    jsonb_build_object(
        'intent_id', 'pu-intent-evidence-negative',
        'protocol_version', 'term_exchange_protocol_v1',
        'term_id', 'pu-term-evidence-negative',
        'kind', 'audit'
    ), 'accepted'
);

insert into public.trnm_economic_receipts (
    receipt_id, intent_id, protocol_version, idempotency_scope,
    idempotency_key, progression_class, status, receipt_json, finalized_at
) values
(
    'pu-receipt-evidence-string', 'pu-intent-evidence-string',
    'term_exchange_protocol_v1', 'pu-shape', 'evidence-string',
    'progression_allowed', 'settled',
    jsonb_build_object(
        'intent_id', 'pu-intent-evidence-string',
        'receipt_id', 'pu-receipt-evidence-string',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled', 'progression_class', 'progression_allowed',
        'evidence', jsonb_build_object('amount_credits', '9')
    ), '2026-01-01T00:08:00Z'
),
(
    'pu-receipt-evidence-fraction', 'pu-intent-evidence-fraction',
    'term_exchange_protocol_v1', 'pu-shape', 'evidence-fraction',
    'progression_allowed', 'settled',
    jsonb_build_object(
        'intent_id', 'pu-intent-evidence-fraction',
        'receipt_id', 'pu-receipt-evidence-fraction',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled', 'progression_class', 'progression_allowed',
        'evidence', jsonb_build_object('amount_credits', 1.5::numeric)
    ), '2026-01-01T00:08:01Z'
),
(
    'pu-receipt-evidence-negative', 'pu-intent-evidence-negative',
    'term_exchange_protocol_v1', 'pu-shape', 'evidence-negative',
    'progression_allowed', 'settled',
    jsonb_build_object(
        'intent_id', 'pu-intent-evidence-negative',
        'receipt_id', 'pu-receipt-evidence-negative',
        'protocol_version', 'term_exchange_protocol_v1',
        'status', 'settled', 'progression_class', 'progression_allowed',
        'evidence', jsonb_build_object('amount_credits', -4)
    ), '2026-01-01T00:08:02Z'
);
SQL
run_db_file "$native_evidence_amount_shapes_db" "$MIGRATION_86" >/dev/null
assert_db "$native_evidence_amount_shapes_db" <<'SQL'
do $check$
begin
    if exists (
        select 1
          from public.trnm_economic_receipt_events_v1
         where receipt_id in (
                   'pu-receipt-evidence-string',
                   'pu-receipt-evidence-fraction',
                   'pu-receipt-evidence-negative'
               )
           and (
               amount_credits is distinct from 0
               or jsonb_typeof(receipt_json #> '{evidence,amount_credits}') is distinct from 'number'
               or receipt_json #>> '{evidence,amount_credits}' is distinct from '0'
           )
    ) then
        raise exception 'malformed omitted-intent evidence authorized a non-zero amount';
    end if;
    if (select count(*)
          from public.trnm_economic_receipt_events_v1
         where receipt_id in (
                   'pu-receipt-evidence-string',
                   'pu-receipt-evidence-fraction',
                   'pu-receipt-evidence-negative'
               )) <> 3 then
        raise exception 'malformed omitted-intent evidence fixture was not backfilled';
    end if;
end
$check$;
SQL
echo "    malformed omitted-intent evidence amounts fail-closed"

echo "receipt partial-upgrade regression passed"

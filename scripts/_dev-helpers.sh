#!/usr/bin/env bash
set -euo pipefail

CEX_SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CEX_PROJECT_ROOT="$(cd -- "$CEX_SCRIPT_DIR/.." && pwd)"
CEX_POSTGRES_CONTAINER_NAME="${CEX_POSTGRES_CONTAINER_NAME:-cex-postgres-1}"
CEX_POSTGRES_USER="${CEX_POSTGRES_USER:-postgres}"
CEX_POSTGRES_DB="${CEX_POSTGRES_DB:-cex_ai}"
CEX_POSTGRES_PASSWORD="${CEX_POSTGRES_PASSWORD:-postgres}"
CEX_DATABASE_URL_DEFAULT="postgres://postgres:postgres@127.0.0.1:5432/cex_ai"
# These markers let callers distinguish values parsed from DATABASE_URL (where
# an explicitly empty password is meaningful) from the helper's local defaults.
# They are deliberately kept private-ish and exported only when a URL is
# synchronized below.
CEX_DATABASE_URL_SYNCED="${CEX_DATABASE_URL_SYNCED:-0}"

cex_load_env() {
  local env_file="${1:-}"
  if [[ -z "$env_file" ]]; then
    if [[ -n "${CEX_ENV_FILE:-}" ]]; then
      env_file="$CEX_ENV_FILE"
    elif [[ -f "$CEX_PROJECT_ROOT/.env" ]]; then
      env_file="$CEX_PROJECT_ROOT/.env"
    elif [[ -f "$CEX_PROJECT_ROOT/.env.example" ]]; then
      env_file="$CEX_PROJECT_ROOT/.env.example"
    fi
  fi

  if [[ -z "$env_file" || ! -f "$env_file" ]]; then
    return 0
  fi

  while IFS='' read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    [[ -z "$line" || "$line" == \#* ]] && continue
    export "$line"
  done < "$env_file"
}

# Resolve the runtime-manager orchestration lane into the canonical profile
# consumed by service startup guards.  `full` is a launcher mode, not a
# RuntimeProfile value; forwarding it to a service makes every child reject
# startup.  The native TRNM lane is production-like even when the repository's
# .env file was written for local development, so force APP_ENV to the same
# production posture and avoid a profile conflict.
cex_select_runtime_profile() {
  local requested="${CEX_RUNTIME_PROFILE:-full}"
  case "$requested" in
    trnm_economy)
      requested="trnm-economy"
      ;;
  esac

  export CEX_RUNTIME_LANE="$requested"
  if [[ "$requested" == "trnm-economy" ]]; then
    export APP_ENV=production
    export CEX_RUNTIME_PROFILE=trnm-economy
  elif [[ "$requested" == "full" ]]; then
    # Full is the default service topology.  Let an explicit APP_ENV (or the
    # optional service-profile override) select the actual startup posture.
    export CEX_RUNTIME_PROFILE="${CEX_RUNTIME_SERVICE_PROFILE:-${APP_ENV:-dev}}"
  else
    export CEX_RUNTIME_PROFILE="$requested"
  fi
}

cex_require_cmd() {
  local cmd
  for cmd in "$@"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      echo "missing required command: $cmd" >&2
      return 127
    fi
  done
}

cex_effective_database_url() {
  printf '%s\n' "${DATABASE_URL:-$CEX_DATABASE_URL_DEFAULT}"
}

# Synchronize the Docker/local PostgreSQL connection defaults with an explicit
# DATABASE_URL.  A number of operator drills source this helper (and therefore
# load the repository .env) before they connect to a caller-selected disposable
# database.  Keeping CEX_POSTGRES_* at their convenience defaults in that case
# can silently authenticate as the wrong role or open the wrong database.  URI
# credentials are decoded once, without printing them, and control characters
# are rejected before they reach an environment variable or a command line.
#
# This is intentionally opt-in: existing local scripts that want the values in
# CEX_POSTGRES_* to remain independent of DATABASE_URL do not change behavior.
cex_sync_postgres_env_from_database_url() {
  local url="${1:-$(cex_effective_database_url)}"
  if ! command -v python3 >/dev/null 2>&1; then
    echo "explicit DATABASE_URL synchronization requires python3" >&2
    return 127
  fi

  local parts
  if ! parts="$(python3 - "$url" <<'PY'
from urllib.parse import unquote, urlsplit
import sys

try:
    raw_url = sys.argv[1]
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in raw_url):
        raise ValueError("DATABASE_URL contains a control character")
    source = urlsplit(raw_url)
    if source.scheme not in {"postgres", "postgresql"} or not source.netloc or not source.hostname:
        raise ValueError("DATABASE_URL must be a PostgreSQL URI with a host")
    if source.fragment:
        raise ValueError("DATABASE_URL fragments are not supported")
    # Accessing .port performs the standard range/format validation.
    port = source.port
    user = unquote(source.username or "")
    password = unquote(source.password or "")
    database = unquote(source.path.lstrip("/"))
    host = source.hostname or ""
    password_present = "1" if source.password is not None else "0"
except (ValueError, UnicodeError) as error:
    raise SystemExit(f"invalid PostgreSQL DATABASE_URL: {error}") from error

for label, value in (("user", user), ("password", password),
                     ("database", database), ("host", host)):
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in value):
        raise SystemExit(f"DATABASE_URL {label} contains a control character")
if not database:
    raise SystemExit("DATABASE_URL must include a database name")
# Unit separator is not valid in any decoded URI component (control
# characters are rejected below) and, unlike a tab, Bash does not collapse
# adjacent non-whitespace delimiters.  This preserves intentionally empty
# user/password/host fields during the shell read.
print("\x1f".join((password_present, user, password, database, host,
                  str(port or ""))))
PY
)"; then
    echo "cannot synchronize PostgreSQL defaults from DATABASE_URL" >&2
    return 2
  fi

  local password_present user password database host port
  IFS=$'\x1f' read -r password_present user password database host port <<<"$parts"
  # Expose presence separately from the decoded value.  An explicitly empty
  # URI password (`postgres://user:@host/db`) is different from an omitted
  # password and must not accidentally fall back to a password imported from
  # `.env` when a Docker client is used.
  export CEX_DATABASE_URL_PASSWORD_PRESENT="$password_present"
  export CEX_DATABASE_URL_SYNCED=1
  [[ -n "$user" ]] && export CEX_POSTGRES_USER="$user"
  if [[ "$password_present" == "1" ]]; then
    export CEX_POSTGRES_PASSWORD="$password"
  fi
  [[ -n "$database" ]] && export CEX_POSTGRES_DB="$database"
  # These are informational defaults for callers that need to choose between
  # a container socket and a full URI.  They do not alter existing behavior.
  export CEX_POSTGRES_HOST="$host"
  export CEX_POSTGRES_PORT="$port"
}

cex_postgres_host_is_local() {
  [[ "${CEX_DATABASE_URL_SYNCED:-0}" != "1" ]] || {
    case "${CEX_POSTGRES_HOST:-}" in
      127.0.0.1|localhost|::1) return 0 ;;
      *) return 1 ;;
    esac
  }
  # Callers that have not opted into URL synchronization retain the helper's
  # historical Docker-socket behavior.
  return 0
}

cex_postgres_password_is_set() {
  [[ ${PGPASSWORD+x} ]] ||
    [[ "${CEX_DATABASE_URL_PASSWORD_PRESENT:-0}" == "1" ]] ||
    [[ -n "${CEX_POSTGRES_PASSWORD:-}" ]]
}

cex_postgres_password_value() {
  if [[ ${PGPASSWORD+x} ]]; then
    printf '%s\n' "$PGPASSWORD"
  else
    printf '%s\n' "${CEX_POSTGRES_PASSWORD:-}"
  fi
}

# Print the supplied PostgreSQL URI with only its database path replaced.  URI
# query options (notably sslmode/connect_timeout) must survive temporary
# database drills; string slicing at the last slash drops those options and
# can redirect a check to a different connection policy.  Database names are
# deliberately restricted to SQL identifier characters because callers use the
# result for short-lived CREATE/DROP DATABASE probes.
cex_database_url_for_database() {
  local database="$1"
  local url="${2:-$(cex_effective_database_url)}"
  if [[ ! "$database" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
    echo "unsafe PostgreSQL database name: $database" >&2
    return 2
  fi
  if ! command -v python3 >/dev/null 2>&1; then
    echo "database URL rewriting requires python3" >&2
    return 127
  fi
  python3 - "$url" "$database" <<'PY'
from urllib.parse import urlsplit, urlunsplit
import sys

try:
    source = urlsplit(sys.argv[1])
    database = sys.argv[2]
    if source.scheme not in {"postgres", "postgresql"} or not source.netloc:
        raise ValueError("DATABASE_URL must be a PostgreSQL URI")
    if source.fragment:
        raise ValueError("DATABASE_URL fragments are not supported")
    # Validate the port even though it is retained verbatim in netloc.
    _ = source.port
except (ValueError, UnicodeError) as error:
    raise SystemExit(f"invalid PostgreSQL DATABASE_URL: {error}") from error
print(urlunsplit((source.scheme, source.netloc, "/" + database, source.query, "")))
PY
}

cex_has_local_psql() {
  command -v psql >/dev/null 2>&1
}

cex_has_local_pg_isready() {
  command -v pg_isready >/dev/null 2>&1
}

cex_docker() {
  if [[ "${CEX_DOCKER_USE_SUDO:-0}" == "1" ]]; then
    sudo -n docker "$@"
    return $?
  fi

  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    docker "$@"
    return $?
  fi

  if command -v sudo >/dev/null 2>&1 && sudo -n docker info >/dev/null 2>&1; then
    sudo -n docker "$@"
    return $?
  fi

  docker "$@"
}

cex_can_use_docker_postgres() {
  command -v docker >/dev/null 2>&1 && \
    cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" true >/dev/null 2>&1
}

cex_wait_postgres() {
  local attempts="${1:-60}"
  local delay="${2:-1}"
  local try

  for try in $(seq 1 "$attempts"); do
    if cex_has_local_pg_isready; then
      if pg_isready -d "$(cex_effective_database_url)" >/dev/null 2>&1; then
        return 0
      fi
    elif cex_has_local_psql; then
      if [[ ${PGPASSWORD+x} ]]; then
        if PGPASSWORD="$PGPASSWORD" psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 -c 'select 1' >/dev/null 2>&1; then
          return 0
        fi
      elif [[ "${CEX_DATABASE_URL_PASSWORD_PRESENT:-0}" == "1" ]]; then
        if psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 -c 'select 1' >/dev/null 2>&1; then
          return 0
        fi
      elif PGPASSWORD="${CEX_POSTGRES_PASSWORD:-}" \
        psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 -c 'select 1' >/dev/null 2>&1; then
        return 0
      fi
    elif cex_can_use_docker_postgres; then
      local -a readiness_args
      if ! cex_postgres_host_is_local; then
        # The container socket is only the right target for a local host.  For
        # an explicitly selected service/remote host, retain the full URI so a
        # readiness probe cannot report a different database as healthy.
        readiness_args=(pg_isready -d "$(cex_effective_database_url)")
      else
        readiness_args=(pg_isready -U "$CEX_POSTGRES_USER" -d "$CEX_POSTGRES_DB")
      fi
      if cex_postgres_password_is_set; then
        local readiness_password
        readiness_password="$(cex_postgres_password_value)"
        if cex_docker exec -e "PGPASSWORD=$readiness_password" \
          "$CEX_POSTGRES_CONTAINER_NAME" "${readiness_args[@]}" >/dev/null 2>&1; then
          return 0
        fi
      elif cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" "${readiness_args[@]}" >/dev/null 2>&1; then
        return 0
      fi
    else
      if exec 3<>/dev/tcp/127.0.0.1/5432; then
        exec 3<&-
        exec 3>&-
        return 0
      fi
    fi
    sleep "$delay"
  done

  echo "postgres did not become ready after ${attempts} attempts" >&2
  return 1
}

cex_psql_stdin() {
  if cex_has_local_psql; then
    if [[ ${PGPASSWORD+x} ]]; then
      PGPASSWORD="$PGPASSWORD" psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 "$@"
    elif [[ "${CEX_DATABASE_URL_PASSWORD_PRESENT:-0}" == "1" ]]; then
      # Let libpq consume the password embedded in DATABASE_URL.  Setting the
      # helper's convenience password here would override a caller-selected
      # URI and commonly turns a valid CI credential into an auth failure.
      psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 "$@"
    elif [[ -n "${CEX_POSTGRES_PASSWORD:-}" ]]; then
      PGPASSWORD="$CEX_POSTGRES_PASSWORD" psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 "$@"
    else
      psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 "$@"
    fi
    return 0
  fi

  if cex_can_use_docker_postgres; then
    local -a psql_args
    if ! cex_postgres_host_is_local; then
      # A Docker fallback must not silently query its local socket when the
      # caller selected a service/remote host.  Preserve URI query options and
      # replace only the database component for the operation.
      psql_args=(psql "$(cex_database_url_for_database "$CEX_POSTGRES_DB")" \
        -v ON_ERROR_STOP=1)
    else
      psql_args=(psql -U "$CEX_POSTGRES_USER" -d "$CEX_POSTGRES_DB" \
        -v ON_ERROR_STOP=1)
    fi
    if cex_postgres_password_is_set; then
      local docker_password
      docker_password="$(cex_postgres_password_value)"
      cex_docker exec -i -e "PGPASSWORD=$docker_password" \
        "$CEX_POSTGRES_CONTAINER_NAME" "${psql_args[@]}" "$@"
    else
      cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" \
        "${psql_args[@]}" "$@"
    fi
    return 0
  fi

  echo "no usable postgres client found; install psql, grant docker access, or use --skip-db-bootstrap when the DB is already provisioned" >&2
  return 1
}

cex_apply_migrations() {
  local migration
  for migration in "$CEX_PROJECT_ROOT"/migrations/*.sql; do
    [[ -f "$migration" ]] || continue
    echo "==> applying $(basename "$migration")"
    cex_psql_stdin -f - < "$migration"
  done
}

cex_seed_local_dev() {
  local key_hash="ed5a18fb8f807f996d649e379d3f35f39c543a91bdbf88c492f2ebd10d4df86c"
  cex_psql_stdin -f - <<SQL
insert into organizations (org_id, name, status, plan)
values ('00000000-0000-0000-0000-00000000ce01', 'Local Dev Org', 'active', 'dev')
on conflict (org_id) do update set name = excluded.name, status = excluded.status, plan = excluded.plan, updated_at = now();

insert into users (user_id, org_id, email, role, status)
values ('00000000-0000-0000-0000-00000000ce11', '00000000-0000-0000-0000-00000000ce01', 'local-dev@example.test', 'owner', 'active')
on conflict (user_id) do update set org_id = excluded.org_id, email = excluded.email, role = excluded.role, status = excluded.status, updated_at = now();

insert into api_keys (api_key_id, org_id, user_id, key_hash, key_prefix, label, status, revoked_at, expires_at)
values (
  '00000000-0000-0000-0000-00000000ce21',
  '00000000-0000-0000-0000-00000000ce01',
  '00000000-0000-0000-0000-00000000ce11',
  '$key_hash',
  'local-dev',
  'Local Dev Key',
  'active',
  null,
  null
)
on conflict (key_hash) do update
set org_id = excluded.org_id,
    user_id = excluded.user_id,
    key_prefix = excluded.key_prefix,
    label = excluded.label,
    status = excluded.status,
    revoked_at = null,
    expires_at = null;

insert into accounts (account_id, org_id, account_type, currency_unit, status, balance, reserved)
values (
  '00000000-0000-0000-0000-00000000ce31',
  '00000000-0000-0000-0000-00000000ce01',
  'org_wallet',
  'credit',
  'active',
  1000,
  0
)
on conflict (account_id) do update
set org_id = excluded.org_id,
    account_type = excluded.account_type,
    currency_unit = excluded.currency_unit,
    status = excluded.status;
SQL
}

#!/usr/bin/env bash
set -euo pipefail

CEX_SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CEX_PROJECT_ROOT="$(cd -- "$CEX_SCRIPT_DIR/.." && pwd)"
# A helper can be sourced more than once in an operator shell.  Keep a small
# snapshot of values that existed before the first source, then restore that
# snapshot before reinitializing.  This prevents a URI-derived host/database/
# password from being mistaken for a caller override on the next source.
if [[ "${CEX_HELPER_SOURCE_ACTIVE:-0}" == "1" \
      && "${CEX_HELPER_SOURCE_BASHPID:-}" == "$BASHPID" ]]; then
  if [[ "${CEX_HELPER_ORIGINAL_PGPASSWORD_SET:-0}" == "1" ]]; then
    export PGPASSWORD="${CEX_HELPER_ORIGINAL_PGPASSWORD}"
  else
    unset PGPASSWORD
  fi
  if [[ "${CEX_HELPER_ORIGINAL_CONTAINER_SET:-0}" == "1" ]]; then
    CEX_POSTGRES_CONTAINER_NAME="${CEX_HELPER_ORIGINAL_CONTAINER}"
  else
    unset CEX_POSTGRES_CONTAINER_NAME
  fi
  if [[ "${CEX_HELPER_ORIGINAL_USER_SET:-0}" == "1" ]]; then
    CEX_POSTGRES_USER="${CEX_HELPER_ORIGINAL_USER}"
  else
    unset CEX_POSTGRES_USER
  fi
  if [[ "${CEX_HELPER_ORIGINAL_DB_SET:-0}" == "1" ]]; then
    CEX_POSTGRES_DB="${CEX_HELPER_ORIGINAL_DB}"
  else
    unset CEX_POSTGRES_DB
  fi
  if [[ "${CEX_HELPER_ORIGINAL_CEX_PASSWORD_SET:-0}" == "1" ]]; then
    CEX_POSTGRES_PASSWORD="${CEX_HELPER_ORIGINAL_CEX_PASSWORD}"
  else
    unset CEX_POSTGRES_PASSWORD
  fi
  unset CEX_POSTGRES_HOST CEX_POSTGRES_PORT CEX_POSTGRES_PASSWORD_EXPLICIT
  CEX_DATABASE_URL_SYNCED=0
  CEX_DATABASE_URL_PASSWORD_PRESENT=0
  CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC=0
  CEX_PGPASSWORD_URI_SUPPRESSED=0
else
  # A child shell can inherit the exported synchronization markers without
  # inheriting the non-exported snapshot above.  Clear values known to be
  # URL-derived before treating the environment as a fresh caller context.
  if [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" ]]; then
    unset CEX_POSTGRES_USER CEX_POSTGRES_DB CEX_POSTGRES_HOST CEX_POSTGRES_PORT
    if [[ "${CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC:-0}" == "1" ]]; then
      unset CEX_POSTGRES_PASSWORD CEX_POSTGRES_PASSWORD_EXPLICIT
    fi
    CEX_DATABASE_URL_SYNCED=0
    CEX_DATABASE_URL_PASSWORD_PRESENT=0
    CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC=0
    CEX_PGPASSWORD_URI_SUPPRESSED=0
  fi
  if [[ ${PGPASSWORD+x} ]]; then
    CEX_HELPER_ORIGINAL_PGPASSWORD_SET=1
    CEX_HELPER_ORIGINAL_PGPASSWORD="$PGPASSWORD"
  else
    CEX_HELPER_ORIGINAL_PGPASSWORD_SET=0
    CEX_HELPER_ORIGINAL_PGPASSWORD=""
  fi
  if [[ ${CEX_POSTGRES_CONTAINER_NAME+x} ]]; then
    CEX_HELPER_ORIGINAL_CONTAINER_SET=1
    CEX_HELPER_ORIGINAL_CONTAINER="$CEX_POSTGRES_CONTAINER_NAME"
  else
    CEX_HELPER_ORIGINAL_CONTAINER_SET=0
    CEX_HELPER_ORIGINAL_CONTAINER=""
  fi
  if [[ ${CEX_POSTGRES_USER+x} ]]; then
    CEX_HELPER_ORIGINAL_USER_SET=1
    CEX_HELPER_ORIGINAL_USER="$CEX_POSTGRES_USER"
  else
    CEX_HELPER_ORIGINAL_USER_SET=0
    CEX_HELPER_ORIGINAL_USER=""
  fi
  if [[ ${CEX_POSTGRES_DB+x} ]]; then
    CEX_HELPER_ORIGINAL_DB_SET=1
    CEX_HELPER_ORIGINAL_DB="$CEX_POSTGRES_DB"
  else
    CEX_HELPER_ORIGINAL_DB_SET=0
    CEX_HELPER_ORIGINAL_DB=""
  fi
  if [[ ${CEX_POSTGRES_PASSWORD+x} ]]; then
    CEX_HELPER_ORIGINAL_CEX_PASSWORD_SET=1
    CEX_HELPER_ORIGINAL_CEX_PASSWORD="$CEX_POSTGRES_PASSWORD"
  else
    CEX_HELPER_ORIGINAL_CEX_PASSWORD_SET=0
    CEX_HELPER_ORIGINAL_CEX_PASSWORD=""
  fi
fi

# Capture the operator's PGPASSWORD before any repository `.env` file is
# loaded.  libpq gives PGPASSWORD precedence over a URI credential, so a value
# imported from `.env` must not silently shadow a caller-selected URI password.
CEX_CALLER_PGPASSWORD_SET="${CEX_HELPER_ORIGINAL_PGPASSWORD_SET:-0}"
CEX_CALLER_PGPASSWORD="${CEX_HELPER_ORIGINAL_PGPASSWORD:-}"
CEX_PGPASSWORD_LOADED_FROM_ENV=0
CEX_PGPASSWORD_LOADED_VALUE=""
CEX_PGPASSWORD_URI_SUPPRESSED=0
CEX_HELPER_CALLER_CEX_PASSWORD_SET="${CEX_HELPER_ORIGINAL_CEX_PASSWORD_SET:-0}"
CEX_HELPER_CALLER_CEX_PASSWORD="${CEX_HELPER_ORIGINAL_CEX_PASSWORD:-}"
CEX_CEX_PASSWORD_LOADED_FROM_ENV=0
CEX_CEX_PASSWORD_LOADED_VALUE=""
CEX_CEX_PASSWORD_RUNTIME_SET=0
CEX_CEX_PASSWORD_RUNTIME_VALUE=""
CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC=0
CEX_POSTGRES_CONTAINER_NAME_PRESET=0
[[ "${CEX_HELPER_ORIGINAL_CONTAINER_SET:-0}" == "1" ]] && CEX_POSTGRES_CONTAINER_NAME_PRESET=1
CEX_POSTGRES_USER_PRESET=0
[[ "${CEX_HELPER_ORIGINAL_USER_SET:-0}" == "1" ]] && CEX_POSTGRES_USER_PRESET=1
CEX_POSTGRES_DB_PRESET=0
[[ "${CEX_HELPER_ORIGINAL_DB_SET:-0}" == "1" ]] && CEX_POSTGRES_DB_PRESET=1
CEX_POSTGRES_PASSWORD_PRESET=0
[[ "${CEX_HELPER_ORIGINAL_CEX_PASSWORD_SET:-0}" == "1" ]] && CEX_POSTGRES_PASSWORD_PRESET=1
CEX_POSTGRES_CONTAINER_NAME="${CEX_POSTGRES_CONTAINER_NAME:-cex-postgres-1}"
CEX_POSTGRES_USER="${CEX_POSTGRES_USER:-postgres}"
CEX_POSTGRES_DB="${CEX_POSTGRES_DB:-cex_ai}"
if [[ "${CEX_HELPER_ORIGINAL_CEX_PASSWORD_SET:-0}" == "1" ]]; then
  CEX_POSTGRES_PASSWORD_EXPLICIT=1
else
  CEX_POSTGRES_PASSWORD_EXPLICIT=0
fi
# Preserve an explicitly empty caller password.  `:-` would silently turn an
# intentional empty credential (for example peer auth or a passwordless URI)
# into the convenience `postgres` password.
CEX_POSTGRES_PASSWORD="${CEX_POSTGRES_PASSWORD-postgres}"
CEX_DATABASE_URL_DEFAULT="postgres://postgres:postgres@127.0.0.1:5432/cex_ai"
# These markers let callers distinguish values parsed from DATABASE_URL (where
# an explicitly empty password is meaningful) from the helper's local defaults.
# They are deliberately kept private-ish and exported only when a URL is
# synchronized below.
CEX_DATABASE_URL_SYNCED="${CEX_DATABASE_URL_SYNCED:-0}"
CEX_HELPER_SOURCE_ACTIVE=1
CEX_HELPER_SOURCE_BASHPID="$BASHPID"

cex_clear_database_url_sync_state() {
  # Restore caller-owned values (or helper defaults) after DATABASE_URL is
  # removed or changed to a non-PostgreSQL scheme.  This is intentionally
  # separate from `cex_sync...`: callers may invoke cex_load_env repeatedly
  # without re-sourcing the helper.
  if [[ "${CEX_HELPER_ORIGINAL_USER_SET:-0}" == "1" ]]; then
    CEX_POSTGRES_USER="$CEX_HELPER_ORIGINAL_USER"
  else
    CEX_POSTGRES_USER="postgres"
  fi
  if [[ "${CEX_HELPER_ORIGINAL_DB_SET:-0}" == "1" ]]; then
    CEX_POSTGRES_DB="$CEX_HELPER_ORIGINAL_DB"
  else
    CEX_POSTGRES_DB="cex_ai"
  fi
  CEX_POSTGRES_USER_PRESET="${CEX_HELPER_ORIGINAL_USER_SET:-0}"
  CEX_POSTGRES_DB_PRESET="${CEX_HELPER_ORIGINAL_DB_SET:-0}"
  if [[ "${CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC:-0}" == "1" ]]; then
    if [[ "${CEX_HELPER_CALLER_CEX_PASSWORD_SET:-0}" == "1" ]]; then
      CEX_POSTGRES_PASSWORD="$CEX_HELPER_CALLER_CEX_PASSWORD"
      CEX_POSTGRES_PASSWORD_EXPLICIT=1
      CEX_POSTGRES_PASSWORD_PRESET=1
    elif [[ "${CEX_CEX_PASSWORD_LOADED_FROM_ENV:-0}" == "1" ]]; then
      CEX_POSTGRES_PASSWORD="$CEX_CEX_PASSWORD_LOADED_VALUE"
      CEX_POSTGRES_PASSWORD_EXPLICIT=1
      CEX_POSTGRES_PASSWORD_PRESET=0
    elif [[ "${CEX_CEX_PASSWORD_RUNTIME_SET:-0}" == "1" ]]; then
      CEX_POSTGRES_PASSWORD="$CEX_CEX_PASSWORD_RUNTIME_VALUE"
      CEX_POSTGRES_PASSWORD_EXPLICIT=1
      CEX_POSTGRES_PASSWORD_PRESET=0
    else
      CEX_POSTGRES_PASSWORD="postgres"
      CEX_POSTGRES_PASSWORD_EXPLICIT=0
      CEX_POSTGRES_PASSWORD_PRESET=0
    fi
  elif [[ "${CEX_HELPER_ORIGINAL_CEX_PASSWORD_SET:-0}" == "1" ]]; then
    CEX_POSTGRES_PASSWORD="$CEX_HELPER_ORIGINAL_CEX_PASSWORD"
    CEX_POSTGRES_PASSWORD_EXPLICIT=1
    CEX_POSTGRES_PASSWORD_PRESET=1
  else
    # Preserve an explicitly empty value while restoring the helper default
    # only when the variable is genuinely unset.
    CEX_POSTGRES_PASSWORD="${CEX_POSTGRES_PASSWORD-postgres}"
    CEX_POSTGRES_PASSWORD_EXPLICIT="${CEX_POSTGRES_PASSWORD_EXPLICIT:-0}"
    CEX_POSTGRES_PASSWORD_PRESET=0
  fi
  if [[ "${CEX_PGPASSWORD_URI_SUPPRESSED:-0}" == "1" \
        && "${CEX_PGPASSWORD_LOADED_FROM_ENV:-0}" == "1" ]]; then
    export PGPASSWORD="$CEX_PGPASSWORD_LOADED_VALUE"
  fi
  CEX_DATABASE_URL_PASSWORD_PRESENT=0
  CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC=0
  CEX_DATABASE_URL_SYNCED=0
  CEX_POSTGRES_HOST="127.0.0.1"
  CEX_POSTGRES_PORT="5432"
  CEX_PGPASSWORD_URI_SUPPRESSED=0
  CEX_PGPASSWORD_LOADED_FROM_ENV=0
  CEX_PGPASSWORD_LOADED_VALUE=""
  CEX_CEX_PASSWORD_LOADED_FROM_ENV=0
  CEX_CEX_PASSWORD_LOADED_VALUE=""
}

cex_load_env() {
  local env_file="${1:-}"
  if [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" ]]; then
    case "${DATABASE_URL-}" in
      postgres://*|postgresql://*) ;;
      *) cex_clear_database_url_sync_state ;;
    esac
  fi
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
    if [[ ${DATABASE_URL+x} && -n "${DATABASE_URL}" ]]; then
      case "$DATABASE_URL" in
        postgres://*|postgresql://*)
          cex_sync_postgres_env_from_database_url "$DATABASE_URL"
          ;;
      esac
    fi
    return 0
  fi

  while IFS='' read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    [[ -z "$line" || "$line" == \#* ]] && continue
    # Keep normal shell-environment precedence: a caller's value wins over a
    # repository `.env` assignment.  The four CEX_POSTGRES_* variables are
    # initialized to helper defaults above; their *_PRESET markers let those
    # defaults still be filled from `.env` while preserving an explicitly
    # exported caller value.  Invalid/non-assignment lines retain the historic
    # behavior and are passed to `export`, which makes the loader fail closed.
    local key preset_var
    if [[ "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)= ]]; then
      key="${BASH_REMATCH[1]}"
      # If synchronization already established an explicit URI password,
      # never import a competing PGPASSWORD from a later `.env` load.
      if [[ "$key" == "PGPASSWORD" && "$CEX_CALLER_PGPASSWORD_SET" != "1" \
            && "${CEX_DATABASE_URL_PASSWORD_PRESENT:-0}" == "1" ]]; then
        continue
      fi
      # An explicitly supplied CEX password is the caller's credential
      # selection too.  Do not let a repository convenience PGPASSWORD shadow
      # it merely because libpq gives PGPASSWORD higher process precedence.
      if [[ "$key" == "PGPASSWORD" && "$CEX_CALLER_PGPASSWORD_SET" != "1" \
            && "$CEX_HELPER_CALLER_CEX_PASSWORD_SET" == "1" ]]; then
        continue
      fi
      # Likewise, preserve the decoded URI password across a later helper
      # reload.  Without this guard a second `cex_load_env` could replace the
      # selected target credential with a stale repository default.
      if [[ "$key" == "CEX_POSTGRES_PASSWORD" \
            && "${CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC:-0}" == "1" ]]; then
        continue
      fi
      if [[ ${!key+x} ]]; then
        preset_var="${key}_PRESET"
        # A missing marker means the variable came from the caller (the helper
        # only marks the defaults it initializes).  Preserve it; a marker of
        # zero means the helper default may be filled from `.env`.
        if [[ "${!preset_var:-caller}" != "0" ]]; then
          continue
        fi
      fi
    fi
    case "$line" in
      CEX_POSTGRES_PASSWORD=*)
        CEX_POSTGRES_PASSWORD_EXPLICIT=1
        if [[ "$CEX_HELPER_CALLER_CEX_PASSWORD_SET" != "1" ]]; then
          CEX_CEX_PASSWORD_LOADED_FROM_ENV=1
          CEX_CEX_PASSWORD_LOADED_VALUE="${line#CEX_POSTGRES_PASSWORD=}"
        fi
        ;;
      PGPASSWORD=*)
        if [[ "$CEX_CALLER_PGPASSWORD_SET" != "1" ]]; then
          CEX_PGPASSWORD_LOADED_FROM_ENV=1
          CEX_PGPASSWORD_LOADED_VALUE="${line#PGPASSWORD=}"
        fi
        ;;
    esac
    export "$line"
  done < "$env_file"

  # Most local entrypoints only need `cex_load_env`; synchronize an imported
  # DATABASE_URL here so the credential-stripped client path still carries its
  # decoded password and selects the same user/database.  Explicit callers
  # that invoke synchronization themselves remain idempotent.
  if [[ ${DATABASE_URL+x} && -n "${DATABASE_URL}" ]]; then
    case "$DATABASE_URL" in
      postgres://*|postgresql://*)
        cex_sync_postgres_env_from_database_url "$DATABASE_URL"
        ;;
    esac
  fi
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

# Return a structurally equivalent PostgreSQL URI with its password removed.
# Keep the raw user/host/path/query spelling (including percent-encoding and
# IPv6 brackets) so libpq sees the same target and connection options.  The
# password is supplied separately through PGPASSWORD or a Docker env-file;
# passing the original URI as a command argument would expose it in process
# listings.  URL input is sent over fd 3 rather than Python argv for the same
# reason.
cex_database_url_without_password() {
  local url="${1:-$(cex_effective_database_url)}"
  if ! command -v python3 >/dev/null 2>&1; then
    echo "credential-stripped database URL handling requires python3" >&2
    return 127
  fi
  python3 - 3<<<"$url" <<'PY'
from urllib.parse import parse_qsl, unquote, urlsplit, urlunsplit
import os

try:
    raw_url = os.fdopen(3, encoding="utf-8").read().rstrip("\n")
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in raw_url):
        raise ValueError("DATABASE_URL contains a control character")
    source = urlsplit(raw_url)
    if source.scheme not in {"postgres", "postgresql"} or not source.netloc:
        raise ValueError("DATABASE_URL must be a PostgreSQL URI with a host")
    if not source.hostname:
        raise ValueError("DATABASE_URL must include a host")
    username = unquote(source.username or "")
    if not username:
        raise ValueError("DATABASE_URL must include a user")
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in username):
        raise ValueError("DATABASE_URL user contains a control character")
    if source.fragment:
        raise ValueError("DATABASE_URL fragments are not supported")
    # Accessing .port performs the standard format/range validation.
    _ = source.port
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
            raise ValueError(f"DATABASE_URL query parameter is not allowed: {key}")
        if any(ord(char) < 0x20 or ord(char) == 0x7f for char in key + value):
            raise ValueError("DATABASE_URL query contains a control character")

    # `urlsplit().netloc` retains the original percent-encoding.  A raw colon
    # (not an encoded %3A) separates username from password; remove everything
    # after that colon while retaining the exact username and host-port bytes.
    authority = source.netloc
    if "@" in authority:
        userinfo, hostport = authority.rsplit("@", 1)
        username = userinfo.split(":", 1)[0]
        authority = f"{username}@{hostport}"
except (ValueError, UnicodeError) as error:
    raise SystemExit(f"invalid PostgreSQL DATABASE_URL: {error}") from error

print(urlunsplit((source.scheme, authority, source.path, source.query, "")))
PY
}

# Synchronize the Docker/local PostgreSQL connection defaults with an explicit
# DATABASE_URL.  A number of operator drills source this helper (and therefore
# load the repository .env) before they connect to a caller-selected disposable
# database.  Keeping CEX_POSTGRES_* at their convenience defaults in that case
# can silently authenticate as the wrong role or open the wrong database.  URI
# credentials are decoded once, without printing them, and control characters
# are rejected before they reach an environment variable or a command line.
#
# `cex_load_env` invokes this for an imported PostgreSQL URL; callers that need
# to opt out can avoid loading a URL (or invoke the helper before assigning it).
cex_sync_postgres_env_from_database_url() {
  local url="${1:-$(cex_effective_database_url)}"
  if ! command -v python3 >/dev/null 2>&1; then
    echo "explicit DATABASE_URL synchronization requires python3" >&2
    return 127
  fi

  local parts
  if ! parts="$(python3 - 3<<<"$url" <<'PY'
from urllib.parse import parse_qsl, unquote, urlsplit
import os
import sys

try:
    raw_url = os.fdopen(3, encoding="utf-8").read().rstrip("\n")
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in raw_url):
        raise ValueError("DATABASE_URL contains a control character")
    source = urlsplit(raw_url)
    if source.scheme not in {"postgres", "postgresql"} or not source.netloc or not source.hostname:
        raise ValueError("DATABASE_URL must be a PostgreSQL URI with a host")
    if source.fragment:
        raise ValueError("DATABASE_URL fragments are not supported")
    # libpq accepts connection parameters in the URI query and gives them
    # precedence over the path/authority components.  A temporary-database
    # probe must never be able to rewrite `/safe_db` while `?dbname=prod` (or
    # `?host=another-server`) silently selects a different target.  Reject all
    # parameters that can alter identity, routing, or startup options rather
    # than trying to canonicalize a query we do not own.
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
            raise ValueError(f"DATABASE_URL query parameter is not allowed: {key}")
        if any(ord(char) < 0x20 or ord(char) == 0x7f for char in key + value):
            raise ValueError("DATABASE_URL query contains a control character")
    # Accessing .port performs the standard range/format validation.  PostgreSQL
    # uses 5432 when the URI omits a port; keep that effective target explicit
    # so Docker/socket and TCP checks do not see an empty port as an unknown
    # server.
    port = source.port or 5432
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
if not user:
    raise SystemExit("DATABASE_URL must include a user for Docker/local parity")
# Unit separator is not valid in any decoded URI component (control
# characters are rejected below) and, unlike a tab, Bash does not collapse
# adjacent non-whitespace delimiters.  This preserves intentionally empty
# user/password/host fields during the shell read.
print("\x1f".join((password_present, user, password, database, host,
                  str(port))))
PY
)"; then
    echo "cannot synchronize PostgreSQL defaults from DATABASE_URL" >&2
    return 2
  fi

  local password_present user password database host port
  local previous_uri_password_set="${CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC:-0}"
  local previous_uri_password="${CEX_POSTGRES_PASSWORD-}"
  IFS=$'\x1f' read -r password_present user password database host port <<<"$parts"
  # If a caller supplied a CEX password after sourcing this helper, retain it
  # as the value to restore when a later sync switches away from a URI
  # credential.  Values imported from `.env` and supplied before sourcing are
  # tracked separately above.
  if [[ "${CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC:-0}" != "1" \
        && "${CEX_POSTGRES_PASSWORD_EXPLICIT:-0}" == "1" \
        && "$CEX_HELPER_CALLER_CEX_PASSWORD_SET" != "1" \
        && "$CEX_CEX_PASSWORD_LOADED_FROM_ENV" != "1" ]]; then
    CEX_CEX_PASSWORD_RUNTIME_SET=1
    CEX_CEX_PASSWORD_RUNTIME_VALUE="${CEX_POSTGRES_PASSWORD:-}"
  fi
  # Expose presence separately from the decoded value.  An explicitly empty
  # URI password (`postgres://user:@host/db`) is different from an omitted
  # password and must not accidentally fall back to a password imported from
  # `.env` when a Docker client is used.
  export CEX_DATABASE_URL_PASSWORD_PRESENT="$password_present"
  export CEX_DATABASE_URL_SYNCED=1
  [[ -n "$user" ]] && export CEX_POSTGRES_USER="$user"
  if [[ "$password_present" == "1" ]]; then
    export CEX_POSTGRES_PASSWORD="$password"
    export CEX_POSTGRES_PASSWORD_EXPLICIT=1
    export CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC=1
    if [[ ${PGPASSWORD+x} && "$CEX_CALLER_PGPASSWORD_SET" != "1" \
          && "$CEX_PGPASSWORD_LOADED_FROM_ENV" == "1" \
          && "$PGPASSWORD" == "$CEX_PGPASSWORD_LOADED_VALUE" ]]; then
      # URI credentials are the selected target's authority when the caller
      # did not explicitly provide PGPASSWORD.  Remove only the value that was
      # imported by this helper; the decoded URI password remains in the
      # private CEX_POSTGRES_PASSWORD variable and env-file wrappers.
      unset PGPASSWORD
      CEX_PGPASSWORD_URI_SUPPRESSED=1
    elif [[ ${PGPASSWORD+x} ]]; then
      # A value that is currently present and was not imported from the
      # repository env is an explicit libpq override.  In particular, honor a
      # caller that deliberately unset an earlier captured value before this
      # synchronization call rather than resurrecting the old secret.
      CEX_PGPASSWORD_URI_SUPPRESSED=0
    else
      CEX_PGPASSWORD_URI_SUPPRESSED=0
    fi
  elif [[ "$previous_uri_password_set" == "1" ]]; then
    # A repeated sync can move from a password-bearing URI to one that relies
    # on peer/.pgpass authentication.  Do not leave the old URI secret in the
    # CEX defaults, and restore an original caller/.env credential only when it
    # actually existed before the URI override.
    if [[ ${CEX_POSTGRES_PASSWORD+x} && "$CEX_POSTGRES_PASSWORD" != "$previous_uri_password" ]]; then
      # The caller changed the value after the previous URI synchronization;
      # preserve that explicit override instead of restoring the old URI
      # credential.
      export CEX_POSTGRES_PASSWORD_EXPLICIT=1
    elif [[ "$CEX_HELPER_CALLER_CEX_PASSWORD_SET" == "1" ]]; then
      export CEX_POSTGRES_PASSWORD="$CEX_HELPER_CALLER_CEX_PASSWORD"
      export CEX_POSTGRES_PASSWORD_EXPLICIT=1
    elif [[ "$CEX_CEX_PASSWORD_LOADED_FROM_ENV" == "1" ]]; then
      export CEX_POSTGRES_PASSWORD="$CEX_CEX_PASSWORD_LOADED_VALUE"
      export CEX_POSTGRES_PASSWORD_EXPLICIT=1
    elif [[ "$CEX_CEX_PASSWORD_RUNTIME_SET" == "1" ]]; then
      export CEX_POSTGRES_PASSWORD="$CEX_CEX_PASSWORD_RUNTIME_VALUE"
      export CEX_POSTGRES_PASSWORD_EXPLICIT=1
    else
      export CEX_POSTGRES_PASSWORD=postgres
      export CEX_POSTGRES_PASSWORD_EXPLICIT=0
    fi
    export CEX_DATABASE_URL_PASSWORD_SET_BY_SYNC=0
    if [[ ! ${PGPASSWORD+x} && "$CEX_PGPASSWORD_URI_SUPPRESSED" == "1" \
          && "$CEX_PGPASSWORD_LOADED_FROM_ENV" == "1" ]]; then
      export PGPASSWORD="$CEX_PGPASSWORD_LOADED_VALUE"
      CEX_PGPASSWORD_URI_SUPPRESSED=0
    else
      # Keep the caller's current state (including an intentional unset).
      CEX_PGPASSWORD_URI_SUPPRESSED=0
    fi
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
    [[ "${CEX_POSTGRES_PASSWORD_EXPLICIT:-0}" == "1" ]] ||
    # Keep the historical local-dev default usable when no explicit URL has
    # been synchronized: cex_effective_database_url() then points at the
    # built-in postgres:postgres endpoint, whose password is now stripped from
    # the client argv.  Once a caller has synchronized a passwordless URI,
    # however, the helper's convenience value must not become an implicit
    # credential and override peer/.pgpass authentication.
    [[ "${CEX_DATABASE_URL_SYNCED:-0}" != "1" &&
       -n "${CEX_POSTGRES_PASSWORD:-}" ]]
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
  python3 - "$database" 3<<<"$url" <<'PY'
from urllib.parse import parse_qsl, unquote, urlsplit, urlunsplit
import os
import sys

try:
    source = urlsplit(os.fdopen(3, encoding="utf-8").read().rstrip("\n"))
    database = sys.argv[1]
    if source.scheme not in {"postgres", "postgresql"} or not source.netloc:
        raise ValueError("DATABASE_URL must be a PostgreSQL URI")
    if not source.hostname:
        raise ValueError("DATABASE_URL must include a host")
    username = unquote(source.username or "")
    if not username:
        raise ValueError("DATABASE_URL must include a user")
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in username):
        raise ValueError("DATABASE_URL user contains a control character")
    if source.fragment:
        raise ValueError("DATABASE_URL fragments are not supported")
    # Validate the port even though it is retained verbatim in netloc.
    _ = source.port
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
            raise ValueError(f"DATABASE_URL query parameter is not allowed: {key}")
        if any(ord(char) < 0x20 or ord(char) == 0x7f for char in key + value):
            raise ValueError("DATABASE_URL query contains a control character")
except (ValueError, UnicodeError) as error:
    raise SystemExit(f"invalid PostgreSQL DATABASE_URL: {error}") from error
print(urlunsplit((source.scheme, source.netloc, "/" + database, source.query, "")))
PY
}

# Docker's `exec -e PGPASSWORD=...` puts the secret in the host process argv,
# where it can be observed by another local user or a process monitor.  Docker
# supports an env-file for exec; pass an anonymous process-substitution pipe so
# the secret enters only the container environment and never the command line
# or a host filesystem path.  An anonymous fd also disappears if the caller is
# interrupted before Docker returns, so a SIGTERM cannot leave a credential
# file behind.  The stdin variant keeps `-i` for callers that stream SQL or a
# dump archive.
cex_docker_exec_with_password() {
  local password="${1-}"
  shift
  if [[ "$password" == *$'\n'* || "$password" == *$'\r'* ||
        "$password" == *$'\t'* || "$password" == *$'\v'* ||
        "$password" == *$'\f'* ]]; then
    echo "refusing a PostgreSQL password containing a control character" >&2
    return 2
  fi
  local env_fd status
  if ! exec {env_fd}< <(printf 'PGPASSWORD=%s\n' "$password"); then
    echo "cannot create an anonymous PostgreSQL credential channel" >&2
    return 1
  fi
  # Resolve the fd through the shell that owns it, not `/proc/self`: when
  # cex_docker falls back to sudo, sudo starts a fresh process whose fd table
  # intentionally closes the anonymous descriptor.  The owning shell remains
  # alive and exposes the pipe to Docker through this read-only proc path.
  local env_path="/proc/${BASHPID}/fd/$env_fd"
  if cex_docker exec --env-file "$env_path" "$@"; then
    status=0
  else
    status=$?
  fi
  exec {env_fd}<&-
  return "$status"
}

cex_docker_exec_with_password_stdin() {
  local password="${1-}"
  shift
  if [[ "$password" == *$'\n'* || "$password" == *$'\r'* ||
        "$password" == *$'\t'* || "$password" == *$'\v'* ||
        "$password" == *$'\f'* ]]; then
    echo "refusing a PostgreSQL password containing a control character" >&2
    return 2
  fi
  local env_fd status
  if ! exec {env_fd}< <(printf 'PGPASSWORD=%s\n' "$password"); then
    echo "cannot create an anonymous PostgreSQL credential channel" >&2
    return 1
  fi
  local env_path="/proc/${BASHPID}/fd/$env_fd"
  if cex_docker exec -i --env-file "$env_path" "$@"; then
    status=0
  else
    status=$?
  fi
  exec {env_fd}<&-
  return "$status"
}

# The same argv-safe credential handoff for `docker run` containers.  Physical
# backup/PITR/standby drills use short-lived PostgreSQL client containers rather
# than `docker exec`; keeping this helper beside the exec variants prevents a
# later drill from regressing to `-e PGPASSWORD=...` on the host command line.
cex_docker_run_with_password() {
  local password="${1-}"
  shift
  if [[ "$password" == *$'\n'* || "$password" == *$'\r'* ||
        "$password" == *$'\t'* || "$password" == *$'\v'* ||
        "$password" == *$'\f'* ]]; then
    echo "refusing a PostgreSQL password containing a control character" >&2
    return 2
  fi
  local env_fd status
  if ! exec {env_fd}< <(printf 'PGPASSWORD=%s\n' "$password"); then
    echo "cannot create an anonymous PostgreSQL credential channel" >&2
    return 1
  fi
  local env_path="/proc/${BASHPID}/fd/$env_fd"
  if cex_docker run --env-file "$env_path" "$@"; then
    status=0
  else
    status=$?
  fi
  exec {env_fd}<&-
  return "$status"
}

cex_docker_run_with_password_stdin() {
  local password="${1-}"
  shift
  if [[ "$password" == *$'\n'* || "$password" == *$'\r'* ||
        "$password" == *$'\t'* || "$password" == *$'\v'* ||
        "$password" == *$'\f'* ]]; then
    echo "refusing a PostgreSQL password containing a control character" >&2
    return 2
  fi
  local env_fd status
  if ! exec {env_fd}< <(printf 'PGPASSWORD=%s\n' "$password"); then
    echo "cannot create an anonymous PostgreSQL credential channel" >&2
    return 1
  fi
  local env_path="/proc/${BASHPID}/fd/$env_fd"
  if cex_docker run -i --env-file "$env_path" "$@"; then
    status=0
  else
    status=$?
  fi
  exec {env_fd}<&-
  return "$status"
}

# A local-looking URI is not proof that the Docker container's socket is the
# selected server.  Verify the published host port before falling back to the
# socket; otherwise `127.0.0.1:55487` could accidentally query whichever
# container happens to be selected with its default port/database.
cex_postgres_docker_socket_is_target() {
  if [[ "${CEX_DATABASE_URL_SYNCED:-0}" != "1" ]]; then
    return 0
  fi
  local expected_port="${CEX_POSTGRES_PORT:-5432}"
  [[ "$expected_port" =~ ^[0-9]+$ ]] || return 1
  local mappings
  mappings="$(cex_docker port "$CEX_POSTGRES_CONTAINER_NAME" 5432/tcp 2>/dev/null || true)"
  if [[ -n "$mappings" ]]; then
    local mapping mapped_port
    while IFS= read -r mapping; do
      mapped_port="${mapping##*:}"
      [[ "$mapped_port" == "$expected_port" ]] && return 0
    done <<<"$mappings"
    return 1
  fi
  # Host networking has no published-port record; in that mode PostgreSQL's
  # default port is the only socket target we can prove.
  [[ "$expected_port" == "5432" ]] || return 1
  [[ "$(cex_docker inspect -f '{{.HostConfig.NetworkMode}}' \
      "$CEX_POSTGRES_CONTAINER_NAME" 2>/dev/null || true)" == "host" ]]
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
  local effective_url safe_url

  effective_url="$(cex_effective_database_url)"
  if ! safe_url="$(cex_database_url_without_password "$effective_url")"; then
    echo "cannot derive a credential-stripped PostgreSQL URL" >&2
    return 2
  fi

  for try in $(seq 1 "$attempts"); do
    if cex_has_local_pg_isready; then
      if cex_postgres_password_is_set; then
        local readiness_password
        readiness_password="$(cex_postgres_password_value)"
        if PGPASSWORD="$readiness_password" pg_isready -d "$safe_url" >/dev/null 2>&1; then
          return 0
        fi
      elif pg_isready -d "$safe_url" >/dev/null 2>&1; then
        return 0
      fi
    elif cex_has_local_psql; then
      if cex_postgres_password_is_set; then
        local psql_readiness_password
        psql_readiness_password="$(cex_postgres_password_value)"
        if PGPASSWORD="$psql_readiness_password" \
          psql "$safe_url" -v ON_ERROR_STOP=1 -c 'select 1' >/dev/null 2>&1; then
          return 0
        fi
      elif psql "$safe_url" -v ON_ERROR_STOP=1 -c 'select 1' >/dev/null 2>&1; then
        return 0
      fi
    elif cex_can_use_docker_postgres; then
      if cex_postgres_host_is_local && ! cex_postgres_docker_socket_is_target; then
        echo "Docker PostgreSQL container port does not match DATABASE_URL; refusing socket fallback" >&2
        return 2
      fi
      local -a readiness_args
      if ! cex_postgres_host_is_local; then
        # The container socket is only the right target for a local host.  For
        # an explicitly selected service/remote host, retain the full URI so a
        # readiness probe cannot report a different database as healthy.  The
        # password is supplied through the env-file below, never in argv.
        readiness_args=(pg_isready -d "$safe_url")
      else
        readiness_args=(pg_isready -U "$CEX_POSTGRES_USER" -d "$CEX_POSTGRES_DB")
      fi
      if cex_postgres_password_is_set; then
        local readiness_password
        readiness_password="$(cex_postgres_password_value)"
        if cex_docker_exec_with_password "$readiness_password" \
          "$CEX_POSTGRES_CONTAINER_NAME" "${readiness_args[@]}" >/dev/null 2>&1; then
          return 0
        fi
      elif cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" "${readiness_args[@]}" >/dev/null 2>&1; then
        return 0
      fi
    else
      # A raw TCP fallback is only a liveness hint.  It must still target the
      # synchronized host/port; probing a hard-coded 5432 can report a
      # different PostgreSQL instance as healthy after a failover.
      if [[ "${CEX_DATABASE_URL_SYNCED:-0}" == "1" ]]; then
        if ! cex_postgres_host_is_local; then
          echo "no PostgreSQL client available for the selected non-local host" >&2
          return 2
        fi
        local tcp_host="${CEX_POSTGRES_HOST:-127.0.0.1}"
        local tcp_port="${CEX_POSTGRES_PORT:-5432}"
        [[ "$tcp_host" == "::1" ]] && tcp_host="127.0.0.1"
        if [[ ! "$tcp_port" =~ ^[0-9]+$ ]] || (( tcp_port < 1 || tcp_port > 65535 )); then
          echo "invalid synchronized PostgreSQL port" >&2
          return 2
        fi
        if exec 3<>"/dev/tcp/$tcp_host/$tcp_port"; then
          exec 3<&-
          exec 3>&-
          return 0
        fi
      elif exec 3<>/dev/tcp/127.0.0.1/5432; then
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
    local effective_url safe_url
    effective_url="$(cex_effective_database_url)"
    if ! safe_url="$(cex_database_url_without_password "$effective_url")"; then
      echo "cannot derive a credential-stripped PostgreSQL URL" >&2
      return 2
    fi
    if cex_postgres_password_is_set; then
      local psql_password
      psql_password="$(cex_postgres_password_value)"
      if PGPASSWORD="$psql_password" psql "$safe_url" -v ON_ERROR_STOP=1 "$@"; then
        return 0
      else
        local psql_status=$?
        return "$psql_status"
      fi
    else
      if psql "$safe_url" -v ON_ERROR_STOP=1 "$@"; then
        return 0
      else
        local psql_status=$?
        return "$psql_status"
      fi
    fi
  fi

  if cex_can_use_docker_postgres; then
    if cex_postgres_host_is_local && ! cex_postgres_docker_socket_is_target; then
      echo "Docker PostgreSQL container port does not match DATABASE_URL; refusing socket fallback" >&2
      return 2
    fi
    local -a psql_args
    if ! cex_postgres_host_is_local; then
      # A Docker fallback must not silently query its local socket when the
      # caller selected a service/remote host.  Preserve URI query options and
      # replace only the database component for the operation.
      local remote_url remote_safe_url
      remote_url="$(cex_database_url_for_database "$CEX_POSTGRES_DB")"
      remote_safe_url="$(cex_database_url_without_password "$remote_url")"
      psql_args=(psql "$remote_safe_url" \
        -v ON_ERROR_STOP=1)
    else
      psql_args=(psql -U "$CEX_POSTGRES_USER" -d "$CEX_POSTGRES_DB" \
        -v ON_ERROR_STOP=1)
    fi
    if cex_postgres_password_is_set; then
      local docker_password
      docker_password="$(cex_postgres_password_value)"
      if cex_docker_exec_with_password_stdin "$docker_password" \
        "$CEX_POSTGRES_CONTAINER_NAME" "${psql_args[@]}" "$@"; then
        return 0
      else
        local docker_status=$?
        return "$docker_status"
      fi
    else
      if cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" \
        "${psql_args[@]}" "$@"; then
        return 0
      else
        local docker_status=$?
        return "$docker_status"
      fi
    fi
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

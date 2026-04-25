#!/usr/bin/env bash
set -euo pipefail

CEX_SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CEX_PROJECT_ROOT="$(cd -- "$CEX_SCRIPT_DIR/.." && pwd)"
CEX_POSTGRES_CONTAINER_NAME="${CEX_POSTGRES_CONTAINER_NAME:-cex-postgres-1}"
CEX_POSTGRES_USER="${CEX_POSTGRES_USER:-postgres}"
CEX_POSTGRES_DB="${CEX_POSTGRES_DB:-cex_ai}"
CEX_POSTGRES_PASSWORD="${CEX_POSTGRES_PASSWORD:-postgres}"
CEX_DATABASE_URL_DEFAULT="postgres://postgres:postgres@127.0.0.1:5432/cex_ai"

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

cex_has_local_psql() {
  command -v psql >/dev/null 2>&1
}

cex_has_local_pg_isready() {
  command -v pg_isready >/dev/null 2>&1
}

cex_can_use_docker_postgres() {
  command -v docker >/dev/null 2>&1 && \
    docker exec "$CEX_POSTGRES_CONTAINER_NAME" true >/dev/null 2>&1
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
      if PGPASSWORD="$CEX_POSTGRES_PASSWORD" psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 -c 'select 1' >/dev/null 2>&1; then
        return 0
      fi
    elif cex_can_use_docker_postgres; then
      if docker exec "$CEX_POSTGRES_CONTAINER_NAME" pg_isready -U "$CEX_POSTGRES_USER" -d "$CEX_POSTGRES_DB" >/dev/null 2>&1; then
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
    PGPASSWORD="$CEX_POSTGRES_PASSWORD" psql "$(cex_effective_database_url)" -v ON_ERROR_STOP=1 "$@"
    return 0
  fi

  if cex_can_use_docker_postgres; then
    docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" \
      psql -U "$CEX_POSTGRES_USER" -d "$CEX_POSTGRES_DB" -v ON_ERROR_STOP=1 "$@"
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
SQL
}

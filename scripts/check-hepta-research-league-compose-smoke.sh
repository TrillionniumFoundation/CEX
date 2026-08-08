#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${HEPTA_IMAGE:?HEPTA_IMAGE must name the already-built immutable candidate}"
: "${HEPTA_EXPECTED_IMAGE_ID:?HEPTA_EXPECTED_IMAGE_ID must freeze the candidate image ID}"

postgres_image='docker.io/library/postgres:17.6-alpine3.22@sha256:ef257d85f76e48da1c64832459b59fcaba1a4dac97bf5d7450c77753542eee94'
scratch=$(mktemp -d)
project="hepta-image-pg-gate-${$}"
env_file="$scratch/gate.env"
override="$scratch/compose.gate.yaml"
headers="$scratch/ready.headers"
body="$scratch/ready.json"
migration_secret_file="$scratch/migration-owner.url"
started=false

cleanup() {
  if [[ "$started" == true ]]; then
    "${compose[@]}" down --volumes --remove-orphans --timeout 10 >/dev/null 2>&1 || true
  fi
  case "$scratch" in
    /tmp/tmp.*)
      if [[ ${docker_command[0]:-} == sudo ]]; then
        sudo -n rm -rf -- "$scratch"
      else
        rm -rf -- "$scratch"
      fi
      ;;
    *) echo "refusing to remove unexpected Compose smoke scratch path" >&2 ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command_name in awk curl docker grep jq python3 rg seq sleep sort; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Hepta Compose smoke requires $command_name" >&2
    exit 1
  }
done
[[ "$HEPTA_EXPECTED_IMAGE_ID" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "HEPTA_EXPECTED_IMAGE_ID is not a canonical Docker config digest" >&2
  exit 2
}

docker_command=(docker)
if ! docker info >/dev/null 2>&1; then
  docker_command=(sudo -n docker)
fi
"${docker_command[@]}" info >/dev/null
if [[ ${docker_command[0]} == sudo ]]; then
  host_kill=(sudo -n kill)
  compose=(sudo -n docker compose --project-name "$project" --env-file "$env_file" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" --file "$override")
  migration_compose=(sudo -n docker compose --project-name "$project" --env-file "$env_file" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" \
    --file "$repo_dir/deploy/hepta-research-league/compose.migration.yaml" \
    --file "$override")
else
  host_kill=(kill)
  compose=(docker compose --project-name "$project" --env-file "$env_file" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" --file "$override")
  migration_compose=(docker compose --project-name "$project" --env-file "$env_file" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" \
    --file "$repo_dir/deploy/hepta-research-league/compose.migration.yaml" \
    --file "$override")
fi

frozen_image_id=$("${docker_command[@]}" image inspect "$HEPTA_IMAGE" --format '{{.Id}}')
[[ "$frozen_image_id" == "$HEPTA_EXPECTED_IMAGE_ID" ]] || {
  echo "Hepta candidate tag does not resolve to the frozen image ID" >&2
  exit 1
}
"${docker_command[@]}" image inspect "$postgres_image" >/dev/null

host_port=$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)
umask 077
printf '%s\n' \
  'postgres://hepta_gate_migrator:hepta_gate_migrator_password@postgres:5432/hepta_gate' \
  >"$migration_secret_file"
chmod 0444 "$migration_secret_file"
cat >"$env_file" <<EOF
HEPTA_IMAGE=$HEPTA_IMAGE
HEPTA_HOST_PORT=$host_port
HEPTA_DATABASE_URL=postgres://hepta_gate_runtime:hepta_gate_runtime_password@postgres:5432/hepta_gate
HEPTA_FINALITY_DATABASE_URL=postgres://hepta_gate_finality:hepta_gate_finality_password@postgres:5432/hepta_gate
HEPTA_MIGRATION_DATABASE_URL_FILE=$migration_secret_file
HEPTA_RUNTIME_DATABASE_ROLE=hepta_gate_runtime
HEPTA_FINALITY_DATABASE_ROLE=hepta_gate_finality
HEPTA_OPERATOR_TOKEN=hepta-gate-operator-token
HEPTA_NAKAMA_TOKEN=hepta-gate-nakama-token
HEPTA_NAKAMA_AUTHORIZATION_ISSUER_KEY_ID=hepta-gate-authorization-v1
HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64=nWGxne/9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A=
HEPTA_NAKAMA_CONTROL_ISSUER_KEY_ID=hepta-gate-control-v1
HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64=TM0Imyj/ltqdtsNG7BFOD1uKMZ81q6Yk2oz27U+4pvs=
HEPTA_NAKAMA_BASE_URL=http://127.0.0.1:7350
HEPTA_NAKAMA_RUNTIME_HTTP_KEY=hepta-gate-runtime-http-key
HEPTA_CONSUMER_EDGE_ISSUER=hepta-gate-consumer
HEPTA_CONSUMER_EDGE_AUDIENCE=hepta-gate-api
HEPTA_CONSUMER_EDGE_ISSUER_KEY_ID=hepta-gate-consumer-v1
HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64=J4EX/BRMcjQPZ9DyMW6Dhs7/vyskKMnFH+98WX8dQm4=
HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEYS_JSON={"hepta-gate-consumer-v1":"J4EX/BRMcjQPZ9DyMW6Dhs7/vyskKMnFH+98WX8dQm4="}
TRNM_NAKAMA_AUTHORITY_KEY_ID=hepta-gate-nakama-authority-v1
TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64=/FHNjmIYoaONpH7QAjDwWAgW7RO6MwOsXeuRFUiQgCU=
TRNM_NAKAMA_AUTHORITY_PUBLIC_KEYS_JSON={"hepta-gate-nakama-authority-v1":"/FHNjmIYoaONpH7QAjDwWAgW7RO6MwOsXeuRFUiQgCU="}
HEPTA_TRNM_TOKEN=hepta-gate-trnm-token
HEPTA_FINALITY_MODE=verified
HEPTA_TRNM_VALIDATOR_SETS_JSON=[]
HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON=["88b73fc902dd554c35b9a44ff582ec6d76e59085a2e4fdf14292183f4b3846d5","9999999999999999999999999999999999999999999999999999999999999999"]
HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES=32768
HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT=1
EOF
cat >"$override" <<EOF
services:
  postgres:
    image: $postgres_image
    pull_policy: never
    environment:
      POSTGRES_USER: hepta_gate_migrator
      POSTGRES_PASSWORD: hepta_gate_migrator_password
      POSTGRES_DB: hepta_gate
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U hepta_gate_migrator -d hepta_gate"]
      interval: 2s
      timeout: 2s
      retries: 30
    volumes:
      - pgdata:/var/lib/postgresql/data
    security_opt:
      - no-new-privileges:true
  hepta:
    pull_policy: never
    environment:
      HEPTA_DATABASE_URL: postgres://hepta_gate_runtime:hepta_gate_runtime_password@postgres:5432/hepta_gate
      HEPTA_FINALITY_DATABASE_URL: postgres://hepta_gate_finality:hepta_gate_finality_password@postgres:5432/hepta_gate
volumes:
  pgdata: {}
EOF

resident_config=$("${compose[@]}" config --format json)
migration_config=$("${migration_compose[@]}" --profile migration config --format json)
configured_image=$(jq -er '.services.hepta.image' <<<"$resident_config")
configured_migrate_image=$(jq -er '.services["hepta-migrate"].image' <<<"$migration_config")
configured_postgres=$(jq -er '.services.postgres.image' <<<"$resident_config")
configured_anchor_pins=$(jq -er \
  '.services.hepta.environment.HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON' \
  <<<"$resident_config")
configured_receipt_cap=$(jq -er \
  '.services.hepta.environment.HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES' <<<"$resident_config")
configured_receipt_in_flight=$(jq -er \
  '.services.hepta.environment.HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT' <<<"$resident_config")
configured_hepta_environment=$(jq -cer '.services.hepta.environment' <<<"$resident_config")
configured_migrate_environment=$(jq -cer '.services["hepta-migrate"].environment' \
  <<<"$migration_config")
configured_migration_secret=$(jq -er '.secrets.hepta_migration_database_url.file' \
  <<<"$migration_config")
[[ "$configured_image" == "$HEPTA_IMAGE" && "$configured_migrate_image" == "$HEPTA_IMAGE" \
  && "$configured_postgres" == "$postgres_image" ]]
[[ $(jq -r 'has("HEPTA_MIGRATION_DATABASE_URL") or has("HEPTA_MIGRATION_DATABASE_URL_FILE")' \
  <<<"$configured_hepta_environment") == false ]]
[[ $(jq -r '.HEPTA_DATABASE_URL' <<<"$configured_hepta_environment") == \
  'postgres://hepta_gate_runtime:hepta_gate_runtime_password@postgres:5432/hepta_gate' ]]
[[ $(jq -r '.HEPTA_FINALITY_DATABASE_URL' <<<"$configured_hepta_environment") == \
  'postgres://hepta_gate_finality:hepta_gate_finality_password@postgres:5432/hepta_gate' ]]
[[ "$configured_migration_secret" == "$migration_secret_file" ]]
jq -e '
  keys == [
    "HEPTA_FINALITY_DATABASE_ROLE",
    "HEPTA_MIGRATION_DATABASE_URL_FILE",
    "HEPTA_RUNTIME_DATABASE_ROLE"
  ]
  and .HEPTA_MIGRATION_DATABASE_URL_FILE == "/run/secrets/hepta_migration_database_url"
  and .HEPTA_RUNTIME_DATABASE_ROLE == "hepta_gate_runtime"
  and .HEPTA_FINALITY_DATABASE_ROLE == "hepta_gate_finality"
' <<<"$configured_migrate_environment" >/dev/null
[[ $(jq -cer '.services["hepta-migrate"].profiles' <<<"$migration_config") == \
  '["migration"]' ]]
[[ $(jq -r '.services | has("hepta-migrate")' <<<"$resident_config") == false ]]
[[ $(jq -r 'has("secrets")' <<<"$resident_config") == false ]]
if grep -Fq 'postgres://hepta_gate_migrator:' <<<"$migration_config"; then
  echo "rendered Compose configuration exposed the migration-owner URL" >&2
  exit 1
fi
[[ "$configured_anchor_pins" == \
  '["88b73fc902dd554c35b9a44ff582ec6d76e59085a2e4fdf14292183f4b3846d5","9999999999999999999999999999999999999999999999999999999999999999"]' ]]
[[ "$configured_receipt_cap" == 32768 && "$configured_receipt_in_flight" == 1 ]]
"${compose[@]}" config --quiet
"${migration_compose[@]}" --profile migration config --quiet
started=true
"${compose[@]}" up -d postgres

wait_postgres() {
  local attempt
  for attempt in $(seq 1 60); do
    if "${compose[@]}" exec -T postgres pg_isready -U hepta_gate_migrator -d hepta_gate >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "pinned PostgreSQL image did not become ready" >&2
  return 1
}
wait_hepta() {
  local attempt
  for attempt in $(seq 1 60); do
    if "${compose[@]}" exec -T hepta \
      /usr/local/bin/hepta-research-league --probe-ready >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "Hepta candidate image did not become ready" >&2
  "${compose[@]}" ps -a >&2 || true
  "${compose[@]}" logs --no-color --tail 200 hepta >&2 || true
  "${compose[@]}" logs --no-color --tail 80 postgres >&2 || true
  local hepta_container
  hepta_container=$("${compose[@]}" ps -a -q hepta 2>/dev/null || true)
  if [[ -n "$hepta_container" ]]; then
    "${docker_command[@]}" inspect "$hepta_container" \
      --format '{{json .State}}' >&2 || true
  fi
  return 1
}

wait_postgres
"${compose[@]}" exec -T postgres psql -X -v ON_ERROR_STOP=1 \
  -U hepta_gate_migrator -d hepta_gate -c \
  "create role hepta_gate_runtime login password 'hepta_gate_runtime_password'
     nosuperuser nocreatedb nocreaterole noinherit noreplication nobypassrls;
   create role hepta_gate_finality login password 'hepta_gate_finality_password'
     nosuperuser nocreatedb nocreaterole noinherit noreplication nobypassrls;"
"${migration_compose[@]}" --profile migration run --rm --no-deps hepta-migrate
[[ -z $("${migration_compose[@]}" --profile migration ps -a -q hepta-migrate) ]]
[[ -z $("${docker_command[@]}" ps -aq \
  --filter "label=com.docker.compose.project=$project" \
  --filter 'label=com.docker.compose.service=hepta-migrate') ]]
rm -f -- "$migration_secret_file"
[[ ! -e "$migration_secret_file" ]]
"${compose[@]}" config --quiet
"${compose[@]}" up -d --no-deps hepta
wait_hepta
hepta_container=$("${compose[@]}" ps -q hepta)
container_image_id=$("${docker_command[@]}" inspect "$hepta_container" --format '{{.Image}}')
[[ "$container_image_id" == "$HEPTA_EXPECTED_IMAGE_ID" ]] || {
  echo "Compose did not start the frozen Hepta image ID" >&2
  exit 1
}
if "${docker_command[@]}" inspect "$hepta_container" --format '{{range .Config.Env}}{{println .}}{{end}}' \
  | grep -Eq '^HEPTA_MIGRATION_DATABASE_URL(_FILE)?='; then
  echo "resident Hepta container retained the migration-owner credential" >&2
  exit 1
fi
if "${docker_command[@]}" inspect "$hepta_container" \
  | grep -Fq 'postgres://hepta_gate_migrator:'; then
  echo "resident Hepta container metadata exposed the migration-owner URL" >&2
  exit 1
fi
"${docker_command[@]}" inspect "$hepta_container" --format '{{range .Config.Env}}{{println .}}{{end}}' \
  | grep -Fx 'HEPTA_FINALITY_DATABASE_URL=postgres://hepta_gate_finality:hepta_gate_finality_password@postgres:5432/hepta_gate' >/dev/null

# The resident lifecycle must be independent of a destroyed migration-owner
# secret: prove both restart-policy recovery and a fresh Compose recreation.
hepta_pid=$("${docker_command[@]}" inspect "$hepta_container" --format '{{.State.Pid}}')
[[ "$hepta_pid" =~ ^[1-9][0-9]*$ ]]
"${host_kill[@]}" -KILL "$hepta_pid"
wait_hepta
"${compose[@]}" up -d --no-deps --force-recreate hepta
wait_hepta
hepta_container=$("${compose[@]}" ps -q hepta)
[[ -n "$hepta_container" && ! -e "$migration_secret_file" ]]

curl --silent --show-error --fail --max-time 5 \
  --dump-header "$headers" --output "$body" "http://127.0.0.1:$host_port/ready"
content_type_count=$(awk -F: 'tolower($1)=="content-type" {gsub(/^[ \t]+|[ \t\r]+$/, "", $2); if ($2=="application/json") count++} END {print count+0}' "$headers")
[[ "$content_type_count" -eq 1 ]]
jq -e '
  .ready == true
  and .storage == "postgresql"
  and .database == "reachable"
  and .security == "valid"
  and .nakama_control == "configured"
  and .failures == []
  and .agent_execution_mode == "external_only"
  and .finality_mode == "verified"
  and .trusted_validator_sets == 0
  and .pinned_cometbft_trust_anchor_hashes == 2
  and .trnm_receipt_v2_max_body_bytes == 32768
  and .trnm_receipt_v2_max_in_flight == 1
  and .paper_chain_finality_v2_command_lane == "awaiting_chain_verifier_upgrade"
  and .paper_scientific_finality_policy == "hepta.paper_raid.scientific_finality_policy.v1"
  and .paper_no_appeal_window_seconds == 86400
' "$body" >/dev/null

migration_count=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 -c \
  "select count(*) from (values
    (to_regclass('public.hepta_league_state')),
    (to_regclass('public.hepta_human_players')),
    (to_regclass('public.hepta_matchmaking_tickets')),
    (to_regclass('public.hepta_paper_evaluations')),
    (to_regclass('public.hepta_agent_binding_nonces')),
    (to_regclass('public.hepta_nakama_research_control_commands')),
    (to_regclass('public.hepta_trnm_cometbft_time_checkpoints_v1')),
    (to_regclass('public.hepta_paper_chain_finality_window_arms_v2')),
    (to_regclass('public.hepta_paper_chain_finality_preparations_v2'))
  ) as migrations(marker) where marker is not null;")
[[ "$migration_count" == 9 ]]
verified_v2_trigger_count=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 -c \
  "with expected(trigger_name, table_name, function_name, trigger_type) as (values
      ('hepta_trnm_time_checkpoint_v1_progress_guard', 'hepta_trnm_cometbft_time_checkpoints_v1', 'hepta_validate_paper_finality_v2_time_checkpoint()', 7),
      ('hepta_trnm_time_checkpoint_v1_immutable_guard', 'hepta_trnm_cometbft_time_checkpoints_v1', 'hepta_reject_paper_finality_v2_evidence_mutation()', 27),
      ('hepta_trnm_time_checkpoint_v1_truncate_guard', 'hepta_trnm_cometbft_time_checkpoints_v1', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_paper_finality_v2_window_arm_guard', 'hepta_paper_chain_finality_window_arms_v2', 'hepta_paper_finality_v2_lock_window_arm()', 7),
      ('hepta_paper_finality_v2_window_arm_immutable_guard', 'hepta_paper_chain_finality_window_arms_v2', 'hepta_reject_paper_finality_v2_evidence_mutation()', 27),
      ('hepta_paper_finality_v2_window_arm_truncate_guard', 'hepta_paper_chain_finality_window_arms_v2', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_paper_finality_v2_preparation_guard', 'hepta_paper_chain_finality_preparations_v2', 'hepta_paper_finality_v2_lock_preparation()', 7),
      ('hepta_paper_finality_v2_preparation_seal_guard', 'hepta_paper_chain_finality_preparations_v2', 'hepta_paper_finality_v2_apply_seal()', 5),
      ('hepta_paper_finality_v2_preparation_immutable_guard', 'hepta_paper_chain_finality_preparations_v2', 'hepta_reject_paper_finality_v2_preparation_mutation()', 27),
      ('hepta_paper_finality_v2_preparation_truncate_guard', 'hepta_paper_chain_finality_preparations_v2', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_paper_projects_finality_v2_source_guard', 'hepta_paper_projects', 'hepta_guard_paper_finality_v2_anchor_mutation()', 27),
      ('hepta_paper_projects_finality_v2_truncate_guard', 'hepta_paper_projects', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_joint_submissions_finality_v2_source_guard', 'hepta_joint_paper_submissions', 'hepta_reject_paper_finality_v2_source_mutation()', 31),
      ('hepta_joint_submissions_finality_v2_truncate_guard', 'hepta_joint_paper_submissions', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_paper_evaluations_finality_v2_source_guard', 'hepta_paper_evaluations', 'hepta_reject_paper_finality_v2_source_mutation()', 31),
      ('hepta_paper_evaluations_finality_v2_truncate_guard', 'hepta_paper_evaluations', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_paper_reproductions_finality_v2_source_guard', 'hepta_paper_reproductions', 'hepta_reject_paper_finality_v2_source_mutation()', 31),
      ('hepta_paper_reproductions_finality_v2_truncate_guard', 'hepta_paper_reproductions', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_paper_appeals_finality_v2_source_guard', 'hepta_paper_appeals', 'hepta_reject_paper_finality_v2_source_mutation()', 31),
      ('hepta_paper_appeals_finality_v2_truncate_guard', 'hepta_paper_appeals', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_paper_resolutions_finality_v2_source_guard', 'hepta_paper_appeal_resolutions', 'hepta_reject_paper_finality_v2_source_mutation()', 31),
      ('hepta_paper_resolutions_finality_v2_truncate_guard', 'hepta_paper_appeal_resolutions', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_research_auth_sets_finality_v2_source_guard', 'hepta_research_session_authorization_sets', 'hepta_reject_paper_finality_v2_source_mutation()', 31),
      ('hepta_research_auth_sets_finality_v2_truncate_guard', 'hepta_research_session_authorization_sets', 'hepta_reject_paper_finality_v2_truncate()', 34),
      ('hepta_nakama_completions_finality_v2_source_guard', 'hepta_nakama_research_session_completions', 'hepta_reject_paper_finality_v2_source_mutation()', 31),
      ('hepta_nakama_completions_finality_v2_truncate_guard', 'hepta_nakama_research_session_completions', 'hepta_reject_paper_finality_v2_truncate()', 34)
    )
    select count(*)
    from expected as e
    join pg_trigger as t
      on t.tgname = e.trigger_name
     and t.tgrelid = to_regclass(e.table_name)
     and t.tgfoid = to_regprocedure(e.function_name)
     and t.tgtype = e.trigger_type
     and t.tgenabled = 'A'
     and not t.tgisinternal;")
[[ "$verified_v2_trigger_count" == 26 ]]
verified_v2_constraint_catalog=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 -c \
  "select constraint_count::text || ':' || catalog_sha256
   from public.hepta_paper_finality_v2_constraint_catalog_fingerprint();")
[[ "$verified_v2_constraint_catalog" == \
  "77:910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba" ]]
runtime_role_boundary=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 -c \
  "select (
      not role.rolsuper
      and not role.rolinherit
      and not role.rolcreatedb
      and not role.rolcreaterole
      and not role.rolreplication
      and not role.rolbypassrls
      and not exists (
        select 1 from pg_auth_members as membership
        where membership.member=role.oid or membership.roleid=role.oid
      )
      and not exists (
        select 1
        from pg_class as relation
        join pg_namespace as namespace on namespace.oid=relation.relnamespace
        where namespace.nspname='public'
          and relation.relowner=role.oid
      )
      and has_schema_privilege(role.rolname, 'public', 'USAGE')
      and not has_schema_privilege(role.rolname, 'public', 'CREATE')
      and has_database_privilege(role.rolname, current_database(), 'CONNECT')
      and not has_database_privilege(role.rolname, current_database(), 'CREATE')
      and not has_database_privilege(role.rolname, current_database(), 'TEMPORARY')
      and not has_function_privilege(
        role.rolname,
        'public.hepta_assert_paper_finality_v2_source_unsealed(uuid)',
        'EXECUTE'
      )
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_lock_window_arm()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_lock_preparation()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_apply_seal()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_reject_paper_finality_v2_source_mutation()', 'EXECUTE')
      and (
        select bool_and(
          has_table_privilege(role.rolname, relation.oid, 'SELECT')
          and not has_table_privilege(role.rolname, relation.oid, 'TRUNCATE')
          and not has_table_privilege(role.rolname, relation.oid, 'REFERENCES')
          and not has_table_privilege(role.rolname, relation.oid, 'TRIGGER')
          and (
            case when relation.relname = any(array[
              'hepta_trnm_cometbft_time_checkpoints_v1',
              'hepta_paper_chain_finality_window_arms_v2',
              'hepta_paper_chain_finality_preparations_v2'
            ]) then
              not has_table_privilege(role.rolname, relation.oid, 'INSERT')
              and not has_table_privilege(role.rolname, relation.oid, 'UPDATE')
              and not has_table_privilege(role.rolname, relation.oid, 'DELETE')
            else
              has_table_privilege(role.rolname, relation.oid, 'INSERT')
              and has_table_privilege(role.rolname, relation.oid, 'UPDATE')
              and has_table_privilege(role.rolname, relation.oid, 'DELETE')
            end
          )
        )
        from pg_class as relation
        join pg_namespace as namespace on namespace.oid=relation.relnamespace
        where namespace.nspname='public' and relation.relkind in ('r','p')
      )
    )::text
   from pg_roles as role where role.rolname='hepta_gate_runtime';")
[[ "$runtime_role_boundary" == t ]]
finality_role_boundary=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 -c \
  "select (
      not role.rolsuper
      and not role.rolinherit
      and not role.rolcreatedb
      and not role.rolcreaterole
      and not role.rolreplication
      and not role.rolbypassrls
      and not exists (
        select 1 from pg_auth_members as membership
        where membership.member=role.oid or membership.roleid=role.oid
      )
      and not exists (
        select 1 from pg_class as relation
        join pg_namespace as namespace on namespace.oid=relation.relnamespace
        where namespace.nspname='public' and relation.relowner=role.oid
      )
      and has_schema_privilege(role.rolname, 'public', 'USAGE')
      and not has_schema_privilege(role.rolname, 'public', 'CREATE')
      and has_database_privilege(role.rolname, current_database(), 'CONNECT')
      and not has_database_privilege(role.rolname, current_database(), 'CREATE')
      and not has_database_privilege(role.rolname, current_database(), 'TEMPORARY')
      and has_function_privilege(
        role.rolname,
        'public.hepta_assert_paper_finality_v2_source_unsealed(uuid)',
        'EXECUTE'
      )
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_lock_window_arm()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_lock_preparation()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_paper_finality_v2_apply_seal()', 'EXECUTE')
      and not has_function_privilege(role.rolname, 'public.hepta_reject_paper_finality_v2_source_mutation()', 'EXECUTE')
      and (
        select bool_and(
          has_table_privilege(role.rolname, relation.oid, 'SELECT')
          and (
            has_table_privilege(role.rolname, relation.oid, 'INSERT') =
            (relation.relname = any(array[
              'hepta_trnm_cometbft_time_checkpoints_v1',
              'hepta_paper_chain_finality_window_arms_v2',
              'hepta_paper_chain_finality_preparations_v2'
            ]))
          )
          and not has_table_privilege(role.rolname, relation.oid, 'UPDATE')
          and not has_table_privilege(role.rolname, relation.oid, 'DELETE')
          and not has_table_privilege(role.rolname, relation.oid, 'TRUNCATE')
          and not has_table_privilege(role.rolname, relation.oid, 'REFERENCES')
          and not has_table_privilege(role.rolname, relation.oid, 'TRIGGER')
        )
        from pg_class as relation
        join pg_namespace as namespace on namespace.oid=relation.relnamespace
        where namespace.nspname='public' and relation.relkind in ('r','p')
      )
    )::text
   from pg_roles as role where role.rolname='hepta_gate_finality';")
[[ "$finality_role_boundary" == t ]]
definer_public_execute_count=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 -c \
  "select count(*)
   from pg_proc as function
   where function.oid = any(array[
     to_regprocedure('public.hepta_paper_finality_v2_lock_window_arm()'),
     to_regprocedure('public.hepta_paper_finality_v2_lock_preparation()'),
     to_regprocedure('public.hepta_paper_finality_v2_apply_seal()'),
     to_regprocedure('public.hepta_assert_paper_finality_v2_source_unsealed(uuid)'),
     to_regprocedure('public.hepta_reject_paper_finality_v2_source_mutation()')
   ])
   and exists (
     select 1
     from aclexplode(coalesce(function.proacl, acldefault('f', function.proowner))) as privilege
     where privilege.grantee=0 and privilege.privilege_type='EXECUTE'
   );")
[[ "$definer_public_execute_count" == 0 ]]
verified_definer_count=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 -c \
  "select count(*)
   from (values
     ('public.hepta_paper_finality_v2_lock_window_arm()'),
     ('public.hepta_paper_finality_v2_lock_preparation()'),
     ('public.hepta_paper_finality_v2_apply_seal()'),
     ('public.hepta_assert_paper_finality_v2_source_unsealed(uuid)'),
     ('public.hepta_reject_paper_finality_v2_source_mutation()')
   ) as expected(signature)
   join pg_proc as function on function.oid=to_regprocedure(expected.signature)
   where function.prosecdef
     and function.proconfig=array['search_path=pg_catalog']::text[];")
[[ "$verified_definer_count" == 5 ]]
state_rows_before=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 \
  -c "select count(*) from hepta_league_state where state_key='primary';")
[[ "$state_rows_before" == 1 ]]

"${compose[@]}" kill -s SIGKILL hepta
"${compose[@]}" up -d --no-deps hepta
wait_hepta
restarted_hepta=$("${compose[@]}" ps -q hepta)
[[ "$("${docker_command[@]}" inspect "$restarted_hepta" --format '{{.Image}}')" == "$HEPTA_EXPECTED_IMAGE_ID" ]]

"${compose[@]}" kill -s SIGKILL postgres
"${compose[@]}" up -d postgres
wait_postgres
wait_hepta
state_rows_after=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate_migrator -d hepta_gate -v ON_ERROR_STOP=1 \
  -c "select count(*) from hepta_league_state where state_key='primary';")
[[ "$state_rows_after" == "$state_rows_before" ]]
[[ -z $("${compose[@]}" ps -a -q hepta-migrate) ]]
[[ -z $("${docker_command[@]}" ps -aq \
  --filter "label=com.docker.compose.project=$project" \
  --filter 'label=com.docker.compose.service=hepta-migrate') ]]
[[ "$("${docker_command[@]}" image inspect "$HEPTA_IMAGE" --format '{{.Id}}')" == "$HEPTA_EXPECTED_IMAGE_ID" ]]

echo "Hepta frozen-image Compose/PostgreSQL SIGKILL smoke: PASS image_id=$HEPTA_EXPECTED_IMAGE_ID"

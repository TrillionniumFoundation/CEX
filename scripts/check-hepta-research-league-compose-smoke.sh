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

for command_name in awk curl docker jq python3 rg seq sleep; do
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
  compose=(sudo -n docker compose --project-name "$project" --env-file "$env_file" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" --file "$override")
else
  compose=(docker compose --project-name "$project" --env-file "$env_file" \
    --file "$repo_dir/deploy/hepta-research-league/compose.yaml" --file "$override")
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
cat >"$env_file" <<EOF
HEPTA_IMAGE=$HEPTA_IMAGE
HEPTA_HOST_PORT=$host_port
HEPTA_DATABASE_URL=postgres://hepta_gate:hepta_gate_password@postgres:5432/hepta_gate
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
HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES=67108864
HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT=1
EOF
cat >"$override" <<EOF
services:
  postgres:
    image: $postgres_image
    pull_policy: never
    environment:
      POSTGRES_USER: hepta_gate
      POSTGRES_PASSWORD: hepta_gate_password
      POSTGRES_DB: hepta_gate
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U hepta_gate -d hepta_gate"]
      interval: 2s
      timeout: 2s
      retries: 30
    volumes:
      - pgdata:/var/lib/postgresql/data
    security_opt:
      - no-new-privileges:true
  hepta:
    pull_policy: never
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      HEPTA_DATABASE_URL: postgres://hepta_gate:hepta_gate_password@postgres:5432/hepta_gate
volumes:
  pgdata: {}
EOF

configured_image=$("${compose[@]}" config --format json | jq -er '.services.hepta.image')
configured_postgres=$("${compose[@]}" config --format json | jq -er '.services.postgres.image')
configured_anchor_pins=$("${compose[@]}" config --format json | jq -er \
  '.services.hepta.environment.HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON')
configured_receipt_cap=$("${compose[@]}" config --format json | jq -er \
  '.services.hepta.environment.HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES')
configured_receipt_in_flight=$("${compose[@]}" config --format json | jq -er \
  '.services.hepta.environment.HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT')
[[ "$configured_image" == "$HEPTA_IMAGE" && "$configured_postgres" == "$postgres_image" ]]
[[ "$configured_anchor_pins" == \
  '["88b73fc902dd554c35b9a44ff582ec6d76e59085a2e4fdf14292183f4b3846d5","9999999999999999999999999999999999999999999999999999999999999999"]' ]]
[[ "$configured_receipt_cap" == 67108864 && "$configured_receipt_in_flight" == 1 ]]
"${compose[@]}" config --quiet
started=true
"${compose[@]}" up -d postgres

wait_postgres() {
  local attempt
  for attempt in $(seq 1 60); do
    if "${compose[@]}" exec -T postgres pg_isready -U hepta_gate -d hepta_gate >/dev/null 2>&1; then
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
  return 1
}

wait_postgres
"${compose[@]}" up -d hepta
wait_hepta
hepta_container=$("${compose[@]}" ps -q hepta)
container_image_id=$("${docker_command[@]}" inspect "$hepta_container" --format '{{.Image}}')
[[ "$container_image_id" == "$HEPTA_EXPECTED_IMAGE_ID" ]] || {
  echo "Compose did not start the frozen Hepta image ID" >&2
  exit 1
}

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
  and .trnm_receipt_v2_max_body_bytes == 67108864
  and .trnm_receipt_v2_max_in_flight == 1
' "$body" >/dev/null

migration_count=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate -d hepta_gate -v ON_ERROR_STOP=1 -c \
  "select count(*) from (values
    (to_regclass('public.hepta_league_state')),
    (to_regclass('public.hepta_human_players')),
    (to_regclass('public.hepta_matchmaking_tickets')),
    (to_regclass('public.hepta_paper_evaluations')),
    (to_regclass('public.hepta_agent_binding_nonces')),
    (to_regclass('public.hepta_nakama_research_control_commands'))
  ) as migrations(marker) where marker is not null;")
[[ "$migration_count" == 6 ]]
state_rows_before=$("${compose[@]}" exec -T postgres psql -X -A -t \
  -U hepta_gate -d hepta_gate -v ON_ERROR_STOP=1 \
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
  -U hepta_gate -d hepta_gate -v ON_ERROR_STOP=1 \
  -c "select count(*) from hepta_league_state where state_key='primary';")
[[ "$state_rows_after" == "$state_rows_before" ]]
[[ "$("${docker_command[@]}" image inspect "$HEPTA_IMAGE" --format '{{.Id}}')" == "$HEPTA_EXPECTED_IMAGE_ID" ]]

echo "Hepta frozen-image Compose/PostgreSQL SIGKILL smoke: PASS image_id=$HEPTA_EXPECTED_IMAGE_ID"

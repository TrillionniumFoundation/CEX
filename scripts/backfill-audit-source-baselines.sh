#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
source "$root/scripts/_dev-helpers.sh"
: "${DATABASE_URL:?DATABASE_URL is required}"
cex_load_env
cex_sync_postgres_env_from_database_url "$DATABASE_URL"

source_selector="all"
batch_size=100
max_outbox_backlog=5000
max_batches=1
sleep_seconds=1
worker_id="${HOSTNAME:-cex}-audit-baseline-$$"

usage() {
  cat <<'EOF'
Usage: scripts/backfill-audit-source-baselines.sh [options]

Options:
  --source execution-service|identity-service|all
  --batch-size N                 Rows per transaction, 1..1000 (default: 100)
  --max-outbox-backlog N         Pause at this nonterminal backlog (default: 5000)
  --max-batches N                Maximum transactions per source (default: 1)
  --sleep-seconds N              Delay between batches (default: 1)
  --worker-id ID                 Stable operator/worker identifier
  --status                       Print durable progress and exit
  -h, --help

The script never performs an unbounded run. Reinvoke it or raise --max-batches after
observing dispatcher throughput and Audit outbox age.
EOF
}

status_only=0
while (($#)); do
  case "$1" in
    --source)
      source_selector=${2:?--source requires a value}
      shift 2
      ;;
    --batch-size)
      batch_size=${2:?--batch-size requires a value}
      shift 2
      ;;
    --max-outbox-backlog)
      max_outbox_backlog=${2:?--max-outbox-backlog requires a value}
      shift 2
      ;;
    --max-batches)
      max_batches=${2:?--max-batches requires a value}
      shift 2
      ;;
    --sleep-seconds)
      sleep_seconds=${2:?--sleep-seconds requires a value}
      shift 2
      ;;
    --worker-id)
      worker_id=${2:?--worker-id requires a value}
      shift 2
      ;;
    --status)
      status_only=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$source_selector" in
  all|execution-service|identity-service) ;;
  *)
    echo "ERROR: --source must be execution-service, identity-service, or all" >&2
    exit 2
    ;;
esac

for value_name in batch_size max_outbox_backlog max_batches sleep_seconds; do
  value=${!value_name}
  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    echo "ERROR: $value_name must be a non-negative integer" >&2
    exit 2
  fi
done

if ((batch_size < 1 || batch_size > 1000)); then
  echo "ERROR: batch_size must be between 1 and 1000" >&2
  exit 2
fi
if ((max_outbox_backlog < 1 || max_outbox_backlog > 10000000)); then
  echo "ERROR: max_outbox_backlog must be between 1 and 10000000" >&2
  exit 2
fi
if ((max_batches < 1 || max_batches > 1000000)); then
  echo "ERROR: max_batches must be between 1 and 1000000" >&2
  exit 2
fi
if ((sleep_seconds > 3600)); then
  echo "ERROR: sleep_seconds must be between 0 and 3600" >&2
  exit 2
fi
if ((${#worker_id} < 1 || ${#worker_id} > 128)); then
  echo "ERROR: worker_id must contain 1..128 characters" >&2
  exit 2
fi

if ((status_only)); then
  cex_psql_stdin -X -P pager=off -c \
    "select * from public.cex_audit_source_baseline_status_v1 order by source_service"
  exit 0
fi

python3 "$root/scripts/check-p0-migrations.py"

if [[ "$source_selector" == "all" ]]; then
  sources=(execution-service identity-service)
else
  sources=("$source_selector")
fi

run_batch() {
  local source_service=$1
  cex_psql_stdin -X -A -t \
    -v source_service="$source_service" \
    -v worker_id="$worker_id" \
    -v batch_size="$batch_size" \
    -v max_outbox_backlog="$max_outbox_backlog" <<'SQL'
select public.cex_backfill_audit_source_baseline_v1(
    :'source_service',
    :'worker_id',
    :batch_size,
    :max_outbox_backlog
)::text;
SQL
}

json_field() {
  local document=$1
  local field=$2
  python3 - "$document" "$field" <<'PY'
import json
import sys

document = json.loads(sys.argv[1])
value = document.get(sys.argv[2])
if value is None:
    print("")
elif isinstance(value, bool):
    print("true" if value else "false")
else:
    print(value)
PY
}

overall_exit=0
for source_service in "${sources[@]}"; do
  echo "Audit baseline source=$source_service worker=$worker_id" >&2
  for ((batch=1; batch<=max_batches; batch++)); do
    result=$(run_batch "$source_service")
    printf '%s\n' "$result"

    status=$(json_field "$result" status)
    processed=$(json_field "$result" processed)
    remaining=$(json_field "$result" remaining)

    case "$status" in
      complete)
        echo "source=$source_service complete processed=$processed remaining=${remaining:-0}" >&2
        break
        ;;
      blocked)
        echo "source=$source_service blocked by Audit outbox backlog" >&2
        overall_exit=3
        break
        ;;
      busy)
        echo "source=$source_service already has an active transaction" >&2
        overall_exit=4
        break
        ;;
      pending)
        if [[ "$processed" == "0" ]]; then
          echo "ERROR: source=$source_service made no progress while remaining=$remaining" >&2
          overall_exit=5
          break
        fi
        ;;
      *)
        echo "ERROR: unexpected baseline status '$status'" >&2
        overall_exit=6
        break
        ;;
    esac

    if ((batch < max_batches && sleep_seconds > 0)); then
      sleep "$sleep_seconds"
    fi
  done
done

exit "$overall_exit"

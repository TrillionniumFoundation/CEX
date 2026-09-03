#!/usr/bin/env bash
set -euo pipefail
set +x

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MIGRATION="$ROOT/services/matrix-entry-adapter/migrations/0001_transport_durability.sql"
EVIDENCE=""

while (($#)); do
  case "$1" in
    --evidence)
      [[ $# -ge 2 ]] || { echo "--evidence requires a path" >&2; exit 64; }
      EVIDENCE="$2"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 64
      ;;
  esac
done

: "${MATRIX_TEST_DATABASE_URL:?MATRIX_TEST_DATABASE_URL is required}"
command -v psql >/dev/null 2>&1 || { echo "psql is required" >&2; exit 69; }
command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; exit 69; }
[[ -f "$MIGRATION" ]] || { echo "missing Matrix migration" >&2; exit 66; }

# Keep credentials out of the process argument list. The parsed values remain
# process-local environment variables and are never written to evidence.
eval "$({ MATRIX_DATABASE_URL="$MATRIX_TEST_DATABASE_URL" python3 - <<'PY'
import os, shlex
from urllib.parse import parse_qs, unquote, urlparse

raw = os.environ["MATRIX_DATABASE_URL"]
value = urlparse(raw)
if value.scheme not in {"postgres", "postgresql"}:
    raise SystemExit("MATRIX_TEST_DATABASE_URL must use postgres/postgresql")
if not value.hostname or not value.path or value.path == "/":
    raise SystemExit("MATRIX_TEST_DATABASE_URL lacks host or database")
query = parse_qs(value.query, keep_blank_values=True)
fields = {
    "PGHOST": value.hostname,
    "PGPORT": str(value.port or 5432),
    "PGDATABASE": unquote(value.path[1:]),
    "PGUSER": unquote(value.username or ""),
    "PGPASSWORD": unquote(value.password or ""),
}
if query.get("sslmode"):
    fields["PGSSLMODE"] = query["sslmode"][-1]
for key, item in fields.items():
    print(f"export {key}={shlex.quote(item)}")
PY
} 2>&1)" || { echo "invalid MATRIX_TEST_DATABASE_URL" >&2; exit 65; }
unset MATRIX_TEST_DATABASE_URL MATRIX_DATABASE_URL

psql -X -q -v ON_ERROR_STOP=1 -f "$MIGRATION"

psql -X -q -v ON_ERROR_STOP=1 <<'SQL'
truncate table
    public.matrix_transport_delivery_history,
    public.matrix_transport_outbox,
    public.matrix_transport_inbox,
    public.matrix_transport_poison_events,
    public.matrix_transport_cursors
restart identity;

DO $test$
declare
    lease_a record;
    lease_b record;
    registration record;
    claim_a record;
    claim_b record;
    claim_c record;
    claim_d record;
    state_row record;
    disposition text;
    poison_count bigint;
    collision_seen boolean := false;
    mutation_seen boolean := false;
    stale_seen boolean := false;
    hash_a constant text := 'sha256:' || repeat('a', 64);
    hash_b constant text := 'sha256:' || repeat('b', 64);
    hash_c constant text := 'sha256:' || repeat('c', 64);
    hash_d constant text := 'sha256:' || repeat('d', 64);
    hash_e constant text := 'sha256:' || repeat('e', 64);
begin
    select * into strict lease_a
      from public.cex_matrix_acquire_cursor_lease_v1('room-alpha', 'poller-a', 60);
    if lease_a.cursor_revision <> 0 or lease_a.lease_fence <> 1 then
        raise exception 'unexpected initial cursor lease: %', row_to_json(lease_a);
    end if;

    if exists (
        select 1
          from public.cex_matrix_acquire_cursor_lease_v1('room-alpha', 'poller-b', 60)
    ) then
        raise exception 'active cursor lease was stolen';
    end if;

    select * into strict registration
      from public.cex_matrix_register_delivery_and_advance_v1(
        'room-alpha',
        'poller-a',
        lease_a.lease_fence,
        0,
        'sync-1',
        'event-1',
        hash_a,
        'sync-0',
        '00000000-0000-0000-0000-000000000001'::uuid,
        'consumer-entry',
        hash_b,
        '{"event":"one"}'::jsonb,
        2
      );
    if registration.event_disposition <> 'accepted'
       or registration.delivery_disposition <> 'enqueued'
       or registration.next_cursor_revision <> 1 then
        raise exception 'unexpected atomic registration: %', row_to_json(registration);
    end if;

    begin
        perform public.cex_matrix_register_delivery_and_advance_v1(
            'room-alpha',
            'poller-a',
            lease_a.lease_fence,
            0,
            'sync-stale',
            'event-stale',
            hash_c,
            'sync-0',
            '00000000-0000-0000-0000-000000000099'::uuid,
            'consumer-entry',
            hash_d,
            '{"event":"stale"}'::jsonb,
            2
        );
    exception when others then
        stale_seen := position('matrix_cursor_lease_or_revision_mismatch' in sqlerrm) > 0;
    end;
    if not stale_seen then
        raise exception 'stale cursor revision was accepted';
    end if;
    if exists (select 1 from public.matrix_transport_inbox where source_event_id = 'event-stale')
       or exists (select 1 from public.matrix_transport_outbox where source_event_id = 'event-stale') then
        raise exception 'failed atomic registration leaked durable state';
    end if;

    disposition := public.cex_matrix_accept_source_event_v1(
        'event-1', hash_a, 'room-alpha', 'sync-0'
    );
    if disposition <> 'replay' then
        raise exception 'exact inbox replay was not stable';
    end if;

    begin
        perform public.cex_matrix_accept_source_event_v1(
            'event-1', hash_c, 'room-alpha', 'sync-0'
        );
    exception when others then
        collision_seen := position('matrix_source_event_identity_collision' in sqlerrm) > 0;
    end;
    if not collision_seen then
        raise exception 'source event collision was accepted';
    end if;

    select * into strict claim_a
      from public.cex_matrix_claim_delivery_v1('sender-a', 60, 1);
    if claim_a.delivery_id <> '00000000-0000-0000-0000-000000000001'::uuid
       or claim_a.attempt_count <> 1 then
        raise exception 'unexpected first delivery claim: %', row_to_json(claim_a);
    end if;

    collision_seen := false;
    begin
        perform public.cex_matrix_finish_delivery_v1(
            claim_a.delivery_id,
            'wrong-owner',
            claim_a.lease_fence,
            'sent',
            null
        );
    exception when others then
        collision_seen := position('matrix_delivery_claim_mismatch' in sqlerrm) > 0;
    end;
    if not collision_seen then
        raise exception 'wrong owner completed a delivery';
    end if;

    disposition := public.cex_matrix_finish_delivery_v1(
        claim_a.delivery_id,
        'sender-a',
        claim_a.lease_fence,
        'retryable_failure',
        'downstream_timeout'
    );
    if disposition <> 'pending' then
        raise exception 'retryable delivery did not return to pending';
    end if;

    select * into strict claim_b
      from public.cex_matrix_claim_delivery_v1('sender-b', 60, 1);
    if claim_b.attempt_count <> 2 or claim_b.lease_fence <= claim_a.lease_fence then
        raise exception 'delivery retry did not advance attempt/fence';
    end if;
    disposition := public.cex_matrix_finish_delivery_v1(
        claim_b.delivery_id,
        'sender-b',
        claim_b.lease_fence,
        'retryable_failure',
        'downstream_timeout'
    );
    if disposition <> 'dead_letter' then
        raise exception 'retry exhaustion did not dead-letter';
    end if;

    select * into strict state_row
      from public.cex_matrix_lookup_delivery_v1(claim_b.delivery_id, hash_b);
    if state_row.status <> 'dead_letter' or state_row.attempt_count <> 2 then
        raise exception 'dead-letter lookup is inconsistent';
    end if;
    if exists (
        select 1 from public.cex_matrix_lookup_delivery_v1(claim_b.delivery_id, hash_c)
    ) then
        raise exception 'delivery lookup ignored payload hash';
    end if;

    perform public.cex_matrix_accept_source_event_v1(
        'event-2', hash_c, 'room-alpha', 'sync-1'
    );
    disposition := public.cex_matrix_enqueue_delivery_v1(
        '00000000-0000-0000-0000-000000000002'::uuid,
        'event-2',
        'consumer-entry',
        hash_d,
        '{"event":"two"}'::jsonb,
        3
    );
    if disposition <> 'enqueued' then
        raise exception 'second delivery was not enqueued';
    end if;
    select * into strict claim_c
      from public.cex_matrix_claim_delivery_v1('sender-c', 60, 1);
    disposition := public.cex_matrix_finish_delivery_v1(
        claim_c.delivery_id,
        'sender-c',
        claim_c.lease_fence,
        'sent',
        null
    );
    if disposition <> 'sent' then
        raise exception 'successful delivery did not become sent';
    end if;
    select * into strict state_row
      from public.cex_matrix_lookup_delivery_v1(claim_c.delivery_id, hash_d);
    if state_row.status <> 'sent' or state_row.sent_at is null then
        raise exception 'sent lookup lacks terminal evidence';
    end if;

    begin
        update public.matrix_transport_outbox
           set payload = '{"mutated":true}'::jsonb
         where delivery_id = claim_c.delivery_id;
    exception when others then
        mutation_seen := position('matrix_delivery_identity_mutation_rejected' in sqlerrm) > 0;
    end;
    if not mutation_seen then
        raise exception 'outbox payload mutation was accepted';
    end if;

    mutation_seen := false;
    begin
        update public.matrix_transport_inbox
           set source_event_sha256 = hash_e
         where source_event_id = 'event-2';
    exception when others then
        mutation_seen := position('matrix_immutable_history_mutation_rejected' in sqlerrm) > 0;
    end;
    if not mutation_seen then
        raise exception 'inbox mutation was accepted';
    end if;

    perform public.cex_matrix_accept_source_event_v1(
        'event-3', hash_e, 'room-alpha', 'sync-1'
    );
    perform public.cex_matrix_enqueue_delivery_v1(
        '00000000-0000-0000-0000-000000000003'::uuid,
        'event-3',
        'consumer-entry',
        hash_a,
        '{"event":"three"}'::jsonb,
        3
    );
    select * into strict claim_c
      from public.cex_matrix_claim_delivery_v1('sender-c', 60, 1);
    update public.matrix_transport_outbox
       set lease_expires_at = clock_timestamp() - interval '1 second'
     where delivery_id = claim_c.delivery_id;
    select * into strict claim_d
      from public.cex_matrix_claim_delivery_v1('sender-d', 60, 1);
    if claim_d.delivery_id <> claim_c.delivery_id
       or claim_d.lease_fence <= claim_c.lease_fence then
        raise exception 'expired delivery lease was not safely reclaimed';
    end if;
    if not exists (
        select 1
          from public.matrix_transport_delivery_history
         where delivery_id = claim_d.delivery_id
           and lease_fence = claim_d.lease_fence
           and from_status = 'claimed'
           and to_status = 'claimed'
           and owner = 'sender-d'
    ) then
        raise exception 'expired claim history lost its previous claimed state';
    end if;

    poison_count := public.cex_matrix_record_poison_event_v1(
        'poison-1', hash_a, 'room-alpha', 'unsupported_event'
    );
    if poison_count <> 1 then
        raise exception 'first poison observation is invalid';
    end if;
    poison_count := public.cex_matrix_record_poison_event_v1(
        'poison-1', hash_a, 'room-alpha', 'unsupported_event'
    );
    if poison_count <> 2 then
        raise exception 'poison replay did not increment';
    end if;
    collision_seen := false;
    begin
        perform public.cex_matrix_record_poison_event_v1(
            'poison-1', hash_b, 'room-alpha', 'unsupported_event'
        );
    exception when others then
        collision_seen := position('matrix_poison_event_identity_collision' in sqlerrm) > 0;
    end;
    if not collision_seen then
        raise exception 'poison identity collision was accepted';
    end if;
    if not public.cex_matrix_acknowledge_poison_event_v1(
        'poison-1', hash_a, 'operator-a', 'reviewed and isolated'
    ) then
        raise exception 'poison acknowledgement failed';
    end if;
    if public.cex_matrix_acknowledge_poison_event_v1(
        'poison-1', hash_a, 'operator-a', 'duplicate'
    ) then
        raise exception 'poison acknowledgement was not one-time';
    end if;

    update public.matrix_transport_cursors
       set lease_expires_at = clock_timestamp() - interval '1 second'
     where partition_id = 'room-alpha';
    select * into strict lease_b
      from public.cex_matrix_acquire_cursor_lease_v1('room-alpha', 'poller-b', 60);
    if lease_b.lease_fence <= lease_a.lease_fence or lease_b.cursor_revision <> 1 then
        raise exception 'cursor takeover did not preserve revision/increase fence';
    end if;
    if public.cex_matrix_advance_cursor_v1(
        'room-alpha', 'poller-a', lease_a.lease_fence, 1, 'forbidden'
    ) then
        raise exception 'stale cursor owner advanced after takeover';
    end if;
end;
$test$;
SQL

if [[ -n "$EVIDENCE" ]]; then
  EVIDENCE_PATH="$EVIDENCE"
  [[ "$EVIDENCE_PATH" = /* ]] || EVIDENCE_PATH="$ROOT/$EVIDENCE_PATH"
  mkdir -p "$(dirname "$EVIDENCE_PATH")"
  MATRIX_MIGRATION="$MIGRATION" MATRIX_EVIDENCE="$EVIDENCE_PATH" python3 - <<'PY'
import hashlib
import json
import os
from datetime import datetime, timezone
from pathlib import Path

migration = Path(os.environ["MATRIX_MIGRATION"])
output = Path(os.environ["MATRIX_EVIDENCE"])
result = {
    "schema": "cex.matrix-transport-postgres-evidence.v1",
    "status": "ok",
    "migration": "services/matrix-entry-adapter/migrations/0001_transport_durability.sql",
    "migration_sha256": hashlib.sha256(migration.read_bytes()).hexdigest(),
    "checks": [
        "cursor lease exclusion and expired takeover",
        "cursor revision and fence compare-and-set",
        "atomic inbox outbox cursor registration",
        "source replay and collision",
        "delivery claim ownership and fencing",
        "retry exhaustion and dead-letter",
        "sent response-loss lookup",
        "immutable inbox history and delivery identity",
        "expired delivery claim history",
        "poison replay collision and acknowledgement",
    ],
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "checker_may_grant_production_authorization": False,
    "production_authorization": "not_granted",
}
output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
fi

printf '%s\n' '{"schema":"cex.matrix-transport-postgres-check.v1","status":"ok","checker_may_grant_production_authorization":false,"production_authorization":"not_granted"}'

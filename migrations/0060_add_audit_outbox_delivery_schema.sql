begin;

alter table public.cex_audit_outbox_v1
    add column if not exists last_http_status integer,
    add column if not exists last_attempt_at timestamptz,
    add column if not exists delivered_event_hash text,
    add column if not exists delivered_tenant_sequence bigint,
    add column if not exists delivery_receipt jsonb,
    add column if not exists dead_lettered_at timestamptz;

do $migration$
begin
    if not exists (
        select 1
          from pg_constraint
         where conname = 'cex_audit_outbox_http_status_v1'
           and conrelid = 'public.cex_audit_outbox_v1'::regclass
    ) then
        alter table public.cex_audit_outbox_v1
            add constraint cex_audit_outbox_http_status_v1
            check (last_http_status is null or last_http_status between 100 and 599);
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conname = 'cex_audit_outbox_delivery_hash_v1'
           and conrelid = 'public.cex_audit_outbox_v1'::regclass
    ) then
        alter table public.cex_audit_outbox_v1
            add constraint cex_audit_outbox_delivery_hash_v1
            check (
                delivered_event_hash is null
                or delivered_event_hash ~ '^sha256:[0-9a-f]{64}$'
            );
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conname = 'cex_audit_outbox_delivery_sequence_v1'
           and conrelid = 'public.cex_audit_outbox_v1'::regclass
    ) then
        alter table public.cex_audit_outbox_v1
            add constraint cex_audit_outbox_delivery_sequence_v1
            check (
                delivered_tenant_sequence is null
                or delivered_tenant_sequence > 0
            );
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conname = 'cex_audit_outbox_delivery_receipt_v1'
           and conrelid = 'public.cex_audit_outbox_v1'::regclass
    ) then
        alter table public.cex_audit_outbox_v1
            add constraint cex_audit_outbox_delivery_receipt_v1
            check (
                delivery_receipt is null
                or jsonb_typeof(delivery_receipt) = 'object'
            );
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conname = 'cex_audit_outbox_dead_letter_time_v1'
           and conrelid = 'public.cex_audit_outbox_v1'::regclass
    ) then
        alter table public.cex_audit_outbox_v1
            add constraint cex_audit_outbox_dead_letter_time_v1
            check (
                (status = 'dead_letter' and dead_lettered_at is not null)
                or
                (status <> 'dead_letter' and dead_lettered_at is null)
            ) not valid;
    end if;

    if not exists (
        select 1
          from pg_constraint
         where conname = 'cex_audit_outbox_verified_delivery_v1'
           and conrelid = 'public.cex_audit_outbox_v1'::regclass
    ) then
        alter table public.cex_audit_outbox_v1
            add constraint cex_audit_outbox_verified_delivery_v1
            check (
                status <> 'delivered'
                or (
                    delivered_at is not null
                    and delivered_event_hash is not null
                    and delivered_tenant_sequence is not null
                    and delivery_receipt is not null
                )
            ) not valid;
    end if;
end
$migration$;

create or replace function public.cex_deterministic_uuid_v1(
    p_material text
)
returns uuid
language plpgsql
immutable
strict
set search_path = pg_catalog, public
as $$
declare
    digest_hex text;
    uuid_text text;
begin
    if length(p_material) > 4096 then
        raise exception 'deterministic UUID material exceeds 4096 characters';
    end if;

    digest_hex := encode(digest(p_material, 'sha256'), 'hex');
    uuid_text :=
        substr(digest_hex, 1, 8) || '-' ||
        substr(digest_hex, 9, 4) || '-' ||
        '5' || substr(digest_hex, 14, 3) || '-' ||
        'a' || substr(digest_hex, 18, 3) || '-' ||
        substr(digest_hex, 21, 12);

    return uuid_text::uuid;
end
$$;

create or replace function public.cex_enqueue_audit_outbox_v1(
    p_source_service text,
    p_event_id uuid,
    p_trace_id uuid,
    p_org_id uuid,
    p_envelope jsonb,
    p_max_attempts integer default 10
)
returns public.cex_audit_outbox_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    existing_outbox public.cex_audit_outbox_v1%rowtype;
    inserted_outbox public.cex_audit_outbox_v1%rowtype;
    envelope_org_id uuid;
    occurred_at_value timestamptz;
begin
    if p_source_service is null
       or p_source_service !~ '^[a-z0-9][a-z0-9._-]{0,127}$' then
        raise exception 'invalid audit outbox source_service';
    end if;
    if p_event_id is null
       or p_event_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'audit outbox event_id must be a non-nil UUID';
    end if;
    if p_trace_id is null
       or p_trace_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'audit outbox trace_id must be a non-nil UUID';
    end if;
    if p_max_attempts is null or p_max_attempts not between 1 and 100 then
        raise exception 'audit outbox max_attempts must be between 1 and 100';
    end if;
    if p_envelope is null or jsonb_typeof(p_envelope) <> 'object' then
        raise exception 'audit outbox envelope must be a JSON object';
    end if;
    if p_envelope ->> 'event_id' is distinct from p_event_id::text then
        raise exception 'audit outbox envelope event_id mismatch';
    end if;
    if p_envelope ->> 'trace_id' is distinct from p_trace_id::text then
        raise exception 'audit outbox envelope trace_id mismatch';
    end if;
    if not (p_envelope ? 'org_id') then
        raise exception 'audit outbox envelope must contain org_id';
    end if;

    if jsonb_typeof(p_envelope -> 'org_id') = 'null' then
        envelope_org_id := null;
    elsif jsonb_typeof(p_envelope -> 'org_id') = 'string' then
        begin
            envelope_org_id := (p_envelope ->> 'org_id')::uuid;
        exception
            when invalid_text_representation then
                raise exception 'audit outbox envelope org_id is not a UUID';
        end;
    else
        raise exception 'audit outbox envelope org_id must be a UUID string or null';
    end if;

    if envelope_org_id is distinct from p_org_id then
        raise exception 'audit outbox envelope org_id mismatch';
    end if;
    if jsonb_typeof(p_envelope -> 'actor_type') is distinct from 'string'
       or length(btrim(p_envelope ->> 'actor_type')) not between 1 and 128
       or (p_envelope ->> 'actor_type') !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$' then
        raise exception 'audit outbox envelope actor_type is invalid';
    end if;
    if p_envelope ? 'actor_id'
       and jsonb_typeof(p_envelope -> 'actor_id') not in ('string', 'null') then
        raise exception 'audit outbox envelope actor_id must be a string or null';
    end if;
    if jsonb_typeof(p_envelope -> 'actor_id') = 'string'
       and length(btrim(p_envelope ->> 'actor_id')) not between 1 and 256 then
        raise exception 'audit outbox envelope actor_id is invalid';
    end if;
    if jsonb_typeof(p_envelope -> 'event_type') is distinct from 'string'
       or length(btrim(p_envelope ->> 'event_type')) not between 1 and 128
       or (p_envelope ->> 'event_type') !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$' then
        raise exception 'audit outbox envelope event_type is invalid';
    end if;
    if jsonb_typeof(p_envelope -> 'schema_version') is distinct from 'string'
       or p_envelope ->> 'schema_version' <> 'cex.audit.event.v2' then
        raise exception 'audit outbox envelope schema_version must be cex.audit.event.v2';
    end if;
    if jsonb_typeof(p_envelope -> 'occurred_at') is distinct from 'string' then
        raise exception 'audit outbox envelope occurred_at must be a timestamp string';
    end if;

    begin
        occurred_at_value := (p_envelope ->> 'occurred_at')::timestamptz;
    exception
        when invalid_datetime_format or datetime_field_overflow then
            raise exception 'audit outbox envelope occurred_at is invalid';
    end;

    if occurred_at_value > clock_timestamp() + interval '5 minutes' then
        raise exception 'audit outbox envelope occurred_at exceeds future-time allowance';
    end if;
    if jsonb_typeof(p_envelope -> 'payload') is distinct from 'object' then
        raise exception 'audit outbox envelope payload must be a JSON object';
    end if;
    if (p_envelope -> 'payload') ? '_cex_audit_writer' then
        raise exception 'audit outbox payload may not set reserved writer metadata';
    end if;
    if p_envelope #>> '{payload,_cex_audit_source_service}'
       is distinct from p_source_service then
        raise exception 'audit outbox source_service and payload source marker differ';
    end if;

    perform pg_advisory_xact_lock(hashtextextended(p_event_id::text, 0));

    select *
      into existing_outbox
      from public.cex_audit_outbox_v1
     where event_id = p_event_id
     for update;

    if found then
        if existing_outbox.source_service is distinct from p_source_service
           or existing_outbox.org_id is distinct from p_org_id
           or existing_outbox.trace_id is distinct from p_trace_id
           or existing_outbox.envelope is distinct from p_envelope
           or existing_outbox.max_attempts is distinct from p_max_attempts then
            raise exception using
                errcode = '23505',
                message = 'audit outbox event id collision with different immutable content';
        end if;
        return existing_outbox;
    end if;

    insert into public.cex_audit_outbox_v1 (
        event_id,
        source_service,
        org_id,
        trace_id,
        max_attempts,
        envelope
    ) values (
        p_event_id,
        p_source_service,
        p_org_id,
        p_trace_id,
        p_max_attempts,
        p_envelope
    )
    returning * into inserted_outbox;

    return inserted_outbox;
end
$$;

commit;

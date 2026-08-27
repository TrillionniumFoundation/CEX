begin;

alter table public.api_keys
    add column if not exists audit_revision bigint not null default 0;

alter table public.api_keys
    drop constraint if exists api_keys_audit_revision_v1,
    add constraint api_keys_audit_revision_v1 check (audit_revision >= 0);

create or replace function public.cex_api_key_prepare_audit_revision_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'INSERT' then
        new.audit_revision := 1;
        return new;
    end if;

    if new.org_id is distinct from old.org_id
       or new.user_id is distinct from old.user_id
       or new.key_hash is distinct from old.key_hash
       or new.key_prefix is distinct from old.key_prefix
       or new.label is distinct from old.label
       or new.status is distinct from old.status
       or new.expires_at is distinct from old.expires_at
       or new.revoked_at is distinct from old.revoked_at
       or new.revoked_reason is distinct from old.revoked_reason then
        new.audit_revision := old.audit_revision + 1;
    else
        new.audit_revision := old.audit_revision;
    end if;

    return new;
end
$$;

create or replace function public.cex_enqueue_api_key_audit_v1(
    old_record public.api_keys,
    new_record public.api_keys
)
returns public.api_keys
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    event_type_value text;
    event_id_value uuid;
    actor_id_value text;
    actor_label_value text;
    payload_value jsonb;
    envelope_value jsonb;
begin
    if old_record is not null
       and new_record.audit_revision = old_record.audit_revision then
        return new_record;
    end if;

    event_type_value := case
        when old_record is null then 'identity.api_key.persisted.issued'
        when new_record.status = 'revoked'
             and old_record.status is distinct from 'revoked'
            then 'identity.api_key.persisted.revoked'
        when new_record.expires_at is distinct from old_record.expires_at
            then 'identity.api_key.persisted.expiry_changed'
        when new_record.key_hash is distinct from old_record.key_hash
             or new_record.key_prefix is distinct from old_record.key_prefix
            then 'identity.api_key.persisted.material_changed'
        else 'identity.api_key.persisted.changed'
    end;

    actor_id_value := coalesce(
        nullif(current_setting('cex.audit.actor_id', true), ''),
        new_record.user_id::text
    );
    actor_label_value := coalesce(
        nullif(current_setting('cex.audit.actor_label', true), ''),
        'api-key:' || new_record.key_prefix
    );

    event_id_value := public.cex_deterministic_uuid_v1(
        'identity-service:' || new_record.api_key_id::text || ':' ||
        new_record.audit_revision::text || ':' || event_type_value
    );

    payload_value := jsonb_build_object(
        '_cex_audit_source_service', 'identity-service',
        'api_key_id', new_record.api_key_id,
        'audit_revision', new_record.audit_revision,
        'org_id', new_record.org_id,
        'user_id', new_record.user_id,
        'key_prefix', new_record.key_prefix,
        'label', new_record.label,
        'previous_status', case when old_record is null then null else old_record.status end,
        'status', new_record.status,
        'previous_expires_at', case when old_record is null then null else old_record.expires_at end,
        'expires_at', new_record.expires_at,
        'revoked_at', new_record.revoked_at,
        'revoked_reason', new_record.revoked_reason,
        'admin_actor_label', actor_label_value
    );

    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', new_record.api_key_id,
        'org_id', new_record.org_id,
        'actor_type', 'identity-service',
        'actor_id', actor_id_value,
        'event_type', event_type_value,
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', clock_timestamp(),
        'payload', payload_value
    );

    perform public.cex_enqueue_audit_outbox_v1(
        'identity-service',
        event_id_value,
        new_record.api_key_id,
        new_record.org_id,
        envelope_value,
        10
    );

    return new_record;
end
$$;

drop trigger if exists trg_cex_api_key_prepare_audit_revision_v1
    on public.api_keys;
create trigger trg_cex_api_key_prepare_audit_revision_v1
before insert or update on public.api_keys
for each row execute function public.cex_api_key_prepare_audit_revision_v1();

drop trigger if exists trg_cex_api_key_enqueue_audit_v1
    on public.api_keys;
create trigger trg_cex_api_key_enqueue_audit_v1
after insert or update on public.api_keys
for each row execute function public.cex_enqueue_api_key_audit_v1(
    case when tg_op = 'INSERT' then null else old end,
    new
);

create index if not exists idx_api_keys_audit_revision_v1
    on public.api_keys (api_key_id, audit_revision);

commit;

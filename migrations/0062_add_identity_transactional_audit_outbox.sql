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

create or replace function public.cex_enqueue_api_key_audit_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    event_type_value text;
    event_id_value uuid;
    actor_id_value text;
    actor_label_value text;
    previous_status_value text;
    previous_expires_at_value timestamptz;
    payload_value jsonb;
    envelope_value jsonb;
begin
    if tg_op <> 'INSERT' then
        if new.audit_revision = old.audit_revision then
            return new;
        end if;
        previous_status_value := old.status;
        previous_expires_at_value := old.expires_at;

        if new.status = 'revoked'
           and old.status is distinct from 'revoked' then
            event_type_value := 'identity.api_key.persisted.revoked';
        elsif new.expires_at is distinct from old.expires_at then
            event_type_value := 'identity.api_key.persisted.expiry_changed';
        elsif new.key_hash is distinct from old.key_hash
              or new.key_prefix is distinct from old.key_prefix then
            event_type_value := 'identity.api_key.persisted.material_changed';
        else
            event_type_value := 'identity.api_key.persisted.changed';
        end if;
    else
        previous_status_value := null;
        previous_expires_at_value := null;
        event_type_value := 'identity.api_key.persisted.issued';
    end if;

    actor_id_value := coalesce(
        nullif(current_setting('cex.audit.actor_id', true), ''),
        new.user_id::text
    );
    actor_label_value := coalesce(
        nullif(current_setting('cex.audit.actor_label', true), ''),
        'api-key:' || new.key_prefix
    );

    event_id_value := public.cex_deterministic_uuid_v1(
        'identity-service:' || new.api_key_id::text || ':' ||
        new.audit_revision::text || ':' || event_type_value
    );

    payload_value := jsonb_build_object(
        '_cex_audit_source_service', 'identity-service',
        'api_key_id', new.api_key_id,
        'audit_revision', new.audit_revision,
        'org_id', new.org_id,
        'user_id', new.user_id,
        'key_prefix', new.key_prefix,
        'label', new.label,
        'previous_status', previous_status_value,
        'status', new.status,
        'previous_expires_at', previous_expires_at_value,
        'expires_at', new.expires_at,
        'revoked_at', new.revoked_at,
        'revoked_reason', new.revoked_reason,
        'admin_actor_label', actor_label_value
    );

    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', new.api_key_id,
        'org_id', new.org_id,
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
        new.api_key_id,
        new.org_id,
        envelope_value,
        10
    );

    return new;
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
for each row execute function public.cex_enqueue_api_key_audit_v1();

create index if not exists idx_api_keys_audit_revision_v1
    on public.api_keys (api_key_id, audit_revision);

commit;

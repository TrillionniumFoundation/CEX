begin;

alter table public.api_keys
    add column if not exists audit_revision bigint not null default 0;

do $migration$
begin
    if not exists (
        select 1
          from pg_constraint
         where conname = 'api_keys_audit_revision_nonnegative_v1'
           and conrelid = 'public.api_keys'::regclass
    ) then
        alter table public.api_keys
            add constraint api_keys_audit_revision_nonnegative_v1
            check (audit_revision >= 0) not valid;
    end if;
end
$migration$;

create or replace function public.cex_prepare_api_key_audit_revision_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'INSERT' then
        new.audit_revision := 1;
        return new;
    end if;

    if old.org_id is distinct from new.org_id
       or old.user_id is distinct from new.user_id
       or old.key_hash is distinct from new.key_hash
       or old.key_prefix is distinct from new.key_prefix
       or old.label is distinct from new.label
       or old.status is distinct from new.status
       or old.expires_at is distinct from new.expires_at
       or old.revoked_at is distinct from new.revoked_at
       or old.revoked_reason is distinct from new.revoked_reason then
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
    envelope_value jsonb;
begin
    if tg_op = 'UPDATE' and new.audit_revision = old.audit_revision then
        return new;
    end if;

    event_type_value := case
        when tg_op = 'INSERT' then 'identity.api_key.persisted.issued'
        when new.status = 'revoked'
          and (old.status is distinct from new.status or old.revoked_at is distinct from new.revoked_at)
            then 'identity.api_key.persisted.revoked'
        when old.expires_at is distinct from new.expires_at
            then 'identity.api_key.persisted.expiry_changed'
        when old.key_hash is distinct from new.key_hash
          or old.key_prefix is distinct from new.key_prefix
            then 'identity.api_key.persisted.material_changed'
        else 'identity.api_key.persisted.changed'
    end;

    event_id_value := public.cex_deterministic_uuid_v1(
        'identity-service:api_keys:' ||
        new.api_key_id::text || ':' ||
        new.audit_revision::text || ':' ||
        event_type_value
    );

    actor_id_value := coalesce(
        nullif(current_setting('cex.audit.actor_id', true), ''),
        new.user_id::text
    );
    actor_label_value := nullif(
        current_setting('cex.audit.actor_label', true),
        ''
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
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'identity-service',
            'api_key_id', new.api_key_id,
            'org_id', new.org_id,
            'user_id', new.user_id,
            'key_prefix', new.key_prefix,
            'label', new.label,
            'status', new.status,
            'expires_at', new.expires_at,
            'last_used_at', new.last_used_at,
            'revoked_at', new.revoked_at,
            'revoked_reason', new.revoked_reason,
            'audit_revision', new.audit_revision,
            'previous_status', case when tg_op = 'INSERT' then null else old.status end,
            'key_material_changed', case
                when tg_op = 'INSERT' then false
                else old.key_hash is distinct from new.key_hash
                  or old.key_prefix is distinct from new.key_prefix
            end,
            'admin_actor_label', actor_label_value
        )
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

drop trigger if exists trg_cex_prepare_api_key_audit_revision_v1
    on public.api_keys;
create trigger trg_cex_prepare_api_key_audit_revision_v1
before insert or update on public.api_keys
for each row execute function public.cex_prepare_api_key_audit_revision_v1();

drop trigger if exists trg_cex_enqueue_api_key_audit_v1
    on public.api_keys;
create trigger trg_cex_enqueue_api_key_audit_v1
after insert or update on public.api_keys
for each row execute function public.cex_enqueue_api_key_audit_v1();

alter table public.api_keys
    validate constraint api_keys_audit_revision_nonnegative_v1;

create index if not exists idx_api_keys_audit_revision_v1
    on public.api_keys (api_key_id, audit_revision);

commit;

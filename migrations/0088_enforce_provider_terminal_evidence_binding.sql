begin;

-- P0 provider-dispatch closure.  Automatic provider success and operator
-- reconciliation remain separate authority models, but both are enforced at
-- the database boundary and both bind terminal state to immutable evidence.

create or replace function public.cex_provider_target_ref_v1(
    p_provider_target text
)
returns text
language plpgsql
immutable
set search_path = pg_catalog, public
as $$
declare
    provider_ref_value text;
begin
    if p_provider_target is null
       or length(p_provider_target) not between length('ollama://') + 1
                                                and length('ollama://') + 503
       or p_provider_target !~ '^ollama://[^[:space:][:cntrl:]]+$' then
        raise exception 'provider target is not a canonical Ollama target';
    end if;

    provider_ref_value := substring(
        p_provider_target from length('ollama://') + 1
    );
    if length(provider_ref_value) not between 1 and 503
       or provider_ref_value is distinct from btrim(provider_ref_value)
       or position('://' in provider_ref_value) > 0 then
        raise exception 'provider target reference is empty or non-canonical';
    end if;
    return provider_ref_value;
end
$$;

create or replace function public.cex_provider_result_sha256_v1(
    p_result_payload jsonb
)
returns text
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select 'sha256:' || encode(digest(p_result_payload::text, 'sha256'), 'hex')
$$;

create or replace function public.cex_validate_provider_live_terminal_result_v1(
    p_provider_target text,
    p_result_payload jsonb,
    p_result_sha256 text
)
returns void
language plpgsql
immutable
set search_path = pg_catalog, public
as $$
declare
    expected_provider_ref text;
    expected_result_sha256 text;
begin
    expected_provider_ref := public.cex_provider_target_ref_v1(p_provider_target);

    if p_result_payload is null or jsonb_typeof(p_result_payload) <> 'object' then
        raise exception 'live provider success requires an object result payload';
    end if;
    if jsonb_typeof(p_result_payload -> 'provider') is distinct from 'string'
       or p_result_payload ->> 'provider' is distinct from 'ollama' then
        raise exception 'live provider result provider does not match immutable target';
    end if;
    if jsonb_typeof(p_result_payload -> 'provider_ref') is distinct from 'string'
       or p_result_payload ->> 'provider_ref' is distinct from expected_provider_ref then
        raise exception 'live provider result provider_ref does not match immutable target';
    end if;
    if jsonb_typeof(p_result_payload -> 'model') is distinct from 'string'
       or p_result_payload ->> 'model' is distinct from expected_provider_ref then
        raise exception 'live provider result model does not match immutable target';
    end if;
    if jsonb_typeof(p_result_payload -> 'done') is distinct from 'boolean'
       or p_result_payload -> 'done' is distinct from 'true'::jsonb then
        raise exception 'live provider result requires explicit done=true';
    end if;
    if jsonb_typeof(p_result_payload -> 'output_text') is distinct from 'string'
       or length(btrim(p_result_payload ->> 'output_text')) = 0 then
        raise exception 'live provider result requires non-empty output evidence';
    end if;

    expected_result_sha256 := public.cex_provider_result_sha256_v1(p_result_payload);
    if p_result_sha256 is null
       or p_result_sha256 is distinct from expected_result_sha256 then
        raise exception 'provider terminal result hash does not match canonical JSONB payload';
    end if;
end
$$;

create or replace function public.cex_validate_provider_reconciled_terminal_result_v1(
    p_command_id uuid,
    p_attempt_count integer,
    p_result_payload jsonb,
    p_result_sha256 text,
    p_completed_at timestamptz
)
returns void
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    evidence_row public.cex_provider_reconciliation_evidence_v1%rowtype;
    expected_result_sha256 text;
begin
    if p_result_payload is null or jsonb_typeof(p_result_payload) <> 'object' then
        raise exception 'reconciled provider success requires an object result payload';
    end if;
    expected_result_sha256 := public.cex_provider_result_sha256_v1(p_result_payload);
    if p_result_sha256 is null
       or p_result_sha256 is distinct from expected_result_sha256 then
        raise exception 'provider terminal result hash does not match canonical JSONB payload';
    end if;

    select * into evidence_row
      from public.cex_provider_reconciliation_evidence_v1
     where command_id = p_command_id
       and incident_attempt_count = p_attempt_count
       and disposition = 'confirmed_executed';
    if not found then
        raise exception 'reconciled provider success requires same-attempt confirmed-executed evidence';
    end if;
    if evidence_row.result_payload is distinct from p_result_payload then
        raise exception 'reconciled provider result differs from immutable reconciliation evidence';
    end if;
    if p_completed_at is null or evidence_row.recorded_at > p_completed_at then
        raise exception 'reconciled provider completion predates its immutable evidence';
    end if;
end
$$;

create or replace function public.cex_validate_provider_dispatch_binding_v1(
    p_execution_id uuid,
    p_invocation_id uuid,
    p_org_id uuid,
    p_trace_id uuid,
    p_provider_target text,
    p_prompt_sha256 text,
    p_request_fingerprint text
)
returns void
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    execution_invocation_id uuid;
    execution_org_id uuid;
    execution_trace_id uuid;
    execution_provider_target text;
    prompt_value text;
    expected_prompt_sha256 text;
    expected_request_fingerprint text;
begin
    perform public.cex_provider_target_ref_v1(p_provider_target);

    select execution.invocation_id,
           execution.org_id,
           execution.trace_id,
           execution.provider_target,
           invocation.request_payload ->> 'prompt'
      into execution_invocation_id,
           execution_org_id,
           execution_trace_id,
           execution_provider_target,
           prompt_value
      from public.executions execution
      join public.invocations invocation
        on invocation.invocation_id = execution.invocation_id
     where execution.execution_id = p_execution_id;
    if not found then
        raise exception 'provider dispatch execution/invocation binding does not exist';
    end if;
    if execution_invocation_id is distinct from p_invocation_id
       or execution_org_id is distinct from p_org_id
       or execution_trace_id is distinct from p_trace_id
       or execution_provider_target is distinct from p_provider_target then
        raise exception 'provider dispatch command differs from immutable execution identity';
    end if;
    if prompt_value is null or length(prompt_value) not between 1 and 1048576 then
        raise exception 'provider dispatch immutable prompt is missing or too large';
    end if;

    expected_prompt_sha256 := 'sha256:'
        || encode(digest(prompt_value, 'sha256'), 'hex');
    expected_request_fingerprint := public.cex_provider_dispatch_fingerprint_v1(
        p_execution_id,
        p_invocation_id,
        p_org_id,
        p_trace_id,
        p_provider_target,
        expected_prompt_sha256
    );
    if p_prompt_sha256 is distinct from expected_prompt_sha256
       or p_request_fingerprint is distinct from expected_request_fingerprint then
        raise exception 'provider dispatch prompt or request fingerprint is invalid';
    end if;
end
$$;

create or replace function public.cex_guard_provider_dispatch_command_contract_v2()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'DELETE' then
        raise exception 'provider dispatch commands cannot be deleted';
    end if;

    if tg_op = 'INSERT' then
        perform public.cex_validate_provider_dispatch_binding_v1(
            new.execution_id,
            new.invocation_id,
            new.org_id,
            new.trace_id,
            new.provider_target,
            new.prompt_sha256,
            new.request_fingerprint
        );
        if new.status <> 'pending'
           or new.attempt_count <> 0
           or new.requeue_count <> 0
           or new.claimed_by is not null
           or new.lease_expires_at is not null
           or new.result_payload is not null
           or new.result_sha256 is not null
           or new.completed_at is not null then
            raise exception 'provider dispatch insert must start as an untouched pending command';
        end if;
        return new;
    end if;

    if new.command_id is distinct from old.command_id
       or new.execution_id is distinct from old.execution_id
       or new.invocation_id is distinct from old.invocation_id
       or new.org_id is distinct from old.org_id
       or new.trace_id is distinct from old.trace_id
       or new.provider_target is distinct from old.provider_target
       or new.prompt_sha256 is distinct from old.prompt_sha256
       or new.request_fingerprint is distinct from old.request_fingerprint
       or new.created_at is distinct from old.created_at then
        raise exception 'provider dispatch immutable fields cannot be changed';
    end if;
    if new.updated_at < old.updated_at then
        raise exception 'provider dispatch updated_at cannot move backwards';
    end if;
    if old.status in ('succeeded', 'cancelled')
       and to_jsonb(new) is distinct from to_jsonb(old) then
        raise exception 'terminal provider dispatch command is immutable';
    end if;

    if new.status is distinct from old.status then
        if old.status = 'pending' and new.status in ('claimed', 'cancelled') then
            null;
        elsif old.status = 'retry_wait' and new.status in ('claimed', 'cancelled') then
            null;
        elsif old.status = 'claimed'
           and new.status in (
               'succeeded', 'retry_wait', 'reconcile_required',
               'dead_letter', 'cancelled'
           ) then
            null;
        elsif old.status in ('reconcile_required', 'dead_letter')
           and new.status in ('pending', 'succeeded', 'cancelled') then
            null;
        else
            raise exception 'invalid provider dispatch transition % -> % for command %',
                old.status, new.status, old.command_id;
        end if;
    end if;

    if new.status = 'claimed' and old.status in ('pending', 'retry_wait') then
        if new.attempt_count <> old.attempt_count + 1 then
            raise exception 'provider claim must increment attempt_count exactly once';
        end if;
    elsif new.attempt_count is distinct from old.attempt_count then
        raise exception 'provider attempt_count may change only during claim';
    end if;

    if new.max_attempts is distinct from old.max_attempts then
        if old.status not in ('reconcile_required', 'dead_letter')
           or new.status <> 'pending'
           or new.max_attempts <= old.max_attempts
           or new.max_attempts - old.max_attempts not between 1 and 20
           or new.last_requeue_additional_attempts
                is distinct from (new.max_attempts - old.max_attempts) then
            raise exception 'provider max_attempts may increase only during bounded operator requeue';
        end if;
    end if;
    if new.requeue_count is distinct from old.requeue_count then
        if old.status not in ('reconcile_required', 'dead_letter')
           or new.status <> 'pending'
           or new.requeue_count <> old.requeue_count + 1 then
            raise exception 'provider requeue_count may increment only during operator requeue';
        end if;
    end if;
    return new;
end
$$;

create or replace function public.cex_guard_provider_terminal_evidence_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    perform public.cex_provider_target_ref_v1(new.provider_target);

    if tg_op = 'UPDATE'
       and old.status in ('succeeded', 'cancelled')
       and to_jsonb(new) is distinct from to_jsonb(old) then
        raise exception 'terminal provider command evidence is immutable';
    end if;

    if new.status = 'succeeded' then
        if tg_op <> 'UPDATE' then
            raise exception 'provider success must be reached through an authorized transition';
        elsif old.status = 'claimed' then
            perform public.cex_validate_provider_live_terminal_result_v1(
                new.provider_target,
                new.result_payload,
                new.result_sha256
            );
        elsif old.status in ('reconcile_required', 'dead_letter') then
            perform public.cex_validate_provider_reconciled_terminal_result_v1(
                new.command_id,
                new.attempt_count,
                new.result_payload,
                new.result_sha256,
                new.completed_at
            );
        else
            raise exception 'provider success has no authorized evidence path';
        end if;
        if new.completed_at is null then
            raise exception 'provider terminal success requires completed_at';
        end if;
    elsif new.result_payload is not null
       or new.result_sha256 is not null
       or new.completed_at is not null then
        raise exception 'non-success provider command cannot retain terminal result evidence';
    end if;
    return new;
end
$$;

-- Refuse to install the guards over divergent historical rows.  The migration
-- never fabricates provider identity, operator evidence or terminal output.
-- Hold a table lock across preflight and trigger installation so no concurrent
-- writer can enter between validation and enforcement.
lock table public.cex_provider_dispatch_commands_v1 in share row exclusive mode;

-- Historical terminal rows are validated against the authority model that
-- produced them: same-attempt confirmed-executed evidence when present,
-- otherwise the live provider envelope.
do $$
declare
    command_row public.cex_provider_dispatch_commands_v1%rowtype;
    reconciled boolean;
begin
    for command_row in
        select *
          from public.cex_provider_dispatch_commands_v1
         order by created_at, command_id
    loop
        perform public.cex_validate_provider_dispatch_binding_v1(
            command_row.execution_id,
            command_row.invocation_id,
            command_row.org_id,
            command_row.trace_id,
            command_row.provider_target,
            command_row.prompt_sha256,
            command_row.request_fingerprint
        );
        if command_row.status = 'succeeded' then
            select exists (
                select 1
                  from public.cex_provider_reconciliation_evidence_v1 evidence
                 where evidence.command_id = command_row.command_id
                   and evidence.incident_attempt_count = command_row.attempt_count
                   and evidence.disposition = 'confirmed_executed'
            ) into reconciled;
            if reconciled then
                perform public.cex_validate_provider_reconciled_terminal_result_v1(
                    command_row.command_id,
                    command_row.attempt_count,
                    command_row.result_payload,
                    command_row.result_sha256,
                    command_row.completed_at
                );
            else
                perform public.cex_validate_provider_live_terminal_result_v1(
                    command_row.provider_target,
                    command_row.result_payload,
                    command_row.result_sha256
                );
                if command_row.completed_at is null then
                    raise exception 'provider terminal success % lacks completed_at',
                        command_row.command_id;
                end if;
            end if;
        elsif command_row.result_payload is not null
           or command_row.result_sha256 is not null
           or command_row.completed_at is not null then
            raise exception
                'non-success provider command % retains terminal result evidence',
                command_row.command_id;
        end if;
    end loop;
end
$$;

drop trigger if exists trg_cex_guard_provider_dispatch_command_contract_v2
    on public.cex_provider_dispatch_commands_v1;
create trigger trg_cex_guard_provider_dispatch_command_contract_v2
before insert or update or delete on public.cex_provider_dispatch_commands_v1
for each row execute function public.cex_guard_provider_dispatch_command_contract_v2();

drop trigger if exists trg_cex_guard_provider_terminal_evidence_v1
    on public.cex_provider_dispatch_commands_v1;
create trigger trg_cex_guard_provider_terminal_evidence_v1
before insert or update on public.cex_provider_dispatch_commands_v1
for each row execute function public.cex_guard_provider_terminal_evidence_v1();

-- Keep both guards as normal-origin triggers. Logical replication and initial
-- table synchronization must not be forced to resolve cross-table evidence
-- before the reconciliation table has been applied. Trigger-bypass authority
-- must remain outside every production application-writer role.

comment on function public.cex_validate_provider_live_terminal_result_v1(text,jsonb,text) is
    'Binds automatic provider success to immutable Ollama target, terminal response and payload hash.';
comment on function public.cex_validate_provider_reconciled_terminal_result_v1(uuid,integer,jsonb,text,timestamptz) is
    'Binds operator-confirmed success to same-attempt append-only reconciliation evidence and payload hash.';
comment on function public.cex_validate_provider_dispatch_binding_v1(uuid,uuid,uuid,uuid,text,text,text) is
    'Validates Execution, Invocation, prompt hash and request fingerprint bindings for a provider command.';

commit;

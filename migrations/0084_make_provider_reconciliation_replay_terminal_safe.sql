begin;

-- 0082 recorded immutable provider reconciliation evidence correctly, but the
-- exact-replay branch was unreachable after confirmed_executed transitioned
-- the command to succeeded. Resolve evidence before requiring a live
-- reconciliation state so response-loss replay remains side-effect free.
create or replace function public.cex_record_provider_reconciliation_v1(
    p_command_id uuid,
    p_actor text,
    p_disposition text,
    p_artifact_uri text,
    p_artifact_sha256 text,
    p_evidence jsonb,
    p_result_payload jsonb default null
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_provider_dispatch_commands_v1%rowtype;
    existing_row public.cex_provider_reconciliation_evidence_v1%rowtype;
    evidence_row public.cex_provider_reconciliation_evidence_v1%rowtype;
    evidence_id_value uuid;
    result_hash_value text;
    event_id_value uuid;
    envelope_value jsonb;
begin
    if p_command_id is null or p_command_id='00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'provider reconciliation command_id must be non-nil';
    end if;
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256 then
        raise exception 'provider reconciliation actor is invalid';
    end if;
    if p_disposition not in ('confirmed_not_executed','confirmed_executed','indeterminate') then
        raise exception 'provider reconciliation disposition is invalid';
    end if;
    if p_artifact_uri is null
       or p_artifact_uri !~ '^(https://|gh://|oci://|s3://|gs://|file://)[^[:space:]]+$'
       or p_artifact_sha256 is null
       or p_artifact_sha256 !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'provider reconciliation requires an immutable artifact URI and SHA-256';
    end if;
    if p_evidence is null or jsonb_typeof(p_evidence) <> 'object' then
        raise exception 'provider reconciliation evidence must be an object';
    end if;
    if p_disposition='confirmed_executed'
       and (p_result_payload is null or jsonb_typeof(p_result_payload) <> 'object') then
        raise exception 'confirmed provider execution requires an object result payload';
    end if;

    select * into command_row
      from public.cex_provider_dispatch_commands_v1
     where command_id=p_command_id
     for update;
    if not found then
        raise exception using errcode='P0002', message='provider command not found';
    end if;

    select * into existing_row
      from public.cex_provider_reconciliation_evidence_v1
     where command_id=p_command_id
       and incident_attempt_count=command_row.attempt_count;
    if found then
        if existing_row.disposition is distinct from p_disposition
           or existing_row.artifact_uri is distinct from p_artifact_uri
           or existing_row.artifact_sha256 is distinct from p_artifact_sha256
           or existing_row.evidence is distinct from p_evidence
           or existing_row.result_payload is distinct from p_result_payload
           or existing_row.recorded_by is distinct from btrim(p_actor) then
            raise exception using errcode='23505', message='provider reconciliation evidence collision';
        end if;
        return jsonb_build_object(
            'replayed',true,
            'evidence',to_jsonb(existing_row),
            'command',to_jsonb(command_row)
        );
    end if;

    if command_row.status not in ('reconcile_required','dead_letter') then
        raise exception 'provider command is not awaiting reconciliation evidence';
    end if;

    evidence_id_value := public.cex_deterministic_uuid_v1(
        'provider-reconciliation:' || p_command_id::text || ':' || command_row.attempt_count::text
    );
    insert into public.cex_provider_reconciliation_evidence_v1 (
        evidence_id,command_id,incident_attempt_count,disposition,
        artifact_uri,artifact_sha256,evidence,result_payload,recorded_by
    ) values (
        evidence_id_value,p_command_id,command_row.attempt_count,p_disposition,
        p_artifact_uri,p_artifact_sha256,p_evidence,p_result_payload,btrim(p_actor)
    ) returning * into evidence_row;

    update public.cex_provider_dispatch_commands_v1
       set acknowledged_by=null,
           acknowledged_reason=null,
           acknowledged_at=null,
           updated_at=now()
     where command_id=p_command_id
    returning * into command_row;

    if p_disposition='confirmed_executed' then
        result_hash_value := 'sha256:' || encode(digest(p_result_payload::text,'sha256'),'hex');
        update public.cex_provider_dispatch_commands_v1
           set status='succeeded',
               result_payload=p_result_payload,
               result_sha256=result_hash_value,
               last_error_code=null,
               last_error_message=null,
               completed_at=now(),
               updated_at=now()
         where command_id=p_command_id
        returning * into command_row;

        perform set_config('cex.provider_dispatch_v1','enabled',true);
        update public.executions
           set status='Succeeded',
               started_at=coalesce(started_at,now()),
               ended_at=now(),
               result_payload=p_result_payload,
               worker_id=null,
               lease_expires_at=null,
               updated_at=now()
         where execution_id=command_row.execution_id;
        update public.invocations
           set status='Succeeded',
               execution_id=command_row.execution_id,
               failure_reason=null,
               updated_at=now()
         where invocation_id=command_row.invocation_id;
    end if;

    event_id_value := public.cex_deterministic_uuid_v1(
        'provider-reconciliation-recorded:' || evidence_id_value::text
    );
    envelope_value := jsonb_build_object(
        'event_id',event_id_value,
        'trace_id',command_row.trace_id,
        'org_id',command_row.org_id,
        'actor_type','provider-reconciliation-operator',
        'actor_id',btrim(p_actor),
        'event_type','execution.provider_dispatch.reconciled',
        'schema_version','cex.audit.event.v2',
        'occurred_at',evidence_row.recorded_at,
        'payload',jsonb_build_object(
            '_cex_audit_source_service','execution-service',
            'command_id',p_command_id,
            'execution_id',command_row.execution_id,
            'attempt_count',command_row.attempt_count,
            'disposition',p_disposition,
            'artifact_uri',p_artifact_uri,
            'artifact_sha256',p_artifact_sha256,
            'result_sha256',command_row.result_sha256
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service',event_id_value,command_row.trace_id,
        command_row.org_id,envelope_value,10
    );

    return jsonb_build_object(
        'replayed',false,
        'evidence',to_jsonb(evidence_row),
        'command',to_jsonb(command_row)
    );
end
$$;

commit;

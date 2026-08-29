-- AgentBinding V3: immutable, Agent-signed capability/resource disclosure.
--
-- The profile is explicitly self-declared and unverified.  It is identity
-- metadata only: no matcher, scientific, scoring, ranking, reward, settlement,
-- or finality table reads this column.

alter table hepta_agent_bindings
    add column if not exists capability_disclosure_hash text;

alter table hepta_agent_bindings
    drop constraint if exists hepta_agent_bindings_capability_disclosure_hash_check;

alter table hepta_agent_bindings
    add constraint hepta_agent_bindings_capability_disclosure_hash_check check (
        capability_disclosure_hash is null
        or capability_disclosure_hash ~ '^sha256:[0-9a-f]{64}$'
    );

alter table hepta_agent_bindings
    drop constraint if exists hepta_agent_bindings_capability_record_check;

create or replace function hepta_agent_capability_disclosure_valid_v1(
    binding_record jsonb,
    disclosure_hash text
)
returns boolean
language plpgsql
immutable
as $$
declare
    disclosure jsonb;
    item text;
    previous_item text;
    max_parallel_text text;
begin
    if disclosure_hash is null then
        return not (binding_record ? 'capability_disclosure')
           and not (binding_record ? 'capability_disclosure_hash');
    end if;
    if disclosure_hash !~ '^sha256:[0-9a-f]{64}$'
       or binding_record ->> 'capability_disclosure_hash' is distinct from disclosure_hash
       or jsonb_typeof(binding_record -> 'capability_disclosure') is distinct from 'object'
    then
        return false;
    end if;
    disclosure := binding_record -> 'capability_disclosure';
    if (select count(*) from jsonb_object_keys(disclosure)) <> 5 then
        return false;
    end if;
    if disclosure ->> 'schema'
            is distinct from 'hepta.paper_raid.agent_capability_disclosure.v1'
       or disclosure ->> 'assurance' is distinct from 'self_declared_unverified'
       or jsonb_typeof(disclosure -> 'capabilities') is distinct from 'array'
       or jsonb_array_length(disclosure -> 'capabilities') not between 1 and 16
       or jsonb_typeof(disclosure -> 'resource_classes') is distinct from 'array'
       or jsonb_array_length(disclosure -> 'resource_classes') > 16
       or jsonb_typeof(disclosure -> 'max_parallel_tasks') is distinct from 'number'
    then
        return false;
    end if;
    max_parallel_text := disclosure ->> 'max_parallel_tasks';
    if max_parallel_text !~ '^[0-9]{1,2}$'
       or max_parallel_text::integer not between 1 and 32
    then
        return false;
    end if;

    previous_item := null;
    for item in select jsonb_array_elements_text(disclosure -> 'capabilities')
    loop
        if item not in (
            'artifact_analysis',
            'citation_verification',
            'evidence_search',
            'experiment_execution',
            'experiment_planning',
            'reproduction',
            'research_session_signing',
            'section_drafting'
        ) or (
            previous_item is not null
            and (item collate "C") <= (previous_item collate "C")
        )
        then
            return false;
        end if;
        previous_item := item;
    end loop;

    previous_item := null;
    for item in select jsonb_array_elements_text(disclosure -> 'resource_classes')
    loop
        if item not in (
            'artifact_io', 'browser', 'code_execution', 'cpu',
            'gpu', 'network', 'sandbox'
        ) or (
            previous_item is not null
            and (item collate "C") <= (previous_item collate "C")
        )
        then
            return false;
        end if;
        previous_item := item;
    end loop;
    return true;
exception
    when others then
        return false;
end;
$$;

alter table hepta_agent_bindings
    add constraint hepta_agent_bindings_capability_record_check check (
        hepta_agent_capability_disclosure_valid_v1(
            record_json,
            capability_disclosure_hash
        )
    );

create or replace function hepta_reject_agent_capability_disclosure_mutation()
returns trigger
language plpgsql
as $$
begin
    if old.capability_disclosure_hash is distinct from new.capability_disclosure_hash
       or (old.record_json -> 'capability_disclosure')
            is distinct from (new.record_json -> 'capability_disclosure')
       or (old.record_json -> 'capability_disclosure_hash')
            is distinct from (new.record_json -> 'capability_disclosure_hash')
    then
        raise exception using
            errcode = '23514',
            message = 'Agent capability disclosure is immutable after binding creation';
    end if;
    return new;
end;
$$;

drop trigger if exists hepta_agent_capability_disclosure_update_guard
    on hepta_agent_bindings;

create trigger hepta_agent_capability_disclosure_update_guard
before update on hepta_agent_bindings
for each row execute function hepta_reject_agent_capability_disclosure_mutation();

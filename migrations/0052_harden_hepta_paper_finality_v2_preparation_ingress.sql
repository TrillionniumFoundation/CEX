-- Harden the durable Paper finality V2 preparation boundary without rewriting
-- deployed migration 0038.  The historical 0038 constraint catalog is itself
-- replay-verified, so this forward migration uses a separately catalogued
-- ENABLE ALWAYS ingress guard rather than weakening historical replayability.

begin;

create or replace function public.hepta_paper_finality_v2_preparation_ingress_valid_v1(
    candidate public.hepta_paper_chain_finality_preparations_v2
)
returns boolean
language sql
immutable
strict
set search_path = pg_catalog
as $function$
    select
        ($1).request_hash ~ '^sha256:[0-9a-f]{64}$'
        and ($1).request_hash <>
            'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        and ($1).commitment_id ~ '^sha256:[0-9a-f]{64}$'
        and ($1).commitment_id <>
            'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        and ($1).binding_fingerprint ~ '^sha256:[0-9a-f]{64}$'
        and ($1).binding_fingerprint <>
            'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        and ($1).source_fingerprint ~ '^sha256:[0-9a-f]{64}$'
        and ($1).source_fingerprint <>
            'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        and ($1).final_checkpoint_hash ~ '^sha256:[0-9a-f]{64}$'
        and ($1).final_checkpoint_hash <>
            'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        and ($1).match_evidence_commitment_id ~ '^sha256:[0-9a-f]{64}$'
        and ($1).match_evidence_commitment_id <>
            'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        and ($1).final_anchor_hash ~ '^[0-9a-f]{64}$'
        and ($1).final_anchor_hash <>
            '0000000000000000000000000000000000000000000000000000000000000000'
        and ($1).final_header_hash ~ '^[0-9a-f]{64}$'
        and ($1).final_header_hash <>
            '0000000000000000000000000000000000000000000000000000000000000000'
        and length(($1).final_chain_id) between 1 and 64
        and ($1).final_chain_id collate "C" ~ '^[a-z0-9._:-]+$'
        and coalesce((
            jsonb_typeof(($1).record_json) = 'object'
            and jsonb_typeof(($1).record_json -> 'binding') = 'object'
            -- Exact keys and JSON scalar types make the durable jsonb value a
            -- fixed point of serde decode/re-encode.  Without this, missing
            -- Option keys, unknown fields, stringified booleans/numbers, or a
            -- null skip_serializing_if field can pass cast parity, get sealed,
            -- and then be rejected forever by the Rust raw-JSON parity gate.
            and ($1).record_json ?& array[
                'schema',
                'preparation_id',
                'idempotency_key',
                'request_hash',
                'binding',
                'binding_fingerprint',
                'status',
                'created_at'
            ]::text[]
            and (
                select count(*)
                from jsonb_object_keys(($1).record_json) as top_key
            ) = 8
            and not exists (
                select 1
                from jsonb_each(($1).record_json) as top_field(field_name, field_value)
                where top_field.field_name <> 'binding'
                  and jsonb_typeof(top_field.field_value) <> 'string'
            )
            and (($1).record_json -> 'binding') ?& array[
                'schema',
                'commitment_id',
                'source_fingerprint',
                'window_arm_id',
                'paper_project_id',
                'submission_id',
                'research_session_id',
                'research_session_roster_version',
                'match_evidence_commitment_id',
                'match_evidence_object_version',
                'release_candidate_hash',
                'paper_bundle_hash',
                'submission_commitment_hash',
                'author_consent_set_hash',
                'tolerance_policy_hash',
                'evaluation_id',
                'evaluation_signing_hash',
                'evaluation_score_bps',
                'evaluation_accepted',
                'evaluation_completed_at_unix_s',
                'evaluation_supersedes_evaluation_id',
                'evaluation_superseded_by_evaluation_id',
                'latest_reproduction_id',
                'latest_reproduction_report_hash',
                'latest_reproduction_accepted',
                'latest_reproduction_completed_at_unix_s',
                'reproduction_supersedes_reproduction_id',
                'reproduction_superseded_by_reproduction_id',
                'appeal_status',
                'appeal_id',
                'appealed_evaluation_id',
                'appeal_resolution_id',
                'appeal_resolution_hash',
                'start_checkpoint_hash',
                'start_checkpoint_anchor_hash',
                'start_checkpoint_chain_id',
                'start_checkpoint_height',
                'start_checkpoint_header_hash',
                'start_checkpoint_consensus_time_unix_ms',
                'final_checkpoint_hash',
                'final_checkpoint_anchor_hash',
                'final_checkpoint_chain_id',
                'final_checkpoint_height',
                'final_checkpoint_header_hash',
                'final_checkpoint_consensus_time_unix_ms',
                'max_chain_time_lag_ms',
                'appeal_window_closes_at_unix_ms',
                'appeal_window_closes_at_unix_s',
                'settlement_policy_hash',
                'scientific_finality',
                'score_eligible',
                'ranking_eligible',
                'reward_eligible',
                'economic_eligible',
                'finalized_at_unix_s'
            ]::text[]
            and (
                select count(*)
                from jsonb_object_keys(
                    ($1).record_json -> 'binding'
                ) as binding_key
            ) = 55 + case
                when ($1).record_json -> 'binding' ? 'rework_lineage'
                then 1
                else 0
            end
            and not exists (
                select 1
                from jsonb_each(
                    ($1).record_json -> 'binding'
                ) as binding_field(field_name, field_value)
                where binding_field.field_name = any(array[
                    'schema',
                    'commitment_id',
                    'source_fingerprint',
                    'window_arm_id',
                    'paper_project_id',
                    'submission_id',
                    'research_session_id',
                    'match_evidence_commitment_id',
                    'release_candidate_hash',
                    'paper_bundle_hash',
                    'submission_commitment_hash',
                    'author_consent_set_hash',
                    'tolerance_policy_hash',
                    'evaluation_id',
                    'evaluation_signing_hash',
                    'latest_reproduction_id',
                    'latest_reproduction_report_hash',
                    'appeal_status',
                    'start_checkpoint_hash',
                    'start_checkpoint_anchor_hash',
                    'start_checkpoint_chain_id',
                    'start_checkpoint_header_hash',
                    'final_checkpoint_hash',
                    'final_checkpoint_anchor_hash',
                    'final_checkpoint_chain_id',
                    'final_checkpoint_header_hash',
                    'settlement_policy_hash'
                ]::text[])
                  and jsonb_typeof(binding_field.field_value) <> 'string'
            )
            and not exists (
                select 1
                from jsonb_each(
                    ($1).record_json -> 'binding'
                ) as optional_field(field_name, field_value)
                where optional_field.field_name = any(array[
                    'evaluation_supersedes_evaluation_id',
                    'evaluation_superseded_by_evaluation_id',
                    'reproduction_supersedes_reproduction_id',
                    'reproduction_superseded_by_reproduction_id',
                    'appeal_id',
                    'appealed_evaluation_id',
                    'appeal_resolution_id',
                    'appeal_resolution_hash'
                ]::text[])
                  and optional_field.field_value <> 'null'::jsonb
                  and jsonb_typeof(optional_field.field_value) <> 'string'
            )
            and not exists (
                select 1
                from jsonb_each(
                    ($1).record_json -> 'binding'
                ) as uuid_field(field_name, field_value)
                where uuid_field.field_name = any(array[
                    'window_arm_id',
                    'paper_project_id',
                    'submission_id',
                    'evaluation_id',
                    'evaluation_supersedes_evaluation_id',
                    'evaluation_superseded_by_evaluation_id',
                    'latest_reproduction_id',
                    'reproduction_supersedes_reproduction_id',
                    'reproduction_superseded_by_reproduction_id',
                    'appeal_id',
                    'appealed_evaluation_id',
                    'appeal_resolution_id'
                ]::text[])
                  and uuid_field.field_value <> 'null'::jsonb
                  and uuid_field.field_value #>> '{}'
                    = '00000000-0000-0000-0000-000000000000'
            )
            and not exists (
                select 1
                from jsonb_each(
                    ($1).record_json -> 'binding'
                ) as number_field(field_name, field_value)
                where number_field.field_name = any(array[
                    'research_session_roster_version',
                    'match_evidence_object_version',
                    'evaluation_score_bps',
                    'evaluation_completed_at_unix_s',
                    'latest_reproduction_completed_at_unix_s',
                    'start_checkpoint_height',
                    'start_checkpoint_consensus_time_unix_ms',
                    'final_checkpoint_height',
                    'final_checkpoint_consensus_time_unix_ms',
                    'max_chain_time_lag_ms',
                    'appeal_window_closes_at_unix_ms',
                    'appeal_window_closes_at_unix_s',
                    'finalized_at_unix_s'
                ]::text[])
                  and (
                    jsonb_typeof(number_field.field_value) <> 'number'
                    or (number_field.field_value #>> '{}') collate "C"
                        !~ '^(0|[1-9][0-9]*)$'
                    or (number_field.field_value #>> '{}')::numeric
                        > 18446744073709551615::numeric
                    or (
                        number_field.field_name = 'evaluation_score_bps'
                        and (number_field.field_value #>> '{}')::numeric > 65535
                    )
                  )
            )
            and not exists (
                select 1
                from jsonb_each(
                    ($1).record_json -> 'binding'
                ) as boolean_field(field_name, field_value)
                where boolean_field.field_name = any(array[
                    'evaluation_accepted',
                    'latest_reproduction_accepted',
                    'scientific_finality',
                    'score_eligible',
                    'ranking_eligible',
                    'reward_eligible',
                    'economic_eligible'
                ]::text[])
                  and jsonb_typeof(boolean_field.field_value) <> 'boolean'
            )
            and ($1).record_json ->> 'schema'
                = 'hepta.paper_raid.trnm_finality_preparation.v2'
            and ($1).record_json ->> 'preparation_id' = ($1).preparation_id::text
            and ($1).record_json ->> 'idempotency_key' = ($1).idempotency_key
            and ($1).record_json ->> 'request_hash' = ($1).request_hash
            and ($1).record_json ->> 'binding_fingerprint'
                = ($1).binding_fingerprint
            and ($1).record_json ->> 'status'
                = 'awaiting_chain_verifier_upgrade'
            -- Chrono's serde DateTime<Utc> spelling is RFC3339 with Z and
            -- SecondsFormat::AutoSi.  PostgreSQL timestamptz only preserves
            -- microseconds, so a canonical 9-digit AutoSi value (which by
            -- definition has non-zero sub-microseconds) cannot round-trip and
            -- is rejected.  Merely casting timestamptz is not
            -- sufficient: PostgreSQL also accepts aliases such as +00:00,
            -- compact dates, and redundant fractional zero padding.  Those
            -- aliases deserialize to the same Rust value but are not the
            -- immutable JSON bytes whose fingerprint was prepared.
            and (($1).record_json ->> 'created_at') collate "C" ~
                '^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](Z|[.](?!000Z)[0-9]{3}Z|[.][0-9]{3}(?!000Z)[0-9]{3}Z)$'
            and (($1).record_json ->> 'created_at')::timestamptz
                = ($1).created_at
            and extract(epoch from ($1).created_at) * 1000
                = ($1).final_consensus_time_unix_ms::numeric
            -- uuid::Uuid serializes as lowercase hyphenated text.  Cast-only
            -- parity admits uppercase, braced, and compact aliases, which can
            -- permanently seal a row the Rust raw-JSON parity check rejects.
            and (($1).record_json ->> 'preparation_id') collate "C" ~
                '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            and ($1).record_json ->> 'preparation_id'
                <> '00000000-0000-0000-0000-000000000000'
            and ($1).record_json #>> '{binding,schema}'
                = 'hepta.paper_raid.trnm_command_binding.v2'
            and ($1).record_json #>> '{binding,commitment_id}'
                = ($1).commitment_id
            and ($1).record_json #>> '{binding,source_fingerprint}'
                = ($1).source_fingerprint
            and ($1).record_json #>> '{binding,window_arm_id}'
                = ($1).arm_id::text
            and (($1).record_json #>> '{binding,window_arm_id}') collate "C" ~
                '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            and ($1).record_json #>> '{binding,paper_project_id}'
                = ($1).paper_project_id::text
            and (($1).record_json #>> '{binding,paper_project_id}') collate "C" ~
                '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            and ($1).record_json #>> '{binding,submission_id}'
                = ($1).submission_id::text
            and (($1).record_json #>> '{binding,submission_id}') collate "C" ~
                '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            and (
                ($1).record_json #> '{binding,rework_lineage}' is null
                or (
                    jsonb_typeof(
                        ($1).record_json #> '{binding,rework_lineage}'
                    ) = 'object'
                    and (($1).record_json
                            #> '{binding,rework_lineage}') ?& array[
                        'schema',
                        'rework_id',
                        'rework_cycle',
                        'rejected_submission_id',
                        'replacement_submission_id',
                        'rejected_revision_id',
                        'replacement_revision_id',
                        'rejected_release_candidate_hash',
                        'replacement_release_candidate_hash',
                        'rejected_paper_bundle_hash',
                        'replacement_paper_bundle_hash',
                        'rejected_rework_content_commitment_sha256',
                        'replacement_rework_content_commitment_sha256'
                    ]::text[]
                    and (
                        select count(*)
                        from jsonb_object_keys(
                            ($1).record_json
                                #> '{binding,rework_lineage}'
                        ) as rework_key
                    ) = 13
                    and not exists (
                        select 1
                        from jsonb_each(
                            ($1).record_json
                                #> '{binding,rework_lineage}'
                        ) as rework_string(field_name, field_value)
                        where rework_string.field_name = any(array[
                            'schema',
                            'rejected_release_candidate_hash',
                            'replacement_release_candidate_hash',
                            'rejected_paper_bundle_hash',
                            'replacement_paper_bundle_hash',
                            'rejected_rework_content_commitment_sha256',
                            'replacement_rework_content_commitment_sha256'
                        ]::text[])
                          and jsonb_typeof(rework_string.field_value) <> 'string'
                    )
                    and jsonb_typeof(
                        ($1).record_json
                            #> '{binding,rework_lineage,rework_cycle}'
                    ) = 'number'
                    and (($1).record_json
                            #>> '{binding,rework_lineage,rework_cycle}') collate "C"
                        ~ '^(0|[1-9][0-9]*)$'
                    and (($1).record_json
                            #>> '{binding,rework_lineage,rework_cycle}')::numeric
                        <= 18446744073709551615::numeric
                    and (($1).record_json
                            #>> '{binding,rework_lineage,rework_id}') collate "C" ~
                        '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    and (($1).record_json
                            #>> '{binding,rework_lineage,rejected_submission_id}') collate "C" ~
                        '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    and (($1).record_json
                            #>> '{binding,rework_lineage,replacement_submission_id}') collate "C" ~
                        '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    and (($1).record_json
                            #>> '{binding,rework_lineage,rejected_revision_id}') collate "C" ~
                        '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    and (($1).record_json
                            #>> '{binding,rework_lineage,replacement_revision_id}') collate "C" ~
                        '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    and not exists (
                        select 1
                        from jsonb_each(
                            ($1).record_json
                                #> '{binding,rework_lineage}'
                        ) as rework_uuid(field_name, field_value)
                        where rework_uuid.field_name = any(array[
                            'rework_id',
                            'rejected_submission_id',
                            'replacement_submission_id',
                            'rejected_revision_id',
                            'replacement_revision_id'
                        ]::text[])
                          and rework_uuid.field_value #>> '{}'
                            = '00000000-0000-0000-0000-000000000000'
                    )
                )
            )
            and ($1).record_json #>> '{binding,evaluation_id}'
                = ($1).evaluation_id::text
            and (($1).record_json #>> '{binding,evaluation_id}') collate "C" ~
                '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            and (
                ($1).record_json
                    #>> '{binding,evaluation_supersedes_evaluation_id}' is null
                or (($1).record_json
                        #>> '{binding,evaluation_supersedes_evaluation_id}') collate "C" ~
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
            and (
                ($1).record_json
                    #>> '{binding,evaluation_superseded_by_evaluation_id}' is null
                or (($1).record_json
                        #>> '{binding,evaluation_superseded_by_evaluation_id}') collate "C" ~
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
            and ($1).record_json #>> '{binding,latest_reproduction_id}'
                = ($1).latest_reproduction_id::text
            and (($1).record_json
                    #>> '{binding,latest_reproduction_id}') collate "C" ~
                '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            and (
                ($1).record_json
                    #>> '{binding,reproduction_supersedes_reproduction_id}' is null
                or (($1).record_json
                        #>> '{binding,reproduction_supersedes_reproduction_id}') collate "C" ~
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
            and (
                ($1).record_json
                    #>> '{binding,reproduction_superseded_by_reproduction_id}' is null
                or (($1).record_json
                        #>> '{binding,reproduction_superseded_by_reproduction_id}') collate "C" ~
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
            and ($1).record_json #>> '{binding,research_session_id}'
                = ($1).research_session_id
            and ($1).record_json #>> '{binding,research_session_roster_version}'
                = ($1).research_session_roster_version::text
            and ($1).record_json #>> '{binding,match_evidence_commitment_id}'
                = ($1).match_evidence_commitment_id
            and ($1).record_json #>> '{binding,appeal_status}'
                = ($1).appeal_status
            and nullif(
                ($1).record_json #>> '{binding,appeal_id}', ''
            )::uuid is not distinct from ($1).appeal_id
            and (
                ($1).record_json #>> '{binding,appeal_id}' is null
                or (($1).record_json #>> '{binding,appeal_id}') collate "C" ~
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
            and nullif(
                ($1).record_json #>> '{binding,appealed_evaluation_id}', ''
            )::uuid is not distinct from ($1).appealed_evaluation_id
            and (
                ($1).record_json #>> '{binding,appealed_evaluation_id}' is null
                or (($1).record_json
                        #>> '{binding,appealed_evaluation_id}') collate "C" ~
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
            and nullif(
                ($1).record_json #>> '{binding,appeal_resolution_id}', ''
            )::uuid is not distinct from ($1).appeal_resolution_id
            and (
                ($1).record_json #>> '{binding,appeal_resolution_id}' is null
                or (($1).record_json
                        #>> '{binding,appeal_resolution_id}') collate "C" ~
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
            and ($1).record_json #>> '{binding,final_checkpoint_hash}'
                = ($1).final_checkpoint_hash
            and ($1).record_json #>> '{binding,final_checkpoint_anchor_hash}'
                = ($1).final_anchor_hash
            and ($1).record_json #>> '{binding,start_checkpoint_chain_id}'
                = ($1).final_chain_id
            and ($1).record_json #>> '{binding,final_checkpoint_chain_id}'
                = ($1).final_chain_id
            and ($1).record_json #>> '{binding,final_checkpoint_height}'
                = ($1).final_height::text
            and ($1).record_json #>> '{binding,final_checkpoint_header_hash}'
                = ($1).final_header_hash
            and ($1).record_json
                    #>> '{binding,final_checkpoint_consensus_time_unix_ms}'
                = ($1).final_consensus_time_unix_ms::text
            and (($1).record_json #>> '{binding,scientific_finality}')::boolean
                = ($1).scientific_finality
            and (($1).record_json #>> '{binding,score_eligible}')::boolean
                = ($1).score_eligible
            and (($1).record_json #>> '{binding,ranking_eligible}')::boolean
                = ($1).ranking_eligible
            and (($1).record_json #>> '{binding,reward_eligible}')::boolean
                = ($1).reward_eligible
            and (($1).record_json #>> '{binding,economic_eligible}')::boolean
                = ($1).economic_eligible
        ), false)
$function$;

revoke all on function
    public.hepta_paper_finality_v2_preparation_ingress_valid_v1(
        public.hepta_paper_chain_finality_preparations_v2
    )
    from public;

-- Existing immutable rows cannot be guessed or normalized.  A deployment
-- containing a row that the current canonical ingress would reject must stop
-- before the guard is installed and be handled by explicit operator review.
do $block$
begin
    if exists (
        select 1
        from public.hepta_paper_chain_finality_preparations_v2 as preparation
        where not public.hepta_paper_finality_v2_preparation_ingress_valid_v1(
            preparation
        )
    ) then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_finality_v2_preparation_ingress_backfill_forbidden',
            detail = 'Existing immutable preparation violates non-zero digest, canonical Chain ID/timestamp/UUID spelling, or relational/JSON parity requirements';
    end if;
end;
$block$;

create or replace function public.hepta_validate_paper_finality_v2_preparation_ingress_v1()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
begin
    if not public.hepta_paper_finality_v2_preparation_ingress_valid_v1(new) then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_finality_v2_preparation_ingress_invalid',
            detail = 'Preparation digests, raw hashes, Chain ID/timestamp/UUID spelling, and relational/JSON projection must exactly match canonical V2 ingress';
    end if;
    return new;
end;
$function$;

revoke all on function
    public.hepta_validate_paper_finality_v2_preparation_ingress_v1()
    from public;

drop trigger if exists hepta_paper_finality_v2_preparation_ingress_guard
    on public.hepta_paper_chain_finality_preparations_v2;
create trigger hepta_paper_finality_v2_preparation_ingress_guard
before insert on public.hepta_paper_chain_finality_preparations_v2
for each row execute function
    public.hepta_validate_paper_finality_v2_preparation_ingress_v1();
alter table public.hepta_paper_chain_finality_preparations_v2
    enable always trigger hepta_paper_finality_v2_preparation_ingress_guard;

-- Exact resident catalog verification.  The body digests below are filled by
-- the static release proof and pin the complete predicates, including every
-- relational/JSON projection, rather than merely trusting stable names.
do $block$
declare
    historical_constraint_count bigint;
    historical_constraint_sha256 text;
    predicate_count bigint;
    predicate_sha256 text;
    guard_count bigint;
    guard_sha256 text;
    trigger_count bigint;
    global_trigger_name_count bigint;
begin
    select constraint_count, catalog_sha256
    into historical_constraint_count, historical_constraint_sha256
    from public.hepta_paper_finality_v2_constraint_catalog_fingerprint();

    if historical_constraint_count <> 77
       or historical_constraint_sha256 <>
          '910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba'
    then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_finality_v2_historical_constraint_catalog_mismatch',
            detail = format(
                'expected historical 77/%s catalog, got %s/%s',
                '910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba',
                historical_constraint_count,
                coalesce(historical_constraint_sha256, '<null>')
            );
    end if;

    select count(*), min(encode(sha256(convert_to(function_row.prosrc, 'UTF8')), 'hex'))
    into predicate_count, predicate_sha256
    from pg_proc as function_row
    join pg_namespace as namespace on namespace.oid = function_row.pronamespace
    where namespace.nspname = 'public'
      and function_row.proname =
          'hepta_paper_finality_v2_preparation_ingress_valid_v1'
      and function_row.pronargs = 1
      and function_row.proargtypes[0] =
          'public.hepta_paper_chain_finality_preparations_v2'::regtype::oid
      and pg_get_function_result(function_row.oid) = 'boolean'
      and function_row.prolang = (
          select language_row.oid
          from pg_language as language_row
          where language_row.lanname = 'sql'
      )
      and function_row.prokind = 'f'
      and function_row.provolatile = 'i'
      and function_row.proisstrict
      and not function_row.prosecdef
      and not function_row.proleakproof
      and not function_row.proretset
      and function_row.proparallel = 'u'
      and function_row.proconfig = array['search_path=pg_catalog']
      and function_row.proowner = (
          select relation.relowner
          from pg_class as relation
          where relation.oid =
              'public.hepta_paper_chain_finality_preparations_v2'::regclass
      )
      and (
          select count(*)
          from aclexplode(coalesce(
              function_row.proacl,
              acldefault('f', function_row.proowner)
          )) as acl
      ) = 1
      and exists (
          select 1
          from aclexplode(coalesce(
              function_row.proacl,
              acldefault('f', function_row.proowner)
          )) as acl
          where acl.grantor = function_row.proowner
            and acl.grantee = function_row.proowner
            and acl.privilege_type = 'EXECUTE'
            and not acl.is_grantable
      );

    select count(*), min(encode(sha256(convert_to(function_row.prosrc, 'UTF8')), 'hex'))
    into guard_count, guard_sha256
    from pg_proc as function_row
    join pg_namespace as namespace on namespace.oid = function_row.pronamespace
    where namespace.nspname = 'public'
      and function_row.proname =
          'hepta_validate_paper_finality_v2_preparation_ingress_v1'
      and function_row.pronargs = 0
      and pg_get_function_result(function_row.oid) = 'trigger'
      and function_row.prolang = (
          select language_row.oid
          from pg_language as language_row
          where language_row.lanname = 'plpgsql'
      )
      and function_row.prokind = 'f'
      and function_row.provolatile = 'v'
      and not function_row.proisstrict
      and function_row.prosecdef
      and not function_row.proleakproof
      and not function_row.proretset
      and function_row.proparallel = 'u'
      and function_row.proconfig = array['search_path=pg_catalog']
      and function_row.proowner = (
          select relation.relowner
          from pg_class as relation
          where relation.oid =
              'public.hepta_paper_chain_finality_preparations_v2'::regclass
      )
      and (
          select count(*)
          from aclexplode(coalesce(
              function_row.proacl,
              acldefault('f', function_row.proowner)
          )) as acl
      ) = 1
      and exists (
          select 1
          from aclexplode(coalesce(
              function_row.proacl,
              acldefault('f', function_row.proowner)
          )) as acl
          where acl.grantor = function_row.proowner
            and acl.grantee = function_row.proowner
            and acl.privilege_type = 'EXECUTE'
            and not acl.is_grantable
      );

    select count(*)
    into trigger_count
    from pg_trigger as trigger_row
    join pg_class as relation on relation.oid = trigger_row.tgrelid
    join pg_namespace as namespace on namespace.oid = relation.relnamespace
    where namespace.nspname = 'public'
      and relation.relname = 'hepta_paper_chain_finality_preparations_v2'
      and trigger_row.tgname =
          'hepta_paper_finality_v2_preparation_ingress_guard'
      and not trigger_row.tgisinternal
      and trigger_row.tgfoid = to_regprocedure(
          'public.hepta_validate_paper_finality_v2_preparation_ingress_v1()'
      )
      and trigger_row.tgtype = 7
      and trigger_row.tgenabled = 'A'
      and trigger_row.tgnargs = 0
      and trigger_row.tgconstraint = 0
      and not trigger_row.tgdeferrable
      and not trigger_row.tginitdeferred
      and trigger_row.tgqual is null
      and trigger_row.tgoldtable is null
      and trigger_row.tgnewtable is null;

    select count(*)
    into global_trigger_name_count
    from pg_trigger as trigger_row
    where trigger_row.tgname =
          'hepta_paper_finality_v2_preparation_ingress_guard'
      and not trigger_row.tgisinternal;

    if predicate_count <> 1
       or predicate_sha256 <> 'c861ea0fea786979507fc23dd0435002c838a4bdbaa1f23e9c4e0420953570e8'
       or guard_count <> 1
       or guard_sha256 <> 'f9db620e91b35d1ae38b2c3abef60a1e44d0c8522b6c30195c0de9bac2835b87'
       or trigger_count <> 1
       or global_trigger_name_count <> 1
    then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_finality_v2_preparation_ingress_catalog_mismatch',
            detail = format(
                'expected predicate 1/%s, guard 1/%s, trigger 1/1; got %s/%s, %s/%s, %s/%s',
                'c861ea0fea786979507fc23dd0435002c838a4bdbaa1f23e9c4e0420953570e8',
                'f9db620e91b35d1ae38b2c3abef60a1e44d0c8522b6c30195c0de9bac2835b87',
                predicate_count,
                coalesce(predicate_sha256, '<null>'),
                guard_count,
                coalesce(guard_sha256, '<null>'),
                trigger_count,
                global_trigger_name_count
            );
    end if;

    if not exists (
        select 1
        from pg_constraint as constraint_row
        where constraint_row.conrelid =
            'public.hepta_paper_chain_finality_preparations_v2'::regclass
          and constraint_row.conname =
              'hepta_paper_finality_v2_record_json_check'
          and constraint_row.contype = 'c'
          and constraint_row.convalidated
          and not constraint_row.condeferrable
          and not constraint_row.condeferred
    ) then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_finality_v2_preparation_record_json_catalog_mismatch',
            detail = 'Historical full relational/JSON parity CHECK must remain present and validated';
    end if;
end;
$block$;

commit;

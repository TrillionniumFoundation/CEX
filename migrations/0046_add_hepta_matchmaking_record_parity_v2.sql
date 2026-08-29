begin;

-- Matcher V2 freezes the exact ordered player preferences and the solver's
-- player-to-role assignment.  Keep those facts in relational columns as well
-- as record_json so every live/cap/match/materialization query has one
-- transactionally enforced truth.
alter table hepta_matchmaking_tickets
    add column if not exists party_code_hash text,
    add column if not exists expires_at timestamptz;

alter table hepta_team_proposals
    add column if not exists solver_version text,
    add column if not exists source_preferences jsonb,
    add column if not exists role_assignments jsonb;

update hepta_matchmaking_tickets
set party_code_hash = record_json->>'party_code_hash'
where party_code_hash is null
  and record_json ? 'party_code_hash';

update hepta_matchmaking_tickets
set expires_at = coalesce(
        case
            when record_json ? 'expires_at'
            then (record_json->>'expires_at')::timestamptz
            else null
        end,
        created_at + interval '30 minutes'
    )
where expires_at is null;

update hepta_matchmaking_tickets
set record_json = jsonb_set(record_json, '{expires_at}', to_jsonb(expires_at), true)
where not (record_json ? 'expires_at');

alter table hepta_matchmaking_tickets
    alter column expires_at set not null;

update hepta_team_proposals
set solver_version = nullif(record_json->>'solver_version', ''),
    source_preferences = case
        when jsonb_typeof(record_json->'source_preferences') = 'array'
        then record_json->'source_preferences'
        else null
    end,
    role_assignments = case
        when jsonb_typeof(record_json->'role_assignments') = 'array'
        then record_json->'role_assignments'
        else null
    end;

-- Pre-V2 active proposals do not contain enough immutable source identity to
-- be accepted or materialized safely.  Expire them and release each exact
-- source ticket.  Eligible, unexpired players re-enter the queue; every other
-- ticket fails closed to expired.  Terminal historical proposals remain
-- readable and are never retroactively presented as V2.
with legacy_active as (
    select proposal_id
    from hepta_team_proposals
    where status in ('proposed', 'accepted')
      and (
            solver_version is not null
            and solver_version is not distinct from 'hepta.paper_raid.alpha_matcher.v2'
            and source_preferences is not null
            and jsonb_typeof(source_preferences) is not distinct from 'array'
            and jsonb_array_length(source_preferences)
                is not distinct from requested_team_size
            and role_assignments is not null
            and jsonb_typeof(role_assignments) is not distinct from 'array'
            and jsonb_array_length(role_assignments)
                is not distinct from requested_team_size
          ) is not true
), released as (
    select ticket.ticket_id,
           case
               when player.status = 'active'
                and (
                    select count(*)
                    from hepta_agent_bindings binding
                    where binding.player_id = ticket.player_id
                      and binding.status = 'active'
                ) = 1
                and ticket.expires_at > clock_timestamp()
               then 'queued'
               else 'expired'
           end as next_status,
           ticket.version + 1 as next_version,
           clock_timestamp() as changed_at
    from hepta_matchmaking_tickets ticket
    join legacy_active legacy
      on legacy.proposal_id = ticket.matched_proposal_id
    left join hepta_human_players player
      on player.player_id = ticket.player_id
    where ticket.status = 'matched'
)
update hepta_matchmaking_tickets ticket
set status = released.next_status,
    matched_proposal_id = null,
    version = released.next_version,
    updated_at = released.changed_at,
    record_json = jsonb_set(
        jsonb_set(
            jsonb_set(
                jsonb_set(
                    ticket.record_json,
                    '{status}',
                    to_jsonb(released.next_status),
                    true
                ),
                '{matched_proposal_id}',
                'null'::jsonb,
                true
            ),
            '{version}',
            to_jsonb(released.next_version),
            true
        ),
        '{updated_at}',
        to_jsonb(released.changed_at),
        true
    )
from released
where ticket.ticket_id = released.ticket_id;

with legacy_active as (
    select proposal_id, version + 1 as next_version, clock_timestamp() as changed_at
    from hepta_team_proposals
    where status in ('proposed', 'accepted')
      and (
            solver_version is not null
            and solver_version is not distinct from 'hepta.paper_raid.alpha_matcher.v2'
            and source_preferences is not null
            and jsonb_typeof(source_preferences) is not distinct from 'array'
            and jsonb_array_length(source_preferences)
                is not distinct from requested_team_size
            and role_assignments is not null
            and jsonb_typeof(role_assignments) is not distinct from 'array'
            and jsonb_array_length(role_assignments)
                is not distinct from requested_team_size
          ) is not true
)
update hepta_team_proposals proposal
set status = 'expired',
    version = legacy.next_version,
    expires_at = least(proposal.expires_at, legacy.changed_at),
    updated_at = legacy.changed_at,
    record_json = jsonb_set(
        jsonb_set(
            jsonb_set(
                jsonb_set(
                    proposal.record_json,
                    '{status}',
                    '"expired"'::jsonb,
                    true
                ),
                '{version}',
                to_jsonb(legacy.next_version),
                true
            ),
            '{expires_at}',
            to_jsonb(least(proposal.expires_at, legacy.changed_at)),
            true
        ),
        '{updated_at}',
        to_jsonb(legacy.changed_at),
        true
    )
from legacy_active legacy
where proposal.proposal_id = legacy.proposal_id;

alter table hepta_matchmaking_tickets
    drop constraint if exists hepta_matchmaking_tickets_party_code_hash_check;
alter table hepta_matchmaking_tickets
    add constraint hepta_matchmaking_tickets_party_code_hash_check check (
        party_code_hash is null
        or party_code_hash ~ '^sha256:[0-9a-f]{64}$'
    );

alter table hepta_matchmaking_tickets
    drop constraint if exists hepta_matchmaking_tickets_record_json_parity_v2_check;
alter table hepta_matchmaking_tickets
    add constraint hepta_matchmaking_tickets_record_json_parity_v2_check
    check (
        (record_json->>'ticket_id') is not distinct from ticket_id::text
        and (record_json->>'player_id') is not distinct from player_id::text
        and (record_json->>'challenge_id') is not distinct from challenge_id::text
        and (record_json->>'requested_team_size') is not distinct from requested_team_size::text
        and (record_json->'roles') is not distinct from to_jsonb(roles)
        and (record_json->>'availability_hash') is not distinct from availability_hash
        and (record_json->>'party_code_hash') is not distinct from party_code_hash
        and (record_json->>'status') is not distinct from status
        and (record_json->>'matched_proposal_id') is not distinct from matched_proposal_id::text
        and (record_json->>'version') is not distinct from version::text
        and (record_json->>'created_at')::timestamptz is not distinct from created_at
        and (record_json->>'updated_at')::timestamptz is not distinct from updated_at
        and (record_json->>'expires_at')::timestamptz is not distinct from expires_at
    ) not valid;
alter table hepta_matchmaking_tickets
    validate constraint hepta_matchmaking_tickets_record_json_parity_v2_check;

alter table hepta_team_proposals
    drop constraint if exists hepta_team_proposals_record_json_parity_v2_check;
alter table hepta_team_proposals
    add constraint hepta_team_proposals_record_json_parity_v2_check
    check (
        (record_json->>'proposal_id') is not distinct from proposal_id::text
        and (record_json->>'challenge_id') is not distinct from challenge_id::text
        and (record_json->>'requested_team_size') is not distinct from requested_team_size::text
        and (record_json->>'deterministic_match_key') is not distinct from deterministic_match_key
        and (record_json->'member_player_ids') is not distinct from to_jsonb(member_player_ids)
        and (record_json->'source_ticket_ids') is not distinct from to_jsonb(source_ticket_ids)
        and (record_json->>'solver_version') is not distinct from solver_version
        and (record_json->'source_preferences') is not distinct from source_preferences
        and (record_json->'role_assignments') is not distinct from role_assignments
        and (record_json->>'status') is not distinct from status
        and (record_json->>'version') is not distinct from version::text
        and (record_json->>'created_at')::timestamptz is not distinct from created_at
        and (record_json->>'updated_at')::timestamptz is not distinct from updated_at
        and (record_json->>'expires_at')::timestamptz is not distinct from expires_at
    ) not valid;
alter table hepta_team_proposals
    validate constraint hepta_team_proposals_record_json_parity_v2_check;

alter table hepta_team_proposals
    drop constraint if exists hepta_team_proposals_active_matcher_v2_check;
alter table hepta_team_proposals
    add constraint hepta_team_proposals_active_matcher_v2_check check (
        status not in ('proposed', 'accepted')
        or (
            solver_version is not null
            and solver_version is not distinct from 'hepta.paper_raid.alpha_matcher.v2'
            and source_preferences is not null
            and jsonb_typeof(source_preferences) is not distinct from 'array'
            and jsonb_array_length(source_preferences)
                is not distinct from requested_team_size
            and role_assignments is not null
            and jsonb_typeof(role_assignments) is not distinct from 'array'
            and jsonb_array_length(role_assignments)
                is not distinct from requested_team_size
        ) is true
    ) not valid;
alter table hepta_team_proposals
    validate constraint hepta_team_proposals_active_matcher_v2_check;

drop index if exists hepta_matchmaking_live_party_idx;
create index hepta_matchmaking_live_party_idx
    on hepta_matchmaking_tickets (challenge_id, party_code_hash, created_at, ticket_id)
    where status in ('queued', 'matched') and party_code_hash is not null;

commit;

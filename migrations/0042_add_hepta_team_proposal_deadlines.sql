-- Bound team-proposal acceptance so one absent player cannot hold the other
-- matched tickets forever.  The JSON projection remains authoritative for API
-- responses; this column provides an indexed, transaction-safe sweep key.

alter table hepta_team_proposals
    add column if not exists expires_at timestamptz;

update hepta_team_proposals
set expires_at = created_at + interval '5 minutes'
where expires_at is null;

do $$
begin
    if exists (
        select 1
        from hepta_team_proposals
        where record_json ? 'expires_at'
          and (record_json->>'expires_at')::timestamptz is distinct from expires_at
    ) then
        raise exception 'team proposal deadline column/JSON projection diverged; operator repair required';
    end if;
end
$$;

update hepta_team_proposals
set record_json = jsonb_set(record_json, '{expires_at}', to_jsonb(expires_at), true);

alter table hepta_team_proposals
    alter column expires_at set not null;

drop index if exists hepta_team_proposals_open_deadline_idx;
create index hepta_team_proposals_open_deadline_idx
    on hepta_team_proposals (challenge_id, expires_at, proposal_id)
    where status in ('proposed', 'accepted');

alter table hepta_team_proposals
    drop constraint if exists hepta_team_proposals_status_check;
alter table hepta_team_proposals
    add constraint hepta_team_proposals_status_check check (
        status in ('proposed', 'accepted', 'materialized', 'declined', 'expired')
    );

-- Every open proposal must already own one exact, ordered matched-ticket set.
-- Surface historical corruption during rollout instead of deploying a latent
-- deadline-sweeper failure that would brick the whole challenge queue.
do $$
begin
    if exists (
        select 1
        from hepta_team_proposals p
        where p.status in ('proposed', 'accepted')
          and (
                cardinality(p.source_ticket_ids) <> p.requested_team_size
                or cardinality(p.member_player_ids) <> p.requested_team_size
                or p.record_json->>'proposal_id' is distinct from p.proposal_id::text
                or p.record_json->>'challenge_id' is distinct from p.challenge_id::text
                or p.record_json->>'requested_team_size' is distinct from p.requested_team_size::text
                or p.record_json->>'deterministic_match_key' is distinct from p.deterministic_match_key
                or p.record_json->>'status' is distinct from p.status
                or p.record_json->>'version' is distinct from p.version::text
                or p.record_json->'source_ticket_ids' is distinct from to_jsonb(p.source_ticket_ids)
                or p.record_json->'member_player_ids' is distinct from to_jsonb(p.member_player_ids)
                or (p.record_json->>'expires_at')::timestamptz is distinct from p.expires_at
                or (p.record_json->>'created_at')::timestamptz is distinct from p.created_at
                or (p.record_json->>'updated_at')::timestamptz is distinct from p.updated_at
              )
    ) then
        raise exception 'open team proposal topology diverged; operator repair required';
    end if;

    if exists (
        select 1
        from hepta_team_proposals p
        cross join lateral generate_subscripts(p.source_ticket_ids, 1) source(slot)
        left join hepta_matchmaking_tickets ticket
          on ticket.ticket_id = p.source_ticket_ids[source.slot]
        where p.status in ('proposed', 'accepted')
          and (
                ticket.ticket_id is null
                or ticket.status <> 'matched'
                or ticket.matched_proposal_id is distinct from p.proposal_id
                or ticket.player_id is distinct from p.member_player_ids[source.slot]
                or ticket.challenge_id is distinct from p.challenge_id
                or ticket.requested_team_size is distinct from p.requested_team_size
                or ticket.record_json->>'ticket_id' is distinct from ticket.ticket_id::text
                or ticket.record_json->>'player_id' is distinct from ticket.player_id::text
                or ticket.record_json->>'challenge_id' is distinct from ticket.challenge_id::text
                or ticket.record_json->>'requested_team_size'
                     is distinct from ticket.requested_team_size::text
                or ticket.record_json->'roles' is distinct from to_jsonb(ticket.roles)
                or ticket.record_json->>'availability_hash' is distinct from ticket.availability_hash
                or ticket.record_json->>'status' is distinct from ticket.status
                or ticket.record_json->>'version' is distinct from ticket.version::text
                or ticket.record_json->>'matched_proposal_id'
                     is distinct from ticket.matched_proposal_id::text
                or (ticket.record_json->>'created_at')::timestamptz
                     is distinct from ticket.created_at
                or (ticket.record_json->>'updated_at')::timestamptz
                     is distinct from ticket.updated_at
              )
    ) then
        raise exception 'open team proposal/source-ticket authority diverged; operator repair required';
    end if;

    if exists (
        select 1
        from hepta_team_proposals p
        cross join lateral unnest(p.member_player_ids) member(player_id)
        left join hepta_human_players player on player.player_id = member.player_id
        where p.status in ('proposed', 'accepted')
          and not exists (
                select 1
                from hepta_research_teams team
                join hepta_outbox event
                  on event.event_type = 'hepta.paper_raid.team.materialized.v1'
                 and event.aggregate_id = team.team_id::text
                 and event.aggregate_version = 1
                 and event.schema_version = 'hepta.paper_raid.event.v2'
                 and event.producer = 'hepta-research-league'
                 and event.payload->>'proposal_id' = p.proposal_id::text
                 and event.payload->>'team_id' = team.team_id::text
                 and event.payload->>'challenge_id' = p.challenge_id::text
                 and (event.payload->>'member_count')::integer = p.requested_team_size
                where team.team_id = p.proposal_id
              )
          and (
                player.player_id is null
                or player.status <> 'active'
                or (
                    select count(*)
                    from hepta_agent_bindings binding
                    where binding.player_id = member.player_id
                      and binding.status = 'active'
                ) <> 1
              )
    ) then
        raise exception 'open team proposal contains a matchmaking-ineligible player';
    end if;

    if exists (
        select 1
        from hepta_team_proposals p
        join hepta_team_proposal_decisions decision
          on decision.proposal_id = p.proposal_id
        where p.status in ('proposed', 'accepted', 'materialized')
          and (
                not (decision.player_id = any(p.member_player_ids))
                or decision.decision <> 'accept'
                or decision.proposal_version < 2
                or decision.proposal_version > p.version
                or decision.record_json->>'decision_id'
                     is distinct from decision.decision_id::text
                or decision.record_json->>'proposal_id'
                     is distinct from decision.proposal_id::text
                or decision.record_json->>'player_id'
                     is distinct from decision.player_id::text
                or decision.record_json->>'decision'
                     is distinct from decision.decision
                or decision.record_json->>'proposal_version'
                     is distinct from decision.proposal_version::text
                or (decision.record_json->>'created_at')::timestamptz
                     is distinct from decision.created_at
              )
    ) then
        raise exception 'open team proposal decision authority diverged; operator repair required';
    end if;

    if exists (
        select 1
        from hepta_team_proposals p
        where p.status in ('proposed', 'accepted', 'materialized')
          and (
                (
                    p.status = 'proposed'
                    and (
                        select count(*)
                        from hepta_team_proposal_decisions decision
                        where decision.proposal_id = p.proposal_id
                          and decision.decision = 'accept'
                    ) >= p.requested_team_size
                )
                or (
                    p.status in ('accepted', 'materialized')
                    and (
                        select count(*)
                        from hepta_team_proposal_decisions decision
                        where decision.proposal_id = p.proposal_id
                          and decision.decision = 'accept'
                    ) <> p.requested_team_size
                )
              )
    ) then
        raise exception 'team proposal status/acceptance quorum diverged; operator repair required';
    end if;

    if exists (
        select source.ticket_id
        from hepta_team_proposals p
        cross join lateral unnest(p.source_ticket_ids) source(ticket_id)
        where p.status in ('proposed', 'accepted')
        group by source.ticket_id
        having count(*) > 1
    ) then
        raise exception 'one matched ticket cannot belong to multiple open team proposals';
    end if;
end
$$;

-- Direct-created teams are valid and have no proposal. A same-ID team and
-- proposal is historical matcher materialization only when the dedicated
-- outbox event proves that formation path. Refuse to infer provenance from a
-- UUID collision or from table presence alone.
do $$
begin
    if exists (
        select 1
        from hepta_research_teams t
        join hepta_team_proposals p on p.proposal_id = t.team_id
        where (
                select count(*)
                from hepta_outbox event
                where event.event_type = 'hepta.paper_raid.team.materialized.v1'
                  and event.aggregate_id = t.team_id::text
                  and event.aggregate_version = 1
                  and event.schema_version = 'hepta.paper_raid.event.v2'
                  and event.producer = 'hepta-research-league'
                  and event.payload->>'proposal_id' = p.proposal_id::text
                  and event.payload->>'team_id' = t.team_id::text
                  and event.payload->>'challenge_id' = p.challenge_id::text
                  and (event.payload->>'member_count')::integer = p.requested_team_size
              ) <> 1
           or p.status not in ('proposed', 'accepted', 'materialized')
           or p.record_json->>'proposal_id' is distinct from p.proposal_id::text
           or p.record_json->>'challenge_id' is distinct from p.challenge_id::text
           or p.record_json->>'requested_team_size' is distinct from p.requested_team_size::text
           or p.record_json->>'deterministic_match_key' is distinct from p.deterministic_match_key
           or p.record_json->>'status' is distinct from p.status
           or p.record_json->>'version' is distinct from p.version::text
           or p.record_json->'source_ticket_ids' is distinct from to_jsonb(p.source_ticket_ids)
           or p.record_json->'member_player_ids' is distinct from to_jsonb(p.member_player_ids)
           or (p.record_json->>'expires_at')::timestamptz is distinct from p.expires_at
           or (p.record_json->>'created_at')::timestamptz is distinct from p.created_at
           or (p.record_json->>'updated_at')::timestamptz is distinct from p.updated_at
           or cardinality(p.source_ticket_ids) <> p.requested_team_size
           or cardinality(p.member_player_ids) <> p.requested_team_size
           or (
                select count(distinct source.ticket_id)
                from unnest(p.source_ticket_ids) source(ticket_id)
              ) <> p.requested_team_size
           or (
                select count(distinct member.player_id)
                from unnest(p.member_player_ids) member(player_id)
              ) <> p.requested_team_size
           or t.challenge_id is distinct from p.challenge_id
           or t.team_id is distinct from p.proposal_id
           or t.record_json->>'team_id' is distinct from t.team_id::text
           or t.record_json->>'challenge_id' is distinct from t.challenge_id::text
           or t.record_json->>'collaboration_compact_hash'
                is distinct from t.collaboration_compact_hash
           or t.record_json->>'status' is distinct from t.status
           or t.record_json->>'version' is distinct from t.version::text
           or t.record_json->>'roster_version' is distinct from t.roster_version::text
           or jsonb_array_length(t.record_json->'members')
                is distinct from p.requested_team_size
           or (
                select count(*)
                from hepta_research_team_members roster
                where roster.team_id = t.team_id
              ) <> p.requested_team_size
    ) then
        raise exception 'materialized team/proposal authority diverged; operator repair required';
    end if;

    if exists (
        select 1
        from hepta_research_teams t
        join hepta_team_proposals p on p.proposal_id = t.team_id
        cross join lateral generate_subscripts(p.member_player_ids, 1) member(slot)
        left join hepta_research_team_members roster
          on roster.team_id = t.team_id and roster.participant_slot = member.slot
        left join hepta_agent_bindings binding on binding.binding_id = roster.binding_id
        where roster.player_id is distinct from p.member_player_ids[member.slot]
           or binding.player_id is distinct from roster.player_id
           or t.record_json->'members'->(member.slot - 1)->>'participant_slot'
                is distinct from member.slot::text
           or t.record_json->'members'->(member.slot - 1)->>'player_id'
                is distinct from p.member_player_ids[member.slot]::text
           or t.record_json->'members'->(member.slot - 1)->>'binding_id'
                is distinct from roster.binding_id::text
           or t.record_json->'members'->(member.slot - 1)->>'agent_id'
                is distinct from binding.agent_id
           or t.record_json->'members'->(member.slot - 1)->>'role'
                is distinct from roster.role
           or (t.record_json->'members'->(member.slot - 1)->>'joined_at')::timestamptz
                is distinct from roster.joined_at
    ) then
        raise exception 'materialized team/roster authority diverged; operator repair required';
    end if;

    if exists (
        select 1
        from hepta_research_teams t
        join hepta_team_proposals p on p.proposal_id = t.team_id
        cross join lateral generate_subscripts(p.source_ticket_ids, 1) source(slot)
        left join hepta_matchmaking_tickets ticket
          on ticket.ticket_id = p.source_ticket_ids[source.slot]
        where ticket.ticket_id is null
           or ticket.status not in ('matched', 'consumed')
           or ticket.matched_proposal_id is distinct from p.proposal_id
           or ticket.player_id is distinct from p.member_player_ids[source.slot]
           or ticket.challenge_id is distinct from p.challenge_id
           or ticket.requested_team_size is distinct from p.requested_team_size
           or ticket.record_json->>'ticket_id' is distinct from ticket.ticket_id::text
           or ticket.record_json->>'status' is distinct from ticket.status
           or ticket.record_json->>'version' is distinct from ticket.version::text
           or ticket.record_json->>'matched_proposal_id'
                is distinct from ticket.matched_proposal_id::text
           or ticket.record_json->>'player_id' is distinct from ticket.player_id::text
           or ticket.record_json->>'challenge_id' is distinct from ticket.challenge_id::text
           or ticket.record_json->>'requested_team_size'
                is distinct from ticket.requested_team_size::text
           or ticket.record_json->'roles' is distinct from to_jsonb(ticket.roles)
           or ticket.record_json->>'availability_hash' is distinct from ticket.availability_hash
           or (ticket.record_json->>'created_at')::timestamptz
                is distinct from ticket.created_at
           or (ticket.record_json->>'updated_at')::timestamptz
                is distinct from ticket.updated_at
    ) then
        raise exception 'materialized team/source-ticket authority diverged; operator repair required';
    end if;

    if exists (
        select source.ticket_id
        from hepta_research_teams t
        join hepta_team_proposals p on p.proposal_id = t.team_id
        cross join lateral unnest(p.source_ticket_ids) source(ticket_id)
        group by source.ticket_id
        having count(*) > 1
    ) then
        raise exception 'one matchmaking ticket cannot materialize multiple research teams';
    end if;

    if exists (
        select 1
        from hepta_team_proposals p
        left join hepta_research_teams t on t.team_id = p.proposal_id
        where p.status = 'materialized'
          and (
                t.team_id is null
                or not exists (
                    select 1
                    from hepta_outbox event
                    where event.event_type = 'hepta.paper_raid.team.materialized.v1'
                      and event.aggregate_id = p.proposal_id::text
                      and event.aggregate_version = 1
                      and event.schema_version = 'hepta.paper_raid.event.v2'
                      and event.producer = 'hepta-research-league'
                      and event.payload->>'proposal_id' = p.proposal_id::text
                      and event.payload->>'team_id' = p.proposal_id::text
                      and event.payload->>'challenge_id' = p.challenge_id::text
                      and (event.payload->>'member_count')::integer = p.requested_team_size
                )
              )
    ) then
        raise exception 'materialized proposal is missing its proven research team';
    end if;
end
$$;

with changed as (
    select p.proposal_id, p.version + 1 as next_version, clock_timestamp() as changed_at
    from hepta_team_proposals p
    join hepta_research_teams t on t.team_id = p.proposal_id
    where p.status in ('proposed', 'accepted')
      and exists (
            select 1
            from hepta_outbox event
            where event.event_type = 'hepta.paper_raid.team.materialized.v1'
              and event.aggregate_id = p.proposal_id::text
              and event.aggregate_version = 1
              and event.schema_version = 'hepta.paper_raid.event.v2'
              and event.producer = 'hepta-research-league'
              and event.payload->>'proposal_id' = p.proposal_id::text
              and event.payload->>'team_id' = t.team_id::text
              and event.payload->>'challenge_id' = p.challenge_id::text
              and (event.payload->>'member_count')::integer = p.requested_team_size
          )
)
update hepta_team_proposals p
set status = 'materialized',
    version = changed.next_version,
    updated_at = changed.changed_at,
    record_json = jsonb_set(
        jsonb_set(
            jsonb_set(p.record_json, '{status}', '"materialized"'::jsonb, true),
            '{version}', to_jsonb(changed.next_version), true
        ),
        '{updated_at}', to_jsonb(changed.changed_at), true
    )
from changed
where p.proposal_id = changed.proposal_id;

-- Materialization consumes the exact source tickets. Without this terminal
-- state a materialized raid would retain a live `matched` ticket forever and
-- permanently prevent the same player from queuing the challenge again.
alter table hepta_matchmaking_tickets
    drop constraint if exists hepta_matchmaking_tickets_status_check;
alter table hepta_matchmaking_tickets
    add constraint hepta_matchmaking_tickets_status_check check (
        status in ('queued', 'matched', 'consumed', 'cancelled', 'expired')
    );

with materialized_tickets as (
    select distinct unnest(p.source_ticket_ids) as ticket_id
    from hepta_team_proposals p
    join hepta_research_teams t on t.team_id = p.proposal_id
    where exists (
        select 1
        from hepta_outbox event
        where event.event_type = 'hepta.paper_raid.team.materialized.v1'
          and event.aggregate_id = p.proposal_id::text
          and event.aggregate_version = 1
          and event.schema_version = 'hepta.paper_raid.event.v2'
          and event.producer = 'hepta-research-league'
          and event.payload->>'proposal_id' = p.proposal_id::text
          and event.payload->>'team_id' = t.team_id::text
          and event.payload->>'challenge_id' = p.challenge_id::text
          and (event.payload->>'member_count')::integer = p.requested_team_size
    )
), changed as (
    select t.ticket_id, t.version + 1 as next_version, clock_timestamp() as changed_at
    from hepta_matchmaking_tickets t
    join materialized_tickets m on m.ticket_id = t.ticket_id
    where t.status = 'matched'
)
update hepta_matchmaking_tickets t
set status = 'consumed',
    version = changed.next_version,
    updated_at = changed.changed_at,
    record_json = jsonb_set(
        jsonb_set(
            jsonb_set(t.record_json, '{status}', '"consumed"'::jsonb, true),
            '{version}', to_jsonb(changed.next_version), true
        ),
        '{updated_at}', to_jsonb(changed.changed_at), true
    )
from changed
where t.ticket_id = changed.ticket_id;

do $$
begin
    if exists (
        select 1
        from hepta_research_teams t
        join hepta_team_proposals p on p.proposal_id = t.team_id
        cross join lateral unnest(p.source_ticket_ids) source(ticket_id)
        left join hepta_matchmaking_tickets ticket on ticket.ticket_id = source.ticket_id
        where exists (
                select 1
                from hepta_outbox event
                where event.event_type = 'hepta.paper_raid.team.materialized.v1'
                  and event.aggregate_id = p.proposal_id::text
                  and event.aggregate_version = 1
                  and event.schema_version = 'hepta.paper_raid.event.v2'
                  and event.producer = 'hepta-research-league'
                  and event.payload->>'proposal_id' = p.proposal_id::text
                  and event.payload->>'team_id' = t.team_id::text
                  and event.payload->>'challenge_id' = p.challenge_id::text
                  and (event.payload->>'member_count')::integer = p.requested_team_size
              )
          and (p.status <> 'materialized'
           or p.record_json->>'status' <> 'materialized'
           or ticket.status <> 'consumed'
           or ticket.record_json->>'status' <> 'consumed'
           or ticket.matched_proposal_id is distinct from p.proposal_id)
    ) then
        raise exception 'materialized proposal/ticket terminal backfill did not converge';
    end if;
end
$$;

-- A matched ticket is live only until proposal withdrawal/expiry or team
-- materialization. Prevent a second queued ticket while a decision is open.
do $$
begin
    if exists (
        select 1
        from hepta_matchmaking_tickets
        where status in ('queued', 'matched')
        group by player_id, challenge_id
        having count(*) > 1
    ) then
        raise exception 'duplicate queued/matched matchmaking authority requires operator review';
    end if;
end
$$;

drop index if exists hepta_matchmaking_one_live_ticket_idx;
create unique index if not exists hepta_matchmaking_one_live_ticket_idx
    on hepta_matchmaking_tickets (player_id, challenge_id)
    where status in ('queued', 'matched');

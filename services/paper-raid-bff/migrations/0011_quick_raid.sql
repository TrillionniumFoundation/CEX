-- Fixed-seed Quick Raid: a bounded, paper-scoped projection used for the
-- 15-minute first session.  It is deliberately separate from
-- practice_unranked: completion is authoritative for this projection, while
-- every portable/scientific/economic eligibility bit remains false.

CREATE TABLE IF NOT EXISTS paper_raid_bff_quick_raid_sessions (
    session_id UUID PRIMARY KEY,
    subject_id TEXT NOT NULL,
    player_id UUID NOT NULL,
    binding_id UUID NOT NULL,
    mode TEXT NOT NULL DEFAULT 'quick_raid'
        CONSTRAINT paper_raid_bff_quick_raid_mode_ck CHECK (mode = 'quick_raid'),
    scenario_id TEXT NOT NULL DEFAULT 'evidence-audit-quick-v1'
        CONSTRAINT paper_raid_bff_quick_raid_scenario_ck CHECK (scenario_id = 'evidence-audit-quick-v1'),
    challenge_key TEXT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_challenge_key_ck CHECK (challenge_key = 'evidence-audit-quick'),
    challenge_id UUID NOT NULL,
    challenge_snapshot_hash TEXT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_snapshot_hash_ck CHECK (challenge_snapshot_hash ~ '^sha256:[0-9a-f]{64}$'),
    pack_id TEXT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_pack_ck CHECK (pack_id = 'paper-raid-evidence-audit-quick-seeded-v1'),
    ruleset_version TEXT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_ruleset_version_ck CHECK (ruleset_version = 'paper-raid-evidence-audit-quick-v1'),
    ruleset_hash TEXT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_ruleset_hash_ck CHECK (ruleset_hash ~ '^sha256:[0-9a-f]{64}$'),
    authority_hash TEXT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_authority_hash_ck CHECK (authority_hash ~ '^sha256:[0-9a-f]{64}$'),
    seed BIGINT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_seed_ck CHECK (seed = 17),
    duration_seconds INTEGER NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_duration_ck CHECK (duration_seconds = 900),
    authority_kind TEXT NOT NULL DEFAULT 'hepta_challenge_pack_projection_v1'
        CONSTRAINT paper_raid_bff_quick_raid_authority_kind_ck CHECK (authority_kind = 'hepta_challenge_pack_projection_v1'),
    stage TEXT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_stage_ck CHECK (stage IN ('evidence_review','experiment_run','paper_bundle_ready','completed','abandoned','expired')),
    version BIGINT NOT NULL
        CONSTRAINT paper_raid_bff_quick_raid_version_ck CHECK (version > 0 AND version <= 9007199254740991),
    evidence_choice TEXT
        CONSTRAINT paper_raid_bff_quick_raid_evidence_choice_ck CHECK (evidence_choice IS NULL OR evidence_choice IN ('flag_citation_gap','accept_as_sufficient')),
    experiment_choice TEXT
        CONSTRAINT paper_raid_bff_quick_raid_experiment_choice_ck CHECK (experiment_choice IS NULL OR experiment_choice IN ('recheck_baseline','run_candidate')),
    experiment_run JSONB,
    conclusion TEXT
        CONSTRAINT paper_raid_bff_quick_raid_conclusion_ck CHECK (conclusion IS NULL OR conclusion IN ('revise_claim','retain_with_caveat')),
    paper_bundle JSONB,
    paper_bundle_hash TEXT
        CONSTRAINT paper_raid_bff_quick_raid_bundle_hash_ck CHECK (paper_bundle_hash IS NULL OR paper_bundle_hash ~ '^sha256:[0-9a-f]{64}$'),
    activation_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    qualification_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    scientific_finality_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    ranking_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    reward_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    score_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    economic_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    completion_portable BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    terminal_at TIMESTAMPTZ,
    terminal_reason TEXT
        CONSTRAINT paper_raid_bff_quick_raid_terminal_reason_ck CHECK (terminal_reason IS NULL OR terminal_reason IN ('completed','player_abandoned','expired')),
    CONSTRAINT paper_raid_bff_quick_raid_owner_fk FOREIGN KEY (binding_id, subject_id, player_id)
        REFERENCES paper_raid_bff_agent_bridge_bindings(binding_id, subject_id, player_id),
    CONSTRAINT paper_raid_bff_quick_raid_subject_ck CHECK (octet_length(subject_id) BETWEEN 1 AND 256 AND subject_id = btrim(subject_id)),
    CONSTRAINT paper_raid_bff_quick_raid_non_nil_ck CHECK (
        session_id <> '00000000-0000-0000-0000-000000000000'::uuid
        AND player_id <> '00000000-0000-0000-0000-000000000000'::uuid
        AND binding_id <> '00000000-0000-0000-0000-000000000000'::uuid
        AND challenge_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    CONSTRAINT paper_raid_bff_quick_raid_boundary_ck CHECK (
        activation_eligible = FALSE AND qualification_eligible = FALSE
        AND scientific_finality_eligible = FALSE AND ranking_eligible = FALSE
        AND reward_eligible = FALSE AND score_eligible = FALSE
        AND economic_eligible = FALSE AND completion_portable = FALSE
    ),
    CONSTRAINT paper_raid_bff_quick_raid_ttl_ck CHECK (
        expires_at = created_at + interval '15 minutes'
        AND updated_at >= created_at
    ),
    CONSTRAINT paper_raid_bff_quick_raid_bundle_pair_ck CHECK ((paper_bundle IS NULL) = (paper_bundle_hash IS NULL)),
    CONSTRAINT paper_raid_bff_quick_raid_terminal_ck CHECK (
        (stage NOT IN ('completed','abandoned','expired') AND terminal_at IS NULL AND terminal_reason IS NULL)
        OR (stage = 'completed' AND terminal_at = updated_at AND terminal_reason = 'completed' AND paper_bundle IS NOT NULL)
        OR (stage = 'abandoned' AND terminal_at = updated_at AND terminal_reason = 'player_abandoned')
        OR (stage = 'expired' AND terminal_at = updated_at AND terminal_reason = 'expired')
    ),
    CONSTRAINT paper_raid_bff_quick_raid_shape_ck CHECK (
        (stage = 'evidence_review' AND evidence_choice IS NULL AND experiment_choice IS NULL AND experiment_run IS NULL AND conclusion IS NULL AND paper_bundle IS NULL)
        OR (stage = 'experiment_run' AND evidence_choice IS NOT NULL AND experiment_choice IS NULL AND experiment_run IS NULL AND conclusion IS NULL AND paper_bundle IS NULL)
        OR (stage = 'paper_bundle_ready' AND evidence_choice IS NOT NULL AND experiment_choice IS NOT NULL AND experiment_run IS NOT NULL AND conclusion IS NULL AND paper_bundle IS NULL)
        OR (stage = 'completed' AND evidence_choice IS NOT NULL AND experiment_choice IS NOT NULL AND experiment_run IS NOT NULL AND conclusion IS NOT NULL AND paper_bundle IS NOT NULL)
        OR stage IN ('abandoned','expired')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_quick_raid_one_live_player
    ON paper_raid_bff_quick_raid_sessions(player_id)
    WHERE stage NOT IN ('completed','abandoned','expired');

CREATE TABLE IF NOT EXISTS paper_raid_bff_quick_raid_events (
    event_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES paper_raid_bff_quick_raid_sessions(session_id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('browser','server')),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('evidence_reviewed','experiment_run','paper_published','abandoned','expired')),
    from_version BIGINT NOT NULL CHECK (from_version > 0 AND from_version < 9007199254740991),
    to_version BIGINT NOT NULL CHECK (to_version = from_version + 1 AND to_version <= 9007199254740991),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^sha256:[0-9a-f]{64}$'),
    choice_code TEXT,
    result_hash TEXT CHECK (result_hash IS NULL OR result_hash ~ '^sha256:[0-9a-f]{64}$'),
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE OR REPLACE FUNCTION paper_raid_bff_reject_quick_raid_event_mutation_v1()
RETURNS trigger LANGUAGE plpgsql SET search_path = pg_catalog AS $function$
BEGIN
    RAISE EXCEPTION 'paper_raid_bff_quick_raid_events is append-only';
END;
$function$;

DROP TRIGGER IF EXISTS paper_raid_bff_quick_raid_events_append_only
    ON paper_raid_bff_quick_raid_events;
CREATE TRIGGER paper_raid_bff_quick_raid_events_append_only
BEFORE UPDATE OR DELETE ON paper_raid_bff_quick_raid_events
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_reject_quick_raid_event_mutation_v1();

DROP TRIGGER IF EXISTS paper_raid_bff_quick_raid_events_no_truncate
    ON paper_raid_bff_quick_raid_events;
CREATE TRIGGER paper_raid_bff_quick_raid_events_no_truncate
BEFORE TRUNCATE ON paper_raid_bff_quick_raid_events
FOR EACH STATEMENT EXECUTE FUNCTION paper_raid_bff_reject_quick_raid_event_mutation_v1();

CREATE OR REPLACE FUNCTION paper_raid_bff_quick_raid_session_monotonic_v1()
RETURNS trigger LANGUAGE plpgsql SET search_path = pg_catalog, public AS $function$
BEGIN
    IF NEW.version <> OLD.version + 1 OR NEW.updated_at < OLD.updated_at THEN
        RAISE EXCEPTION 'paper_raid_bff_quick_raid_sessions version is not monotonic';
    END IF;
    IF NEW.session_id IS DISTINCT FROM OLD.session_id
       OR NEW.subject_id IS DISTINCT FROM OLD.subject_id
       OR NEW.player_id IS DISTINCT FROM OLD.player_id
       OR NEW.binding_id IS DISTINCT FROM OLD.binding_id
       OR NEW.challenge_key IS DISTINCT FROM OLD.challenge_key
       OR NEW.challenge_id IS DISTINCT FROM OLD.challenge_id
       OR NEW.challenge_snapshot_hash IS DISTINCT FROM OLD.challenge_snapshot_hash
       OR NEW.pack_id IS DISTINCT FROM OLD.pack_id
       OR NEW.ruleset_version IS DISTINCT FROM OLD.ruleset_version
       OR NEW.ruleset_hash IS DISTINCT FROM OLD.ruleset_hash
       OR NEW.authority_hash IS DISTINCT FROM OLD.authority_hash
       OR NEW.seed IS DISTINCT FROM OLD.seed
       OR NEW.duration_seconds IS DISTINCT FROM OLD.duration_seconds
       OR NEW.authority_kind IS DISTINCT FROM OLD.authority_kind
       OR NEW.activation_eligible OR NEW.qualification_eligible
       OR NEW.scientific_finality_eligible OR NEW.ranking_eligible
       OR NEW.reward_eligible OR NEW.score_eligible OR NEW.economic_eligible
       OR NEW.completion_portable
       OR NEW.created_at IS DISTINCT FROM OLD.created_at
       OR NEW.expires_at IS DISTINCT FROM OLD.expires_at
    THEN
        RAISE EXCEPTION 'paper_raid_bff_quick_raid_sessions authority is immutable';
    END IF;
    IF OLD.evidence_choice IS NOT NULL AND NEW.evidence_choice IS DISTINCT FROM OLD.evidence_choice
       OR OLD.experiment_choice IS NOT NULL AND NEW.experiment_choice IS DISTINCT FROM OLD.experiment_choice
       OR OLD.experiment_run IS NOT NULL AND NEW.experiment_run IS DISTINCT FROM OLD.experiment_run
       OR OLD.conclusion IS NOT NULL AND NEW.conclusion IS DISTINCT FROM OLD.conclusion
       OR OLD.paper_bundle IS NOT NULL AND NEW.paper_bundle IS DISTINCT FROM OLD.paper_bundle
       OR OLD.paper_bundle_hash IS NOT NULL AND NEW.paper_bundle_hash IS DISTINCT FROM OLD.paper_bundle_hash
    THEN
        RAISE EXCEPTION 'paper_raid_bff_quick_raid_sessions answers are immutable';
    END IF;
    IF NEW.stage IN ('abandoned','expired') THEN
        IF NEW.evidence_choice IS DISTINCT FROM OLD.evidence_choice
           OR NEW.experiment_choice IS DISTINCT FROM OLD.experiment_choice
           OR NEW.experiment_run IS DISTINCT FROM OLD.experiment_run
           OR NEW.conclusion IS DISTINCT FROM OLD.conclusion
           OR NEW.paper_bundle IS DISTINCT FROM OLD.paper_bundle
           OR NEW.paper_bundle_hash IS DISTINCT FROM OLD.paper_bundle_hash
        THEN
            RAISE EXCEPTION 'terminal Quick Raid transition changed answers';
        END IF;
    ELSIF OLD.stage = 'evidence_review' AND NEW.stage = 'experiment_run' THEN
        IF NEW.evidence_choice IS NULL THEN RAISE EXCEPTION 'evidence choice required'; END IF;
    ELSIF OLD.stage = 'experiment_run' AND NEW.stage = 'paper_bundle_ready' THEN
        IF NEW.experiment_choice IS NULL OR NEW.experiment_run IS NULL THEN RAISE EXCEPTION 'experiment result required'; END IF;
    ELSIF OLD.stage = 'paper_bundle_ready' AND NEW.stage = 'completed' THEN
        IF NEW.conclusion IS NULL OR NEW.paper_bundle IS NULL OR NEW.paper_bundle_hash IS NULL THEN RAISE EXCEPTION 'paper bundle required'; END IF;
    ELSE
        RAISE EXCEPTION 'paper_raid_bff_quick_raid_sessions exact transition rejected';
    END IF;
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS paper_raid_bff_quick_raid_session_monotonic
    ON paper_raid_bff_quick_raid_sessions;
CREATE TRIGGER paper_raid_bff_quick_raid_session_monotonic
BEFORE UPDATE ON paper_raid_bff_quick_raid_sessions
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_quick_raid_session_monotonic_v1();

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('quick_raid_fixed_seed_v1')
ON CONFLICT (capability) DO NOTHING;

COMMENT ON TABLE paper_raid_bff_quick_raid_sessions IS
    'Authoritative fixed-seed Quick Raid projection; no portable/scientific/rank/reward/economic authority.';
COMMENT ON TABLE paper_raid_bff_quick_raid_events IS
    'Append-only Quick Raid transition evidence.';

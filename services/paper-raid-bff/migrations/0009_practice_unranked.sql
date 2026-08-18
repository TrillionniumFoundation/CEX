-- BFF-local practice state only.  These rows are structurally incapable of
-- activating a Challenge, qualifying a player, creating scientific finality,
-- changing a score/rank/reward, or carrying economic authority.

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_agent_binding_owner_uq
    ON paper_raid_bff_agent_bridge_bindings(binding_id, subject_id, player_id);

CREATE TABLE IF NOT EXISTS paper_raid_bff_practice_sessions (
    practice_session_id UUID
        CONSTRAINT paper_raid_bff_practice_sessions_pk PRIMARY KEY,
    subject_id TEXT NOT NULL,
    player_id UUID NOT NULL,
    binding_id UUID NOT NULL,
    mode TEXT NOT NULL DEFAULT 'practice_unranked'
        CONSTRAINT paper_raid_bff_practice_mode_ck
        CHECK (mode = 'practice_unranked'),
    scenario_id TEXT NOT NULL
        CONSTRAINT paper_raid_bff_practice_scenario_ck
        CHECK (scenario_id = 'evidence-audit-intro-v1'),
    authority_kind TEXT NOT NULL DEFAULT 'none'
        CONSTRAINT paper_raid_bff_practice_authority_kind_ck
        CHECK (authority_kind = 'none'),
    stage TEXT NOT NULL
        CONSTRAINT paper_raid_bff_practice_stage_ck CHECK (stage IN (
            'captain_plan', 'evidence_assessment',
            'experiment_waiting_bridge', 'experiment_interpretation',
            'captain_aar', 'completed', 'abandoned', 'expired'
        )),
    version BIGINT NOT NULL
        CONSTRAINT paper_raid_bff_practice_version_ck
        CHECK (version > 0 AND version <= 9007199254740991),
    captain_plan TEXT
        CONSTRAINT paper_raid_bff_practice_captain_plan_ck CHECK (
            captain_plan IS NULL OR captain_plan IN (
                'audit_highest_risk_claim', 'audit_evidence_chain_first'
            )
        ),
    evidence_assessment TEXT
        CONSTRAINT paper_raid_bff_practice_evidence_assessment_ck CHECK (
            evidence_assessment IS NULL OR evidence_assessment IN (
                'unsupported_claim', 'citation_mismatch', 'evidence_sufficient'
            )
        ),
    bridge_task_id UUID NOT NULL,
    bridge_task_state TEXT NOT NULL
        CONSTRAINT paper_raid_bff_practice_bridge_state_ck
        CHECK (bridge_task_state IN ('pending', 'claimed', 'completed')),
    bridge_result_code TEXT
        CONSTRAINT paper_raid_bff_practice_bridge_result_code_ck CHECK (
            bridge_result_code IS NULL OR bridge_result_code IN (
                'concern_confirmed', 'concern_not_detected', 'inconclusive'
            )
        ),
    bridge_result_hash TEXT
        CONSTRAINT paper_raid_bff_practice_bridge_result_hash_ck CHECK (
            bridge_result_hash IS NULL
            OR bridge_result_hash ~ '^sha256:[0-9a-f]{64}$'
        ),
    experiment_interpretation TEXT
        CONSTRAINT paper_raid_bff_practice_interpretation_ck CHECK (
            experiment_interpretation IS NULL OR experiment_interpretation IN (
                'revise_claim', 'request_more_evidence',
                'retain_claim_with_caveat'
            )
        ),
    aar_choice TEXT
        CONSTRAINT paper_raid_bff_practice_aar_ck CHECK (
            aar_choice IS NULL OR aar_choice IN (
                'improve_evidence_triage', 'improve_experiment_design',
                'improve_team_coordination'
            )
        ),
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
        CONSTRAINT paper_raid_bff_practice_terminal_reason_ck CHECK (
            terminal_reason IS NULL OR terminal_reason IN (
                'completed', 'player_abandoned', 'expired'
            )
        ),
    CONSTRAINT paper_raid_bff_practice_non_nil_ids_ck CHECK (
        practice_session_id <> '00000000-0000-0000-0000-000000000000'::uuid
        AND player_id <> '00000000-0000-0000-0000-000000000000'::uuid
        AND binding_id <> '00000000-0000-0000-0000-000000000000'::uuid
        AND bridge_task_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    CONSTRAINT paper_raid_bff_practice_subject_ck CHECK (
        octet_length(subject_id) BETWEEN 1 AND 256
        AND subject_id = btrim(subject_id)
    ),
    CONSTRAINT paper_raid_bff_practice_binding_owner_fk FOREIGN KEY (
        binding_id, subject_id, player_id
    ) REFERENCES paper_raid_bff_agent_bridge_bindings(
        binding_id, subject_id, player_id
    ),
    CONSTRAINT paper_raid_bff_practice_ttl_ck CHECK (
        expires_at > created_at
        AND expires_at <= created_at + interval '45 minutes'
        AND updated_at >= created_at
    ),
    CONSTRAINT paper_raid_bff_practice_no_authority_ck CHECK (
        authority_kind = 'none'
        AND activation_eligible = FALSE
        AND qualification_eligible = FALSE
        AND scientific_finality_eligible = FALSE
        AND ranking_eligible = FALSE
        AND reward_eligible = FALSE
        AND score_eligible = FALSE
        AND economic_eligible = FALSE
        AND completion_portable = FALSE
    ),
    CONSTRAINT paper_raid_bff_practice_bridge_result_pair_ck CHECK (
        (bridge_result_code IS NULL) = (bridge_result_hash IS NULL)
        AND (bridge_task_state = 'completed') = (bridge_result_code IS NOT NULL)
    ),
    CONSTRAINT paper_raid_bff_practice_answer_prefix_ck CHECK (
        (evidence_assessment IS NULL OR captain_plan IS NOT NULL)
        AND (
            bridge_task_state = 'pending'
            OR evidence_assessment IS NOT NULL
        )
        AND (
            experiment_interpretation IS NULL
            OR (
                evidence_assessment IS NOT NULL
                AND bridge_task_state = 'completed'
            )
        )
        AND (aar_choice IS NULL OR experiment_interpretation IS NOT NULL)
    ),
    CONSTRAINT paper_raid_bff_practice_terminal_binding_ck CHECK (
        (
            stage NOT IN ('completed', 'abandoned', 'expired')
            AND terminal_at IS NULL AND terminal_reason IS NULL
        ) OR (
            stage = 'completed' AND terminal_at = updated_at
            AND terminal_reason = 'completed'
        ) OR (
            stage = 'abandoned' AND terminal_at = updated_at
            AND terminal_reason = 'player_abandoned'
        ) OR (
            stage = 'expired' AND terminal_at = updated_at
            AND terminal_reason = 'expired'
        )
    ),
    CONSTRAINT paper_raid_bff_practice_lifecycle_ck CHECK (
        (stage = 'captain_plan'
            AND captain_plan IS NULL AND evidence_assessment IS NULL
            AND bridge_task_state = 'pending'
            AND experiment_interpretation IS NULL AND aar_choice IS NULL)
        OR (stage = 'evidence_assessment'
            AND captain_plan IS NOT NULL AND evidence_assessment IS NULL
            AND bridge_task_state = 'pending'
            AND experiment_interpretation IS NULL AND aar_choice IS NULL)
        OR (stage = 'experiment_waiting_bridge'
            AND captain_plan IS NOT NULL AND evidence_assessment IS NOT NULL
            AND bridge_task_state IN ('pending', 'claimed')
            AND experiment_interpretation IS NULL AND aar_choice IS NULL)
        OR (stage = 'experiment_interpretation'
            AND captain_plan IS NOT NULL AND evidence_assessment IS NOT NULL
            AND bridge_task_state = 'completed'
            AND experiment_interpretation IS NULL AND aar_choice IS NULL)
        OR (stage = 'captain_aar'
            AND captain_plan IS NOT NULL AND evidence_assessment IS NOT NULL
            AND bridge_task_state = 'completed'
            AND experiment_interpretation IS NOT NULL AND aar_choice IS NULL)
        OR (stage = 'completed'
            AND captain_plan IS NOT NULL AND evidence_assessment IS NOT NULL
            AND bridge_task_state = 'completed'
            AND experiment_interpretation IS NOT NULL AND aar_choice IS NOT NULL)
        OR stage IN ('abandoned', 'expired')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_live_practice_per_player
    ON paper_raid_bff_practice_sessions(player_id)
    WHERE stage NOT IN ('completed', 'abandoned', 'expired');

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_live_practice_per_binding
    ON paper_raid_bff_practice_sessions(binding_id)
    WHERE stage NOT IN ('completed', 'abandoned', 'expired');

CREATE INDEX IF NOT EXISTS paper_raid_bff_practice_expiry_idx
    ON paper_raid_bff_practice_sessions(expires_at)
    WHERE stage NOT IN ('completed', 'abandoned', 'expired');

CREATE TABLE IF NOT EXISTS paper_raid_bff_practice_events (
    event_id UUID CONSTRAINT paper_raid_bff_practice_events_pk PRIMARY KEY,
    practice_session_id UUID NOT NULL
        CONSTRAINT paper_raid_bff_practice_event_session_fk
        REFERENCES paper_raid_bff_practice_sessions(practice_session_id),
    actor_kind TEXT NOT NULL
        CONSTRAINT paper_raid_bff_practice_event_actor_ck
        CHECK (actor_kind IN ('browser', 'agent', 'server')),
    event_kind TEXT NOT NULL
        CONSTRAINT paper_raid_bff_practice_event_kind_ck CHECK (event_kind IN (
            'captain_planned', 'evidence_assessed', 'experiment_claimed',
            'experiment_completed', 'experiment_interpreted',
            'aar_completed', 'abandoned', 'expired'
        )),
    from_version BIGINT NOT NULL,
    to_version BIGINT NOT NULL,
    request_hash TEXT NOT NULL
        CONSTRAINT paper_raid_bff_practice_event_request_hash_ck
        CHECK (request_hash ~ '^sha256:[0-9a-f]{64}$'),
    choice_code TEXT,
    result_hash TEXT
        CONSTRAINT paper_raid_bff_practice_event_result_hash_ck CHECK (
            result_hash IS NULL OR result_hash ~ '^sha256:[0-9a-f]{64}$'
        ),
    occurred_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT paper_raid_bff_practice_event_non_nil_ck CHECK (
        event_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    CONSTRAINT paper_raid_bff_practice_event_version_ck CHECK (
        from_version > 0 AND from_version < 9007199254740991
        AND to_version = from_version + 1
    ),
    CONSTRAINT paper_raid_bff_practice_event_shape_ck CHECK (
        (event_kind = 'captain_planned' AND actor_kind = 'browser'
            AND choice_code IN (
                'audit_highest_risk_claim', 'audit_evidence_chain_first'
            ) AND result_hash IS NULL)
        OR (event_kind = 'evidence_assessed' AND actor_kind = 'browser'
            AND choice_code IN (
                'unsupported_claim', 'citation_mismatch', 'evidence_sufficient'
            ) AND result_hash IS NULL)
        OR (event_kind = 'experiment_claimed' AND actor_kind = 'agent'
            AND choice_code IS NULL AND result_hash IS NULL)
        OR (event_kind = 'experiment_completed' AND actor_kind = 'agent'
            AND choice_code IN (
                'concern_confirmed', 'concern_not_detected', 'inconclusive'
            ) AND result_hash IS NOT NULL)
        OR (event_kind = 'experiment_interpreted' AND actor_kind = 'browser'
            AND choice_code IN (
                'revise_claim', 'request_more_evidence',
                'retain_claim_with_caveat'
            ) AND result_hash IS NULL)
        OR (event_kind = 'aar_completed' AND actor_kind = 'browser'
            AND choice_code IN (
                'improve_evidence_triage', 'improve_experiment_design',
                'improve_team_coordination'
            ) AND result_hash IS NULL)
        OR (event_kind = 'abandoned' AND actor_kind = 'browser'
            AND choice_code IS NULL AND result_hash IS NULL)
        OR (event_kind = 'expired' AND actor_kind = 'server'
            AND choice_code IS NULL AND result_hash IS NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_practice_event_transition_uq
    ON paper_raid_bff_practice_events(practice_session_id, to_version);

CREATE INDEX IF NOT EXISTS paper_raid_bff_practice_event_order_idx
    ON paper_raid_bff_practice_events(practice_session_id, from_version);

CREATE OR REPLACE FUNCTION paper_raid_bff_practice_session_monotonic_v1()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, public
AS $function$
BEGIN
    IF OLD.stage IN ('completed', 'abandoned', 'expired') THEN
        RAISE EXCEPTION 'paper_raid_bff_practice_sessions terminal rows are immutable';
    END IF;
    IF NEW.practice_session_id IS DISTINCT FROM OLD.practice_session_id
       OR NEW.subject_id IS DISTINCT FROM OLD.subject_id
       OR NEW.player_id IS DISTINCT FROM OLD.player_id
       OR NEW.binding_id IS DISTINCT FROM OLD.binding_id
       OR NEW.mode IS DISTINCT FROM OLD.mode
       OR NEW.scenario_id IS DISTINCT FROM OLD.scenario_id
       OR NEW.authority_kind IS DISTINCT FROM OLD.authority_kind
       OR NEW.bridge_task_id IS DISTINCT FROM OLD.bridge_task_id
       OR NEW.created_at IS DISTINCT FROM OLD.created_at
       OR NEW.expires_at IS DISTINCT FROM OLD.expires_at
       OR NEW.activation_eligible IS DISTINCT FROM OLD.activation_eligible
       OR NEW.qualification_eligible IS DISTINCT FROM OLD.qualification_eligible
       OR NEW.scientific_finality_eligible IS DISTINCT FROM OLD.scientific_finality_eligible
       OR NEW.ranking_eligible IS DISTINCT FROM OLD.ranking_eligible
       OR NEW.reward_eligible IS DISTINCT FROM OLD.reward_eligible
       OR NEW.score_eligible IS DISTINCT FROM OLD.score_eligible
       OR NEW.economic_eligible IS DISTINCT FROM OLD.economic_eligible
       OR NEW.completion_portable IS DISTINCT FROM OLD.completion_portable
    THEN
        RAISE EXCEPTION 'paper_raid_bff_practice_sessions boundary is immutable';
    END IF;
    IF NEW.version IS DISTINCT FROM OLD.version + 1
       OR NEW.updated_at < OLD.updated_at
    THEN
        RAISE EXCEPTION 'paper_raid_bff_practice_sessions version is not monotonic';
    END IF;
    IF OLD.captain_plan IS NOT NULL
       AND NEW.captain_plan IS DISTINCT FROM OLD.captain_plan
       OR OLD.evidence_assessment IS NOT NULL
       AND NEW.evidence_assessment IS DISTINCT FROM OLD.evidence_assessment
       OR OLD.bridge_result_code IS NOT NULL
       AND NEW.bridge_result_code IS DISTINCT FROM OLD.bridge_result_code
       OR OLD.bridge_result_hash IS NOT NULL
       AND NEW.bridge_result_hash IS DISTINCT FROM OLD.bridge_result_hash
       OR OLD.experiment_interpretation IS NOT NULL
       AND NEW.experiment_interpretation IS DISTINCT FROM OLD.experiment_interpretation
       OR OLD.aar_choice IS NOT NULL
       AND NEW.aar_choice IS DISTINCT FROM OLD.aar_choice
    THEN
        RAISE EXCEPTION 'paper_raid_bff_practice_sessions answers are immutable';
    END IF;
    IF NOT (
        (OLD.stage = 'captain_plan' AND NEW.stage IN (
            'evidence_assessment', 'abandoned', 'expired'
        ))
        OR (OLD.stage = 'evidence_assessment' AND NEW.stage IN (
            'experiment_waiting_bridge', 'abandoned', 'expired'
        ))
        OR (OLD.stage = 'experiment_waiting_bridge'
            AND NEW.stage IN (
                'experiment_waiting_bridge', 'experiment_interpretation',
                'abandoned', 'expired'
            ))
        OR (OLD.stage = 'experiment_interpretation' AND NEW.stage IN (
            'captain_aar', 'abandoned', 'expired'
        ))
        OR (OLD.stage = 'captain_aar' AND NEW.stage IN (
            'completed', 'abandoned', 'expired'
        ))
    ) THEN
        RAISE EXCEPTION 'paper_raid_bff_practice_sessions stage transition rejected';
    END IF;
    IF NEW.stage IN ('abandoned', 'expired') THEN
        IF NEW.captain_plan IS DISTINCT FROM OLD.captain_plan
           OR NEW.evidence_assessment IS DISTINCT FROM OLD.evidence_assessment
           OR NEW.bridge_task_state IS DISTINCT FROM OLD.bridge_task_state
           OR NEW.bridge_result_code IS DISTINCT FROM OLD.bridge_result_code
           OR NEW.bridge_result_hash IS DISTINCT FROM OLD.bridge_result_hash
           OR NEW.experiment_interpretation IS DISTINCT FROM OLD.experiment_interpretation
           OR NEW.aar_choice IS DISTINCT FROM OLD.aar_choice
        THEN
            RAISE EXCEPTION 'paper_raid_bff_practice_sessions terminal transition changed answers';
        END IF;
    ELSIF OLD.stage = 'captain_plan' AND NEW.stage = 'evidence_assessment' THEN
        IF NEW.captain_plan IS NULL
           OR NEW.evidence_assessment IS DISTINCT FROM OLD.evidence_assessment
           OR NEW.bridge_task_state IS DISTINCT FROM OLD.bridge_task_state
           OR NEW.bridge_result_code IS DISTINCT FROM OLD.bridge_result_code
           OR NEW.bridge_result_hash IS DISTINCT FROM OLD.bridge_result_hash
           OR NEW.experiment_interpretation IS DISTINCT FROM OLD.experiment_interpretation
           OR NEW.aar_choice IS DISTINCT FROM OLD.aar_choice
        THEN
            RAISE EXCEPTION 'paper_raid_bff_practice_sessions captain transition rejected';
        END IF;
    ELSIF OLD.stage = 'evidence_assessment'
          AND NEW.stage = 'experiment_waiting_bridge' THEN
        IF NEW.captain_plan IS DISTINCT FROM OLD.captain_plan
           OR NEW.evidence_assessment IS NULL
           OR NEW.bridge_task_state IS DISTINCT FROM OLD.bridge_task_state
           OR NEW.bridge_result_code IS DISTINCT FROM OLD.bridge_result_code
           OR NEW.bridge_result_hash IS DISTINCT FROM OLD.bridge_result_hash
           OR NEW.experiment_interpretation IS DISTINCT FROM OLD.experiment_interpretation
           OR NEW.aar_choice IS DISTINCT FROM OLD.aar_choice
        THEN
            RAISE EXCEPTION 'paper_raid_bff_practice_sessions evidence transition rejected';
        END IF;
    ELSIF OLD.stage = 'experiment_waiting_bridge'
          AND NEW.stage = 'experiment_waiting_bridge' THEN
        IF OLD.bridge_task_state <> 'pending' OR NEW.bridge_task_state <> 'claimed'
           OR NEW.captain_plan IS DISTINCT FROM OLD.captain_plan
           OR NEW.evidence_assessment IS DISTINCT FROM OLD.evidence_assessment
           OR NEW.bridge_result_code IS DISTINCT FROM OLD.bridge_result_code
           OR NEW.bridge_result_hash IS DISTINCT FROM OLD.bridge_result_hash
           OR NEW.experiment_interpretation IS DISTINCT FROM OLD.experiment_interpretation
           OR NEW.aar_choice IS DISTINCT FROM OLD.aar_choice
        THEN
            RAISE EXCEPTION 'paper_raid_bff_practice_sessions claim transition rejected';
        END IF;
    ELSIF OLD.stage = 'experiment_waiting_bridge'
          AND NEW.stage = 'experiment_interpretation' THEN
        IF OLD.bridge_task_state <> 'claimed' OR NEW.bridge_task_state <> 'completed'
           OR NEW.bridge_result_code IS NULL OR NEW.bridge_result_hash IS NULL
           OR NEW.captain_plan IS DISTINCT FROM OLD.captain_plan
           OR NEW.evidence_assessment IS DISTINCT FROM OLD.evidence_assessment
           OR NEW.experiment_interpretation IS DISTINCT FROM OLD.experiment_interpretation
           OR NEW.aar_choice IS DISTINCT FROM OLD.aar_choice
        THEN
            RAISE EXCEPTION 'paper_raid_bff_practice_sessions result transition rejected';
        END IF;
    ELSIF OLD.stage = 'experiment_interpretation' AND NEW.stage = 'captain_aar' THEN
        IF NEW.experiment_interpretation IS NULL
           OR NEW.captain_plan IS DISTINCT FROM OLD.captain_plan
           OR NEW.evidence_assessment IS DISTINCT FROM OLD.evidence_assessment
           OR NEW.bridge_task_state IS DISTINCT FROM OLD.bridge_task_state
           OR NEW.bridge_result_code IS DISTINCT FROM OLD.bridge_result_code
           OR NEW.bridge_result_hash IS DISTINCT FROM OLD.bridge_result_hash
           OR NEW.aar_choice IS DISTINCT FROM OLD.aar_choice
        THEN
            RAISE EXCEPTION 'paper_raid_bff_practice_sessions interpretation transition rejected';
        END IF;
    ELSIF OLD.stage = 'captain_aar' AND NEW.stage = 'completed' THEN
        IF NEW.aar_choice IS NULL
           OR NEW.captain_plan IS DISTINCT FROM OLD.captain_plan
           OR NEW.evidence_assessment IS DISTINCT FROM OLD.evidence_assessment
           OR NEW.bridge_task_state IS DISTINCT FROM OLD.bridge_task_state
           OR NEW.bridge_result_code IS DISTINCT FROM OLD.bridge_result_code
           OR NEW.bridge_result_hash IS DISTINCT FROM OLD.bridge_result_hash
           OR NEW.experiment_interpretation IS DISTINCT FROM OLD.experiment_interpretation
        THEN
            RAISE EXCEPTION 'paper_raid_bff_practice_sessions AAR transition rejected';
        END IF;
    ELSE
        RAISE EXCEPTION 'paper_raid_bff_practice_sessions exact transition rejected';
    END IF;
    IF NEW.stage = 'expired' AND statement_timestamp() < OLD.expires_at THEN
        RAISE EXCEPTION 'paper_raid_bff_practice_sessions cannot expire early';
    END IF;
    IF NEW.stage <> 'expired' AND statement_timestamp() >= OLD.expires_at THEN
        RAISE EXCEPTION 'paper_raid_bff_practice_sessions expired before transition';
    END IF;
    RETURN NEW;
END;
$function$;

CREATE OR REPLACE FUNCTION paper_raid_bff_reject_practice_event_mutation_v1()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
BEGIN
    RAISE EXCEPTION 'paper_raid_bff_practice_events is append-only';
END;
$function$;

DROP TRIGGER IF EXISTS paper_raid_bff_practice_session_monotonic
    ON paper_raid_bff_practice_sessions;
CREATE TRIGGER paper_raid_bff_practice_session_monotonic
BEFORE UPDATE ON paper_raid_bff_practice_sessions
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_practice_session_monotonic_v1();

DROP TRIGGER IF EXISTS paper_raid_bff_practice_events_append_only
    ON paper_raid_bff_practice_events;
CREATE TRIGGER paper_raid_bff_practice_events_append_only
BEFORE UPDATE OR DELETE ON paper_raid_bff_practice_events
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_reject_practice_event_mutation_v1();

DROP TRIGGER IF EXISTS paper_raid_bff_practice_events_no_truncate
    ON paper_raid_bff_practice_events;
CREATE TRIGGER paper_raid_bff_practice_events_no_truncate
BEFORE TRUNCATE ON paper_raid_bff_practice_events
FOR EACH STATEMENT EXECUTE FUNCTION paper_raid_bff_reject_practice_event_mutation_v1();

COMMENT ON TABLE paper_raid_bff_practice_sessions IS
    'BFF-local unranked practice progress; never a scientific, qualification, rank, reward, score, or economic authority.';
COMMENT ON TABLE paper_raid_bff_practice_events IS
    'Append-only BFF-local practice transitions with no portable or scientific authority.';

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('practice_unranked_v1')
ON CONFLICT (capability) DO NOTHING;

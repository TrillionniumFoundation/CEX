CREATE TABLE IF NOT EXISTS paper_raid_bff_product_events (
    event_id UUID PRIMARY KEY,
    session_id UUID,
    player_id UUID NOT NULL,
    event_name TEXT NOT NULL CHECK (event_name IN (
        'login_succeeded', 'queue_started', 'team_formed', 'first_action',
        'phase_entered', 'abandoned', 'reconnected', 'raid_completed',
        'replay_started', 'continue_opened'
    )),
    challenge_id UUID,
    team_id UUID,
    paper_id UUID,
    phase TEXT,
    source TEXT NOT NULL CHECK (source IN ('server_command', 'browser_signal')),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS paper_raid_bff_product_events_player_time_idx
    ON paper_raid_bff_product_events(player_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS paper_raid_bff_product_events_funnel_idx
    ON paper_raid_bff_product_events(event_name, occurred_at DESC);

-- Product telemetry is deliberately identifier-only. It stores no free-form
-- paper text, credentials, signatures, request bodies, or Agent output.

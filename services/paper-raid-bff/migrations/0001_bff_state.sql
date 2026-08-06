CREATE TABLE IF NOT EXISTS paper_raid_bff_session_generation (
    subject_id TEXT PRIMARY KEY,
    generation BIGINT NOT NULL CHECK (generation >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_sessions (
    session_id UUID PRIMARY KEY,
    subject_id TEXT NOT NULL,
    generation BIGINT NOT NULL CHECK (generation >= 0),
    csrf_hash BYTEA NOT NULL CHECK (octet_length(csrf_hash) = 32),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS paper_raid_bff_sessions_subject_idx
    ON paper_raid_bff_sessions(subject_id, generation);

CREATE TABLE IF NOT EXISTS paper_raid_bff_csrf_uses (
    session_id UUID NOT NULL REFERENCES paper_raid_bff_sessions(session_id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL CHECK (octet_length(token_hash) = 32),
    used_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, token_hash)
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_assertions (
    assertion_id UUID PRIMARY KEY,
    subject_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_idempotency (
    subject_id TEXT NOT NULL,
    idempotency_key UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('pending', 'completed')),
    response_status INTEGER,
    response_body BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK (
        (state = 'pending' AND response_status IS NULL AND response_body IS NULL AND completed_at IS NULL)
        OR
        (state = 'completed' AND response_status BETWEEN 100 AND 599 AND response_body IS NOT NULL AND completed_at IS NOT NULL)
    ),
    PRIMARY KEY (subject_id, idempotency_key)
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_read_cache (
    cache_key TEXT PRIMARY KEY,
    etag TEXT,
    payload JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Every table in this schema is BFF-local session, replay, or read-cache
-- state. Team, paper, artifact and research facts remain authoritative in
-- Hepta and content-addressed storage.

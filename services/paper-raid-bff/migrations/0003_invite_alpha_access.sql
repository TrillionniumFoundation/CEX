-- Invite-Key SSH Alpha access directory. This is BFF-local identity/session
-- state only; it is never a legacy league, Paper, score, or reward authority.

CREATE TABLE IF NOT EXISTS paper_raid_bff_schema_capabilities (
    capability TEXT PRIMARY KEY,
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_accounts (
    account_id UUID PRIMARY KEY,
    subject_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    nakama_user_id UUID NOT NULL UNIQUE,
    player_id UUID NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('invited', 'active', 'suspended', 'closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    activated_at TIMESTAMPTZ,
    suspended_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (
        (state = 'invited' AND activated_at IS NULL AND suspended_at IS NULL AND closed_at IS NULL)
        OR (state = 'active' AND activated_at IS NOT NULL AND suspended_at IS NULL AND closed_at IS NULL)
        OR (state = 'suspended' AND activated_at IS NOT NULL AND suspended_at IS NOT NULL AND closed_at IS NULL)
        OR (state = 'closed' AND closed_at IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_account_scopes (
    account_id UUID NOT NULL REFERENCES paper_raid_bff_accounts(account_id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN ('author', 'evaluator', 'reviewer', 'reproducer')),
    PRIMARY KEY (account_id, scope)
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_account_author_roles (
    account_id UUID NOT NULL REFERENCES paper_raid_bff_accounts(account_id) ON DELETE CASCADE,
    author_role TEXT NOT NULL CHECK (author_role IN ('captain', 'evidence', 'experiment')),
    PRIMARY KEY (account_id, author_role)
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_invite_batches (
    batch_id UUID PRIMARY KEY,
    label TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'paused', 'revoked')),
    max_issued INTEGER NOT NULL CHECK (max_issued BETWEEN 1 AND 64),
    issued_count INTEGER NOT NULL DEFAULT 0 CHECK (issued_count >= 0 AND issued_count <= max_issued),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at IS NULL OR expires_at > created_at)
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_invites (
    invite_id UUID PRIMARY KEY,
    batch_id UUID NOT NULL REFERENCES paper_raid_bff_invite_batches(batch_id),
    account_id UUID NOT NULL UNIQUE REFERENCES paper_raid_bff_accounts(account_id),
    secret_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(secret_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('issued', 'redeemed', 'revoked')),
    expires_at TIMESTAMPTZ,
    credential_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    redeemed_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at IS NULL OR expires_at > created_at),
    CHECK (credential_expires_at IS NULL OR credential_expires_at > created_at),
    CHECK (
        (state = 'issued' AND redeemed_at IS NULL AND revoked_at IS NULL)
        OR (state = 'redeemed' AND redeemed_at IS NOT NULL AND revoked_at IS NULL)
        OR (state = 'revoked' AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_login_credentials (
    credential_id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES paper_raid_bff_accounts(account_id) ON DELETE CASCADE,
    secret_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(secret_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('active', 'revoked')),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at IS NULL OR expires_at > created_at),
    CHECK (
        (state = 'active' AND revoked_at IS NULL)
        OR (state = 'revoked' AND revoked_at IS NOT NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_active_credential_per_account
    ON paper_raid_bff_login_credentials(account_id)
    WHERE state = 'active';

CREATE TABLE IF NOT EXISTS paper_raid_bff_access_audit (
    audit_id UUID PRIMARY KEY,
    operator_subject TEXT,
    account_id UUID REFERENCES paper_raid_bff_accounts(account_id),
    action TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'denied')),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(metadata) = 'object'),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS paper_raid_bff_access_audit_account_time_idx
    ON paper_raid_bff_access_audit(account_id, occurred_at DESC);

CREATE OR REPLACE FUNCTION paper_raid_bff_reject_access_audit_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'paper_raid_bff_access_audit is append-only';
END;
$$;

DO $migration$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
        WHERE tgname = 'paper_raid_bff_access_audit_append_only'
          AND tgrelid = 'paper_raid_bff_access_audit'::regclass
          AND NOT tgisinternal
    ) THEN
        EXECUTE 'CREATE TRIGGER paper_raid_bff_access_audit_append_only
                 BEFORE UPDATE OR DELETE ON paper_raid_bff_access_audit
                 FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_reject_access_audit_mutation()';
    END IF;
END;
$migration$;

CREATE TABLE IF NOT EXISTS paper_raid_bff_quota_windows (
    quota_kind TEXT NOT NULL CHECK (quota_kind IN ('login_global', 'login_bucket', 'authenticated_mutation')),
    principal_hash BYTEA NOT NULL CHECK (octet_length(principal_hash) = 32),
    window_started_at TIMESTAMPTZ NOT NULL,
    request_count BIGINT NOT NULL CHECK (request_count > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (quota_kind, principal_hash, window_started_at)
);

CREATE INDEX IF NOT EXISTS paper_raid_bff_quota_windows_expiry_idx
    ON paper_raid_bff_quota_windows(window_started_at);

CREATE TABLE IF NOT EXISTS paper_raid_bff_retention_runs (
    run_id UUID PRIMARY KEY,
    policy_id TEXT NOT NULL,
    cutoff TIMESTAMPTZ NOT NULL,
    operator_subject TEXT NOT NULL,
    deleted_counts JSONB NOT NULL CHECK (jsonb_typeof(deleted_counts) = 'object'),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('invite_alpha_access_v1')
ON CONFLICT (capability) DO NOTHING;

-- Invite secrets and credentials are represented only by SHA-256 digests.
-- No cleartext login material, OIDC token, Agent secret, Paper content,
-- signature, score, reward, or legacy league authority record is stored here.

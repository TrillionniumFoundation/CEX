\set ON_ERROR_STOP on

-- This fixture is expected to fail while applying 0006. The caller must run
-- it in an empty disposable database and assert the exact drift error.
\ir ../0003_invite_alpha_access.sql

CREATE OR REPLACE FUNCTION paper_raid_bff_reject_access_audit_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'tampered append-only guard';
END;
$$;

\ir ../0006_accessctl_operator_audit.sql

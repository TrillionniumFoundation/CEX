\set ON_ERROR_STOP on

-- This fixture is expected to fail while applying 0006. It keeps the
-- canonical function but weakens the named trigger to UPDATE-only, proving
-- that migration admission validates the trigger definition independently.
\ir ../0003_invite_alpha_access.sql

DROP TRIGGER paper_raid_bff_access_audit_append_only
    ON paper_raid_bff_access_audit;
CREATE TRIGGER paper_raid_bff_access_audit_append_only
BEFORE UPDATE ON paper_raid_bff_access_audit
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_reject_access_audit_mutation();

\ir ../0006_accessctl_operator_audit.sql

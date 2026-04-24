-- 0005_add_api_key_revoked_reason.sql
-- Persist revoke reason separately from status/label metadata.

alter table api_keys
    add column if not exists revoked_reason text;

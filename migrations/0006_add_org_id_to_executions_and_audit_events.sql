alter table executions
    add column if not exists org_id uuid references organizations(org_id);

create index if not exists idx_executions_org_id on executions(org_id);

alter table audit_events
    add column if not exists org_id uuid references organizations(org_id);

create index if not exists idx_audit_events_org_id on audit_events(org_id);
create index if not exists idx_audit_events_trace_org_id on audit_events(trace_id, org_id);

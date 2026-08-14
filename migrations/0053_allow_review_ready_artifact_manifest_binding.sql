-- Admit the immutable Review-ready artifact manifest projection without
-- rewriting the deployed collaboration-kernel migration 0033.  Both binding
-- schemas are product authorities; no other spelling is accepted.

begin;

alter table public.hepta_artifact_manifests
    drop constraint if exists hepta_artifact_manifests_binding_schema_check;

alter table public.hepta_artifact_manifests
    add constraint hepta_artifact_manifests_binding_schema_check
    check (
        binding_schema = 'hepta.paper_raid.artifact_manifest_binding.v1'
        or binding_schema = 'hepta.paper_raid.review_ready_artifact_manifest_binding.v1'
    );

commit;

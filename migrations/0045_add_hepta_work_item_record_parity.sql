begin;

alter table hepta_paper_work_items
    drop constraint if exists hepta_paper_work_items_record_json_parity_check;

alter table hepta_paper_work_items
    add constraint hepta_paper_work_items_record_json_parity_check
    check (
        (record_json->>'work_item_id') is not distinct from work_item_id::text
        and (record_json->>'paper_project_id') is not distinct from paper_project_id::text
        and (record_json->>'assigned_player_id') is not distinct from assigned_player_id::text
        and (record_json->>'assigned_binding_id') is not distinct from assigned_binding_id::text
        and (record_json->>'status') is not distinct from status
        and (record_json->>'version') is not distinct from version::text
    ) not valid;

alter table hepta_paper_work_items
    validate constraint hepta_paper_work_items_record_json_parity_check;

commit;

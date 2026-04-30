use super::{
    authorize_user_session, build_chat_identity_scope, build_chat_org_rate_limit_key,
    build_chat_rate_limit_key, build_chat_replay_key, build_chat_request_fingerprint,
    build_chat_room_rate_limit_key, build_chat_session_rate_limit_key,
    build_chat_user_rate_limit_key, build_matrix_identity_scope, build_matrix_org_rate_limit_key,
    build_matrix_rate_limit_key, build_matrix_replay_key, build_matrix_room_rate_limit_key,
    build_matrix_session_rate_limit_key, build_matrix_user_rate_limit_key, build_router,
    client_app_json, client_feed_json, default_league_state,
    evaluate_identity_binding_reload_governance, get_client_app_web_shell, get_world_web_shell,
    league_state_hash, league_state_repository_write_set_for_command,
    league_state_sql_cutover_plan_json, league_state_sql_shadow_validation_json,
    load_identity_binding_revision_approval_state, load_identity_binding_store,
    load_rate_limit_cache, load_session_auth_issuer_registry,
    load_session_auth_issuer_registry_revision_approval_state,
    normalized_repository_client_feed_read_model_sql, normalized_repository_command_shadow_sql,
    normalized_repository_direct_write_contract_json,
    normalized_repository_read_model_contract_json,
    normalized_repository_world_home_read_model_sql, normalized_world_shadow_sql_contract_json,
    parse_csv_list, project_consumer_status, prune_rate_limit_cache, real_world_map_engine_json,
    resolve_chat_identity, session_auth_issuer_registry_active_key_diff_json,
    sign_user_session_assertion, validate_text_payload, world_home_json, world_map_json,
    world_map_viewport_json, world_route_ui_contract_json, AppState, AppStateInner,
    ConsumerEntryConfig, ConsumerEntryMetrics, CreateChatTaskRequest, IdentityBindingAuditState,
    IdentityBindingEntry, IdentityBindingMetadata, IdentityBindingRevisionApprovalState,
    IdentityBindingStore, IdentityBindings, LeagueStateRepositorySnapshot, MatrixMessageRequest,
    ProductUserIdentity, RateLimitCache, ReplayCache, RuntimeProfile,
    SessionAuthIssuerRegistryIssuer, SessionAuthIssuerRegistryMetadata,
    SessionAuthIssuerRegistryRuntimeState, UserSessionAuthClaims, WorldContract,
    WorldContractCompletion, WorldEconomyEvent, WorldEvent, WorldMapNode, WorldPlayerPosition,
    WorldRelationship, DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS, DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS,
    DEFAULT_MAX_TEXT_CHARS, TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR, USER_SESSION_ASSERTION_HEADER,
    USER_SESSION_SIGNATURE_HEADER, WORLD_ROUTE_ACTION_TEXTAREA_ID, WORLD_ROUTE_CONTRACTS_PANEL_ID,
    WORLD_ROUTE_CONTRACT_INPUT_ID, WORLD_ROUTE_WORK_DELIVER_TEXTAREA_ID,
};
use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request, StatusCode},
};
use base64::Engine as _;
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, RwLock as StdRwLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock};
use tower::ServiceExt;

#[test]
fn projects_core_runtime_states() {
    assert_eq!(project_consumer_status("Created"), "received");
    assert_eq!(project_consumer_status("Queued"), "queued");
    assert_eq!(
        project_consumer_status("AwaitingApproval"),
        "waiting_for_confirmation"
    );
    assert_eq!(project_consumer_status("Running"), "processing");
    assert_eq!(project_consumer_status("Succeeded"), "done");
    assert_eq!(project_consumer_status("Failed"), "failed");
    assert_eq!(project_consumer_status("Refunded"), "refunded");
}

#[test]
fn validate_text_payload_rejects_empty_and_large_input() {
    assert!(validate_text_payload("   ", DEFAULT_MAX_TEXT_CHARS).is_err());
    assert!(validate_text_payload(
        &"x".repeat(DEFAULT_MAX_TEXT_CHARS + 1),
        DEFAULT_MAX_TEXT_CHARS
    )
    .is_err());
    assert_eq!(
        validate_text_payload("  hello  ", DEFAULT_MAX_TEXT_CHARS).unwrap(),
        "hello"
    );
}

#[test]
fn builds_stable_rate_limit_keys() {
    let chat = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: Some("req-1".to_string()),
        metadata: None,
    };
    assert_eq!(build_chat_rate_limit_key(&chat), "chat:user-1:room-1");

    let matrix = MatrixMessageRequest {
        matrix_user_id: "@alice:local.dev".to_string(),
        room_id: "!room:local.dev".to_string(),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        message: "hello".to_string(),
        capability_id: None,
        account_id: None,
        event_id: None,
        idempotency_key: Some("msg-1".to_string()),
        metadata: None,
    };
    assert_eq!(
        build_matrix_rate_limit_key(&matrix),
        "matrix:@alice:local.dev:!room:local.dev"
    );
    assert_eq!(
        build_chat_replay_key(&chat).as_deref(),
        Some("chat:user-1:room-1:req-1")
    );
    assert_eq!(
        build_chat_user_rate_limit_key(&chat).as_deref(),
        Some("chat-user:user-1")
    );
    assert_eq!(
        build_chat_room_rate_limit_key(&chat).as_deref(),
        Some("chat-room:room-1")
    );
    assert_eq!(
        build_chat_session_rate_limit_key(&chat).as_deref(),
        Some("chat-session:session-1")
    );
    assert_eq!(
        build_chat_org_rate_limit_key(&chat).as_deref(),
        Some("chat-org:org-1")
    );
    assert_eq!(
        build_matrix_replay_key(&matrix).as_deref(),
        Some("matrix:@alice:local.dev:!room:local.dev:msg-1")
    );
    assert_eq!(
        build_matrix_user_rate_limit_key(&matrix),
        "matrix-user:@alice:local.dev"
    );
    assert_eq!(
        build_matrix_room_rate_limit_key(&matrix),
        "matrix-room:!room:local.dev"
    );
    assert_eq!(
        build_matrix_session_rate_limit_key(&matrix).as_deref(),
        Some("matrix-session:session-1")
    );
    assert_eq!(
        build_matrix_org_rate_limit_key(&matrix).as_deref(),
        Some("matrix-org:org-1")
    );

    let chat_scope = build_chat_identity_scope(&chat);
    assert_eq!(chat_scope.source_kind, "chat_task");
    assert_eq!(chat_scope.org_id.as_deref(), Some("org-1"));

    let matrix_scope = build_matrix_identity_scope(&matrix);
    assert_eq!(matrix_scope.source_kind, "matrix_message");
    assert_eq!(matrix_scope.user_id.as_deref(), Some("@alice:local.dev"));
    assert_eq!(matrix_scope.org_id.as_deref(), Some("org-1"));
}

#[test]
fn matrix_event_id_wins_over_generic_idempotency_key() {
    let matrix = MatrixMessageRequest {
        matrix_user_id: "@alice:local.dev".to_string(),
        room_id: "!room:local.dev".to_string(),
        session_id: None,
        org_id: None,
        message: "hello".to_string(),
        capability_id: None,
        account_id: None,
        event_id: Some("$event-123".to_string()),
        idempotency_key: Some("msg-1".to_string()),
        metadata: None,
    };

    assert_eq!(
        build_matrix_replay_key(&matrix).as_deref(),
        Some("matrix-event:$event-123")
    );
}

#[test]
fn league_sql_cutover_plan_exposes_normalized_world_tables() {
    let league = default_league_state();
    let hash = league_state_hash(&league).unwrap();
    let plan = league_state_sql_cutover_plan_json(&league, &hash, "test-generated-at");
    assert_eq!(
        plan.get("next_repository").and_then(Value::as_str),
        Some("normalized_sql_dual_write")
    );
    assert_eq!(
        plan.get("schema").and_then(Value::as_str),
        Some("trillionnium_normalized_world_v1")
    );
    assert_eq!(
        plan.get("migration_floor").and_then(Value::as_str),
        Some(TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR)
    );
    let repository_contract = plan
        .get("repository_contract")
        .expect("cutover plan should expose repository contract");
    assert_eq!(
        repository_contract
            .get("contract_version")
            .and_then(Value::as_str),
        Some("trillionnium_repository_cutover_v1")
    );
    let dual_write_plan = repository_contract
        .get("dual_write_plan")
        .expect("repository contract should expose dual-write plan");
    assert_eq!(
        dual_write_plan.get("plan_version").and_then(Value::as_str),
        Some("trillionnium_repository_dual_write_plan_v1")
    );
    let read_switch_requirements = dual_write_plan
        .get("read_switch_requirements")
        .and_then(Value::as_array)
        .expect("dual-write plan should expose read-switch requirements");
    assert!(read_switch_requirements
        .iter()
        .any(|gate| gate == "repository_write_set_audit_green"));
    assert!(read_switch_requirements
        .iter()
        .any(|gate| gate == "normalized_runtime_dual_write_gate_green"));
    assert!(read_switch_requirements
        .iter()
        .any(|gate| gate == "normalized_runtime_read_switch_gate_green"));
    assert!(read_switch_requirements
        .iter()
        .any(|gate| gate == "normalized_world_home_read_model_green"));
    assert!(read_switch_requirements
        .iter()
        .any(|gate| gate == "normalized_client_feed_read_model_green"));
    assert!(dual_write_plan
        .get("write_sets")
        .and_then(Value::as_array)
        .is_some_and(|write_sets| write_sets.iter().any(|write_set| {
            write_set.get("command").and_then(Value::as_str) == Some("world_work_accept")
                && write_set
                    .get("tables")
                    .and_then(Value::as_array)
                    .is_some_and(|tables| {
                        tables
                            .iter()
                            .any(|table| table.as_str() == Some("world_work_acceptances"))
                    })
        })));
    assert!(dual_write_plan
        .get("write_sets")
        .and_then(Value::as_array)
        .is_some_and(|write_sets| write_sets.iter().any(|write_set| {
            write_set.get("command").and_then(Value::as_str) == Some("world_map_move")
                && write_set
                    .get("tables")
                    .and_then(Value::as_array)
                    .is_some_and(|tables| {
                        tables
                            .iter()
                            .any(|table| table.as_str() == Some("world_player_positions"))
                            && tables
                                .iter()
                                .any(|table| table.as_str() == Some("world_economy_events"))
                    })
        })));
    assert!(dual_write_plan
        .get("write_sets")
        .and_then(Value::as_array)
        .is_some_and(|write_sets| write_sets.iter().any(|write_set| {
            write_set.get("command").and_then(Value::as_str) == Some("world_work_deliver")
                && write_set
                    .get("tables")
                    .and_then(Value::as_array)
                    .is_some_and(|tables| {
                        tables
                            .iter()
                            .any(|table| table.as_str() == Some("world_work_deliveries"))
                            && tables
                                .iter()
                                .any(|table| table.as_str() == Some("world_economy_events"))
                            && tables
                                .iter()
                                .any(|table| table.as_str() == Some("world_faction_standings"))
                    })
        })));
    let write_set_audit = repository_contract
        .get("write_set_audit")
        .expect("repository contract should expose write-set audit contract");
    assert_eq!(
        write_set_audit.get("audit_version").and_then(Value::as_str),
        Some("trillionnium_repository_write_set_audit_v1")
    );
    assert_eq!(
        write_set_audit
            .get("write_set_count")
            .and_then(Value::as_u64),
        Some(12)
    );
    assert_eq!(
        repository_contract
            .get("state_boundary")
            .and_then(|boundary| boundary.get("world_state_owner"))
            .and_then(Value::as_str),
        Some("WorldState")
    );
    assert!(repository_contract
        .get("state_boundary")
        .and_then(|boundary| boundary.get("runtime_dual_write_seam"))
        .and_then(Value::as_str)
        .is_some_and(|seam| seam.contains("persist_league_state")));
    assert!(repository_contract
        .get("state_boundary")
        .and_then(|boundary| boundary.get("runtime_read_switch_seam"))
        .and_then(Value::as_str)
        .is_some_and(|seam| seam.contains("league_state_snapshots")
            && seam.contains("normalized world-home read-model")
            && seam.contains("normalized client-feed read-model")));
    assert!(repository_contract
        .get("state_boundary")
        .and_then(|boundary| boundary.get("runtime_command_write_sql_helper"))
        .and_then(Value::as_str)
        .is_some_and(|seam| seam.contains("normalized_repository_command_shadow_sql")));
    assert!(repository_contract
        .get("state_boundary")
        .and_then(|boundary| boundary.get("runtime_read_model_sql_helper"))
        .and_then(Value::as_str)
        .is_some_and(
            |seam| seam.contains("normalized_repository_world_home_read_model_sql")
                && seam.contains("normalized_repository_client_feed_read_model_sql")
        ));
    assert_eq!(
        repository_contract
            .get("read_model_contract")
            .and_then(|contract| contract.get("world_home"))
            .and_then(|world_home| world_home.get("read_model_version"))
            .and_then(Value::as_str),
        Some("trillionnium_normalized_world_home_read_model_v1")
    );
    assert_eq!(
        repository_contract
            .get("read_model_contract")
            .and_then(|contract| contract.get("world_home"))
            .and_then(|world_home| world_home.get("startup_gate"))
            .and_then(Value::as_str),
        Some("normalized_read_model_startup_gate_green")
    );
    assert_eq!(
        repository_contract
            .get("read_model_contract")
            .and_then(|contract| contract.get("client_feed"))
            .and_then(|client_feed| client_feed.get("read_model_version"))
            .and_then(Value::as_str),
        Some("trillionnium_normalized_client_feed_read_model_v1")
    );
    assert_eq!(
        repository_contract
            .get("read_model_contract")
            .and_then(|contract| contract.get("client_feed"))
            .and_then(|client_feed| client_feed.get("startup_gate"))
            .and_then(Value::as_str),
        Some("normalized_client_feed_read_model_startup_gate_green")
    );
    assert!(repository_contract
        .get("runtime_validation")
        .and_then(|runtime| runtime.get("checks"))
        .and_then(Value::as_array)
        .is_some_and(|checks| checks
            .iter()
            .any(|check| check == "verify_command_scoped_world_table_upserts")
            && checks
                .iter()
                .any(|check| check == "verify_normalized_world_home_read_model_sql")
            && checks
                .iter()
                .any(|check| check == "verify_normalized_client_feed_read_model_sql")));
    assert!(repository_contract
        .get("read_switch_gates")
        .and_then(Value::as_array)
        .is_some_and(|gates| {
            gates.iter().any(|gate| gate == "matrix_live_e2e_green")
                && gates.iter().any(|gate| gate == "repository_audit_green")
                && gates
                    .iter()
                    .any(|gate| gate == "repository_write_set_audit_green")
                && gates
                    .iter()
                    .any(|gate| gate == "normalized_read_model_startup_gate_green")
                && gates
                    .iter()
                    .any(|gate| gate == "normalized_client_feed_read_model_startup_gate_green")
                && gates.iter().any(|gate| gate == "sql_snapshot_gate_green")
                && gates
                    .iter()
                    .any(|gate| gate == "normalized_runtime_dual_write_gate_green")
                && gates
                    .iter()
                    .any(|gate| gate == "normalized_runtime_read_switch_gate_green")
                && gates
                    .iter()
                    .any(|gate| gate == "normalized_world_home_read_model_green")
                && gates
                    .iter()
                    .any(|gate| gate == "normalized_client_feed_read_model_green")
        }));
    assert_eq!(
        repository_contract
            .get("runtime_validation")
            .and_then(|validation| validation.get("script"))
            .and_then(Value::as_str),
        Some("scripts/check-trillionnium-league-normalized-runtime-dual-write.sh")
    );
    let tables = plan
        .get("tables")
        .and_then(Value::as_array)
        .expect("cutover plan should expose tables");
    assert!(tables.iter().any(|table| {
        table.get("table_name").and_then(Value::as_str) == Some("world_work_orders")
            && table.get("source_path").and_then(Value::as_str) == Some("world.world_work_orders")
            && table.get("row_count").and_then(Value::as_u64)
                == Some(league.world.world_work_orders.len() as u64)
    }));
    assert!(tables.iter().any(|table| {
        table.get("table_name").and_then(Value::as_str) == Some("world_contracts")
            && table.get("primary_key").and_then(Value::as_str) == Some("contract_id")
    }));
    let shadow_validation = league_state_sql_shadow_validation_json(&plan);
    assert_eq!(
        shadow_validation
            .get("validation_version")
            .and_then(Value::as_str),
        Some("trillionnium_sql_shadow_validation_v1")
    );
    assert!(shadow_validation
        .get("checks")
        .and_then(Value::as_array)
        .is_some_and(|checks| checks.iter().any(|check| {
            check.get("table_name").and_then(Value::as_str) == Some("world_contracts")
                && check.get("expected_rows").and_then(Value::as_u64)
                    == Some(league.world.world_contracts.len() as u64)
        })));
    assert!(shadow_validation
        .get("sql")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .contains("from world_contracts"));

    let repository_snapshot = LeagueStateRepositorySnapshot::from_league(&league).unwrap();
    let sql = String::from_utf8(repository_snapshot.sql_snapshot_bytes().unwrap()).unwrap();
    assert!(sql.contains("Normalized repository cutover plan"));
    assert!(sql.contains("SQL shadow validation"));
    assert!(sql.contains("Normalized WorldState shadow upserts"));
    assert!(sql.contains("insert into world_zones"));
    assert!(sql.contains("insert into world_map_nodes"));
    let shadow_sql_contract = normalized_world_shadow_sql_contract_json(
        repository_snapshot.normalized_world_shadow_sql.len(),
    );
    assert_eq!(
        shadow_sql_contract
            .get("contract_version")
            .and_then(Value::as_str),
        Some("trillionnium_normalized_world_shadow_sql_v1")
    );
    assert_eq!(
        shadow_sql_contract
            .get("index_layer")
            .and_then(Value::as_str),
        Some("WorldIndexes::normalized_shadow_sorted_ids_v1")
    );
    assert_eq!(
        shadow_sql_contract
            .get("sorted_vector_index_layer")
            .and_then(Value::as_str),
        Some("WorldIndexes::normalized_shadow_sorted_vector_indices_v1")
    );
    assert!(shadow_sql_contract
        .get("tables")
        .and_then(Value::as_array)
        .is_some_and(|tables| tables.iter().any(|table| table == "world_work_acceptances")));
    assert!(sql.contains("trillionnium_sql_shadow_validation_v1"));
    assert!(sql.contains("trillionnium_repository_dual_write_plan_v1"));
    assert!(sql.contains(TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR));
    assert!(sql.contains("league_state_repository_snapshots"));
    assert!(sql.contains("league_state_repository_write_set_audits"));
    assert!(sql.contains("on conflict (state_hash, cutover_phase)"));
    assert!(sql.contains("on conflict (state_hash, cutover_phase, command)"));
    assert!(sql.contains("trillionnium_repository_cutover_v1"));
    assert!(sql.contains("world_contracts"));
    assert!(sql.contains("normalized_sql_dual_write"));

    let repository_audit = repository_snapshot.repository_audit.json();
    assert_eq!(
        repository_audit
            .get("audit_version")
            .and_then(Value::as_str),
        Some("trillionnium_repository_cutover_audit_v1")
    );
    assert_eq!(
        repository_audit
            .get("cutover_phase")
            .and_then(Value::as_str),
        Some("final_cutover")
    );
    assert_eq!(
        repository_audit
            .get("next_repository")
            .and_then(Value::as_str),
        Some("normalized_sql_dual_write")
    );
    assert_eq!(
        repository_audit
            .get("dual_write_plan_version")
            .and_then(Value::as_str),
        Some("trillionnium_repository_dual_write_plan_v1")
    );
    let endpoint_json = repository_snapshot.endpoint_json(&league, true, true, true, true, true);
    assert_eq!(
        endpoint_json
            .get("normalized_repository")
            .and_then(|repository| repository.get("active"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        endpoint_json
            .get("normalized_repository")
            .and_then(|repository| repository.get("read_switch_active"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        endpoint_json
            .get("repository_cutover_audit")
            .and_then(|audit| audit.get("audit_version"))
            .and_then(Value::as_str),
        Some("trillionnium_repository_cutover_audit_v1")
    );
    assert_eq!(
        endpoint_json
            .get("repository_write_set_audit")
            .and_then(|audit| audit.get("audit_version"))
            .and_then(Value::as_str),
        Some("trillionnium_repository_write_set_audit_v1")
    );

    let repository_migration = std::fs::read_to_string(format!(
        "{}/../../migrations/{TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("repository write-set audit migration should exist");
    assert!(repository_migration.contains("league_state_repository_write_set_audits"));
    assert!(repository_migration.contains("write_set jsonb not null"));
    assert!(repository_migration.contains("unique(state_hash, cutover_phase, command)"));
}

#[test]
fn normalized_repository_command_shadow_sql_uses_write_set_tables() {
    let mut league = default_league_state();
    league.world.world_player_positions.insert(
        "@map-move:local.dev".to_string(),
        WorldPlayerPosition {
            matrix_user_id: "@map-move:local.dev".to_string(),
            node_id: "starter-studio".to_string(),
            location_id: "starter-studio".to_string(),
            updated_at_epoch: 1,
        },
    );
    league.world.world_economy_events.push(WorldEconomyEvent {
        economy_event_id: "world-econ-test-map-move".to_string(),
        matrix_user_id: "@map-move:local.dev".to_string(),
        event_kind: "map_move".to_string(),
        subject_id: "starter-studio".to_string(),
        credits_delta: 0,
        reputation_delta: 0,
        created_at_epoch: 1,
    });
    let write_set = league_state_repository_write_set_for_command("world_map_move")
        .expect("world_map_move write-set should be declared");
    let tables = write_set
        .get("tables")
        .and_then(Value::as_array)
        .expect("write-set should list normalized tables");
    assert!(tables
        .iter()
        .any(|table| table.as_str() == Some("world_player_positions")));
    assert!(tables
        .iter()
        .any(|table| table.as_str() == Some("world_economy_events")));

    let command_sql = normalized_repository_command_shadow_sql(&league.world, "world_map_move")
        .unwrap()
        .expect("world_map_move should produce command-scoped SQL");
    assert!(command_sql.contains("trillionnium_normalized_repository_command_shadow_sql_v1"));
    assert!(command_sql.contains("\"command\":\"world_map_move\""));
    assert!(command_sql.contains("insert into world_player_positions"));
    assert!(command_sql.contains("insert into world_economy_events"));
    assert!(command_sql.contains("insert into world_map_nodes"));
    assert!(command_sql.contains("\"dependency_world_tables\""));
    assert!(!command_sql.contains("insert into world_events"));

    league.world.world_events.push(WorldEvent {
        event_id: "world-event-command-smoke".to_string(),
        actor_matrix_user_id: "@runtime-dual:local.dev".to_string(),
        room_id: Some("!runtime-dual:local.dev".to_string()),
        location_id: "zbj-market-gate".to_string(),
        event_kind: "explore".to_string(),
        body: "Runtime dual-write smoke".to_string(),
        result: "recorded".to_string(),
        impact_score: 10,
        cex_task_id: None,
        cex_status: None,
        created_at_epoch: 1,
    });
    league.world.world_relationships.push(WorldRelationship {
        relationship_id: "world-rel-command-smoke".to_string(),
        from_id: "@runtime-dual:local.dev".to_string(),
        to_id: "zbj-market-gate".to_string(),
        relation_kind: "explore".to_string(),
        strength: 10,
        updated_at_epoch: 1,
    });
    let action_sql = normalized_repository_command_shadow_sql(&league.world, "world_action")
        .unwrap()
        .expect("world_action should produce command-scoped SQL");
    assert!(action_sql.contains("insert into world_zones"));
    assert!(action_sql.contains("insert into world_locations"));
    assert!(action_sql.contains("insert into world_events"));
    assert!(action_sql.contains("insert into world_relationships"));
    assert!(!action_sql.contains("insert into world_map_nodes"));
    assert!(
        normalized_repository_command_shadow_sql(&league.world, "unknown_command")
            .unwrap()
            .is_none()
    );
}

#[test]
fn normalized_repository_runtime_dual_write_scopes_world_tables_to_command() {
    let mut league = default_league_state();
    league.world.world_player_positions.insert(
        "@map-move:local.dev".to_string(),
        WorldPlayerPosition {
            matrix_user_id: "@map-move:local.dev".to_string(),
            node_id: "starter-studio".to_string(),
            location_id: "starter-studio".to_string(),
            updated_at_epoch: 1,
        },
    );
    league.world.world_economy_events.push(WorldEconomyEvent {
        economy_event_id: "world-econ-test-map-move".to_string(),
        matrix_user_id: "@map-move:local.dev".to_string(),
        event_kind: "map_move".to_string(),
        subject_id: "starter-studio".to_string(),
        credits_delta: 0,
        reputation_delta: 0,
        created_at_epoch: 1,
    });
    let repository_snapshot = LeagueStateRepositorySnapshot::from_league(&league).unwrap();
    let full_sql = String::from_utf8(repository_snapshot.sql_snapshot_bytes().unwrap()).unwrap();
    assert!(full_sql.contains("insert into world_map_nodes"));

    let runtime_sql = String::from_utf8(
        repository_snapshot
            .runtime_dual_write_sql_bytes(Some("world_map_move"))
            .unwrap(),
    )
    .unwrap();
    assert!(runtime_sql.contains("insert into league_state_snapshots"));
    assert!(runtime_sql.contains("league_state_repository_write_set_audits"));
    assert!(runtime_sql.contains("trillionnium_normalized_repository_command_shadow_sql_v1"));
    assert!(runtime_sql.contains("insert into world_player_positions"));
    assert!(runtime_sql.contains("insert into world_economy_events"));
    assert!(runtime_sql.contains("insert into world_map_nodes"));
    assert!(runtime_sql.contains("\"dependency_world_tables\""));
    assert!(!runtime_sql.contains("insert into world_events"));
}

#[test]
fn normalized_repository_direct_write_contract_declares_command_helpers() {
    let contract = normalized_repository_direct_write_contract_json();
    assert_eq!(
        contract.get("contract_version").and_then(Value::as_str),
        Some("trillionnium_normalized_repository_direct_write_v1")
    );
    assert_eq!(
        contract.get("runtime_helper").and_then(Value::as_str),
        Some("execute_normalized_repository_direct_command_write")
    );
    assert_eq!(
        contract.get("transaction_mode").and_then(Value::as_str),
        Some("single_pg_transaction_direct_sql_primary_plus_snapshot_export")
    );
    assert!(contract
        .get("transaction_boundary")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .contains("direct typed SQLx upserts first"));
    let supported_commands = contract
        .get("supported_commands")
        .and_then(Value::as_array)
        .expect("direct write contract should list supported commands");
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_action")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_contract_completion")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_map_move")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_asset_upgrade")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_company")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_listing")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_buy")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_work_deliver")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_work_accept")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_work_reject")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_work_reopen")));
    assert!(supported_commands
        .iter()
        .any(|command| command.as_str() == Some("world_work_cancel")));
    assert_eq!(
        contract.get("fallback_helper").and_then(Value::as_str),
        Some("snapshot_export_only")
    );

    let repository_contract = crate::league_state_repository_contract_json();
    assert_eq!(
        repository_contract
            .get("direct_write_contract")
            .and_then(|direct| direct.get("contract_version"))
            .and_then(Value::as_str),
        Some("trillionnium_normalized_repository_direct_write_v1")
    );
    assert!(repository_contract
        .get("direct_write_contract")
        .and_then(|direct| direct.get("index_reuse"))
        .and_then(Value::as_str)
        .is_some_and(|index_reuse| index_reuse.contains("one WorldIndexes snapshot")));
}

#[test]
fn normalized_repository_direct_write_audit_bridge_omits_generated_world_upserts() {
    let mut league = default_league_state();
    league.world.world_events.push(WorldEvent {
        event_id: "world-event-direct-helper-smoke".to_string(),
        actor_matrix_user_id: "@direct-helper:local.dev".to_string(),
        room_id: Some("!direct-helper:local.dev".to_string()),
        location_id: "zbj-market-gate".to_string(),
        event_kind: "explore".to_string(),
        body: "Direct helper smoke".to_string(),
        result: "recorded".to_string(),
        impact_score: 10,
        cex_task_id: None,
        cex_status: None,
        created_at_epoch: 1,
    });
    let repository_snapshot = LeagueStateRepositorySnapshot::from_league(&league).unwrap();
    let bridge_sql = String::from_utf8(
        repository_snapshot
            .runtime_audit_bridge_sql_bytes(Some("world_action"))
            .unwrap(),
    )
    .unwrap();
    assert!(bridge_sql.contains("insert into league_state_snapshots"));
    assert!(bridge_sql.contains("league_state_repository_write_set_audits"));
    assert!(bridge_sql.contains("execute_normalized_repository_direct_command_write"));
    assert!(!bridge_sql.contains("insert into world_events"));
    assert!(!bridge_sql.contains("insert into world_map_nodes"));
}

#[test]
fn normalized_repository_world_home_read_model_declares_direct_sql_seam() {
    let read_model_sql = normalized_repository_world_home_read_model_sql();
    assert!(read_model_sql.contains("trillionnium_normalized_world_home_read_model_v1"));
    assert!(read_model_sql.contains("from world_events"));
    assert!(read_model_sql.contains("from world_relationships"));
    assert!(read_model_sql.contains("from world_map_nodes"));
    assert!(read_model_sql.contains("latest_event_ids"));
    let contract = normalized_repository_read_model_contract_json();
    assert_eq!(
        contract.get("contract_version").and_then(Value::as_str),
        Some("trillionnium_normalized_repository_read_model_v1")
    );
    assert_eq!(
        contract
            .get("world_home")
            .and_then(|world_home| world_home.get("sql_helper"))
            .and_then(Value::as_str),
        Some("normalized_repository_world_home_read_model_sql")
    );
    assert_eq!(
        contract
            .get("world_home")
            .and_then(|world_home| world_home.get("startup_gate"))
            .and_then(Value::as_str),
        Some("normalized_read_model_startup_gate_green")
    );
    let feed_read_model_sql = normalized_repository_client_feed_read_model_sql();
    assert!(feed_read_model_sql.contains("trillionnium_normalized_client_feed_read_model_v1"));
    assert!(feed_read_model_sql.contains("from world_purchases"));
    assert!(feed_read_model_sql.contains("from world_work_orders"));
    assert!(feed_read_model_sql.contains("from world_work_acceptances"));
    assert!(feed_read_model_sql.contains("latest_feed_items"));
    assert_eq!(
        contract
            .get("client_feed")
            .and_then(|client_feed| client_feed.get("sql_helper"))
            .and_then(Value::as_str),
        Some("normalized_repository_client_feed_read_model_sql")
    );
    assert_eq!(
        contract
            .get("client_feed")
            .and_then(|client_feed| client_feed.get("startup_gate"))
            .and_then(Value::as_str),
        Some("normalized_client_feed_read_model_startup_gate_green")
    );
}

#[test]
fn normalized_repository_runtime_dual_write_unknown_command_is_audit_only() {
    let league = default_league_state();
    let repository_snapshot = LeagueStateRepositorySnapshot::from_league(&league).unwrap();
    let runtime_sql = String::from_utf8(
        repository_snapshot
            .runtime_dual_write_sql_bytes(Some("unknown_command"))
            .unwrap(),
    )
    .unwrap();
    assert!(runtime_sql.contains("insert into league_state_snapshots"));
    assert!(runtime_sql.contains("league_state_repository_write_set_audits"));
    assert!(runtime_sql.contains("unknown write-set command=unknown_command"));
    assert!(!runtime_sql.contains("insert into world_map_nodes"));
    assert!(!runtime_sql.contains("insert into world_events"));
}

fn test_config() -> ConsumerEntryConfig {
    ConsumerEntryConfig {
        runtime_profile: RuntimeProfile::LocalDev,
        bind_addr: "127.0.0.1:8090".to_string(),
        cex_gateway_base_url: "http://127.0.0.1:8080".to_string(),
        cex_gateway_api_key: "local-dev-key".to_string(),
        ledger_base_url: "http://127.0.0.1:7002".to_string(),
        ledger_admin_token: None,
        default_capability_id: None,
        default_account_id: None,
        ingress_token: None,
        require_session_auth: false,
        session_auth_secret: None,
        session_auth_issuer_secrets: HashMap::new(),
        session_auth_issuer_keys: HashMap::new(),
        session_auth_issuer_registry_path: None,
        session_auth_issuer_registry: HashMap::new(),
        session_auth_issuer_registry_load_error: None,
        session_auth_issuer_registry_metadata: SessionAuthIssuerRegistryMetadata::default(),
        session_auth_allowed_issuers: Vec::new(),
        session_auth_expected_audience: None,
        session_auth_issuer_registry_approved_revisions_path: None,
        session_auth_issuer_registry_require_approved_revision: false,
        session_auth_issuer_registry_require_actor: false,
        session_auth_issuer_registry_actor_header: "x-session-auth-issuer-registry-actor"
            .to_string(),
        session_auth_issuer_registry_allowed_actors: Vec::new(),
        session_auth_max_clock_skew_secs: 300,
        session_auth_max_ttl_secs: 900,
        identity_bindings_path: None,
        identity_registry_path: None,
        identity_binding_audit_log_path: None,
        identity_binding_approved_revisions_path: None,
        identity_binding_reload_require_revision: false,
        identity_binding_reload_reject_same_revision: false,
        identity_binding_reload_allow_legacy_format: true,
        identity_binding_reload_require_approved_revision: false,
        identity_binding_reload_allow_rollback: true,
        identity_binding_reload_require_actor: false,
        identity_binding_reload_actor_header: "x-identity-binding-actor".to_string(),
        identity_binding_reload_allowed_actors: Vec::new(),
        require_identity_binding: false,
        max_text_chars: DEFAULT_MAX_TEXT_CHARS,
        rate_limit_window_secs: 60,
        rate_limit_max_requests: 30,
        rate_limit_user_max_requests: 0,
        rate_limit_room_max_requests: 0,
        rate_limit_session_max_requests: 0,
        rate_limit_org_max_requests: 0,
        rate_limit_store_path: None,
        replay_window_secs: 600,
        replay_cache_size: 2048,
        replay_store_path: None,
        league_state_path: None,
        league_sql_snapshot_path: None,
        league_normalized_database_url: None,
        league_normalized_dual_write_enabled: false,
        league_normalized_read_switch_enabled: false,
        league_normalized_final_cutover_enabled: false,
        league_hidden_tests_enabled: true,
        league_llm_judge_url: None,
        league_llm_judge_token: None,
        league_llm_judge_required: false,
        league_llm_judge_timeout_ms: DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS,
        league_web_session_required: false,
        league_web_session_secret: None,
        league_web_session_cookie_name: "cex_league_session".to_string(),
        league_web_session_ttl_secs: DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS,
    }
}

fn test_state(
    config: ConsumerEntryConfig,
    identity_bindings: IdentityBindings,
    product_users: HashMap<String, ProductUserIdentity>,
) -> AppState {
    let session_auth_issuer_registry_state = SessionAuthIssuerRegistryRuntimeState {
        metadata: config.session_auth_issuer_registry_metadata.clone(),
        registry: config.session_auth_issuer_registry.clone(),
    };
    AppState {
        inner: Arc::new(AppStateInner {
            http: Client::new(),
            config,
            identity_binding_store: RwLock::new(IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "test".to_string(),
                    version: 1,
                    revision: Some("rev-test".to_string()),
                    source_path: None,
                    source_modified_epoch: None,
                    loaded_at_epoch: Some(1_760_000_000),
                    load_status: "loaded".to_string(),
                    load_error: None,
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: identity_bindings,
                product_users,
            }),
            identity_binding_audit_state: RwLock::new(IdentityBindingAuditState::default()),
            session_auth_issuer_registry_state: StdRwLock::new(session_auth_issuer_registry_state),
            league_state: Mutex::new(default_league_state()),
            rate_limits: Mutex::new(RateLimitCache::default()),
            replay_cache: Mutex::new(ReplayCache::default()),
            metrics: ConsumerEntryMetrics::default(),
        }),
    }
}

#[test]
fn world_map_viewport_includes_prefetch_density_and_live_events() {
    let mut league = default_league_state();
    let starter_location_id = league
        .world
        .world_map_nodes
        .get("starter-studio")
        .map(|node| node.location_id.clone())
        .unwrap_or_else(|| "starter-studio".to_string());
    league.world.world_events.push(WorldEvent {
        event_id: "world-event-test-1".to_string(),
        actor_matrix_user_id: "@alice:local.dev".to_string(),
        room_id: Some("!test:local.dev".to_string()),
        location_id: starter_location_id,
        event_kind: "world_contract".to_string(),
        body: "Test viewport stream event".to_string(),
        result: "queued".to_string(),
        impact_score: 7,
        cex_task_id: Some("task-viewport-1".to_string()),
        cex_status: Some("Queued".to_string()),
        created_at_epoch: 1_777_230_001,
    });

    let viewport = world_map_viewport_json(
        &league.world,
        "@alice:local.dev",
        None,
        None,
        Some(15),
        None,
        None,
    );

    assert_eq!(viewport["active_region"]["status"], "active");
    assert!(viewport["stream_region_count"].as_u64().unwrap_or(0) >= 1);
    assert!(viewport["tile_shard_count"].as_u64().unwrap_or(0) >= 1);
    assert!(viewport["prefetch_count"].as_u64().unwrap_or(0) >= 1);
    assert!(viewport["live_event_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(viewport["player_density"]["mode"], "dense");
    assert_eq!(
        viewport["live_event_stream_index_layer"],
        "WorldIndexes::event_indices_by_location_v1"
    );
    assert_eq!(
        viewport["live_event_stream"][0]["event_kind"],
        "world_contract"
    );
}

#[test]
fn world_client_surfaces_expose_projection_layer_contracts() {
    let league = default_league_state();
    let matrix_user_id = "@alice:local.dev";

    let home = world_home_json(&league);
    assert_eq!(home["projection_layer"], "world_home_projection_v1");
    assert_eq!(home["projection_context"], "WorldHomeProjectionContext");
    assert_eq!(
        home["index_layer"],
        "WorldIndexes::world_home_sorted_ids_v1"
    );

    let map = world_map_json(&league, matrix_user_id);
    assert_eq!(map["projection_layer"], "world_map_projection_v1");
    assert_eq!(map["projection_context"], "WorldMapProjectionContext");
    assert_eq!(map["index_layer"], "WorldIndexes::sorted_map_node_ids_v1");
    assert_eq!(
        map["route_preview"]["projection_layer"],
        "world_route_projection_v1"
    );
    assert_eq!(
        map["route_preview"]["index_layer"],
        "WorldIndexes::recent_route_indices_v1"
    );
    assert_eq!(
        map["route_task_graph"]["projection_layer"],
        "world_route_task_graph_projection_v1"
    );

    let feed = client_feed_json(&league, matrix_user_id);
    assert_eq!(feed["projection_layer"], "client_feed_projection_v1");
    assert_eq!(feed["projection_context"], "ClientFeedProjectionContext");
    assert_eq!(
        feed["index_layer"],
        "WorldIndexes::client_feed_recent_indices_v1"
    );

    let app = client_app_json(&league, matrix_user_id);
    assert_eq!(app["projection_layer"], "client_app_projection_v1");
    assert_eq!(app["projection_context"], "ClientAppProjectionContext");
    assert_eq!(
        app["index_layer"],
        "WorldIndexes::client_app_sorted_entities_v1"
    );
    assert_eq!(
        app["map"]["projection_context"],
        "WorldMapProjectionContext"
    );
    assert_eq!(
        app["feed"]["projection_context"],
        "ClientFeedProjectionContext"
    );
    assert_eq!(
        app["onboarding"]["contract_version"],
        "trillionnium_first_playable_onboarding_v1"
    );
    assert_eq!(
        app["onboarding"]["completion_target"],
        "first_playable_loop_100"
    );
    assert!(app["onboarding"]["steps"].as_array().unwrap().len() >= 5);
    assert!(app["onboarding"]["acceptance_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check == "route_task_graph_next_action_visible"));
    assert!(app["onboarding"]["beta_readiness_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check == "matrix_app_card_exposes_onboarding"));
}

#[test]
fn real_world_map_engine_declares_shared_renderer_adapter() {
    let league = default_league_state();
    let nodes: Vec<WorldMapNode> = league.world.world_map_nodes.values().cloned().collect();
    let engine = real_world_map_engine_json(&nodes, None);
    assert_eq!(
        engine["renderer_adapter"]["adapter_id"],
        "leaflet_renderer_adapter_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["active_engine_id"],
        "leaflet_openstreetmap_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["runtime_handle_name"],
        "mapRuntime"
    );
    assert_eq!(
        engine["planned_upgrade_engine"]["engine_id"],
        "maplibre_gl_v1"
    );
    assert_eq!(
        engine["planned_upgrade_engine"]["gating_contract"],
        "renderer_adapter.adapter_contract_version >= 1"
    );
    assert_eq!(
        engine["renderer_adapter"]["adapter_contract"]["supports_future_engine_swap"],
        true
    );
    let adapter_methods = engine["renderer_adapter"]["adapter_methods"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(adapter_methods.iter().any(|method| method == "createMap"));
    assert!(adapter_methods
        .iter()
        .any(|method| method == "setOverlayVisibility"));
    assert!(adapter_methods
        .iter()
        .any(|method| method == "renderRouteLine"));
    assert!(adapter_methods
        .iter()
        .any(|method| method == "renderTileFrame"));
    assert!(adapter_methods
        .iter()
        .any(|method| method == "renderEventPulse"));
    assert!(adapter_methods.iter().any(|method| method == "getCenter"));
    assert!(adapter_methods.iter().any(|method| method == "getZoom"));
    assert!(adapter_methods
        .iter()
        .any(|method| method == "onViewportChange"));
    assert!(adapter_methods.iter().any(|method| method == "focus"));
}

#[test]
fn world_home_json_exposes_shared_renderer_adapter_for_matrix_cards() {
    let league = default_league_state();
    let home = world_home_json(&league);
    let engine = &home["real_world_map_engine"];

    assert_eq!(engine["engine_id"], "leaflet_openstreetmap_v1");
    assert_eq!(engine["tile_provider"], "OpenStreetMap");
    assert_eq!(
        engine["renderer_adapter"]["adapter_id"],
        "leaflet_renderer_adapter_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["runtime_handle_name"],
        "mapRuntime"
    );
    assert_eq!(
        engine["renderer_adapter"]["future_engine_candidate"],
        "maplibre_gl_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["adapter_contract"]["supports_future_engine_swap"],
        true
    );
    assert_eq!(
        engine["planned_upgrade_engine"]["gating_contract"],
        "renderer_adapter.adapter_contract_version >= 1"
    );
}

#[test]
fn route_contract_is_shared_across_world_map_app_and_feed_surfaces() {
    let league = default_league_state();
    let expected = world_route_ui_contract_json();
    let home = world_home_json(&league);
    let map = world_map_json(&league, "@alice:local.dev");
    let app = client_app_json(&league, "@alice:local.dev");
    let feed = client_feed_json(&league, "@alice:local.dev");

    assert_eq!(home["route_contract"], expected);
    assert_eq!(map["route_contract"], expected);
    assert_eq!(app["route_contract"], expected);
    assert_eq!(app["map"]["route_contract"], expected);
    assert_eq!(app["feed"]["route_contract"], expected);
    assert_eq!(app["map_hub"]["route_contract"], expected);
    assert_eq!(feed["route_contract"], expected);
    assert_eq!(
        expected["fields"]["action_textarea"],
        WORLD_ROUTE_ACTION_TEXTAREA_ID
    );
    assert_eq!(
        expected["panel_defaults"][WORLD_ROUTE_CONTRACTS_PANEL_ID]["input_id"],
        WORLD_ROUTE_CONTRACT_INPUT_ID
    );
    assert_eq!(
        expected["work_lanes"]["delivery"]["textarea_id"],
        WORLD_ROUTE_WORK_DELIVER_TEXTAREA_ID
    );
    assert_eq!(
        expected["handoff"]["storage_key"],
        "trillionnium-world-handoff"
    );
    assert_eq!(expected["handoff"]["panel_id"], "web_panel_id");
}

#[test]
fn client_app_map_hub_projects_stream_counts() {
    let mut league = default_league_state();
    let starter_location_id = league
        .world
        .world_map_nodes
        .get("starter-studio")
        .map(|node| node.location_id.clone())
        .unwrap_or_else(|| "starter-studio".to_string());
    league.world.world_events.push(WorldEvent {
        event_id: "world-event-test-2".to_string(),
        actor_matrix_user_id: "@alice:local.dev".to_string(),
        room_id: Some("!test:local.dev".to_string()),
        location_id: starter_location_id,
        event_kind: "listing_purchase".to_string(),
        body: "Test app stream event".to_string(),
        result: "settled".to_string(),
        impact_score: 9,
        cex_task_id: Some("task-app-1".to_string()),
        cex_status: Some("Succeeded".to_string()),
        created_at_epoch: 1_777_230_002,
    });

    let app = client_app_json(&league, "@alice:local.dev");
    assert!(app["map_hub"]["prefetch_count"].as_u64().unwrap_or(0) >= 1);
    assert!(app["map_hub"]["live_event_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(app["map_hub"]["player_density_mode"], "dense");
    assert_eq!(
        app["feed"]["active_region_id"],
        app["map_hub"]["active_region_id"]
    );
    assert_eq!(
        app["feed"]["route_task_graph"]["task_count"],
        app["map_hub"]["route_task_graph"]["task_count"]
    );
    assert!(app["modules"][0]["summary"]
        .as_str()
        .unwrap_or_default()
        .contains("live events"));
}

#[tokio::test]
async fn web_map_shells_render_live_event_task_focus_metadata() {
    let mut league = default_league_state();
    let starter_location_id = league
        .world
        .world_map_nodes
        .get("starter-studio")
        .map(|node| node.location_id.clone())
        .unwrap_or_else(|| "starter-studio".to_string());
    league.world.world_events.push(WorldEvent {
        event_id: "world-event-web-focus-1".to_string(),
        actor_matrix_user_id: "@alice:local.dev".to_string(),
        room_id: Some("!test:local.dev".to_string()),
        location_id: starter_location_id,
        event_kind: "world_contract".to_string(),
        body: "Web shell live event focus metadata".to_string(),
        result: "queued".to_string(),
        impact_score: 8,
        cex_task_id: Some("task-web-event-focus-1".to_string()),
        cex_status: Some("Queued".to_string()),
        created_at_epoch: 1_777_230_150,
    });

    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    {
        let mut guard = state.inner.league_state.lock().await;
        *guard = league;
    }

    let app_html = get_client_app_web_shell(axum::extract::State(state.clone()), HeaderMap::new())
        .await
        .0;
    assert!(app_html.contains("data-focus-kind=\"event\""));
    assert!(app_html.contains("data-task-id=\"task-web-event-focus-1\""));
    assert!(app_html.contains("findLiveEventByFocus"));
    assert!(app_html.contains("buildEventFocus"));
    assert!(app_html.contains("mapFocusButtonAttrs"));
    assert!(app_html.contains("mapEventFocusButton"));
    assert!(app_html.contains("mapViewportCardModel"));
    assert!(app_html.contains("mapViewportCardHtml"));
    assert!(app_html.contains("buildMapFocusFromButton"));
    assert!(app_html.contains("buildSelectionFocusFromButton"));
    assert!(app_html.contains("filterLiveEventStream"));
    assert!(app_html.contains("createRealWorldMapAdapter"));
    assert!(app_html.contains("leaflet_renderer_adapter_v1"));
    assert!(app_html.contains("maplibre_gl_v1"));
    assert!(app_html.contains("const mapRuntime"));
    assert!(!app_html.contains("leafletMap"));
    assert!(app_html.contains("renderRouteLine"));
    assert!(app_html.contains("renderTileFrame"));
    assert!(app_html.contains("renderEventPulse"));
    assert!(app_html.contains("handleOverlayToggleButton"));
    assert!(app_html.contains("mapMarkerActionButtonHtml"));
    assert!(app_html.contains("closestFromEvent"));
    assert!(app_html.contains("mapClickSelectors"));
    assert!(app_html.contains("handleMapActionButton"));
    assert!(app_html.contains("handleSelectionActionButton"));
    assert!(app_html.contains("handleMapCameraActionButton"));
    assert!(app_html.contains("handleMapFocusButton"));
    assert!(app_html.contains("onViewportChange"));
    assert!(app_html.contains("getCenter"));
    assert!(app_html.contains("getZoom"));
    assert!(app_html.contains("stream lens"));
    assert!(app_html.contains("Filter route by focus"));
    assert!(app_html.contains("Show full route"));
    assert!(app_html.contains("selectionActionButtonHtml"));
    assert!(app_html.contains("Focused event brief:"));
    assert!(app_html.contains("web_event_id"));
    assert!(app_html.contains("app-global-search"));
    assert!(app_html.contains("app-bottom-tabs"));
    assert!(app_html.contains("app-tab-messages"));
    assert!(app_html.contains("app-tab-map"));
    assert!(app_html.contains("app-tab-feed"));
    assert!(app_html.contains("app-tab-me"));
    assert!(app_html.contains("消息"));
    assert!(app_html.contains("data-app-tab=\"map\">世界</button>"));
    assert!(app_html.contains("动态"));
    assert!(app_html.contains("我"));
    assert!(app_html.contains("app-first-playable-onboarding"));
    assert!(app_html.contains("First playable onboarding"));
    assert!(app_html.contains("first_playable_loop_100"));
    assert!(app_html.contains("trillionnium_first_playable_onboarding_v1"));
    assert!(app_html.contains("data-onboarding-step=\"commerce_delivery\""));
    assert!(app_html.contains("route_task_graph_next_action_visible"));
    assert!(app_html.contains("/v1/client/feed/@alice:local.dev"));
    assert!(app_html.contains("app-feed-api-status"));
    assert!(app_html.contains("app-feed-filter-actions"));
    assert!(app_html.contains("app-feed-summary"));
    assert!(app_html.contains("app-feed-items-live"));
    assert!(app_html.contains("Unified Feed Timeline"));
    assert!(app_html.contains("trillionnium-app-feed-filter"));
    assert!(app_html.contains("feedFilterButtonHtml"));
    assert!(app_html.contains("trillionnium-app-feed-action"));
    assert!(app_html.contains("data-feed-group=\"live_event\""));
    assert!(app_html.contains("loadFeedSurface"));
    assert!(app_html.contains("const routeUiContract ="));
    assert!(app_html.contains("const routeHandoffStorageKey ="));
    assert!(app_html.contains("const buildRouteHandoffState ="));
    assert!(app_html.contains("const buildRouteHandoffRecord ="));
    assert!(app_html.contains("buildRouteActionFromButton"));
    assert!(app_html.contains("handleRouteActionButton"));
    assert!(app_html.contains("handleIndexedRouteActionButton"));
    assert!(app_html.contains("routeFilterModeFromButton"));
    assert!(app_html.contains("routeTaskGraphActionButtonsHtml"));
    assert!(app_html.contains("indexedRouteActionButtonHtml"));
    assert!(app_html.contains("routeFlowActionAttrs"));
    assert!(app_html.contains("\"contract_version\":1"));

    let world_html = get_world_web_shell(axum::extract::State(state), HeaderMap::new())
        .await
        .0;
    assert!(world_html.contains("data-focus-kind=\"event\""));
    assert!(world_html.contains("data-task-id=\"task-web-event-focus-1\""));
    assert!(world_html.contains("findLiveEventByFocus"));
    assert!(world_html.contains("buildEventFocus"));
    assert!(world_html.contains("mapFocusButtonAttrs"));
    assert!(world_html.contains("mapEventFocusButton"));
    assert!(world_html.contains("mapViewportCardModel"));
    assert!(world_html.contains("mapViewportCardHtml"));
    assert!(world_html.contains("buildMapFocusFromButton"));
    assert!(world_html.contains("buildSelectionFocusFromButton"));
    assert!(world_html.contains("filterLiveEventStream"));
    assert!(world_html.contains("createRealWorldMapAdapter"));
    assert!(world_html.contains("leaflet_renderer_adapter_v1"));
    assert!(world_html.contains("maplibre_gl_v1"));
    assert!(world_html.contains("const mapRuntime"));
    assert!(!world_html.contains("leafletMap"));
    assert!(world_html.contains("renderRouteLine"));
    assert!(world_html.contains("renderTileFrame"));
    assert!(world_html.contains("renderEventPulse"));
    assert!(world_html.contains("handleOverlayToggleButton"));
    assert!(world_html.contains("mapMarkerActionButtonHtml"));
    assert!(world_html.contains("closestFromEvent"));
    assert!(world_html.contains("mapClickSelectors"));
    assert!(world_html.contains("handleMapActionButton"));
    assert!(world_html.contains("handleSelectionActionButton"));
    assert!(world_html.contains("handleMapCameraActionButton"));
    assert!(world_html.contains("handleMapFocusButton"));
    assert!(world_html.contains("onViewportChange"));
    assert!(world_html.contains("getCenter"));
    assert!(world_html.contains("getZoom"));
    assert!(world_html.contains("stream lens"));
    assert!(world_html.contains("selectionActionButtonHtml"));
    assert!(world_html.contains("Focused event brief:"));
    assert!(world_html.contains("focusRouteEvent"));
    assert!(world_html.contains("world-event-timeline-item-"));
    assert!(world_html.contains("const routeUiContract ="));
    assert!(world_html.contains("const routeHandoffStorageKey ="));
    assert!(world_html.contains("const buildRouteHandoffState ="));
    assert!(world_html.contains("const buildRouteHandoffRecord ="));
    assert!(world_html.contains("buildRouteActionFromButton"));
    assert!(world_html.contains("handleRouteActionButton"));
    assert!(world_html.contains("handleIndexedRouteActionButton"));
    assert!(world_html.contains("routeFilterModeFromButton"));
    assert!(world_html.contains("routeTaskGraphActionButtonsHtml"));
    assert!(world_html.contains("indexedRouteActionButtonHtml"));
    assert!(world_html.contains("routeFlowActionAttrs"));
    assert!(world_html.contains("\"contract_version\":1"));
}

#[test]
fn client_feed_json_aggregates_mobile_shell_sources() {
    let mut league = default_league_state();
    let starter_location_id = league
        .world
        .world_map_nodes
        .get("starter-studio")
        .map(|node| node.location_id.clone())
        .unwrap_or_else(|| "starter-studio".to_string());
    league.world.world_events.push(WorldEvent {
        event_id: "world-event-feed-1".to_string(),
        actor_matrix_user_id: "@alice:local.dev".to_string(),
        room_id: Some("!test:local.dev".to_string()),
        location_id: starter_location_id.clone(),
        event_kind: "world_contract".to_string(),
        body: "Feed aggregation event".to_string(),
        result: "queued".to_string(),
        impact_score: 6,
        cex_task_id: Some("task-feed-1".to_string()),
        cex_status: Some("Queued".to_string()),
        created_at_epoch: 1_777_297_700,
    });
    league.world.world_contracts.push(WorldContract {
        contract_id: "world-contract-feed-1".to_string(),
        event_id: "world-event-feed-1".to_string(),
        actor_matrix_user_id: "@alice:local.dev".to_string(),
        location_id: starter_location_id,
        task_id: "task-feed-1".to_string(),
        title: "Feed contract".to_string(),
        body: "Feed contract body".to_string(),
        status: "open".to_string(),
        cex_status: Some("Running".to_string()),
        value_score: 52,
        created_at_epoch: 1_777_297_701,
    });
    league
        .world
        .world_contract_completions
        .push(WorldContractCompletion {
            completion_id: "world-completion-feed-1".to_string(),
            contract_id: "world-contract-feed-1".to_string(),
            matrix_user_id: "@alice:local.dev".to_string(),
            body: "Feed completion body".to_string(),
            score: 88.0,
            grade: "A".to_string(),
            reward_amount: 4.2,
            judge_status: "passed".to_string(),
            payout_status: "queued".to_string(),
            anti_cheat_flags: Vec::new(),
            score_events: Vec::new(),
            ledger_status: Some("settled".to_string()),
            ledger_account_id: None,
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: None,
            created_at_epoch: 1_777_297_702,
        });

    let feed = client_feed_json(&league, "@alice:local.dev");
    assert_eq!(
        feed.get("kind").and_then(Value::as_str),
        Some("trillionnium_client_feed")
    );
    assert_eq!(
        feed.get("api_path").and_then(Value::as_str),
        Some("/v1/client/feed/@alice:local.dev")
    );
    assert_eq!(
        feed.pointer("/route_contract/fields/action_textarea")
            .and_then(Value::as_str),
        Some(WORLD_ROUTE_ACTION_TEXTAREA_ID)
    );
    assert!(feed.get("item_count").and_then(Value::as_u64).unwrap_or(0) >= 3);
    let feed_items = feed
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(feed_items
        .iter()
        .any(|item| item.get("feed_kind").and_then(Value::as_str) == Some("live_event")));
    assert!(feed_items
        .iter()
        .any(|item| item.get("feed_kind").and_then(Value::as_str) == Some("route_task")));
    assert!(feed_items
        .iter()
        .any(|item| item.get("feed_kind").and_then(Value::as_str) == Some("contract")));
    assert!(feed_items
        .iter()
        .any(|item| item.get("feed_kind").and_then(Value::as_str) == Some("completion")));
    let live_event_item = feed_items
        .iter()
        .find(|item| item.get("feed_kind").and_then(Value::as_str) == Some("live_event"))
        .expect("live event feed item");
    assert_eq!(
        live_event_item.get("feed_group").and_then(Value::as_str),
        Some("live_event")
    );
    assert_eq!(
        live_event_item.get("focus_kind").and_then(Value::as_str),
        Some("event")
    );
    assert_eq!(
        live_event_item.get("action_label").and_then(Value::as_str),
        Some("继续推进")
    );
    assert_eq!(
        live_event_item
            .get("action_event_task_id")
            .and_then(Value::as_str),
        Some("task-feed-1")
    );
    let route_task_item = feed_items
        .iter()
        .find(|item| item.get("feed_kind").and_then(Value::as_str) == Some("route_task"))
        .expect("route task feed item");
    assert!(route_task_item
        .get("action_panel_id")
        .and_then(Value::as_str)
        .map(|value| !value.is_empty())
        .unwrap_or(false));
    assert!(route_task_item
        .get("action_body_base")
        .and_then(Value::as_str)
        .map(|value| !value.is_empty())
        .unwrap_or(false));
    assert!(
        feed.get("live_event_stream")
            .and_then(Value::as_array)
            .map(std::vec::Vec::len)
            .unwrap_or(0)
            > 0
    );
    assert!(
        feed.pointer("/route_task_graph/task_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 1
    );
    assert!(
        feed.pointer("/snapshots/contracts/recent")
            .and_then(Value::as_array)
            .map(std::vec::Vec::len)
            .unwrap_or(0)
            > 0
    );
    assert!(
        feed.pointer("/snapshots/completions/recent")
            .and_then(Value::as_array)
            .map(std::vec::Vec::len)
            .unwrap_or(0)
            > 0
    );
    assert!(
        feed.pointer("/snapshots/social/entity_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 1
    );
}

#[test]
fn client_app_map_hub_projects_route_preview() {
    let mut league = default_league_state();
    let starter_location_id = league
        .world
        .world_map_nodes
        .get("starter-studio")
        .map(|node| node.location_id.clone())
        .unwrap_or_else(|| "starter-studio".to_string());
    league.world.world_events.push(WorldEvent {
        event_id: "world-event-route-preview".to_string(),
        actor_matrix_user_id: "@alice:local.dev".to_string(),
        room_id: Some("!test:local.dev".to_string()),
        location_id: starter_location_id.clone(),
        event_kind: "world_contract".to_string(),
        body: "Linked event for route preview".to_string(),
        result: "held_review".to_string(),
        impact_score: 7,
        cex_task_id: Some("task-route-preview-1".to_string()),
        cex_status: Some("Running".to_string()),
        created_at_epoch: 1_777_230_099,
    });
    league.world.world_contracts.push(WorldContract {
        contract_id: "world-contract-route-preview".to_string(),
        event_id: "world-event-route-preview".to_string(),
        actor_matrix_user_id: "@alice:local.dev".to_string(),
        location_id: starter_location_id,
        task_id: "task-route-preview-1".to_string(),
        title: "Route preview contract".to_string(),
        body: "Deliver linked route preview output".to_string(),
        status: "open".to_string(),
        cex_status: Some("Running".to_string()),
        value_score: 42,
        created_at_epoch: 1_777_230_100,
    });
    league
        .world
        .world_contract_completions
        .push(WorldContractCompletion {
            completion_id: "world-completion-route-preview".to_string(),
            contract_id: "world-contract-route-preview".to_string(),
            matrix_user_id: "@alice:local.dev".to_string(),
            body: "Completion closes the linked route preview contract".to_string(),
            score: 88.0,
            grade: "A".to_string(),
            reward_amount: 4.2,
            judge_status: "rubric_hidden_pipeline_v2".to_string(),
            payout_status: "settled".to_string(),
            anti_cheat_flags: Vec::new(),
            score_events: Vec::new(),
            ledger_status: Some("settled".to_string()),
            ledger_account_id: None,
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: None,
            created_at_epoch: 1_777_230_101,
        });

    let app = client_app_json(&league, "@alice:local.dev");
    let items = app["map_hub"]["route_preview"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(items.iter().any(|item| item["route_bucket"] == "event"));
    assert!(items.iter().any(|item| item["route_bucket"] == "contract"));
    assert!(items
        .iter()
        .any(|item| item["route_bucket"] == "completion"));
    assert!(items
        .iter()
        .any(|item| item["task_id"] == "task-route-preview-1"));
    assert!(app["map_hub"]["route_task_graph"]["tasks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .any(|task| {
            task["task_id"] == "task-route-preview-1"
                && task["completion_count"] == 1
                && task["latest_completion_id"] == "world-completion-route-preview"
                && task["outcome_summary"]
                    .as_str()
                    .map(|value| value.contains("world-completion-route-preview"))
                    .unwrap_or(false)
                && task["feedback_focus"]
                    .as_str()
                    .map(|value| value.contains("client feedback"))
                    .unwrap_or(false)
                && task["next_opportunity_kind"] == "repeat_order_upsell_referral"
                && task["next_opportunity_hint"]
                    .as_str()
                    .map(|value| value.contains("repeat order") || value.contains("upsell"))
                    .unwrap_or(false)
                && task["next_opportunity_playbook"]
                    .as_str()
                    .map(|value| value.contains("testimonial") || value.contains("premium"))
                    .unwrap_or(false)
                && task["next_opportunity_command"]
                    .as_str()
                    .map(|value| value.contains("/sell latest"))
                    .unwrap_or(false)
                && task["next_opportunity_node_id"] == "starter-studio"
                && task["next_opportunity_panel_id"] == "world-listings-panel"
                && task["next_opportunity_input_id"] == "world-listing-company-id"
                && task["next_opportunity_input_value"] == "latest"
                && task["next_opportunity_textarea_id"] == "world-listing-body"
                && task["next_opportunity_body"]
                    .as_str()
                    .map(|value| value.contains("复购") || value.contains("upgrade"))
                    .unwrap_or(false)
                && task["suggested_node_id"] == "starter-studio"
                && task["suggested_action_label"] == "Draft completion follow-up"
                && task["suggested_panel_id"] == "world-action-console"
                && task["suggested_input_id"] == ""
                && task["suggested_input_value"] == ""
                && task["suggested_textarea_id"] == "world-action-body"
                && task["next_opportunity_action_label"] == "Open listing lane"
        }));
    assert_eq!(
        app["map_hub"]["route_story"]["next_task_id"],
        json!("task-route-preview-1")
    );
    assert_eq!(
            app["map_hub"]["route_story"]["next_command_hint"],
            json!("/world action 跟进已完成任务 task-route-preview-1：围绕 Completion world-completion-route-preview 记录交付证据、客户反馈、复盘和下一单机会。")
        );
    assert_eq!(
        app["map_hub"]["route_story"]["next_opportunity_target"]["panel_id"],
        json!("world-listings-panel")
    );
    assert_eq!(
        app["map_hub"]["route_story"]["next_opportunity_target"]["node_id"],
        json!("starter-studio")
    );
}

#[test]
fn world_route_command_target_maps_structured_web_targets() {
    let listing = crate::world_route_command_target(
        "/sell latest 为复购客户起草升级方案，包含推荐语、加价包和转介绍激励。",
    );
    assert_eq!(listing.panel_id, "world-listings-panel");
    assert_eq!(listing.input_id, "world-listing-company-id");
    assert_eq!(listing.input_value, "latest");
    assert_eq!(listing.textarea_id, "world-listing-body");
    assert_eq!(listing.action_label, "Open listing lane");
    assert!(listing.body.contains("复购客户"));

    let purchase = crate::world_route_command_target(
        "/buy latest 购买当前上架服务，并附上验收标准、交付范围和时间要求。",
    );
    assert_eq!(purchase.panel_id, "world-commerce-panel");
    assert_eq!(purchase.input_id, "world-buy-listing-id");
    assert_eq!(purchase.input_value, "latest");
    assert_eq!(purchase.textarea_id, "world-buy-body");
    assert_eq!(purchase.action_label, "Open purchase lane");
    assert!(purchase.body.contains("验收标准"));

    let completion = crate::world_route_command_target(
        "/complete world-contract-123 交付最终稿、证据包、风险复盘和下一步协作建议。",
    );
    assert_eq!(completion.panel_id, "world-contracts-panel");
    assert_eq!(completion.input_id, "world-contract-completion-id");
    assert_eq!(completion.input_value, "world-contract-123");
    assert_eq!(completion.textarea_id, "world-contract-completion-body");
    assert_eq!(completion.action_label, "Open contract completion lane");
    assert!(completion.body.contains("交付最终稿"));

    let rejection = crate::world_route_command_target(
        "/work reject latest 缺少原始文件、尺寸说明和修改承诺，请先补齐。",
    );
    assert_eq!(rejection.panel_id, "world-commerce-panel");
    assert_eq!(rejection.input_id, "world-work-reject-id");
    assert_eq!(rejection.input_value, "latest");
    assert_eq!(rejection.textarea_id, "world-work-reject-body");
    assert_eq!(rejection.action_label, "Open rejection lane");
    assert!(rejection.body.contains("缺少原始文件"));
}

#[test]
fn local_dev_profile_allows_default_consumer_config() {
    let config = test_config();
    assert!(config.validate_runtime_profile().is_ok());
}

#[test]
fn normalized_repository_runtime_modes_require_database_url() {
    let mut config = test_config();
    config.league_normalized_dual_write_enabled = true;
    config.league_normalized_read_switch_enabled = true;
    config.league_normalized_final_cutover_enabled = true;

    let errors = config.validate_runtime_profile().unwrap_err();
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED=true")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED=true")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED=true")));

    config.league_normalized_database_url = Some("postgres://local/cex".to_string());
    assert!(config.validate_runtime_profile().is_ok());
}

#[test]
fn beta_profile_requires_auth_replay_and_identity_binding() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Beta;

    let errors = config.validate_runtime_profile().unwrap_err();
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_INGRESS_TOKEN")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_SESSION_AUTH_SECRET")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_REPLAY_STORE_PATH")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH")));
    assert!(errors
        .iter()
        .any(|item| item.contains("CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true")));
    assert!(errors
        .iter()
        .any(|item| item.contains("non-default CEX_GATEWAY_API_KEY")));
}

#[test]
fn authorizes_matching_signed_user_session() {
    let mut config = test_config();
    config.require_session_auth = true;
    config.session_auth_secret = Some("test-session-secret".to_string());
    config.session_auth_allowed_issuers = vec!["test-suite".to_string()];
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let scope = build_chat_identity_scope(&payload);
    let now_epoch = Utc::now().timestamp();
    let claims = UserSessionAuthClaims {
        version: 1,
        issuer: "test-suite".to_string(),
        key_id: None,
        subject: "user-1".to_string(),
        source_kind: "chat_task".to_string(),
        audience: Some("consumer-entry-api".to_string()),
        request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        account_id: None,
        issued_at_epoch: now_epoch,
        expires_at_epoch: now_epoch + 60,
    };
    let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());
    let signature = sign_user_session_assertion(&assertion, "test-session-secret").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
    headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

    let authorized = authorize_user_session(
        &state,
        &headers,
        &scope,
        build_chat_request_fingerprint(&payload).as_str(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(authorized.claims.subject, "user-1");
    assert_eq!(authorized.claims.source_kind, "chat_task");
    assert_eq!(
        authorized.claims.audience.as_deref(),
        Some("consumer-entry-api")
    );
}

#[test]
fn authorizes_matching_signed_user_session_with_issuer_specific_secret() {
    let mut config = test_config();
    config.require_session_auth = true;
    config.session_auth_secret = None;
    config
        .session_auth_issuer_secrets
        .insert("issuer-specific".to_string(), "issuer-secret".to_string());
    config.session_auth_allowed_issuers = vec!["issuer-specific".to_string()];
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let scope = build_chat_identity_scope(&payload);
    let now_epoch = Utc::now().timestamp();
    let claims = UserSessionAuthClaims {
        version: 1,
        issuer: "issuer-specific".to_string(),
        key_id: None,
        subject: "user-1".to_string(),
        source_kind: "chat_task".to_string(),
        audience: Some("consumer-entry-api".to_string()),
        request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        account_id: None,
        issued_at_epoch: now_epoch,
        expires_at_epoch: now_epoch + 60,
    };
    let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());
    let signature = sign_user_session_assertion(&assertion, "issuer-secret").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
    headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

    let authorized = authorize_user_session(
        &state,
        &headers,
        &scope,
        build_chat_request_fingerprint(&payload).as_str(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(authorized.claims.issuer, "issuer-specific");
}

#[test]
fn authorizes_matching_signed_user_session_with_issuer_key_registry() {
    let mut config = test_config();
    config.require_session_auth = true;
    config.session_auth_secret = None;
    config.session_auth_issuer_keys.insert(
        "issuer-keys".to_string(),
        HashMap::from([("v1".to_string(), "issuer-key-secret".to_string())]),
    );
    config.session_auth_allowed_issuers = vec!["issuer-keys".to_string()];
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let scope = build_chat_identity_scope(&payload);
    let now_epoch = Utc::now().timestamp();
    let claims = UserSessionAuthClaims {
        version: 1,
        issuer: "issuer-keys".to_string(),
        key_id: Some("v1".to_string()),
        subject: "user-1".to_string(),
        source_kind: "chat_task".to_string(),
        audience: Some("consumer-entry-api".to_string()),
        request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        account_id: None,
        issued_at_epoch: now_epoch,
        expires_at_epoch: now_epoch + 60,
    };
    let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());
    let signature = sign_user_session_assertion(&assertion, "issuer-key-secret").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
    headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

    let authorized = authorize_user_session(
        &state,
        &headers,
        &scope,
        build_chat_request_fingerprint(&payload).as_str(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(authorized.claims.issuer, "issuer-keys");
    assert_eq!(authorized.claims.key_id.as_deref(), Some("v1"));
}

#[test]
fn authorizes_matching_signed_user_session_with_shared_issuer_registry() {
    let mut config = test_config();
    config.require_session_auth = true;
    config.session_auth_secret = None;
    config.session_auth_issuer_registry.insert(
        "issuer-registry".to_string(),
        SessionAuthIssuerRegistryIssuer {
            active_key_id: Some("v1".to_string()),
            keys: HashMap::from([("v1".to_string(), "issuer-registry-secret".to_string())]),
        },
    );
    config.session_auth_allowed_issuers = vec!["issuer-registry".to_string()];
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let scope = build_chat_identity_scope(&payload);
    let now_epoch = Utc::now().timestamp();
    let claims = UserSessionAuthClaims {
        version: 1,
        issuer: "issuer-registry".to_string(),
        key_id: Some("v1".to_string()),
        subject: "user-1".to_string(),
        source_kind: "chat_task".to_string(),
        audience: Some("consumer-entry-api".to_string()),
        request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        account_id: None,
        issued_at_epoch: now_epoch,
        expires_at_epoch: now_epoch + 60,
    };
    let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());
    let signature = sign_user_session_assertion(&assertion, "issuer-registry-secret").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
    headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

    let authorized = authorize_user_session(
        &state,
        &headers,
        &scope,
        build_chat_request_fingerprint(&payload).as_str(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(authorized.claims.issuer, "issuer-registry");
    assert_eq!(authorized.claims.key_id.as_deref(), Some("v1"));
}

#[test]
fn session_auth_issuer_registry_active_key_diff_reports_changes() {
    let current = HashMap::from([
        (
            "matrix-entry-adapter".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: Some("v1".to_string()),
                keys: HashMap::from([
                    ("v1".to_string(), "secret-a".to_string()),
                    ("v2".to_string(), "secret-b".to_string()),
                ]),
            },
        ),
        (
            "worker".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: None,
                keys: HashMap::from([("w1".to_string(), "secret-c".to_string())]),
            },
        ),
    ]);
    let candidate = HashMap::from([
        (
            "matrix-entry-adapter".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: Some("v2".to_string()),
                keys: HashMap::from([
                    ("v1".to_string(), "secret-a".to_string()),
                    ("v2".to_string(), "secret-b".to_string()),
                ]),
            },
        ),
        (
            "worker".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: Some("w1".to_string()),
                keys: HashMap::from([("w1".to_string(), "secret-c".to_string())]),
            },
        ),
    ]);

    let diff = session_auth_issuer_registry_active_key_diff_json(&current, &candidate);
    assert_eq!(diff["matches"], Value::Bool(false));
    assert_eq!(diff["changed_active_key_count"], Value::from(2));
    assert_eq!(
        diff.get("changes").and_then(Value::as_array).map(Vec::len),
        Some(2)
    );
}

#[test]
fn load_session_auth_issuer_registry_rejects_invalid_active_key() {
    let temp_path = std::env::temp_dir().join(format!(
        "cex-session-auth-issuer-registry-invalid-{}-{}.json",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));
    std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"missing","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write issuer registry");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_path.to_str());
    assert_eq!(metadata.load_status, "invalid_active_key");
    assert_eq!(metadata.revision.as_deref(), Some("rev-a"));
    assert!(metadata
        .load_error
        .as_deref()
        .unwrap_or_default()
        .contains("active key missing"));
    assert!(registry.is_empty());

    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn rejects_signed_user_session_with_unexpected_issuer() {
    let mut config = test_config();
    config.require_session_auth = true;
    config.session_auth_secret = Some("test-session-secret".to_string());
    config.session_auth_allowed_issuers = vec!["trusted-issuer".to_string()];
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let scope = build_chat_identity_scope(&payload);
    let now_epoch = Utc::now().timestamp();
    let claims = UserSessionAuthClaims {
        version: 1,
        issuer: "unexpected-issuer".to_string(),
        key_id: None,
        subject: "user-1".to_string(),
        source_kind: "chat_task".to_string(),
        audience: Some("consumer-entry-api".to_string()),
        request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        account_id: None,
        issued_at_epoch: now_epoch,
        expires_at_epoch: now_epoch + 60,
    };
    let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());
    let signature = sign_user_session_assertion(&assertion, "test-session-secret").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
    headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

    let response = authorize_user_session(
        &state,
        &headers,
        &scope,
        build_chat_request_fingerprint(&payload).as_str(),
    )
    .unwrap_err();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn rejects_signed_user_session_with_mismatched_request_fingerprint() {
    let mut config = test_config();
    config.require_session_auth = true;
    config.session_auth_secret = Some("test-session-secret".to_string());
    config.session_auth_allowed_issuers = vec!["test-suite".to_string()];
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let scope = build_chat_identity_scope(&payload);
    let now_epoch = Utc::now().timestamp();
    let claims = UserSessionAuthClaims {
        version: 1,
        issuer: "test-suite".to_string(),
        key_id: None,
        subject: "user-1".to_string(),
        source_kind: "chat_task".to_string(),
        audience: Some("consumer-entry-api".to_string()),
        request_fingerprint: Some("wrong-fingerprint".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        account_id: None,
        issued_at_epoch: now_epoch,
        expires_at_epoch: now_epoch + 60,
    };
    let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());
    let signature = sign_user_session_assertion(&assertion, "test-session-secret").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
    headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

    let response = authorize_user_session(
        &state,
        &headers,
        &scope,
        build_chat_request_fingerprint(&payload).as_str(),
    )
    .unwrap_err();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn production_profile_requires_strict_identity_reload_governance() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    config.ingress_token = Some("entry-secret".to_string());
    config.replay_store_path = Some("/tmp/consumer-entry-replay.json".to_string());
    config.rate_limit_store_path = Some("/tmp/consumer-entry-rate-limits.json".to_string());
    config.identity_bindings_path = Some("/tmp/identity-bindings.json".to_string());
    config.require_identity_binding = true;
    config.cex_gateway_api_key = "prod-gateway-key".to_string();

    let errors = config.validate_runtime_profile().unwrap_err();
    assert!(errors
        .iter()
        .any(|item| item.contains("IDENTITY_BINDING_AUDIT_LOG_PATH")));
    assert!(errors
        .iter()
        .any(|item| item.contains("APPROVED_REVISIONS_PATH")));
    assert!(errors
        .iter()
        .any(|item| item.contains("RELOAD_REQUIRE_REVISION=true")));
    assert!(errors
        .iter()
        .any(|item| item.contains("RELOAD_REQUIRE_APPROVED_REVISION=true")));
    assert!(errors
        .iter()
        .any(|item| item.contains("RELOAD_REQUIRE_ACTOR=true")));
    assert!(errors
        .iter()
        .any(|item| item.contains("RELOAD_ALLOWED_ACTORS")));
}

#[test]
fn prune_rate_limit_cache_drops_expired_and_oversized_entries() {
    let now = 1_760_000_100;
    let mut cache = RateLimitCache {
        seen: HashMap::from([(
            "chat:user-1:room-1".to_string(),
            VecDeque::from([now - 120, now - 30, now - 20, now - 10]),
        )]),
    };

    prune_rate_limit_cache(&mut cache, now, 60, 2);
    assert_eq!(
        cache.seen.get("chat:user-1:room-1").cloned(),
        Some(VecDeque::from([now - 20, now - 10]))
    );
}

#[test]
fn load_rate_limit_cache_prunes_persisted_entries() {
    let path = std::env::temp_dir().join(format!(
        "consumer-entry-rate-limit-{}.json",
        std::process::id()
    ));
    let now = chrono::Utc::now().timestamp();
    std::fs::write(
        &path,
        serde_json::to_vec(&RateLimitCache {
            seen: HashMap::from([(
                "chat:user-1:room-1".to_string(),
                VecDeque::from([now - 120, now - 5]),
            )]),
        })
        .unwrap(),
    )
    .unwrap();

    let mut config = test_config();
    config.rate_limit_store_path = Some(path.display().to_string());
    let cache = load_rate_limit_cache(&config);
    assert_eq!(
        cache.seen.get("chat:user-1:room-1").cloned(),
        Some(VecDeque::from([now - 5]))
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn load_identity_binding_store_prefers_separate_registry_file_when_configured() {
    let binding_path = std::env::temp_dir().join(format!(
        "consumer-entry-bindings-{}.json",
        std::process::id()
    ));
    let registry_path = std::env::temp_dir().join(format!(
        "consumer-entry-registry-{}.json",
        std::process::id()
    ));

    std::fs::write(
        &binding_path,
        r#"{
                "version": 1,
                "revision": "bindings-a",
                "product_users": {
                    "pu-1": { "org_id": "org-embedded", "account_id": "acct-embedded" }
                },
                "chat_users": {
                    "user-1": { "product_user_id": "pu-1" }
                }
            }"#,
    )
    .unwrap();
    std::fs::write(
            &registry_path,
            r#"{
                "version": 1,
                "revision": "registry-a",
                "product_users": {
                    "pu-1": { "org_id": "org-registry", "account_id": "acct-registry", "status": "active" }
                }
            }"#,
        )
        .unwrap();

    let mut config = test_config();
    config.identity_bindings_path = Some(binding_path.display().to_string());
    config.identity_registry_path = Some(registry_path.display().to_string());

    let store = load_identity_binding_store(&config);
    assert_eq!(store.metadata.revision.as_deref(), Some("bindings-a"));
    assert_eq!(
        store.registry_metadata.revision.as_deref(),
        Some("registry-a")
    );
    assert_eq!(store.registry_metadata.format, "separate-registry-document");
    assert_eq!(
        store
            .product_users
            .get("pu-1")
            .and_then(|user| user.org_id.as_deref()),
        Some("org-registry")
    );

    let _ = std::fs::remove_file(binding_path);
    let _ = std::fs::remove_file(registry_path);
}

#[test]
fn load_identity_binding_revision_approval_state_reads_ordered_revisions() {
    let path = std::env::temp_dir().join(format!(
        "consumer-entry-approval-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &path,
        r#"{
                "version": 1,
                "revision": "approval-doc-a",
                "approved_revisions": ["rev-a", "rev-b", "rev-a", "  "]
            }"#,
    )
    .unwrap();

    let mut config = test_config();
    config.identity_binding_approved_revisions_path = Some(path.display().to_string());

    let approval_state = load_identity_binding_revision_approval_state(&config);
    assert_eq!(approval_state.load_status, "loaded");
    assert_eq!(approval_state.revision.as_deref(), Some("approval-doc-a"));
    assert_eq!(approval_state.approved_revisions, vec!["rev-a", "rev-b"]);

    let _ = std::fs::remove_file(path);
}

#[test]
fn load_session_auth_issuer_registry_revision_approval_state_reads_ordered_revisions() {
    let path = std::env::temp_dir().join(format!(
        "consumer-entry-session-auth-approval-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &path,
        r#"{
                "version": 1,
                "revision": "session-approval-a",
                "approved_revisions": ["sess-reg-a", "sess-reg-b", "sess-reg-a", "  "]
            }"#,
    )
    .unwrap();

    let mut config = test_config();
    config.session_auth_issuer_registry_approved_revisions_path = Some(path.display().to_string());

    let approval_state = load_session_auth_issuer_registry_revision_approval_state(&config);
    assert_eq!(approval_state.load_status, "loaded");
    assert_eq!(
        approval_state.revision.as_deref(),
        Some("session-approval-a")
    );
    assert_eq!(
        approval_state.approved_revisions,
        vec!["sess-reg-a", "sess-reg-b"]
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn reload_governance_can_require_revision() {
    let mut config = test_config();
    config.identity_binding_reload_require_revision = true;

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "legacy-flat-map".to_string(),
            version: 1,
            revision: None,
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };

    let decision = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        None,
    );
    assert!(!decision.accepted);
    assert_eq!(decision.reason, "revision_required");
}

#[test]
fn reload_governance_rejects_missing_product_user_refs() {
    let mut config = test_config();
    config.identity_registry_path = Some("/tmp/registry.json".to_string());

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("bind-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata {
            format: "separate-registry-document".to_string(),
            version: 1,
            revision: Some("reg-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        bindings: IdentityBindings {
            chat_users: HashMap::from([(
                "chat-1".to_string(),
                IdentityBindingEntry {
                    org_id: None,
                    account_id: None,
                    product_user_id: Some("pu-1".to_string()),
                },
            )]),
            matrix_users: HashMap::new(),
        },
        product_users: HashMap::from([(
            "pu-1".to_string(),
            ProductUserIdentity {
                org_id: Some("org-a".to_string()),
                account_id: Some("acct-a".to_string()),
                status: None,
            },
        )]),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("bind-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata {
            format: "separate-registry-document".to_string(),
            version: 1,
            revision: Some("reg-b".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        bindings: current.bindings.clone(),
        product_users: HashMap::new(),
    };

    let decision = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        None,
    );
    assert!(!decision.accepted);
    assert_eq!(decision.reason, "missing_product_user_refs");
    assert_eq!(decision.current_missing_product_user_refs, 0);
    assert_eq!(decision.candidate_missing_product_user_refs, 1);
}

#[test]
fn reload_governance_with_separate_registry_requires_effective_revision() {
    let mut config = test_config();
    config.identity_registry_path = Some("/tmp/registry.json".to_string());
    config.identity_binding_reload_require_revision = true;

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("bind-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata {
            format: "separate-registry-document".to_string(),
            version: 1,
            revision: Some("reg-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("bind-b".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata {
            format: "separate-registry-document".to_string(),
            version: 1,
            revision: None,
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };

    let decision = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        None,
    );
    assert!(!decision.accepted);
    assert_eq!(decision.reason, "effective_revision_required");
}

#[test]
fn reload_governance_can_reject_same_revision() {
    let mut config = test_config();
    config.identity_binding_reload_reject_same_revision = true;

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };

    let decision = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        None,
    );
    assert!(!decision.accepted);
    assert_eq!(decision.reason, "same_revision_rejected");
}

#[test]
fn reload_governance_with_separate_registry_uses_effective_revision_for_approval() {
    let mut config = test_config();
    config.identity_registry_path = Some("/tmp/registry.json".to_string());
    config.identity_binding_reload_require_approved_revision = true;

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("bind-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata {
            format: "separate-registry-document".to_string(),
            version: 1,
            revision: Some("reg-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("bind-b".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata {
            format: "separate-registry-document".to_string(),
            version: 1,
            revision: Some("reg-b".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let approval_state = IdentityBindingRevisionApprovalState {
        source_path: None,
        source_modified_epoch: None,
        loaded_at_epoch: Some(2),
        load_status: "loaded".to_string(),
        load_error: None,
        version: 1,
        revision: Some("approval-1".to_string()),
        approved_revisions: vec!["binding:bind-b|registry:reg-b".to_string()],
    };

    let decision = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &approval_state,
        None,
    );
    assert!(decision.accepted);
    assert_eq!(
        decision.candidate_effective_revision.as_deref(),
        Some("binding:bind-b|registry:reg-b")
    );
    assert_eq!(decision.candidate_revision_approved, Some(true));
}

#[test]
fn reload_governance_can_require_approved_revision() {
    let mut config = test_config();
    config.identity_binding_reload_require_approved_revision = true;

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-c".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let approval_state = IdentityBindingRevisionApprovalState {
        source_path: None,
        source_modified_epoch: None,
        loaded_at_epoch: Some(3),
        load_status: "loaded".to_string(),
        load_error: None,
        version: 1,
        revision: Some("approval-rev-1".to_string()),
        approved_revisions: vec!["rev-a".to_string(), "rev-b".to_string()],
    };

    let decision = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &approval_state,
        None,
    );
    assert!(!decision.accepted);
    assert_eq!(decision.reason, "candidate_revision_not_approved");
    assert_eq!(decision.candidate_revision_approved, Some(false));
}

#[test]
fn reload_governance_can_reject_rollback_revision() {
    let mut config = test_config();
    config.identity_binding_reload_allow_rollback = false;

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-b".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let approval_state = IdentityBindingRevisionApprovalState {
        source_path: None,
        source_modified_epoch: None,
        loaded_at_epoch: Some(3),
        load_status: "loaded".to_string(),
        load_error: None,
        version: 1,
        revision: Some("approval-rev-1".to_string()),
        approved_revisions: vec!["rev-a".to_string(), "rev-b".to_string()],
    };

    let decision = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &approval_state,
        None,
    );
    assert!(!decision.accepted);
    assert_eq!(decision.reason, "rollback_revision_rejected");
    assert!(decision.rollback_blocked);
}

#[test]
fn parse_csv_list_trims_empties_and_deduplicates() {
    assert_eq!(
        parse_csv_list(" alice , bob ,,alice, ,carol, bob ,,"),
        vec!["alice", "bob", "carol"]
    );
}

#[test]
fn reload_governance_can_require_actor_allow_list_non_empty() {
    let mut config = test_config();
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = vec!["alice".to_string(), "bob".to_string()];

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };

    let denied = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        Some("mallory"),
    );
    assert!(!denied.accepted);
    assert_eq!(denied.reason, "actor_not_authorized");
    assert_eq!(denied.actor_authorized, Some(false));
    assert_eq!(denied.actor_reason.as_deref(), Some("actor_not_allowed"));
}

#[test]
fn reload_governance_can_require_actor_with_empty_allowlist() {
    let mut config = test_config();
    config.identity_binding_reload_require_actor = true;

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };

    let denied = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        Some("mallory"),
    );
    assert!(!denied.accepted);
    assert_eq!(denied.reason, "actor_not_authorized");
    assert_eq!(denied.actor_authorized, Some(false));
    assert_eq!(
        denied.actor_reason.as_deref(),
        Some("no_allowed_actors_configured")
    );
}

#[test]
fn reload_governance_can_require_actor_header_missing() {
    let mut config = test_config();
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };

    let denied = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        None,
    );
    assert!(!denied.accepted);
    assert_eq!(denied.reason, "actor_not_authorized");
    assert_eq!(denied.actor_authorized, Some(false));
    assert_eq!(denied.actor_reason.as_deref(), Some("actor_missing"));
}

#[test]
fn reload_governance_can_require_allowed_actor() {
    let mut config = test_config();
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_reject_same_revision = true;
    config.identity_binding_reload_allowed_actors = vec!["alice".to_string(), "bob".to_string()];

    let current = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(1),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };
    let candidate = IdentityBindingStore {
        metadata: IdentityBindingMetadata {
            format: "versioned-document".to_string(),
            version: 1,
            revision: Some("rev-a".to_string()),
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
        },
        registry_metadata: IdentityBindingMetadata::default(),
        bindings: IdentityBindings::default(),
        product_users: HashMap::new(),
    };

    let denied = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        Some("mallory"),
    );
    assert!(!denied.accepted);
    assert_eq!(denied.reason, "actor_not_authorized");
    assert_eq!(denied.actor_authorized, Some(false));
    assert_eq!(denied.actor_reason.as_deref(), Some("actor_not_allowed"));

    let allowed = evaluate_identity_binding_reload_governance(
        &config,
        &current,
        &candidate,
        &IdentityBindingRevisionApprovalState::default(),
        Some("alice"),
    );
    assert!(!allowed.accepted);
    assert_eq!(allowed.reason, "same_revision_rejected");
    assert_eq!(allowed.actor_authorized, Some(true));
    assert_eq!(allowed.actor_reason, None);
}

#[tokio::test]
async fn chat_identity_binding_can_fill_org_and_account() {
    let mut bindings = IdentityBindings::default();
    bindings.chat_users.insert(
        "user-1".to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("org-bound".to_string()),
            account_id: Some("acct-bound".to_string()),
        },
    );
    let state = test_state(test_config(), bindings, HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: None,
        org_id: None,
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };

    let resolved = resolve_chat_identity(&state, &payload).await.unwrap();
    assert_eq!(resolved.scope.org_id.as_deref(), Some("org-bound"));
    assert_eq!(resolved.scope.account_id.as_deref(), Some("acct-bound"));
    assert!(resolved.resolution.matched);
    assert_eq!(resolved.resolution.binding_version, 1);
}

#[tokio::test]
async fn chat_identity_binding_rejects_org_mismatch() {
    let mut bindings = IdentityBindings::default();
    bindings.chat_users.insert(
        "user-1".to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("org-bound".to_string()),
            account_id: Some("acct-bound".to_string()),
        },
    );
    let state = test_state(test_config(), bindings, HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: None,
        org_id: Some("org-other".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };

    assert!(resolve_chat_identity(&state, &payload).await.is_err());
}

#[tokio::test]
async fn chat_identity_binding_can_resolve_via_product_user_registry() {
    let mut bindings = IdentityBindings::default();
    bindings.chat_users.insert(
        "user-1".to_string(),
        IdentityBindingEntry {
            product_user_id: Some("pu-1".to_string()),
            org_id: None,
            account_id: None,
        },
    );
    let product_users = HashMap::from([(
        "pu-1".to_string(),
        ProductUserIdentity {
            org_id: Some("org-registry".to_string()),
            account_id: Some("acct-registry".to_string()),
            status: Some("active".to_string()),
        },
    )]);
    let state = test_state(test_config(), bindings, product_users);
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: None,
        org_id: None,
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };

    let resolved = resolve_chat_identity(&state, &payload).await.unwrap();
    assert_eq!(resolved.scope.org_id.as_deref(), Some("org-registry"));
    assert_eq!(resolved.scope.account_id.as_deref(), Some("acct-registry"));
    assert_eq!(resolved.resolution.product_user_id.as_deref(), Some("pu-1"));
    assert_eq!(
        resolved.resolution.binding_source_kind,
        "product_user_registry".to_string()
    );
}

#[tokio::test]
async fn chat_identity_binding_rejects_unknown_product_user_registry_ref() {
    let mut bindings = IdentityBindings::default();
    bindings.chat_users.insert(
        "user-1".to_string(),
        IdentityBindingEntry {
            product_user_id: Some("pu-missing".to_string()),
            org_id: None,
            account_id: None,
        },
    );
    let state = test_state(test_config(), bindings, HashMap::new());
    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: None,
        org_id: None,
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };

    assert!(resolve_chat_identity(&state, &payload).await.is_err());
}

async fn send_text_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, String) {
    let mut request = Request::builder().method(method).uri(uri);

    for (name, value) in headers {
        request = request.header(*name, *value);
    }

    let request = request.body(Body::empty()).expect("build request body");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("request response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body bytes");
    let body = String::from_utf8(bytes.to_vec()).expect("decode response body as utf8");

    (status, body)
}

async fn send_identity_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let (status, body) = send_text_request(app, method, uri, headers).await;
    let body: Value = serde_json::from_str(&body).expect("decode reload response body");

    (status, body)
}

async fn send_health_request(app: &axum::Router) -> (StatusCode, Value) {
    send_identity_request(app, "GET", "/health", &[]).await
}

async fn send_metrics_request(app: &axum::Router) -> (StatusCode, String) {
    send_text_request(app, "GET", "/metrics", &[]).await
}

async fn send_identity_binding_reload_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "POST", "/v1/admin/identity-bindings/reload", headers).await
}

async fn send_identity_registry_reload_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "POST", "/v1/admin/identity-registry/reload", headers).await
}

async fn send_identity_registry_validate_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "POST", "/v1/admin/identity-registry/validate", headers).await
}

async fn send_identity_registry_status_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "GET", "/v1/admin/identity-registry/status", headers).await
}

async fn send_identity_registry_audit_request(
    app: &axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "GET", uri, headers).await
}

async fn send_session_auth_issuer_registry_status_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(
        app,
        "GET",
        "/v1/admin/session-auth/issuer-registry/status",
        headers,
    )
    .await
}

async fn send_session_auth_issuer_registry_reload_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(
        app,
        "POST",
        "/v1/admin/session-auth/issuer-registry/reload",
        headers,
    )
    .await
}

async fn send_session_auth_issuer_registry_validate_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(
        app,
        "POST",
        "/v1/admin/session-auth/issuer-registry/validate",
        headers,
    )
    .await
}

async fn send_session_auth_issuer_registry_approval_status_request(
    app: &axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "GET", uri, headers).await
}

async fn send_session_auth_issuer_registry_approval_validate_request(
    app: &axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "POST", uri, headers).await
}

async fn send_session_auth_issuer_registry_actor_status_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(
        app,
        "GET",
        "/v1/admin/session-auth/issuer-registry/actors/status",
        headers,
    )
    .await
}

async fn send_session_auth_issuer_registry_actor_validate_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(
        app,
        "POST",
        "/v1/admin/session-auth/issuer-registry/actors/validate",
        headers,
    )
    .await
}

async fn send_identity_approval_status_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "GET", "/v1/admin/identity-approval/status", headers).await
}

async fn send_identity_approval_validate_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "POST", "/v1/admin/identity-approval/validate", headers).await
}

async fn send_identity_approval_source_request(
    app: &axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "GET", uri, headers).await
}

async fn send_identity_approval_source_validate_request(
    app: &axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "POST", uri, headers).await
}

async fn send_identity_governance_status_request(
    app: &axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "GET", uri, headers).await
}

async fn send_identity_governance_validate_request(
    app: &axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "POST", uri, headers).await
}

async fn send_identity_actor_status_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "GET", "/v1/admin/identity-actors/status", headers).await
}

async fn send_identity_actor_validate_request(
    app: &axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    send_identity_request(app, "POST", "/v1/admin/identity-actors/validate", headers).await
}

fn temp_identity_bindings_path(suffix: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "consumer-entry-identity-bindings-{suffix}-{pid}-{nanos}.json",
        suffix = suffix,
        pid = std::process::id(),
        nanos = nanos
    ))
}

#[tokio::test]
async fn session_auth_issuer_registry_status_endpoint_reports_live_metadata() {
    let temp_registry_path = temp_identity_bindings_path("session-auth-registry-status");
    let temp_approval_path = temp_identity_bindings_path("session-auth-registry-status-approval");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v1":"secret-a","v2":"secret-b"}},"worker":{"keys":{"w1":"secret-c"}}}}"#,
        )
        .expect("write session auth issuer registry");
    std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-a","approved_revisions":["sess-reg-a","sess-reg-b"]}"#,
        )
        .expect("write session auth issuer registry approval");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_allowed_issuers = vec![
        "matrix-entry-adapter".to_string(),
        "missing-issuer".to_string(),
    ];
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry = registry;
    config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
    config.session_auth_issuer_registry_metadata = metadata;

    let app = build_router(AppState::new(config));
    let (status, body) =
        send_session_auth_issuer_registry_status_request(&app, &[("x-entry-token", "admin-token")])
            .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("status").and_then(Value::as_str), Some("ok"));
    let registry = body
        .get("session_auth_issuer_registry")
        .expect("session auth issuer registry object");
    assert_eq!(registry.get("status").and_then(Value::as_str), Some("ok"));
    assert_eq!(
        registry
            .get("metadata")
            .and_then(|value| value.get("revision"))
            .and_then(Value::as_str),
        Some("sess-reg-a")
    );
    assert_eq!(
        registry.get("issuer_count").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(registry.get("key_count").and_then(Value::as_u64), Some(3));
    assert_eq!(
        registry
            .get("allowed_issuers_missing")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        registry
            .get("issuers_without_active_key")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        registry
            .get("issuer_active_keys")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        Some(2)
    );
    assert_eq!(
        registry
            .get("issuer_active_keys")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|value| value.get("active_key_id"))
            .and_then(Value::as_str),
        Some("v2")
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_approval")
            .and_then(|value| value.get("current_revision_approved"))
            .and_then(Value::as_bool),
        Some(true)
    );

    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn session_auth_issuer_registry_validate_endpoint_reloads_source_file() {
    let temp_registry_path = temp_identity_bindings_path("session-auth-registry-validate");
    let temp_approval_path = temp_identity_bindings_path("session-auth-registry-validate-approval");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-b","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
    std::fs::write(
        &temp_approval_path,
        r#"{"version":1,"revision":"sess-approval-b","approved_revisions":["sess-reg-b"]}"#,
    )
    .expect("write session auth approval state");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_require_approved_revision = true;
    config.session_auth_issuer_registry = registry;
    config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
    config.session_auth_issuer_registry_metadata = metadata;

    let app = build_router(AppState::new(config));
    let (status, body) = send_session_auth_issuer_registry_validate_request(
        &app,
        &[("x-entry-token", "admin-token")],
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("valid").and_then(Value::as_bool), Some(true));
    assert_eq!(body.get("status").and_then(Value::as_str), Some("ok"));
    assert_eq!(
        body.get("matches_loaded_revision").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_source")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("revision"))
            .and_then(Value::as_str),
        Some("sess-reg-b")
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_source_approval")
            .and_then(|value| value.get("current_revision_approved"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        body.get("matches_loaded_active_keys")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_active_key_diff")
            .and_then(|value| value.get("changed_active_key_count"))
            .and_then(Value::as_u64),
        Some(0)
    );

    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn session_auth_issuer_registry_validate_endpoint_reports_active_key_diff() {
    let temp_registry_path = temp_identity_bindings_path("session-auth-registry-active-key-diff");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-diff-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v1":"secret-a","v2":"secret-b"}}}}"#,
        )
        .expect("write session auth issuer registry");

    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry = HashMap::from([(
        "matrix-entry-adapter".to_string(),
        SessionAuthIssuerRegistryIssuer {
            active_key_id: Some("v1".to_string()),
            keys: HashMap::from([
                ("v1".to_string(), "secret-a".to_string()),
                ("v2".to_string(), "secret-b".to_string()),
            ]),
        },
    )]);
    config.session_auth_issuer_registry_metadata = SessionAuthIssuerRegistryMetadata {
        version: 1,
        revision: Some("sess-reg-diff-a".to_string()),
        source_path: Some(temp_registry_path.to_string_lossy().to_string()),
        source_modified_epoch: None,
        loaded_at_epoch: Some(Utc::now().timestamp()),
        load_status: "loaded".to_string(),
        load_error: None,
        issuer_count: 1,
        key_count: 2,
    };

    let app = build_router(AppState::new(config));
    let (status, body) = send_session_auth_issuer_registry_validate_request(
        &app,
        &[("x-entry-token", "admin-token")],
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.get("matches_loaded_active_keys")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_active_key_diff")
            .and_then(|value| value.get("changed_active_key_count"))
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_active_key_diff")
            .and_then(|value| value.get("changes"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|value| value.get("candidate_active_key_id"))
            .and_then(Value::as_str),
        Some("v2")
    );

    let _ = std::fs::remove_file(&temp_registry_path);
}

#[tokio::test]
async fn session_auth_issuer_registry_validate_endpoint_requires_authorized_actor() {
    let temp_registry_path = temp_identity_bindings_path("session-auth-registry-validate-actor");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-actor-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_require_actor = true;
    config.session_auth_issuer_registry_actor_header = "x-session-auth-registry-actor".to_string();
    config.session_auth_issuer_registry_allowed_actors = vec!["alice".to_string()];
    config.session_auth_issuer_registry = registry;
    config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
    config.session_auth_issuer_registry_metadata = metadata;

    let app = build_router(AppState::new(config));
    let (status_missing, body_missing) = send_session_auth_issuer_registry_validate_request(
        &app,
        &[("x-entry-token", "admin-token")],
    )
    .await;

    assert_eq!(status_missing, StatusCode::CONFLICT);
    assert_eq!(
        body_missing.get("status").and_then(Value::as_str),
        Some("actor_missing")
    );
    assert_eq!(
        body_missing
            .get("session_auth_issuer_registry_actor_request")
            .and_then(|value| value.get("authorized"))
            .and_then(Value::as_bool),
        Some(false)
    );

    let (status_ok, body_ok) = send_session_auth_issuer_registry_validate_request(
        &app,
        &[
            ("x-entry-token", "admin-token"),
            ("x-session-auth-registry-actor", "alice"),
        ],
    )
    .await;

    assert_eq!(status_ok, StatusCode::OK);
    assert_eq!(body_ok.get("status").and_then(Value::as_str), Some("ok"));
    assert_eq!(
        body_ok
            .get("session_auth_issuer_registry_actor_request")
            .and_then(|value| value.get("authorized"))
            .and_then(Value::as_bool),
        Some(true)
    );

    let _ = std::fs::remove_file(&temp_registry_path);
}

#[tokio::test]
async fn session_auth_issuer_registry_reload_endpoint_updates_live_runtime_state() {
    let temp_registry_path = temp_identity_bindings_path("session-auth-registry-reload-live");
    let temp_approval_path =
        temp_identity_bindings_path("session-auth-registry-reload-live-approval");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-live-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write live session auth issuer registry");
    std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-live","approved_revisions":["sess-reg-live-a","sess-reg-live-b"]}"#,
        )
        .expect("write session auth approval state");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_require_approved_revision = true;
    config.session_auth_issuer_registry_require_actor = true;
    config.session_auth_issuer_registry_actor_header = "x-session-auth-registry-actor".to_string();
    config.session_auth_issuer_registry_allowed_actors = vec!["alice".to_string()];
    config.session_auth_issuer_registry = registry;
    config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
    config.session_auth_issuer_registry_metadata = metadata;

    let state = AppState::new(config);
    let app = build_router(state.clone());

    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-live-b","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v1":"secret-a","v2":"secret-b"}}}}"#,
        )
        .expect("rewrite live session auth issuer registry");

    let (status_missing, body_missing) =
        send_session_auth_issuer_registry_reload_request(&app, &[("x-entry-token", "admin-token")])
            .await;
    assert_eq!(status_missing, StatusCode::FORBIDDEN);
    assert_eq!(
        body_missing
            .get("session_auth_issuer_registry_reload_governance")
            .and_then(|value| value.get("actor_reason"))
            .and_then(Value::as_str),
        Some("actor_missing")
    );

    let (status_before, body_before) =
        send_session_auth_issuer_registry_status_request(&app, &[("x-entry-token", "admin-token")])
            .await;
    assert_eq!(status_before, StatusCode::OK);
    assert_eq!(
        body_before
            .get("session_auth_issuer_registry")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("revision"))
            .and_then(Value::as_str),
        Some("sess-reg-live-a")
    );

    let (status_ok, body_ok) = send_session_auth_issuer_registry_reload_request(
        &app,
        &[
            ("x-entry-token", "admin-token"),
            ("x-session-auth-registry-actor", "alice"),
        ],
    )
    .await;
    assert_eq!(status_ok, StatusCode::OK);
    assert_eq!(body_ok.get("reloaded").and_then(Value::as_bool), Some(true));
    assert_eq!(
        body_ok
            .get("session_auth_issuer_registry")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("revision"))
            .and_then(Value::as_str),
        Some("sess-reg-live-b")
    );

    let (status_after, body_after) =
        send_session_auth_issuer_registry_status_request(&app, &[("x-entry-token", "admin-token")])
            .await;
    assert_eq!(status_after, StatusCode::OK);
    assert_eq!(
        body_after
            .get("session_auth_issuer_registry")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("revision"))
            .and_then(Value::as_str),
        Some("sess-reg-live-b")
    );

    let payload = CreateChatTaskRequest {
        user_id: Some("user-1".to_string()),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        text: "hello".to_string(),
        capability_id: None,
        account_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let scope = build_chat_identity_scope(&payload);
    let now_epoch = Utc::now().timestamp();
    let claims = UserSessionAuthClaims {
        version: 1,
        issuer: "matrix-entry-adapter".to_string(),
        key_id: Some("v2".to_string()),
        subject: "user-1".to_string(),
        source_kind: "chat_task".to_string(),
        audience: Some("consumer-entry-api".to_string()),
        request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
        room_id: Some("room-1".to_string()),
        session_id: Some("session-1".to_string()),
        org_id: Some("org-1".to_string()),
        account_id: None,
        issued_at_epoch: now_epoch,
        expires_at_epoch: now_epoch + 60,
    };
    let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());
    let signature = sign_user_session_assertion(&assertion, "secret-b").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
    headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

    let authorized = authorize_user_session(
        &state,
        &headers,
        &scope,
        build_chat_request_fingerprint(&payload).as_str(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(authorized.claims.key_id.as_deref(), Some("v2"));

    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn session_auth_issuer_registry_actor_status_endpoint_reports_configuration() {
    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_issuer_registry_path =
        Some("./run/local-runtime/session-auth-issuer-registry.json".to_string());
    config.session_auth_issuer_registry_require_actor = true;
    config.session_auth_issuer_registry_actor_header = "x-session-auth-registry-actor".to_string();
    config.session_auth_issuer_registry_allowed_actors =
        vec!["alice".to_string(), "bob".to_string()];

    let app = build_router(AppState::new(config));
    let (status, body) = send_session_auth_issuer_registry_actor_status_request(
        &app,
        &[("x-entry-token", "admin-token")],
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("actor_valid").and_then(Value::as_bool), Some(true));
    assert_eq!(
        body.get("session_auth_issuer_registry_actor_checks")
            .and_then(|value| value.get("actor_header"))
            .and_then(Value::as_str),
        Some("x-session-auth-registry-actor")
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_actor_checks")
            .and_then(|value| value.get("allowed_actor_count"))
            .and_then(Value::as_u64),
        Some(2)
    );
}

#[tokio::test]
async fn session_auth_issuer_registry_actor_validate_endpoint_rejects_missing_allowed_actors() {
    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_issuer_registry_path =
        Some("./run/local-runtime/session-auth-issuer-registry.json".to_string());
    config.session_auth_issuer_registry_require_actor = true;
    config.session_auth_issuer_registry_actor_header = "x-session-auth-registry-actor".to_string();

    let app = build_router(AppState::new(config));
    let (status, body) = send_session_auth_issuer_registry_actor_validate_request(
        &app,
        &[("x-entry-token", "admin-token")],
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body.get("valid").and_then(Value::as_bool), Some(false));
    assert_eq!(
        body.get("status").and_then(Value::as_str),
        Some("no_allowed_actors_configured")
    );
}

#[tokio::test]
async fn session_auth_issuer_registry_approval_validate_endpoint_rejects_unapproved_revision() {
    let temp_registry_path = temp_identity_bindings_path("session-auth-registry-approval-validate");
    let temp_approval_path =
        temp_identity_bindings_path("session-auth-registry-approval-validate-source");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-c","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
    std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-c","approved_revisions":["sess-reg-a","sess-reg-b"]}"#,
        )
        .expect("write session auth approval state");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_require_approved_revision = true;
    config.session_auth_issuer_registry = registry;
    config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
    config.session_auth_issuer_registry_metadata = metadata;

    let app = build_router(AppState::new(config));
    let (status, body) = send_session_auth_issuer_registry_approval_validate_request(
        &app,
        "/v1/admin/session-auth/issuer-registry/approval/validate?limit=1",
        &[("x-entry-token", "admin-token")],
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body.get("status").and_then(Value::as_str),
        Some("current_revision_not_approved")
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_approval")
            .and_then(|value| value.get("current_revision_approved"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_approval_source")
            .and_then(|value| value.get("returned_revision_count"))
            .and_then(Value::as_u64),
        Some(1)
    );

    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn session_auth_issuer_registry_approval_status_endpoint_reports_source() {
    let temp_registry_path = temp_identity_bindings_path("session-auth-registry-approval-status");
    let temp_approval_path =
        temp_identity_bindings_path("session-auth-registry-approval-status-source");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-d","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
    std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-d","approved_revisions":["sess-reg-c","sess-reg-d"]}"#,
        )
        .expect("write session auth approval state");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
    let mut config = test_config();
    config.ingress_token = Some("admin-token".to_string());
    config.require_session_auth = true;
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry = registry;
    config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
    config.session_auth_issuer_registry_metadata = metadata;

    let app = build_router(AppState::new(config));
    let (status, body) = send_session_auth_issuer_registry_approval_status_request(
        &app,
        "/v1/admin/session-auth/issuer-registry/approval/status?limit=1",
        &[("x-entry-token", "admin-token")],
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("status").and_then(Value::as_str), Some("ok"));
    assert_eq!(
        body.get("session_auth_issuer_registry_approval")
            .and_then(|value| value.get("current_revision_approved"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        body.get("session_auth_issuer_registry_approval_source")
            .and_then(|value| value.get("returned_revision_count"))
            .and_then(Value::as_u64),
        Some(1)
    );

    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn reload_identity_bindings_endpoint_rejects_actor_gate_miss_and_denied_with_403() {
    let temp_path = temp_identity_bindings_path("actor-gate");
    std::fs::write(
        &temp_path,
        r#"{"version":1,"revision":"rev-a","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];
    config.identity_binding_reload_reject_same_revision = true;

    let app = build_router(AppState::new(config));

    let (status_missing, body_missing) = send_identity_binding_reload_request(&app, &[]).await;
    assert_eq!(status_missing, StatusCode::FORBIDDEN);
    assert_eq!(
        body_missing["identity_binding_reload_governance"]["actor_authorized"],
        false,
    );
    assert_eq!(
        body_missing["identity_binding_reload_governance"]["actor_reason"],
        "actor_missing",
    );

    let (status_denied, body_denied) =
        send_identity_binding_reload_request(&app, &[("x-identity-binding-actor", "mallory")])
            .await;
    assert_eq!(status_denied, StatusCode::FORBIDDEN);
    assert_eq!(
        body_denied["identity_binding_reload_governance"]["actor_authorized"],
        false,
    );
    assert_eq!(
        body_denied["identity_binding_reload_governance"]["actor_reason"],
        "actor_not_allowed",
    );

    let _ = std::fs::remove_file(&temp_path);
}

#[tokio::test]
async fn reload_identity_bindings_endpoint_returns_409_when_actor_allowed_but_governance_rejects() {
    let temp_path = temp_identity_bindings_path("actor-allowed-409");
    std::fs::write(
        &temp_path,
        r#"{"version":1,"revision":"rev-a","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];
    config.identity_binding_reload_reject_same_revision = true;

    let app = build_router(AppState::new(config));

    let (status_rejected, body_rejected) =
        send_identity_binding_reload_request(&app, &[("x-identity-binding-actor", "alice")]).await;
    assert_eq!(status_rejected, StatusCode::CONFLICT);
    assert_eq!(
        body_rejected["identity_binding_reload_governance"]["actor_authorized"],
        true,
    );
    assert!(body_rejected["identity_binding_reload_governance"]["actor_reason"].is_null());
    assert_eq!(
        body_rejected["identity_binding_reload_governance"]["reason"],
        "same_revision_rejected",
    );

    let _ = std::fs::remove_file(&temp_path);
}

#[tokio::test]
async fn reload_identity_bindings_endpoint_supports_custom_actor_header() {
    let temp_path = temp_identity_bindings_path("actor-custom-header");
    std::fs::write(
        &temp_path,
        r#"{"version":1,"revision":"rev-a","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_actor_header = "x-deploy-actor".to_string();
    config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];
    config.identity_binding_reload_reject_same_revision = true;

    let app = build_router(AppState::new(config));

    let (status_missing, body_missing) = send_identity_binding_reload_request(&app, &[]).await;
    assert_eq!(status_missing, StatusCode::FORBIDDEN);
    assert_eq!(
        body_missing["identity_binding_reload_governance"]["actor_authorized"],
        false,
    );
    assert_eq!(
        body_missing["identity_binding_reload_governance"]["actor_reason"],
        "actor_missing",
    );

    let (status_wrong_header, body_wrong_header) =
        send_identity_binding_reload_request(&app, &[("x-deploy-actor", "mallory")]).await;
    assert_eq!(status_wrong_header, StatusCode::FORBIDDEN);
    assert_eq!(
        body_wrong_header["identity_binding_reload_governance"]["actor_authorized"],
        false,
    );
    assert_eq!(
        body_wrong_header["identity_binding_reload_governance"]["actor_reason"],
        "actor_not_allowed",
    );

    let (status_ok, body_ok) =
        send_identity_binding_reload_request(&app, &[("x-deploy-actor", "alice")]).await;
    assert_eq!(status_ok, StatusCode::CONFLICT);
    assert_eq!(
        body_ok["identity_binding_reload_governance"]["actor_authorized"],
        true,
    );
    assert!(body_ok["identity_binding_reload_governance"]["actor_reason"].is_null());
    assert_eq!(
        body_ok["identity_binding_reload_governance"]["reason"],
        "same_revision_rejected",
    );

    let _ = std::fs::remove_file(&temp_path);
}

#[tokio::test]
async fn reload_identity_bindings_endpoint_returns_success_after_revision_bump() {
    let temp_path = temp_identity_bindings_path("actor-success");
    std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{"chat-1":{"org_id":"org-old","account_id":"acct-old"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
    config.identity_binding_reload_reject_same_revision = false;

    let app = build_router(AppState::new(config));

    std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{"chat-1":{"org_id":"org-new","account_id":"acct-new"},"chat-2":{"org_id":"org-new2"}},"matrix_users":{"mx-9":{"org_id":"org-mx"}}}"#,
        )
        .expect("write revised identity bindings");

    let (status_ok, body_ok) = send_identity_binding_reload_request(&app, &[]).await;
    assert_eq!(status_ok, StatusCode::OK);
    assert_eq!(body_ok["ok"], true);
    assert_eq!(body_ok["reloaded"], true);
    assert_eq!(
        body_ok["identity_binding_reload_governance"]["accepted"],
        true,
    );
    assert!(body_ok["identity_binding_reload_governance"]["actor_authorized"].is_null());
    assert!(body_ok["identity_binding_reload_governance"]["actor_reason"].is_null());
    assert_eq!(
        body_ok["identity_binding_reload_governance"]["reason"],
        "policy_ok",
    );
    assert_eq!(
        body_ok["identity_binding_reload_governance"]["current_revision"],
        "rev-a",
    );
    assert_eq!(
        body_ok["identity_binding_reload_governance"]["candidate_revision"],
        "rev-b",
    );
    assert_eq!(
        body_ok["identity_binding_reload_governance"]["candidate_revision_approved"],
        false,
    );
    assert_eq!(body_ok["identity_binding_counts"]["chat_users"], 2,);
    assert_eq!(body_ok["identity_binding_counts"]["matrix_users"], 1,);
    assert_eq!(body_ok["identity_binding_metadata"]["revision"], "rev-b",);

    let _ = std::fs::remove_file(&temp_path);
}

#[tokio::test]
async fn reload_identity_registry_endpoint_returns_conflict_when_registry_not_configured() {
    let temp_path = temp_identity_bindings_path("registry-not-configured");
    std::fs::write(
        &temp_path,
        r#"{"version":1,"revision":"bind-a","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());

    let app = build_router(AppState::new(config));

    let (status, body) = send_identity_registry_reload_request(&app, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["reloaded"], false);
    assert_eq!(body["error"], "identity_registry_not_configured");

    let _ = std::fs::remove_file(&temp_path);
}

#[tokio::test]
async fn reload_identity_registry_endpoint_rejects_missing_product_user_refs() {
    let temp_bindings_path = temp_identity_bindings_path("registry-missing-ref-bindings");
    let temp_registry_path = temp_identity_bindings_path("registry-missing-ref-file");
    std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-old","account_id":"acct-old"}}}"#,
        )
        .expect("write initial identity registry");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_revision = true;
    config.identity_binding_reload_reject_same_revision = true;

    let app = build_router(AppState::new(config));

    std::fs::write(
        &temp_registry_path,
        r#"{"version":1,"revision":"reg-b","product_users":{}}"#,
    )
    .expect("write broken identity registry");

    let (status, body) = send_identity_registry_reload_request(&app, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["reloaded"], false);
    assert_eq!(
        body["identity_binding_reload_governance"]["reason"],
        "missing_product_user_refs",
    );
    assert_eq!(
        body["identity_binding_reload_governance"]["candidate_missing_product_user_refs"],
        1,
    );
    assert_eq!(
        body["identity_binding_counts"]["missing_product_user_refs"],
        1
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_registry_path);
}

#[tokio::test]
async fn reload_identity_registry_endpoint_returns_success_after_registry_revision_bump() {
    let temp_bindings_path = temp_identity_bindings_path("registry-reload-bindings");
    let temp_registry_path = temp_identity_bindings_path("registry-reload-file");
    let temp_audit_path = temp_identity_bindings_path("registry-reload-audit");
    std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-old","account_id":"acct-old"}}}"#,
        )
        .expect("write initial identity registry");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
    config.identity_binding_audit_log_path = Some(temp_audit_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_revision = true;
    config.identity_binding_reload_reject_same_revision = true;

    let app = build_router(AppState::new(config));

    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-b","product_users":{"pu-1":{"org_id":"org-new","account_id":"acct-new"},"pu-2":{"org_id":"org-extra"}}}"#,
        )
        .expect("write revised identity registry");

    let (status, body) = send_identity_registry_reload_request(&app, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["reloaded"], true);
    assert_eq!(body["identity_binding_reload_governance"]["accepted"], true,);
    assert_eq!(
        body["identity_binding_reload_governance"]["reason"],
        "policy_ok",
    );
    assert_eq!(
        body["identity_binding_reload_governance"]["current_revision"],
        "bind-a",
    );
    assert_eq!(
        body["identity_binding_reload_governance"]["candidate_revision"],
        "bind-a",
    );
    assert_eq!(
        body["identity_binding_reload_governance"]["current_registry_revision"],
        "reg-a",
    );
    assert_eq!(
        body["identity_binding_reload_governance"]["candidate_registry_revision"],
        "reg-b",
    );
    assert_eq!(
        body["identity_binding_reload_governance"]["current_effective_revision"],
        "binding:bind-a|registry:reg-a",
    );
    assert_eq!(
        body["identity_binding_reload_governance"]["candidate_effective_revision"],
        "binding:bind-a|registry:reg-b",
    );
    assert_eq!(
        body["identity_binding_reload_governance"]["candidate_registry_load_status"],
        "loaded",
    );
    assert_eq!(body["identity_registry_metadata"]["revision"], "reg-b");
    assert_eq!(body["identity_binding_counts"]["product_users"], 2);
    assert_eq!(
        body["identity_binding_audit"]["last_event_kind"],
        "registry_reload"
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_audit_path);
}

#[tokio::test]
async fn validate_identity_registry_endpoint_previews_candidate_without_applying_it() {
    let temp_bindings_path = temp_identity_bindings_path("registry-validate-bindings");
    let temp_registry_path = temp_identity_bindings_path("registry-validate-file");
    std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-old","account_id":"acct-old"}}}"#,
        )
        .expect("write initial identity registry");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_revision = true;
    config.identity_binding_reload_reject_same_revision = true;

    let app = build_router(AppState::new(config));

    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-b","product_users":{"pu-1":{"org_id":"org-new","account_id":"acct-new"}}}"#,
        )
        .expect("write candidate identity registry");

    let (validate_status, validate_body) = send_identity_registry_validate_request(&app, &[]).await;
    assert_eq!(validate_status, StatusCode::OK);
    assert_eq!(validate_body["ok"], true);
    assert_eq!(validate_body["validated"], true);
    assert_eq!(validate_body["valid"], true);
    assert_eq!(validate_body["checked_only"], true);
    assert_eq!(validate_body["would_reload"], true);
    assert_eq!(
        validate_body["identity_registry_metadata"]["revision"],
        "reg-b"
    );
    assert_eq!(
        validate_body["identity_binding_reload_governance"]["current_effective_revision"],
        "binding:bind-a|registry:reg-a",
    );
    assert_eq!(
        validate_body["identity_binding_reload_governance"]["candidate_effective_revision"],
        "binding:bind-a|registry:reg-b",
    );
    assert_eq!(
        validate_body["effective_revision"],
        "binding:bind-a|registry:reg-b"
    );

    let (status_status, status_body) = send_identity_registry_status_request(&app, &[]).await;
    assert_eq!(status_status, StatusCode::OK);
    assert_eq!(status_body["ok"], true);
    assert_eq!(
        status_body["effective_revision"],
        "binding:bind-a|registry:reg-a"
    );
    assert_eq!(
        status_body["identity_registry_metadata"]["revision"],
        "reg-a"
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_registry_path);
}

#[tokio::test]
async fn validate_identity_registry_endpoint_rejects_missing_product_user_refs() {
    let temp_bindings_path = temp_identity_bindings_path("registry-validate-missing-bindings");
    let temp_registry_path = temp_identity_bindings_path("registry-validate-missing-file");
    std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-old","account_id":"acct-old"}}}"#,
        )
        .expect("write initial identity registry");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_revision = true;
    config.identity_binding_reload_reject_same_revision = true;

    let app = build_router(AppState::new(config));

    std::fs::write(
        &temp_registry_path,
        r#"{"version":1,"revision":"reg-b","product_users":{}}"#,
    )
    .expect("write broken candidate identity registry");

    let (status, body) = send_identity_registry_validate_request(&app, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["validated"], true);
    assert_eq!(body["valid"], false);
    assert_eq!(body["would_reload"], false);
    assert_eq!(body["missing_product_user_refs"], 1);
    assert_eq!(
        body["identity_binding_reload_governance"]["reason"],
        "missing_product_user_refs",
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_registry_path);
}

#[tokio::test]
async fn identity_approval_status_endpoint_reports_current_effective_revision_state() {
    let temp_bindings_path = temp_identity_bindings_path("approval-status-bindings");
    let temp_registry_path = temp_identity_bindings_path("approval-status-registry");
    let temp_approval_path = temp_identity_bindings_path("approval-status-file");
    std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-a","account_id":"acct-a"}}}"#,
        )
        .expect("write initial identity registry");
    std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["binding:bind-a|registry:reg-a","binding:bind-b|registry:reg-b"]}"#,
        )
        .expect("write approval state");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
    config.identity_binding_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());

    let app = build_router(AppState::new(config));
    let (status, body) = send_identity_approval_status_request(&app, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["approval_valid"], true);
    assert_eq!(body["identity_approval_checks"]["status"], "ok");
    assert_eq!(
        body["identity_approval_checks"]["current_effective_revision"],
        "binding:bind-a|registry:reg-a",
    );
    assert_eq!(
        body["identity_approval_checks"]["current_effective_revision_approved"],
        true,
    );
    assert_eq!(
        body["identity_approval_checks"]["current_effective_revision_index"],
        0
    );
    assert_eq!(
        body["identity_approval_checks"]["latest_approved_revision"],
        "binding:bind-b|registry:reg-b",
    );
    assert_eq!(
        body["identity_approval_checks"]["current_matches_latest_approved"],
        false
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn identity_approval_validate_endpoint_rejects_unapproved_current_effective_revision() {
    let temp_bindings_path = temp_identity_bindings_path("approval-validate-bindings");
    let temp_registry_path = temp_identity_bindings_path("approval-validate-registry");
    let temp_approval_path = temp_identity_bindings_path("approval-validate-file");
    std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-a","account_id":"acct-a"}}}"#,
        )
        .expect("write initial identity registry");
    std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["binding:bind-b|registry:reg-b"]}"#,
        )
        .expect("write approval state");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
    config.identity_binding_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());

    let app = build_router(AppState::new(config));
    let (status, body) = send_identity_approval_validate_request(&app, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["validated"], true);
    assert_eq!(body["valid"], false);
    assert_eq!(body["status"], "current_effective_revision_not_approved");
    assert_eq!(
        body["identity_approval_checks"]["current_effective_revision_approved"],
        false,
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn identity_approval_source_endpoint_returns_latest_revisions_preview() {
    let temp_bindings_path = temp_identity_bindings_path("approval-source-bindings");
    let temp_approval_path = temp_identity_bindings_path("approval-source-file");
    std::fs::write(
        &temp_bindings_path,
        r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");
    std::fs::write(
        &temp_approval_path,
        r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-a","rev-b","rev-c"]}"#,
    )
    .expect("write approval state");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_binding_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());

    let app = build_router(AppState::new(config));
    let (status, body) = send_identity_approval_source_request(
        &app,
        "/v1/admin/identity-approval/source?limit=2",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["source_valid"], true);
    assert_eq!(body["identity_approval_source"]["status"], "ok");
    assert_eq!(
        body["identity_approval_source"]["approved_revision_count"],
        3
    );
    assert_eq!(
        body["identity_approval_source"]["returned_revision_count"],
        2
    );
    assert_eq!(
        body["identity_approval_source"]["latest_approved_revision"],
        "rev-c"
    );
    assert_eq!(
        body["identity_approval_source"]["current_effective_revision"],
        "rev-b"
    );
    assert_eq!(
        body["identity_approval_source"]["revisions"][0]["revision"],
        "rev-c"
    );
    assert_eq!(
        body["identity_approval_source"]["revisions"][0]["is_latest"],
        true
    );
    assert_eq!(
        body["identity_approval_source"]["revisions"][1]["revision"],
        "rev-b"
    );
    assert_eq!(
        body["identity_approval_source"]["revisions"][1]["is_current_effective"],
        true
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn identity_approval_source_validate_endpoint_rejects_empty_revision_set() {
    let temp_approval_path = temp_identity_bindings_path("approval-source-empty");
    std::fs::write(
        &temp_approval_path,
        r#"{"version":1,"revision":"approval-a","approved_revisions":[]}"#,
    )
    .expect("write empty approval state");

    let mut config = test_config();
    config.identity_binding_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());

    let app = build_router(AppState::new(config));
    let (status, body) = send_identity_approval_source_validate_request(
        &app,
        "/v1/admin/identity-approval/source/validate",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["validated"], true);
    assert_eq!(body["valid"], false);
    assert_eq!(body["status"], "approved_revision_set_empty");
    assert_eq!(
        body["identity_approval_source"]["approved_revision_count"],
        0
    );
    assert_eq!(
        body["identity_approval_source"]["returned_revision_count"],
        0
    );

    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn health_endpoint_exposes_identity_governance_overview() {
    let temp_bindings_path = temp_identity_bindings_path("health-governance-bindings");
    let temp_approval_path = temp_identity_bindings_path("health-governance-approval");
    std::fs::write(
        &temp_bindings_path,
        r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");
    std::fs::write(
        &temp_approval_path,
        r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-b"]}"#,
    )
    .expect("write approval state");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_binding_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = Vec::new();
    config.league_normalized_database_url = Some("postgres://local/cex".to_string());
    config.league_normalized_dual_write_enabled = true;
    config.league_normalized_read_switch_enabled = true;
    config.league_normalized_final_cutover_enabled = true;

    let app = build_router(AppState::new(config));
    let (status, body) = send_health_request(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(
        body["identity_governance_overview"]["status"],
        "no_allowed_actors_configured"
    );
    assert_eq!(body["identity_governance_overview"]["valid"], false);
    assert_eq!(
        body["profile_validation"]["checks"]["identity_governance_valid"],
        false
    );
    assert_eq!(
        body["league_repository_runtime"]["normalized_dual_write_active"],
        true
    );
    assert_eq!(
        body["league_repository_runtime"]["normalized_read_switch_active"],
        true
    );
    assert_eq!(
        body["league_repository_runtime"]["effective_repository"],
        "normalized_sql_direct_write_final"
    );
    assert_eq!(
        body["league_repository_runtime"]["repository_cutover_status"],
        "normalized_sql_direct_write_final_cutover_active"
    );
    assert_eq!(
        body["league_repository_runtime"]["normalized_final_cutover_active"],
        true
    );
    assert_eq!(
        body["profile_validation"]["checks"]["league_normalized_read_switch_active"],
        true
    );
    assert_eq!(
        body["trillionnium_world_maturity"]["contract_version"],
        "trillionnium_world_maturity_axes_v1"
    );
    assert_eq!(
        body["trillionnium_world_maturity"]["target"],
        "all_4_axes_100_percent"
    );
    assert!(
        body["trillionnium_world_maturity"]["axes"]["first_playable"]["percent"]
            .as_u64()
            .is_some()
    );
    assert!(
        body["trillionnium_world_maturity"]["axes"]["technical_alpha"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |check| check["check_id"] == "effective_repository_is_normalized"
                    && check["passed"] == true
            )
    );
    assert_eq!(
        body["trillionnium_world_closed_beta_prototype"]["contract_version"],
        "trillionnium_world_closed_beta_prototype_v1"
    );
    assert_eq!(
        body["trillionnium_world_closed_beta_prototype"]["target"],
        "closed_beta_prototype_100_percent"
    );
    assert!(
        body["trillionnium_world_closed_beta_prototype"]["axes"]["access_governance"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "identity_governance_valid")
    );
    assert_eq!(
        body["trillionnium_world_real_user_beta"]["contract_version"],
        "trillionnium_world_real_user_beta_v1"
    );
    assert_eq!(
        body["trillionnium_world_real_user_beta"]["target"],
        "real_user_long_term_beta_100_percent"
    );
    assert!(
        body["trillionnium_world_real_user_beta"]["axes"]["access_safety"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "web_session_required_and_configured")
    );
    assert_eq!(
        body["trillionnium_world_public_commercial_product"]["contract_version"],
        "trillionnium_world_public_commercial_product_v1"
    );
    assert_eq!(
        body["trillionnium_world_public_commercial_product"]["target"],
        "public_commercial_product_100_percent"
    );
    assert!(
        body["trillionnium_world_public_commercial_product"]["axes"]["commercial_engine"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "active_listing_ready")
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn metrics_endpoint_exposes_identity_governance_gauges() {
    let temp_bindings_path = temp_identity_bindings_path("metrics-governance-bindings");
    let temp_approval_path = temp_identity_bindings_path("metrics-governance-approval");
    std::fs::write(
        &temp_bindings_path,
        r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");
    std::fs::write(
        &temp_approval_path,
        r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-b"]}"#,
    )
    .expect("write approval state");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_binding_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = Vec::new();

    let app = build_router(AppState::new(config));
    let (status, body) = send_metrics_request(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("cex_consumer_entry_identity_governance_valid 0"));
    assert!(body.contains("cex_consumer_entry_identity_binding_loaded 1"));
    assert!(body.contains("cex_consumer_entry_identity_registry_loaded 1"));
    assert!(body.contains("cex_consumer_entry_identity_ref_integrity_ok 1"));
    assert!(body.contains("cex_consumer_entry_identity_actor_gate_valid 0"));
    assert!(body.contains("cex_consumer_entry_identity_approval_source_valid 1"));
    assert!(body.contains("cex_consumer_entry_identity_approval_coverage_valid 1"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_maturity_overall_percent"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_maturity_first_playable_percent"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_maturity_technical_alpha_percent"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_maturity_beta_readiness_percent"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_maturity_full_vision_percent"));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_world_closed_beta_prototype_overall_percent"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_closed_beta_prototype_product_loop_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_closed_beta_prototype_access_governance_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_closed_beta_prototype_persistence_runtime_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_closed_beta_prototype_world_depth_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_closed_beta_prototype_commerce_recovery_percent"
    ));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_real_user_beta_overall_percent"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_real_user_beta_product_retention_percent"
    ));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_world_real_user_beta_access_safety_percent")
    );
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_real_user_beta_durable_persistence_percent"
    ));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_world_real_user_beta_economy_recovery_percent"));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_world_real_user_beta_world_capacity_percent"));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_world_real_user_beta_ops_runtime_percent")
    );
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_public_commercial_product_overall_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_public_commercial_product_public_launch_surface_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_public_commercial_product_commercial_engine_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_public_commercial_product_trust_safety_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_public_commercial_product_durable_scale_ops_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_public_commercial_product_growth_network_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_public_commercial_product_public_world_depth_percent"
    ));

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn health_endpoint_exposes_session_auth_registry_governance_overview() {
    let temp_registry_path = temp_identity_bindings_path("health-session-auth-registry");
    let temp_approval_path = temp_identity_bindings_path("health-session-auth-approval");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-health-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
    std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-health-a","approved_revisions":["sess-reg-other"]}"#,
        )
        .expect("write session auth approval state");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
    let mut config = test_config();
    config.require_session_auth = true;
    config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry = registry;
    config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
    config.session_auth_issuer_registry_metadata = metadata;
    config.session_auth_issuer_registry_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_require_approved_revision = true;

    let app = build_router(AppState::new(config));
    let (status, body) = send_health_request(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["session_auth_issuer_registry_governance_overview"]["status"],
        "current_revision_not_approved"
    );
    assert_eq!(
        body["session_auth_issuer_registry_governance_overview"]["valid"],
        false
    );
    assert_eq!(
        body["profile_validation"]["checks"]["session_auth_issuer_registry_governance_valid"],
        false
    );

    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn metrics_endpoint_exposes_session_auth_registry_governance_gauges() {
    let temp_registry_path = temp_identity_bindings_path("metrics-session-auth-registry");
    let temp_approval_path = temp_identity_bindings_path("metrics-session-auth-approval");
    std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-metrics-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
    std::fs::write(
        &temp_approval_path,
        r#"{"version":1,"revision":"sess-approval-metrics-a","approved_revisions":[]}"#,
    )
    .expect("write session auth approval state");

    let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
    let mut config = test_config();
    config.require_session_auth = true;
    config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
    config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
    config.session_auth_issuer_registry_path =
        Some(temp_registry_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry = registry;
    config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
    config.session_auth_issuer_registry_metadata = metadata;
    config.session_auth_issuer_registry_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.session_auth_issuer_registry_require_approved_revision = true;

    let app = build_router(AppState::new(config));
    let (status, body) = send_metrics_request(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("cex_consumer_entry_session_auth_issuer_registry_governance_valid 0"));
    assert!(
        body.contains("cex_consumer_entry_session_auth_issuer_registry_approval_source_valid 0")
    );
    assert!(
        body.contains("cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid 0")
    );

    let _ = std::fs::remove_file(&temp_registry_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn identity_governance_status_endpoint_returns_combined_overview() {
    let temp_bindings_path = temp_identity_bindings_path("governance-status-bindings");
    let temp_approval_path = temp_identity_bindings_path("governance-status-approval");
    std::fs::write(
        &temp_bindings_path,
        r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");
    std::fs::write(
        &temp_approval_path,
        r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-a","rev-b"]}"#,
    )
    .expect("write approval state");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_binding_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];

    let app = build_router(AppState::new(config));
    let (status, body) = send_identity_governance_status_request(
        &app,
        "/v1/admin/identity-governance/status?limit=1",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["governance_valid"], true);
    assert_eq!(body["identity_governance_overview"]["status"], "ok");
    assert_eq!(body["identity_governance_overview"]["valid"], true);
    assert_eq!(
        body["identity_governance_overview"]["checks"]["binding_loaded"],
        true
    );
    assert_eq!(
        body["identity_governance_overview"]["checks"]["actor_gate_valid"],
        true
    );
    assert_eq!(
        body["identity_governance_overview"]["checks"]["approval_source_valid"],
        true
    );
    assert_eq!(
        body["identity_governance_overview"]["checks"]["approval_coverage_valid"],
        true
    );
    assert_eq!(
        body["identity_governance_overview"]["identity_approval_source"]["returned_revision_count"],
        1
    );
    assert_eq!(
        body["identity_governance_overview"]["identity_actor_checks"]["allowed_actor_count"],
        1
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn identity_governance_validate_endpoint_rejects_invalid_actor_gate() {
    let temp_bindings_path = temp_identity_bindings_path("governance-validate-bindings");
    let temp_approval_path = temp_identity_bindings_path("governance-validate-approval");
    std::fs::write(
        &temp_bindings_path,
        r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
    )
    .expect("write initial identity bindings");
    std::fs::write(
        &temp_approval_path,
        r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-b"]}"#,
    )
    .expect("write approval state");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_binding_approved_revisions_path =
        Some(temp_approval_path.to_string_lossy().to_string());
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = Vec::new();

    let app = build_router(AppState::new(config));
    let (status, body) = send_identity_governance_validate_request(
        &app,
        "/v1/admin/identity-governance/validate",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["validated"], true);
    assert_eq!(body["valid"], false);
    assert_eq!(body["status"], "no_allowed_actors_configured");
    assert_eq!(
        body["identity_governance_overview"]["checks"]["actor_gate_valid"],
        false
    );
    assert_eq!(
        body["identity_governance_overview"]["identity_actor_checks"]["status"],
        "no_allowed_actors_configured"
    );

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_approval_path);
}

#[tokio::test]
async fn identity_actor_status_endpoint_reports_current_actor_gate_configuration() {
    let mut config = test_config();
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_actor_header = "x-deploy-actor".to_string();
    config.identity_binding_reload_allowed_actors = vec!["alice".to_string(), "bob".to_string()];

    let app = build_router(AppState::new(config));
    let (status, body) = send_identity_actor_status_request(&app, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["actor_valid"], true);
    assert_eq!(body["identity_actor_checks"]["status"], "ok");
    assert_eq!(body["identity_actor_checks"]["require_actor"], true);
    assert_eq!(
        body["identity_actor_checks"]["actor_header"],
        "x-deploy-actor"
    );
    assert_eq!(body["identity_actor_checks"]["allowed_actor_count"], 2);
    assert_eq!(body["identity_actor_checks"]["allowed_actors"][0], "alice");
    assert_eq!(body["identity_actor_checks"]["allowed_actors"][1], "bob");
}

#[tokio::test]
async fn identity_actor_validate_endpoint_rejects_missing_allowed_actors() {
    let mut config = test_config();
    config.identity_binding_reload_require_actor = true;
    config.identity_binding_reload_allowed_actors = Vec::new();

    let app = build_router(AppState::new(config));
    let (status, body) = send_identity_actor_validate_request(&app, &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["validated"], true);
    assert_eq!(body["valid"], false);
    assert_eq!(body["status"], "no_allowed_actors_configured");
    assert_eq!(body["identity_actor_checks"]["valid"], false);
    assert_eq!(body["identity_actor_checks"]["allowed_actor_count"], 0);
}

#[tokio::test]
async fn identity_registry_audit_endpoint_returns_conflict_when_audit_not_configured() {
    let app = build_router(AppState::new(test_config()));

    let (status, body) =
        send_identity_registry_audit_request(&app, "/v1/admin/identity-registry/audit", &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "identity_audit_not_configured");
}

#[tokio::test]
async fn identity_registry_audit_endpoint_returns_latest_registry_events_only() {
    let temp_audit_path = temp_identity_bindings_path("registry-audit-read");
    let mut config = test_config();
    config.identity_binding_audit_log_path = Some(temp_audit_path.to_string_lossy().to_string());

    let app = build_router(AppState::new(config));
    std::fs::write(
        &temp_audit_path,
        concat!(
            "{\"event_kind\":\"reload\",\"event_epoch\":1}\n",
            "{\"event_kind\":\"registry_reload_rejected\",\"event_epoch\":2}\n",
            "not-json\n",
            "{\"event_kind\":\"registry_reload\",\"event_epoch\":3}\n"
        ),
    )
    .expect("write audit fixture");

    let (status_one, body_one) = send_identity_registry_audit_request(
        &app,
        "/v1/admin/identity-registry/audit?limit=1",
        &[],
    )
    .await;
    assert_eq!(status_one, StatusCode::OK);
    assert_eq!(body_one["ok"], true);
    assert_eq!(body_one["returned_event_count"], 1);
    assert_eq!(body_one["parse_error_count"], 0);
    assert_eq!(body_one["events"][0]["event_kind"], "registry_reload");

    let (status_all, body_all) = send_identity_registry_audit_request(
        &app,
        "/v1/admin/identity-registry/audit?limit=5",
        &[],
    )
    .await;
    assert_eq!(status_all, StatusCode::OK);
    assert_eq!(body_all["returned_event_count"], 2);
    assert_eq!(body_all["parse_error_count"], 1);
    assert_eq!(body_all["events"][0]["event_kind"], "registry_reload");
    assert_eq!(
        body_all["events"][1]["event_kind"],
        "registry_reload_rejected"
    );

    let _ = std::fs::remove_file(&temp_audit_path);
}

#[tokio::test]
async fn reload_identity_bindings_endpoint_appends_audit_event_when_path_configured() {
    let temp_bindings_path = temp_identity_bindings_path("audit-path");
    std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{"chat-1":{"org_id":"org-old"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

    let temp_audit_path = temp_identity_bindings_path("audit-log");
    let mut config = test_config();
    config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
    config.identity_binding_audit_log_path = Some(temp_audit_path.to_string_lossy().to_string());
    config.identity_binding_reload_reject_same_revision = false;

    let app = build_router(AppState::new(config));

    std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{"chat-1":{"org_id":"org-new"},"chat-2":{"org_id":"org-new2"}},"matrix_users":{}}"#,
        )
        .expect("write revised identity bindings");

    let (status_ok, body_ok) = send_identity_binding_reload_request(&app, &[]).await;
    assert_eq!(status_ok, StatusCode::OK);
    assert_eq!(
        body_ok["identity_binding_audit"]["path"],
        temp_audit_path.to_string_lossy().to_string()
    );
    assert_eq!(body_ok["identity_binding_audit"]["last_status"], "written");
    assert_eq!(
        body_ok["identity_binding_audit"]["last_event_kind"],
        "reload"
    );
    assert_eq!(
        body_ok["identity_binding_audit"]["last_policy_decision"],
        "accepted"
    );

    let raw_audit = std::fs::read_to_string(&temp_audit_path).expect("read audit log");
    let lines: Vec<&str> = raw_audit
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(!lines.is_empty(), "expected at least one audit event");
    let event: Value = serde_json::from_str(lines.last().expect("latest audit event exists"))
        .expect("decode audit event jsonl");
    assert_eq!(event["event_kind"], "reload");
    assert_eq!(event["identity_binding_metadata"]["revision"], "rev-b");
    assert_eq!(event["identity_binding_metadata"]["load_status"], "loaded");
    assert_eq!(event["identity_binding_counts"]["chat_users"], 2);
    assert_eq!(event["identity_binding_counts"]["matrix_users"], 0);
    assert_eq!(event["governance"]["accepted"], true);

    let _ = std::fs::remove_file(&temp_bindings_path);
    let _ = std::fs::remove_file(&temp_audit_path);
}

#[tokio::test]
async fn reload_identity_bindings_endpoint_shows_disabled_audit_when_path_not_configured() {
    let temp_path = temp_identity_bindings_path("audit-path-missing");
    std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{"chat-1":{"org_id":"org-old"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

    let mut config = test_config();
    config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
    config.identity_binding_reload_reject_same_revision = false;

    let app = build_router(AppState::new(config));

    std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{"chat-1":{"org_id":"org-new"},"chat-2":{"org_id":"org-new2"}},"matrix_users":{"mx-1":{"org_id":"org-mx"}}}"#,
        )
        .expect("write revised identity bindings");

    let (status_ok, body_ok) = send_identity_binding_reload_request(&app, &[]).await;
    assert_eq!(status_ok, StatusCode::OK);
    assert!(body_ok["identity_binding_audit"]["path"].is_null());
    assert_eq!(body_ok["identity_binding_audit"]["last_status"], "disabled");
    assert!(body_ok["identity_binding_audit"]["last_policy_decision"].is_null());

    let _ = std::fs::remove_file(&temp_path);
}

use super::{
    authorize_league_web_session, authorize_league_web_session_readonly, authorize_user_session,
    build_chat_identity_scope, build_chat_org_rate_limit_key, build_chat_rate_limit_key,
    build_chat_replay_key, build_chat_request_fingerprint, build_chat_room_rate_limit_key,
    build_chat_session_rate_limit_key, build_chat_user_rate_limit_key, build_matrix_identity_scope,
    build_matrix_org_rate_limit_key, build_matrix_rate_limit_key, build_matrix_replay_key,
    build_matrix_room_rate_limit_key, build_matrix_session_rate_limit_key,
    build_matrix_user_rate_limit_key, build_router, client_app_json, client_feed_json,
    default_league_state, encode_league_web_session, evaluate_identity_binding_reload_governance,
    get_client_app_web_shell, get_world_web_shell, league_hash_id, league_hidden_test_event,
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
    IdentityBindingStore, IdentityBindings, LeagueMatchEntry, LeaguePlayer, LeagueReward,
    LeagueStateRepositorySnapshot, LeagueSubmission, LeagueWebSessionClaims, MatrixMessageRequest,
    ProductUserIdentity, RateLimitCache, ReplayCache, RuntimeProfile,
    SessionAuthIssuerRegistryIssuer, SessionAuthIssuerRegistryMetadata,
    SessionAuthIssuerRegistryRuntimeState, UserSessionAuthClaims, WorldAsset, WorldCompany,
    WorldContract, WorldContractCompletion, WorldEconomyEvent, WorldEvent, WorldListing,
    WorldMapNode, WorldPlayerPosition, WorldPurchase, WorldRelationship, WorldShop, WorldWorkOrder,
    DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS, DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS,
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
use ledger_service::{
    build_router as build_ledger_router, repository::postgres::PostgresLedgerRepository,
    state::AppState as LedgerAppState,
};
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
    assert!(full_sql.contains("begin;"));
    assert!(full_sql.contains("commit;"));

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
    assert!(!runtime_sql.contains("begin;"));
    assert!(!runtime_sql.contains("commit;"));
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
    assert!(!bridge_sql.contains("begin;"));
    assert!(!bridge_sql.contains("commit;"));
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
fn league_web_session_readonly_validates_cookie_without_requiring_csrf() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    config.league_web_session_required = true;
    config.league_web_session_secret = Some("readonly-session-secret".to_string());
    let cookie_name = config.league_web_session_cookie_name.clone();
    let secret = config.league_web_session_secret.clone().unwrap();
    let state = test_state(config, IdentityBindings::default(), HashMap::new());

    let claims = LeagueWebSessionClaims {
        version: 1,
        matrix_user_id: "@alice:local.dev".to_string(),
        room_id: Some("!room:local.dev".to_string()),
        session_id: Some("readonly-test".to_string()),
        csrf: "csrf-readonly-test".to_string(),
        issued_at_epoch: Utc::now().timestamp(),
        expires_at_epoch: Utc::now().timestamp() + 300,
    };
    let token = encode_league_web_session(&claims, &secret).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("cookie", format!("{cookie_name}={token}").parse().unwrap());

    let readonly = authorize_league_web_session_readonly(&state, &headers, false)
        .unwrap()
        .unwrap();
    assert_eq!(readonly.matrix_user_id, "@alice:local.dev");
    assert!(authorize_league_web_session(&state, &headers, None).is_err());
    assert!(
        authorize_league_web_session(&state, &headers, Some(&claims.csrf))
            .unwrap()
            .is_some()
    );

    let empty_headers = HeaderMap::new();
    assert!(
        authorize_league_web_session_readonly(&state, &empty_headers, true)
            .unwrap()
            .is_none()
    );
    assert!(authorize_league_web_session_readonly(&state, &empty_headers, false).is_err());
}

#[tokio::test]
async fn league_web_shell_explains_score_and_reward_formula() {
    let app = build_router(AppState::new(test_config()));
    let (status, body) = send_text_request(&app, "GET", "/league", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("id=\"league-scoring-rewards\""));
    assert!(body.contains("Scoring &amp; Rewards") || body.contains("Scoring & Rewards"));
    assert!(body.contains("Reward formula"));
    assert!(body.contains("Reward = score"));
    assert!(body.contains("delivery 30%"));
    assert!(body.contains("cex") || body.contains("score-mini"));
    let judgement = crate::judge_league_submission(
        "Guild raid delivery with scout, builder, evidence, risk controls, next step, and team review.",
        "guild_raid",
    );
    assert!(judgement.score_events.iter().any(|event| {
        event.dimension == "encounter_state"
            && event
                .evidence
                .get("contract_version")
                .and_then(Value::as_str)
                == Some("trillionnium_league_encounter_state_v1")
    }));
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
    assert_eq!(
        home["playability_runtime"]["contract_version"],
        "trillionnium_world_playability_runtime_v1"
    );
    assert_eq!(
        home["playability_runtime"]["optimization_scope"],
        "p0_p1_p2_full_playability"
    );
    assert!(home["playability_runtime"]["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|lane| lane["lane_id"] == "p1_strategy_depth"));

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
    assert_eq!(
        app["playability_coach"]["contract_version"],
        "trillionnium_playability_coach_v1"
    );
    assert_eq!(
        app["playability_coach"]["optimization_scope"],
        "p0_p1_p2_full_playability"
    );
    let coach_lanes = app["playability_coach"]["lanes"].as_array().unwrap();
    for expected_lane in ["p0_first_session", "p1_strategy_depth", "p2_retention_ops"] {
        assert!(coach_lanes
            .iter()
            .any(|lane| lane["lane_id"] == expected_lane));
    }
    assert!(
        app["playability_coach"]["next_best_actions"]
            .as_array()
            .unwrap()
            .len()
            >= 4
    );
    let coach_checks = app["playability_coach"]["readiness_checks"]
        .as_array()
        .unwrap();
    for expected_check in [
        "p0_next_best_action_visible",
        "p1_economy_tradeoffs_visible",
        "p1_social_coop_choices_visible",
        "p2_telemetry_contract_visible",
        "economy_tradeoff_cards_visible",
        "retention_calendar_visible",
        "playability_funnel_visible",
        "anti_cheese_policy_visible",
        "ops_refresh_hooks_visible",
    ] {
        assert!(coach_checks.iter().any(|check| check == expected_check));
    }
    assert_eq!(
        app["economy_retention_ops"]["contract_version"],
        "trillionnium_economy_retention_ops_v1"
    );
    assert!(app["economy_retention_ops"]["economy_tradeoff_cards"]
        .as_array()
        .is_some_and(|cards| cards.len() >= 4));
    assert!(app["economy_retention_ops"]["playability_funnel"]["steps"]
        .as_array()
        .is_some_and(|steps| steps.len() >= 7));
    assert!(
        app["economy_retention_ops"]["anti_cheese_policy"]["cooldown_seconds"]
            .as_i64()
            .unwrap_or(0)
            >= 300
    );
    assert_eq!(
        app["economy_retention_ops"]["anti_cheese_policy"]["duplicate_gate"],
        "review_hold_zero_reward"
    );
    assert_eq!(
        app["economy_retention_ops"]["anti_cheese_policy"]["backend_gate_enforced"],
        true
    );
    assert_eq!(
        app["economy_retention_ops"]["engine_contracts"]["world_action_engine"],
        "trillionnium_world_action_engine_v1"
    );
    assert_eq!(
        app["economy_retention_ops"]["engine_contracts"]["market_simulator"],
        "trillionnium_market_simulator_v1"
    );
    assert_eq!(
        app["economy_retention_ops"]["engine_contracts"]["league_encounter_state"],
        "trillionnium_league_encounter_state_v1"
    );
    assert_eq!(
        app["economy_retention_ops"]["playability_balance_config"]["contract_version"],
        "trillionnium_playability_balance_config_v1"
    );
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
    assert_eq!(
        app["mobile_shell_contract"]["contract_version"],
        "trillionnium_mobile_shell_ux_v1"
    );
    let mobile_shell_checks = app["mobile_shell_contract"]["readiness_checks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for check in [
        "mobile_tablist_a11y_visible",
        "keyboard_tab_navigation_visible",
        "search_empty_state_visible",
        "search_clear_and_escape_visible",
        "aria_live_ux_status_visible",
        "offline_feed_fallback_status_visible",
        "web_session_feed_hydration_visible",
        "next_action_rail_visible",
        "playability_coach_visible",
        "p0_next_best_action_visible",
        "p1_strategy_choices_visible",
        "p2_retention_telemetry_visible",
        "failure_recovery_lane_visible",
        "economy_tradeoff_cards_visible",
        "retention_calendar_visible",
        "playability_funnel_visible",
        "anti_cheese_policy_visible",
        "ops_refresh_hooks_visible",
    ] {
        assert!(mobile_shell_checks.iter().any(|value| value == check));
    }
    assert_eq!(app["feed"]["web_session_path"], "/app/web/feed");
    let beta_checks = app["onboarding"]["beta_readiness_checks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(beta_checks
        .iter()
        .any(|value| value == "mobile_tablist_a11y_visible"));
    assert!(app["modules"][0]["summary"]
        .as_str()
        .unwrap_or_default()
        .contains("实时事件"));
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
    assert!(app_html.contains("条事件镜头"));
    assert!(app_html.contains("按焦点筛选路线"));
    assert!(app_html.contains("显示完整路线"));
    assert!(app_html.contains("selectionActionButtonHtml"));
    assert!(app_html.contains("事件简报："));
    assert!(app_html.contains("web_event_id"));
    assert!(app_html.contains("app-global-search"));
    assert!(app_html.contains("app-search-clear"));
    assert!(app_html.contains("app-search-empty-state"));
    assert!(app_html.contains("app-ux-live-status"));
    assert!(app_html.contains("trillionnium_mobile_shell_ux_v1"));
    assert!(app_html.contains("keyboard_tab_navigation_visible"));
    assert!(app_html.contains("offline_feed_fallback_status_visible"));
    assert!(app_html.contains("web_session_feed_hydration_visible"));
    assert!(app_html.contains("/app/web/feed"));
    assert!(app_html.contains("app-bottom-tabs"));
    assert!(app_html.contains("role=\"tablist\""));
    assert!(app_html.contains("role=\"tab\""));
    assert!(app_html.contains("role=\"tabpanel\""));
    assert!(app_html.contains("aria-selected=\"true\""));
    assert!(app_html.contains("handleAppTabKeydown"));
    assert!(app_html.contains("announceUxStatus"));
    assert!(app_html.contains("app-tab-messages"));
    assert!(app_html.contains("app-tab-map"));
    assert!(app_html.contains("app-tab-feed"));
    assert!(app_html.contains("app-tab-me"));
    assert!(app_html.contains("消息"));
    assert!(app_html.contains("data-app-tab=\"map\" role=\"tab\""));
    assert!(app_html.contains("aria-controls=\"app-tab-map\""));
    assert!(app_html.contains("动态"));
    assert!(app_html.contains("我"));
    assert!(app_html.contains("app-first-playable-onboarding"));
    assert!(app_html.contains("app-playability-coach"));
    assert!(app_html.contains("新手主线"));
    assert!(app_html.contains("first_playable_loop_100"));
    assert!(app_html.contains("trillionnium_first_playable_onboarding_v1"));
    assert!(app_html.contains("trillionnium_playability_coach_v1"));
    assert!(app_html.contains("data-playability-lane=\"p0_first_session\""));
    assert!(app_html.contains("data-playability-lane=\"p1_strategy_depth\""));
    assert!(app_html.contains("data-playability-lane=\"p2_retention_ops\""));
    assert!(app_html.contains("data-onboarding-step=\"quest_delivery\""));
    assert!(app_html.contains("route_task_graph_next_action_visible"));
    assert!(app_html.contains("playability_coach_visible"));
    assert!(app_html.contains("p2_retention_telemetry_visible"));
    assert!(app_html.contains("app-economy-retention-ops"));
    assert!(app_html.contains("economy_tradeoff_cards_visible"));
    assert!(app_html.contains("playability_funnel_visible"));
    assert!(app_html.contains("anti_cheese_policy_visible"));
    assert!(app_html.contains("data-economy-tradeoff=\"high_reward_delivery\""));
    assert!(app_html.contains("/v1/client/feed/@alice:local.dev"));
    assert!(app_html.contains("app-feed-api-status"));
    assert!(app_html.contains("app-feed-filter-actions"));
    assert!(app_html.contains("app-feed-summary"));
    assert!(app_html.contains("app-feed-items-live"));
    assert!(app_html.contains("世界动态时间线"));
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
    assert!(app_html.contains("routePlayabilityBody"));
    assert!(app_html.contains("\"contract_version\":1"));

    let world_html = get_world_web_shell(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::extract::Query(HashMap::new()),
    )
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
    let mut recovery_query = HashMap::new();
    recovery_query.insert("recovery".to_string(), "action-input".to_string());
    let world_recovery_html = get_world_web_shell(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::extract::Query(recovery_query),
    )
    .await
    .0;
    assert!(world_recovery_html.contains("world-action-recovery-card"));
    assert!(world_recovery_html.contains("data-recovery=\"world_action_failure\""));
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
    assert!(world_html.contains("条事件镜头"));
    assert!(world_html.contains("selectionActionButtonHtml"));
    assert!(world_html.contains("事件简报："));
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
    assert!(world_html.contains("routePlayabilityBody"));
    assert!(world_html.contains("\"contract_version\":1"));
    assert!(world_html.contains("customer deliverable"));
    assert!(world_html.contains("evidence package, risk controls, next action"));
    assert!(world_html.contains("world-work-cancel-body"));
    assert!(world_html.contains("refund risk controls, next action, and self-review"));
    assert!(world_html.contains("客户交付方案"));
    assert!(world_html.contains("退款风险控制"));
}

fn prompt_has_delivery_anchor(body: &str, lower: &str) -> bool {
    lower.contains("deliver")
        || lower.contains("customer")
        || body.contains("客户")
        || body.contains("交付")
        || body.contains("成果")
        || body.contains("方案")
}

fn prompt_has_evidence_anchor(body: &str, lower: &str) -> bool {
    lower.contains("evidence")
        || lower.contains("proof")
        || lower.contains("source")
        || lower.contains("data")
        || body.contains("证据")
        || body.contains("依据")
}

fn prompt_has_risk_anchor(body: &str, lower: &str) -> bool {
    lower.contains("risk") || body.contains("风险")
}

fn prompt_has_next_anchor(body: &str, lower: &str) -> bool {
    lower.contains("next") || body.contains("下一步") || body.contains("计划")
}

fn prompt_has_review_anchor(body: &str, lower: &str) -> bool {
    lower.contains("review")
        || lower.contains("self-check")
        || lower.contains("self check")
        || lower.contains("self-review")
        || body.contains("自评")
        || body.contains("自检")
        || body.contains("复盘")
}

fn assert_hidden_test_ready_prompt(label: &str, body: &str) {
    let lower = body.to_ascii_lowercase();
    assert!(
        prompt_has_delivery_anchor(body, &lower),
        "{label} missing customer/deliverable anchor: {body}"
    );
    assert!(
        prompt_has_evidence_anchor(body, &lower),
        "{label} missing evidence anchor: {body}"
    );
    assert!(
        prompt_has_risk_anchor(body, &lower),
        "{label} missing risk anchor: {body}"
    );
    assert!(
        prompt_has_next_anchor(body, &lower),
        "{label} missing next-action anchor: {body}"
    );
    assert!(
        prompt_has_review_anchor(body, &lower),
        "{label} missing self-review anchor: {body}"
    );
    let (hidden_event, flags) = league_hidden_test_event(body, "world_first_session");
    assert!(
        flags.is_empty(),
        "{label} should not trigger hidden-test review flags: {flags:?}"
    );
    assert!(
        hidden_event.score >= 70.0,
        "{label} should pass hidden-test anchors, got {}",
        hidden_event.score
    );
}

fn route_command_body(command: &str) -> String {
    let trimmed = command.trim();
    for prefix in [
        "/upgrade latest",
        "/company latest",
        "/sell latest",
        "/buy latest",
        "/work deliver latest",
        "/work accept latest",
        "/work reject latest",
        "/work reopen latest",
        "/work cancel latest",
        "/world action",
        "/contract",
    ] {
        if let Some(body) = trimmed.strip_prefix(prefix) {
            return body.trim().to_string();
        }
    }
    if let Some(rest) = trimmed.strip_prefix("/complete ") {
        return rest
            .trim()
            .split_once(' ')
            .map(|(_, body)| body.trim().to_string())
            .unwrap_or_default();
    }
    trimmed.to_string()
}

#[test]
fn world_first_session_default_prompts_pass_hidden_test_anchors() {
    let default_bodies = [
        (
            "world_action_default",
            "Launch an AI Design Studio for global customers: define the customer deliverable, evidence package, risk controls, next action, self-review, and League quest handoff.",
        ),
        (
            "world_asset_default",
            "Upgrade this world item for a customer deliverable: strengthen capability, evidence package, risk controls, next action loop, self-review, and side-quest handoff.",
        ),
        (
            "world_company_default",
            "Launch a global-facing studio hub with this item: define customer deliverables, evidence package, risk controls, next action loop, self-review, and the first bounty route.",
        ),
        (
            "world_listing_default",
            "Publish a global bounty card: specify deliverables, reward logic, evidence package, commitments, risk controls, self-review, and next action.",
        ),
        (
            "world_buy_default",
            "Accept this quest card and open an adventure commission: confirm customer deliverables, evidence package, rating standards, risk controls, next action, and self-review.",
        ),
        (
            "world_work_deliver_default",
            "Result package: deliverable, evidence package, rating checklist, risk review, next action, and self-check notes.",
        ),
        (
            "world_work_accept_default",
            "Rating passed: confirm customer deliverable, evidence package, quality note, risk controls, next side quest, reputation reward, and self-review.",
        ),
        (
            "world_work_reject_default",
            "Revision required: record customer deliverable gap, evidence package, refund risk controls, revision requirement, next action, and self-review.",
        ),
        (
            "world_work_reopen_default",
            "Reopen commission: escrow reward again, list customer deliverable revisions, evidence gaps, risk controls, rating standards, next resubmission action, and self-review.",
        ),
        (
            "world_work_cancel_default",
            "Cancel commission: record customer deliverable status, evidence package, refund risk controls, next action, and self-review before closing the route.",
        ),
        (
            "world_contract_default",
            "World contract report: customer deliverable, evidence package, risk review, next step, rating standards, and self-review.",
        ),
        (
            "world_listing_web_fallback",
            "Publish a Trillionnium World service listing with deliverable, price logic, evidence package, customer promise, risk controls, self-review, and next action.",
        ),
        (
            "world_work_accept_web_fallback",
            "Buyer acceptance: confirm customer deliverable, evidence package, quality note, risk controls, next collaboration, reputation confirmation, and self-review.",
        ),
        (
            "world_work_reject_web_fallback",
            "Buyer rejection: delivery is not accepted; record customer deliverable gap, evidence package, refund risk controls, revision requirements, next action, and self-review.",
        ),
        (
            "world_work_reopen_web_fallback",
            "Buyer reopen: reserve funds again, list customer deliverable revisions, evidence gaps, risk controls, acceptance standard, next redelivery action, and self-review.",
        ),
        (
            "world_work_cancel_web_fallback",
            "Buyer cancel: record customer deliverable status, evidence package, refund risk controls, next action, and self-review before closing the work order.",
        ),
    ];

    for (label, body) in default_bodies {
        assert_hidden_test_ready_prompt(label, body);
    }
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
                    .map(|value| value.contains("委托方反馈"))
                    .unwrap_or(false)
                && task["next_opportunity_kind"] == "repeat_order_upsell_referral"
                && task["next_opportunity_hint"]
                    .as_str()
                    .map(|value| value.contains("回访委托") || value.contains("升级悬赏"))
                    .unwrap_or(false)
                && task["next_opportunity_playbook"]
                    .as_str()
                    .map(|value| value.contains("评价") || value.contains("升级悬赏"))
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
                    .map(|value| value.contains("回访") || value.contains("升级悬赏"))
                    .unwrap_or(false)
                && task["suggested_node_id"] == "starter-studio"
                && task["suggested_action_label"] == "起草战报后续"
                && task["suggested_panel_id"] == "world-action-console"
                && task["suggested_input_id"] == ""
                && task["suggested_input_value"] == ""
                && task["suggested_textarea_id"] == "world-action-body"
                && task["next_opportunity_action_label"] == "打开任务牌路线"
        }));
    let route_tasks = app["map_hub"]["route_task_graph"]["tasks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let route_task = route_tasks
        .iter()
        .find(|task| task["task_id"] == "task-route-preview-1")
        .expect("route task should exist");
    assert_hidden_test_ready_prompt(
        "route_next_opportunity_body",
        route_task["next_opportunity_body"].as_str().unwrap_or(""),
    );
    assert_hidden_test_ready_prompt(
        "route_next_opportunity_command",
        &route_command_body(
            route_task["next_opportunity_command"]
                .as_str()
                .unwrap_or(""),
        ),
    );
    assert_hidden_test_ready_prompt(
        "route_suggested_body",
        route_task["suggested_body"].as_str().unwrap_or(""),
    );
    assert_hidden_test_ready_prompt(
        "route_suggested_matrix_command",
        &route_command_body(
            route_task["suggested_matrix_command"]
                .as_str()
                .unwrap_or(""),
        ),
    );
    assert_eq!(
        app["map_hub"]["route_story"]["next_task_id"],
        json!("task-route-preview-1")
    );
    assert_eq!(
        app["map_hub"]["route_story"]["next_command_hint"],
        json!("/world action 跟进已完成任务 task-route-preview-1：围绕 契约战报 world-completion-route-preview 记录成果证据、委托方反馈、复盘和下一条支线。 补齐客户交付方案、风险控制、下一步行动。")
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
        "/sell latest 为回访委托方起草升级方案，包含推荐语、追加奖励包和转介绍激励。",
    );
    assert_eq!(listing.panel_id, "world-listings-panel");
    assert_eq!(listing.input_id, "world-listing-company-id");
    assert_eq!(listing.input_value, "latest");
    assert_eq!(listing.textarea_id, "world-listing-body");
    assert_eq!(listing.action_label, "打开任务牌路线");
    assert!(listing.body.contains("回访委托方"));
    assert_hidden_test_ready_prompt("listing_route_target", &listing.body);

    let purchase = crate::world_route_command_target(
        "/buy latest 接取当前任务牌，并附上评级标准、成果范围和时间要求。",
    );
    assert_eq!(purchase.panel_id, "world-commerce-panel");
    assert_eq!(purchase.input_id, "world-buy-listing-id");
    assert_eq!(purchase.input_value, "latest");
    assert_eq!(purchase.textarea_id, "world-buy-body");
    assert_eq!(purchase.action_label, "打开接取路线");
    assert!(purchase.body.contains("评级标准"));
    assert_hidden_test_ready_prompt("purchase_route_target", &purchase.body);

    let completion = crate::world_route_command_target(
        "/complete world-contract-123 提交最终成果、证据包、风险复盘和下一步协作建议。",
    );
    assert_eq!(completion.panel_id, "world-contracts-panel");
    assert_eq!(completion.input_id, "world-contract-completion-id");
    assert_eq!(completion.input_value, "world-contract-123");
    assert_eq!(completion.textarea_id, "world-contract-completion-body");
    assert_eq!(completion.action_label, "打开契约完成路线");
    assert!(completion.body.contains("提交最终成果"));
    assert_hidden_test_ready_prompt("completion_route_target", &completion.body);

    let rejection = crate::world_route_command_target(
        "/work reject latest 缺少原始文件、尺寸说明和修改承诺，请先补齐。",
    );
    assert_eq!(rejection.panel_id, "world-commerce-panel");
    assert_eq!(rejection.input_id, "world-work-reject-id");
    assert_eq!(rejection.input_value, "latest");
    assert_eq!(rejection.textarea_id, "world-work-reject-body");
    assert_eq!(rejection.action_label, "打开返工路线");
    assert!(rejection.body.contains("缺少原始文件"));
    assert_hidden_test_ready_prompt("rejection_route_target", &rejection.body);
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

#[tokio::test]
async fn world_action_duplicate_cooldown_enforces_review_hold_without_rewards() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@anti-cheese:local.dev";
    let body = "craft a repeatable anti-cheese proof with deliverable, evidence, risk control, next action, self review, and a concrete durable world outcome.";
    let payload = json!({
        "matrix_user_id": matrix_user_id,
        "room_id": "!anti-cheese:local.dev",
        "location_id": "starter-studio",
        "body": body
    });

    let (status, first) =
        send_json_request(&app, "POST", "/v1/world/action", &[], payload.clone()).await;
    assert_eq!(status, StatusCode::OK, "first world action failed: {first}");
    assert_eq!(first["playability_outcome"]["payout_status"], "settled");
    assert!(
        first["playability_outcome"]["final_impact"]
            .as_i64()
            .unwrap_or(0)
            > 0
    );
    let (
        asset_count_after_first,
        relationship_count_after_first,
        economy_event_count_after_first,
        player_xp_after_first,
        player_reputation_after_first,
        player_rating_after_first,
    ) = {
        let league = state.inner.league_state.lock().await;
        let player = league
            .players_by_matrix_user
            .get(matrix_user_id)
            .expect("first action should create player progress");
        (
            league.world.world_assets.len(),
            league.world.world_relationships.len(),
            league.world.world_economy_events.len(),
            player.xp,
            player.reputation,
            player.rating,
        )
    };

    let (status, duplicate) =
        send_json_request(&app, "POST", "/v1/world/action", &[], payload).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "duplicate world action should be held, not crash: {duplicate}"
    );
    assert_eq!(duplicate["playability_outcome"]["status"], "review_hold");
    assert_eq!(
        duplicate["playability_outcome"]["success_tier"],
        "cooldown_review_hold"
    );
    assert_eq!(
        duplicate["playability_outcome"]["payout_status"],
        "review_hold"
    );
    assert_eq!(
        duplicate["playability_outcome"]["anti_cheese_gate_enforced"],
        true
    );
    assert_eq!(duplicate["playability_outcome"]["final_impact"], 0);
    assert_eq!(
        duplicate["playability_outcome"]["reward_delta_reputation"],
        0
    );
    assert!(
        duplicate["playability_outcome"]["remaining_cooldown_seconds"]
            .as_i64()
            .unwrap_or(0)
            > 0
    );
    assert!(duplicate["playability_outcome"]["risk_flags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|flag| flag == "duplicate_action_signature"));
    assert_eq!(duplicate["playability_telemetry"]["reputation_delta"], 0);

    let league = state.inner.league_state.lock().await;
    assert_eq!(league.world.world_assets.len(), asset_count_after_first);
    assert_eq!(
        league.world.world_relationships.len(),
        relationship_count_after_first
    );
    assert_eq!(
        league.world.world_economy_events.len(),
        economy_event_count_after_first
    );
    let player = league
        .players_by_matrix_user
        .get(matrix_user_id)
        .expect("duplicate should not remove player");
    assert_eq!(player.xp, player_xp_after_first);
    assert_eq!(player.reputation, player_reputation_after_first);
    assert_eq!(player.rating, player_rating_after_first);
    let duplicate_event = league
        .world
        .world_events
        .iter()
        .rev()
        .find(|event| event.actor_matrix_user_id == matrix_user_id)
        .expect("duplicate event recorded");
    assert_eq!(duplicate_event.impact_score, 0);
}

#[tokio::test]
async fn world_action_review_hold_does_not_attach_contract_task_or_progression_artifacts() {
    let mut config = test_config();
    config.cex_gateway_base_url = "http://127.0.0.1:9".to_string();
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@anti-cheese-contract:local.dev";
    let body = "contract commission for a durable customer deliverable with evidence package, risk controls, next action, self-review, and concrete acceptance criteria.";
    let first_payload = json!({
        "matrix_user_id": matrix_user_id,
        "room_id": "!anti-cheese-contract:local.dev",
        "location_id": "zbj-market-gate",
        "body": body,
        "cex_task_id": "task-contract-first",
        "cex_status": "received"
    });
    let duplicate_payload = json!({
        "matrix_user_id": matrix_user_id,
        "room_id": "!anti-cheese-contract:local.dev",
        "location_id": "zbj-market-gate",
        "body": body,
        "cex_task_id": "task-contract-duplicate",
        "cex_status": "received"
    });
    let duplicate_without_task_payload = json!({
        "matrix_user_id": matrix_user_id,
        "room_id": "!anti-cheese-contract:local.dev",
        "location_id": "zbj-market-gate",
        "body": body
    });

    let (status, first) =
        send_json_request(&app, "POST", "/v1/world/action", &[], first_payload).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "first contract action failed: {first}"
    );
    assert_eq!(first["playability_outcome"]["payout_status"], "settled");
    assert_eq!(first["contract"]["task_id"], "task-contract-first");

    let (contract_count_after_first, relationship_count_after_first, economy_count_after_first) = {
        let league = state.inner.league_state.lock().await;
        (
            league.world.world_contracts.len(),
            league.world.world_relationships.len(),
            league.world.world_economy_events.len(),
        )
    };

    let (status, duplicate) =
        send_json_request(&app, "POST", "/v1/world/action", &[], duplicate_payload).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "duplicate contract action should be held, not crash: {duplicate}"
    );
    assert_eq!(
        duplicate["playability_outcome"]["payout_status"],
        "review_hold"
    );
    assert_eq!(duplicate["playability_outcome"]["final_impact"], 0);
    assert!(duplicate["contract"].is_null());
    assert!(duplicate["event"]["cex_task_id"].is_null());
    assert!(duplicate["event"]["cex_status"].is_null());

    let league = state.inner.league_state.lock().await;
    assert_eq!(
        league.world.world_contracts.len(),
        contract_count_after_first
    );
    assert_eq!(
        league.world.world_relationships.len(),
        relationship_count_after_first
    );
    assert_eq!(
        league.world.world_economy_events.len(),
        economy_count_after_first
    );
    assert!(!league
        .world
        .world_contracts
        .iter()
        .any(|contract| contract.task_id == "task-contract-duplicate"));
    drop(league);

    let (status, skipped_task_duplicate) = send_json_request(
        &app,
        "POST",
        "/v1/world/action",
        &[],
        duplicate_without_task_payload,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "held duplicate should not try to reach cex gateway: {skipped_task_duplicate}"
    );
    assert!(skipped_task_duplicate["task"].is_null());
    assert!(skipped_task_duplicate["contract"].is_null());
    assert_eq!(
        skipped_task_duplicate["playability_outcome"]["payout_status"],
        "review_hold"
    );

    let league = state.inner.league_state.lock().await;
    assert_eq!(
        league.world.world_contracts.len(),
        contract_count_after_first
    );
}

#[tokio::test]
async fn world_commerce_e2e_uses_real_configured_ledger_for_consume_refund_reopen_and_cancel() {
    let (ledger_base_url, ledger_admin_token) = start_real_ledger_service_for_world_e2e().await;
    let http = Client::new();
    let buyer_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 1_000.0).await;
    let seller_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 0.0).await;

    let buyer_matrix_user_id = "@world-ledger-buyer:local.dev";
    let seller_matrix_user_id = "@world-ledger-seller:local.dev";
    let room_id = "!world-commerce-real-ledger:local.dev";
    let mut bindings = IdentityBindings::default();
    bindings.matrix_users.insert(
        buyer_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-commerce-org".to_string()),
            account_id: Some(buyer_account_id.clone()),
        },
    );
    bindings.matrix_users.insert(
        seller_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-commerce-org".to_string()),
            account_id: Some(seller_account_id.clone()),
        },
    );
    let mut config = test_config();
    config.ledger_base_url = ledger_base_url.clone();
    config.ledger_admin_token = Some(ledger_admin_token.clone());
    let state = test_state(config, bindings, HashMap::new());
    let app = build_router(state.clone());

    let (status, action) = send_json_request(
        &app,
        "POST",
        "/v1/world/action",
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "room_id": room_id,
            "location_id": "starter-studio",
            "body": "craft a real commerce seed with customer deliverable, evidence checklist, risk control, next action, self review, and durable world business plan for the market ledger test."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "world action failed: {action}");
    assert_eq!(
        action["playability_outcome"]["contract_version"],
        "trillionnium_world_action_engine_v1"
    );
    assert!(
        action["playability_outcome"]["final_impact"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );
    assert_eq!(
        action["playability_telemetry"]["event_kind"],
        "playability_telemetry"
    );

    let (status, company) = send_json_request(
        &app,
        "POST",
        "/v1/world/companies",
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "asset_id": "latest",
            "body": "建立 AI 设计工坊：写清客户交付方案、委托成果、证据来源包、风险清单、下一步行动和自检记录。"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "world company failed: {company}");

    let (status, listing) = send_json_request(
        &app,
        "POST",
        "/v1/world/listings",
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "company_id": "latest",
            "body": "发布工坊任务牌：写清委托成果、评级证据、来源说明、风险控制、下一步行动和自检记录。"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "world listing failed: {listing}");
    assert_eq!(listing["listing"]["status"], "listed");
    let listing_id = listing["listing"]["listing_id"]
        .as_str()
        .expect("listing id")
        .to_string();

    let buy_world_listing = |app: axum::Router, listing_id: String| async move {
        send_json_request(
            &app,
            "POST",
            &format!("/v1/world/listings/{listing_id}/buy"),
            &[],
            json!({
                "matrix_user_id": buyer_matrix_user_id,
                "room_id": room_id,
                "body": "委托方接取任务牌并开启冒险委托：写清成果、证据、评级标准、风险备注和下一步。"
            }),
        )
        .await
    };

    let (status, buy_one) = buy_world_listing(app.clone(), listing_id.clone()).await;
    assert_eq!(status, StatusCode::OK, "world buy one failed: {buy_one}");
    assert_eq!(buy_one["buyer_ledger_status"], "reserved");
    assert_eq!(buy_one["ledger_status"], "settled");
    assert_eq!(
        buy_one["market_simulation"]["contract_version"],
        "trillionnium_market_simulator_v1"
    );
    assert!(
        buy_one["market_simulation"]["dynamic_price_credits"]
            .as_i64()
            .unwrap_or(0)
            >= buy_one["market_simulation"]["base_price_credits"]
                .as_i64()
                .unwrap_or(0)
    );
    assert!(buy_one["buyer_ledger_entry_id"].as_str().is_some());
    assert!(buy_one["ledger_entry_id"].as_str().is_some());
    let price_credits = buy_one["purchase"]["price_credits"]
        .as_i64()
        .expect("price credits") as f64;
    let seller_net_credits = buy_one["market_simulation"]["seller_net_credits"]
        .as_i64()
        .expect("seller net credits") as f64;
    assert!(
        seller_net_credits < price_credits,
        "market tax sink should reduce seller ledger settlement below gross price"
    );
    let work_one_id = buy_one["work_order"]["work_order_id"]
        .as_str()
        .expect("work order one id")
        .to_string();

    let (status, deliver_one) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_one_id}/deliver"),
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "room_id": room_id,
            "body": "Seller delivery includes final deliverable, acceptance evidence, source notes, risk resolution, next action, and self review."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "world deliver one failed: {deliver_one}"
    );
    assert_eq!(deliver_one["work_order"]["status"], "delivered");

    let (status, accept_one) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_one_id}/accept"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Buyer accepts the delivered work with evidence reviewed, risk closed, quality approved, and next collaboration action."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "world accept one failed: {accept_one}"
    );
    assert_eq!(accept_one["buyer_consume_status"], "consumed");
    assert_eq!(accept_one["purchase"]["status"], "completed");
    assert_eq!(accept_one["work_order"]["status"], "completed");
    assert!(accept_one["buyer_consume_entry_id"].as_str().is_some());

    let (status, buy_two) = buy_world_listing(app.clone(), listing_id.clone()).await;
    assert_eq!(status, StatusCode::OK, "world buy two failed: {buy_two}");
    assert_eq!(buy_two["buyer_ledger_status"], "reserved");
    assert_eq!(buy_two["ledger_status"], "settled");
    let price_two_credits = buy_two["purchase"]["price_credits"]
        .as_i64()
        .expect("price two credits") as f64;
    let seller_net_two_credits = buy_two["market_simulation"]["seller_net_credits"]
        .as_i64()
        .expect("seller net two credits") as f64;
    assert!(
        price_two_credits >= price_credits,
        "market simulator should not lower same-day repeat demand price"
    );
    let work_two_id = buy_two["work_order"]["work_order_id"]
        .as_str()
        .expect("work order two id")
        .to_string();

    let (status, deliver_two) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_two_id}/deliver"),
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "room_id": room_id,
            "body": "Second seller delivery includes deliverable, evidence, source notes, risk handling, next action, and self review."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "world deliver two failed: {deliver_two}"
    );

    let (status, reject_two) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_two_id}/reject"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Buyer rejects because evidence gaps remain; refund reserved funds, document risk, request next revision, and self review."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "world reject two failed: {reject_two}"
    );
    assert_eq!(reject_two["buyer_refund_status"], "refunded");
    assert_eq!(
        reject_two["seller_chargeback_status"],
        "seller_chargeback_consumed"
    );
    assert_eq!(reject_two["purchase"]["status"], "rejected_refunded");
    assert_eq!(
        reject_two["purchase"]["ledger_status"],
        "seller_chargeback_consumed"
    );
    assert!(reject_two["buyer_refund_entry_id"].as_str().is_some());
    assert!(reject_two["seller_chargeback_entry_id"].as_str().is_some());

    let (status, reopen_two) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_two_id}/reopen"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "委托方重开委托：补充修订后的评级证据、风险清单、下一步、重新锁定奖励和清晰自检。"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "world reopen two failed: {reopen_two}"
    );
    assert_eq!(reopen_two["buyer_reopen_reserve_status"], "reserved");
    assert_eq!(
        reopen_two["seller_reopen_settlement_status"],
        "reopened_settled"
    );
    assert_eq!(reopen_two["purchase"]["status"], "reopened_reserved");
    assert_eq!(reopen_two["purchase"]["ledger_status"], "reopened_settled");
    assert_eq!(reopen_two["work_order"]["status"], "open");
    assert!(reopen_two["buyer_reopen_reserve_entry_id"]
        .as_str()
        .is_some());
    assert!(reopen_two["seller_reopen_settlement_entry_id"]
        .as_str()
        .is_some());

    let (status, cancel_two) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_two_id}/cancel"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "委托方在再次提交前放弃委托：退回预留奖励，记录证据缺口、风险理由、下一步和自检。"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "world cancel two failed: {cancel_two}"
    );
    assert_eq!(cancel_two["buyer_cancel_refund_status"], "refunded");
    assert_eq!(
        cancel_two["seller_chargeback_status"],
        "seller_chargeback_consumed"
    );
    assert_eq!(cancel_two["purchase"]["status"], "cancelled_refunded");
    assert_eq!(
        cancel_two["purchase"]["ledger_status"],
        "seller_chargeback_consumed"
    );
    assert!(cancel_two["buyer_cancel_refund_entry_id"]
        .as_str()
        .is_some());

    let buyer_account = get_real_ledger_account(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        &buyer_account_id,
    )
    .await;
    let seller_account = get_real_ledger_account(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        &seller_account_id,
    )
    .await;
    assert_eq!(buyer_account["reserved"].as_f64().unwrap(), 0.0);
    assert_eq!(seller_account["reserved"].as_f64().unwrap(), 0.0);
    assert_eq!(
        buyer_account["balance"].as_f64().unwrap(),
        1_000.0 - price_credits
    );
    assert_eq!(
        seller_account["balance"].as_f64().unwrap(),
        seller_net_credits
    );

    let league = state.inner.league_state.lock().await;
    assert!(league.world.world_purchases.iter().any(|purchase| purchase
        .buyer_ledger_status
        .as_deref()
        == Some("reserved")
        && purchase.ledger_status.as_deref() == Some("settled")
        && purchase.buyer_consume_status.as_deref() == Some("consumed")));
    assert!(league
        .world
        .world_purchases
        .iter()
        .any(|purchase| purchase.status == "cancelled_refunded"
            && purchase.ledger_status.as_deref() == Some("seller_chargeback_consumed")
            && purchase.buyer_ledger_status.as_deref() == Some("reopened_reserved")
            && purchase.buyer_consume_status.as_deref() == Some("refunded")));
    assert!(league.world.world_economy_events.iter().any(|event| {
        event.event_kind == "seller_chargeback"
            && event.credits_delta == -(seller_net_two_credits as i64)
    }));
}

#[tokio::test]
async fn world_buy_does_not_release_commercial_progression_without_buyer_reserve() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let buyer_matrix_user_id = "@world-unreserved-buyer:local.dev";
    let seller_matrix_user_id = "@world-unreserved-seller:local.dev";
    let company_id = "company-unreserved-buy";
    let shop_id = "shop-unreserved-buy";
    let listing_id = "listing-unreserved-buy";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unreserved-buy".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unreserved Buy Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 100,
            reputation_score: 20,
            level: 2,
            created_at_epoch: 1_777_897_940,
        });
        league.world.world_shops.push(WorldShop {
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unreserved Buy Guard Storefront".to_string(),
            shop_kind: "studio".to_string(),
            status: "operating".to_string(),
            listing_count: 1,
            gross_merchandise_score: 100,
            created_at_epoch: 1_777_897_940,
        });
        league.world.world_listings.push(WorldListing {
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unreserved-buy".to_string(),
            title: "Unreserved buy guard offer".to_string(),
            listing_kind: "service_offer".to_string(),
            status: "listed".to_string(),
            price_credits: 80,
            quality_score: 75,
            created_at_epoch: 1_777_897_940,
        });
    }

    let (status, buy) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/listings/{listing_id}/buy"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "body": "Buyer attempts to start a paid world commission without a Matrix room, ledger reserve, evidence trail, or settlement proof."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "world buy response: {buy}");
    assert_eq!(buy["buyer_ledger_status"], "skipped_missing_room");
    assert_eq!(buy["ledger_status"], "skipped_buyer_reserve");
    assert_eq!(buy["purchase"]["status"], "payment_hold");
    assert_eq!(buy["work_order"]["status"], "payment_hold");
    assert!(buy["economy_event"].is_null());
    assert!(buy["seller_standing"].is_null());
    assert!(buy["buyer_standing"].is_null());
    let purchase_id = buy["purchase"]["purchase_id"]
        .as_str()
        .expect("purchase id")
        .to_string();

    let league = state.inner.league_state.lock().await;
    let company = league
        .world
        .world_companies
        .iter()
        .find(|company| company.company_id == company_id)
        .expect("company should remain present");
    assert_eq!(company.revenue_score, 100);
    assert_eq!(company.reputation_score, 20);
    assert_eq!(company.level, 2);
    let shop = league
        .world
        .world_shops
        .iter()
        .find(|shop| shop.shop_id == shop_id)
        .expect("shop should remain present");
    assert_eq!(shop.gross_merchandise_score, 100);
    assert!(!league
        .players_by_matrix_user
        .contains_key(buyer_matrix_user_id));
    assert!(!league
        .players_by_matrix_user
        .contains_key(seller_matrix_user_id));
    assert!(!league.world.world_economy_events.iter().any(|event| {
        event.subject_id == purchase_id
            && (event.event_kind == "listing_purchase" || event.event_kind == "market_tax_sink")
    }));
    assert!(
        !league.world.world_relationships.iter().any(|relationship| {
            relationship.from_id == buyer_matrix_user_id
                && relationship.to_id == company_id
                && relationship.relation_kind == "customer"
        })
    );
    assert!(!league
        .world
        .world_faction_standings
        .iter()
        .any(|standing| standing.matrix_user_id == buyer_matrix_user_id
            || standing.matrix_user_id == seller_matrix_user_id));
}

#[tokio::test]
async fn world_buy_does_not_open_work_without_seller_settlement() {
    let (ledger_base_url, ledger_admin_token) = start_real_ledger_service_for_world_e2e().await;
    let http = Client::new();
    let buyer_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 500.0).await;

    let buyer_matrix_user_id = "@world-unsettled-buy-buyer:local.dev";
    let seller_matrix_user_id = "@world-unsettled-buy-seller:local.dev";
    let room_id = "!world-unsettled-buy:local.dev";
    let company_id = "company-unsettled-buy";
    let shop_id = "shop-unsettled-buy";
    let listing_id = "listing-unsettled-buy";
    let mut bindings = IdentityBindings::default();
    bindings.matrix_users.insert(
        buyer_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-unsettled-buy-org".to_string()),
            account_id: Some(buyer_account_id),
        },
    );
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    config.ledger_base_url = ledger_base_url;
    config.ledger_admin_token = Some(ledger_admin_token);
    let state = test_state(config, bindings, HashMap::new());
    let app = build_router(state.clone());
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unsettled-buy".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unsettled Buy Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 140,
            reputation_score: 28,
            level: 2,
            created_at_epoch: 1_777_897_942,
        });
        league.world.world_shops.push(WorldShop {
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unsettled Buy Guard Storefront".to_string(),
            shop_kind: "studio".to_string(),
            status: "operating".to_string(),
            listing_count: 1,
            gross_merchandise_score: 140,
            created_at_epoch: 1_777_897_942,
        });
        league.world.world_listings.push(WorldListing {
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unsettled-buy".to_string(),
            title: "Unsettled buy guard offer".to_string(),
            listing_kind: "service_offer".to_string(),
            status: "listed".to_string(),
            price_credits: 90,
            quality_score: 82,
            created_at_epoch: 1_777_897_942,
        });
    }

    let (status, buy) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/listings/{listing_id}/buy"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Buyer reserves a world commission with customer deliverable, evidence package, risk controls, next action, and self-review, but seller has no ledger account."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "world buy response: {buy}");
    assert_eq!(buy["buyer_ledger_status"], "reserved");
    assert_eq!(buy["ledger_status"], "skipped_missing_account");
    assert_eq!(buy["purchase"]["status"], "seller_settlement_pending");
    assert_eq!(buy["work_order"]["status"], "seller_settlement_pending");
    assert!(buy["economy_event"].is_null());
    assert!(buy["seller_standing"].is_null());
    assert!(buy["buyer_standing"].is_null());

    let work_order_id = buy["work_order"]["work_order_id"]
        .as_str()
        .expect("work order id")
        .to_string();
    let (delivery_status, delivery) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/deliver"),
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "room_id": room_id,
            "body": "Seller tries to deliver before settlement with deliverable, evidence package, risk controls, next action, and self-review."
        }),
    )
    .await;
    assert_eq!(
        delivery_status,
        StatusCode::CONFLICT,
        "delivery response: {delivery}"
    );
    assert_eq!(delivery["error"], "work order is not deliverable");

    let league = state.inner.league_state.lock().await;
    assert!(!league
        .world
        .world_economy_events
        .iter()
        .any(|event| event.subject_id == buy["purchase"]["purchase_id"].as_str().unwrap()));
    assert!(!league
        .players_by_matrix_user
        .contains_key(buyer_matrix_user_id));
    assert!(!league
        .players_by_matrix_user
        .contains_key(seller_matrix_user_id));
}

#[tokio::test]
async fn world_work_delivery_and_acceptance_require_active_seller_settlement() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let buyer_matrix_user_id = "@world-unsettled-buyer:local.dev";
    let seller_matrix_user_id = "@world-unsettled-seller:local.dev";
    let company_id = "company-unsettled-work";
    let shop_id = "shop-unsettled-work";
    let listing_id = "listing-unsettled-work";
    let deliver_purchase_id = "purchase-unsettled-deliver";
    let deliver_work_order_id = "work-unsettled-deliver";
    let accept_purchase_id = "purchase-unsettled-accept";
    let accept_work_order_id = "work-unsettled-accept";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unsettled-work".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unsettled Work Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 180,
            reputation_score: 24,
            level: 2,
            created_at_epoch: 1_777_897_945,
        });
        for (purchase_id, work_order_id, work_status) in [
            (deliver_purchase_id, deliver_work_order_id, "open"),
            (accept_purchase_id, accept_work_order_id, "delivered"),
        ] {
            league.world.world_purchases.push(WorldPurchase {
                purchase_id: purchase_id.to_string(),
                listing_id: listing_id.to_string(),
                shop_id: shop_id.to_string(),
                company_id: company_id.to_string(),
                buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
                seller_matrix_user_id: seller_matrix_user_id.to_string(),
                price_credits: 95,
                status: "seller_settlement_failed".to_string(),
                ledger_status: Some("failed_ledger".to_string()),
                ledger_account_id: Some("seller-account".to_string()),
                ledger_entry_id: None,
                ledger_balance_after: None,
                ledger_error: Some("seller grant failed".to_string()),
                buyer_ledger_status: Some("reserved".to_string()),
                buyer_ledger_account_id: Some("buyer-account".to_string()),
                buyer_ledger_entry_id: Some("buyer-reserve-entry".to_string()),
                buyer_ledger_balance_after: Some(0.0),
                buyer_ledger_error: None,
                buyer_consume_status: Some("pending_acceptance".to_string()),
                buyer_consume_entry_id: None,
                buyer_consume_balance_after: None,
                buyer_consume_error: None,
                created_at_epoch: 1_777_897_945,
            });
            league.world.world_work_orders.push(WorldWorkOrder {
                work_order_id: work_order_id.to_string(),
                purchase_id: purchase_id.to_string(),
                listing_id: listing_id.to_string(),
                buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
                seller_matrix_user_id: seller_matrix_user_id.to_string(),
                company_id: company_id.to_string(),
                status: work_status.to_string(),
                brief: "Unsettled work should not release progress".to_string(),
                value_score: 95,
                created_at_epoch: 1_777_897_945,
            });
        }
    }

    let anchored_delivery_body = "Seller delivery includes customer deliverable, evidence package, source notes, risk controls, next action, and self-review.";
    let (status, delivery) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{deliver_work_order_id}/deliver"),
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "room_id": "!world-unsettled-work:local.dev",
            "body": anchored_delivery_body
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "delivery response: {delivery}"
    );
    assert_eq!(
        delivery["error"],
        "world purchase seller settlement is not active"
    );
    assert_eq!(delivery["ledger_status"], "failed_ledger");

    let (status, acceptance) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{accept_work_order_id}/accept"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": "!world-unsettled-work:local.dev",
            "body": "Buyer acceptance checks customer deliverable, evidence package, risk controls, next action, and self-review."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "acceptance response: {acceptance}"
    );
    assert_eq!(
        acceptance["error"],
        "world purchase seller settlement is not active"
    );
    assert_eq!(acceptance["ledger_status"], "failed_ledger");

    let league = state.inner.league_state.lock().await;
    let company = league
        .world
        .world_companies
        .iter()
        .find(|company| company.company_id == company_id)
        .expect("company should remain present");
    assert_eq!(company.reputation_score, 24);
    assert!(league.world.world_work_deliveries.is_empty());
    assert!(league.world.world_work_acceptances.is_empty());
    assert!(!league
        .players_by_matrix_user
        .contains_key(seller_matrix_user_id));
    assert!(!league
        .world
        .world_economy_events
        .iter()
        .any(|event| event.event_kind == "work_delivered" || event.event_kind == "work_accepted"));
}

#[tokio::test]
async fn world_accept_does_not_release_reputation_without_buyer_consume() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let buyer_matrix_user_id = "@world-unconsumed-buyer:local.dev";
    let seller_matrix_user_id = "@world-unconsumed-seller:local.dev";
    let company_id = "company-unconsumed-accept";
    let shop_id = "shop-unconsumed-accept";
    let listing_id = "listing-unconsumed-accept";
    let purchase_id = "purchase-unconsumed-accept";
    let work_order_id = "work-unconsumed-accept";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unconsumed-accept".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unconsumed Accept Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 180,
            reputation_score: 20,
            level: 2,
            created_at_epoch: 1_777_897_950,
        });
        league.world.world_shops.push(WorldShop {
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unconsumed Accept Guard Storefront".to_string(),
            shop_kind: "studio".to_string(),
            status: "operating".to_string(),
            listing_count: 1,
            gross_merchandise_score: 180,
            created_at_epoch: 1_777_897_950,
        });
        league.world.world_listings.push(WorldListing {
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unconsumed-accept".to_string(),
            title: "Unconsumed accept guard offer".to_string(),
            listing_kind: "service_offer".to_string(),
            status: "listed".to_string(),
            price_credits: 90,
            quality_score: 80,
            created_at_epoch: 1_777_897_950,
        });
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 90,
            status: "reserved".to_string(),
            ledger_status: Some("settled".to_string()),
            ledger_account_id: Some("seller-account".to_string()),
            ledger_entry_id: Some("seller-grant-entry".to_string()),
            ledger_balance_after: Some(90.0),
            ledger_error: None,
            buyer_ledger_status: Some("reserved".to_string()),
            buyer_ledger_account_id: Some("buyer-account".to_string()),
            buyer_ledger_entry_id: Some("buyer-reserve-entry".to_string()),
            buyer_ledger_balance_after: Some(0.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("pending_acceptance".to_string()),
            buyer_consume_entry_id: None,
            buyer_consume_balance_after: None,
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_950,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: company_id.to_string(),
            status: "delivered".to_string(),
            brief: "Delivered work awaiting buyer consume".to_string(),
            value_score: 90,
            created_at_epoch: 1_777_897_950,
        });
    }

    let (status, acceptance) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/accept"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "body": "Buyer accepts the work textually, but omits room context so ledger consume cannot release the reserved payment."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "accept response: {acceptance}");
    assert_eq!(acceptance["buyer_consume_status"], "skipped_missing_room");
    assert_eq!(acceptance["purchase"]["status"], "accepted_payment_hold");
    assert_eq!(acceptance["work_order"]["status"], "accepted_payment_hold");
    assert_eq!(acceptance["acceptance"]["status"], "accepted_payment_hold");
    assert!(acceptance["economy_event"].is_null());
    assert!(acceptance["standing"].is_null());

    let league = state.inner.league_state.lock().await;
    let company = league
        .world
        .world_companies
        .iter()
        .find(|company| company.company_id == company_id)
        .expect("company should remain present");
    assert_eq!(company.reputation_score, 20);
    assert!(!league
        .players_by_matrix_user
        .contains_key(buyer_matrix_user_id));
    assert!(!league
        .players_by_matrix_user
        .contains_key(seller_matrix_user_id));
    assert!(!league
        .world
        .world_economy_events
        .iter()
        .any(|event| { event.subject_id == work_order_id && event.event_kind == "work_accepted" }));
    assert!(!league
        .world
        .world_faction_standings
        .iter()
        .any(|standing| standing.matrix_user_id == seller_matrix_user_id));
}

#[tokio::test]
async fn world_reject_does_not_emit_refund_economy_without_buyer_refund() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let buyer_matrix_user_id = "@world-unrefunded-buyer:local.dev";
    let seller_matrix_user_id = "@world-unrefunded-seller:local.dev";
    let company_id = "company-unrefunded-reject";
    let shop_id = "shop-unrefunded-reject";
    let listing_id = "listing-unrefunded-reject";
    let purchase_id = "purchase-unrefunded-reject";
    let work_order_id = "work-unrefunded-reject";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unrefunded-reject".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unrefunded Reject Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 160,
            reputation_score: 25,
            level: 2,
            created_at_epoch: 1_777_897_960,
        });
        league.world.world_shops.push(WorldShop {
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unrefunded Reject Guard Storefront".to_string(),
            shop_kind: "studio".to_string(),
            status: "operating".to_string(),
            listing_count: 1,
            gross_merchandise_score: 160,
            created_at_epoch: 1_777_897_960,
        });
        league.world.world_listings.push(WorldListing {
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-unrefunded-reject".to_string(),
            title: "Unrefunded reject guard offer".to_string(),
            listing_kind: "service_offer".to_string(),
            status: "listed".to_string(),
            price_credits: 70,
            quality_score: 70,
            created_at_epoch: 1_777_897_960,
        });
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 70,
            status: "reserved".to_string(),
            ledger_status: Some("settled".to_string()),
            ledger_account_id: Some("seller-account".to_string()),
            ledger_entry_id: Some("seller-grant-entry".to_string()),
            ledger_balance_after: Some(70.0),
            ledger_error: None,
            buyer_ledger_status: Some("reserved".to_string()),
            buyer_ledger_account_id: Some("buyer-account".to_string()),
            buyer_ledger_entry_id: Some("buyer-reserve-entry".to_string()),
            buyer_ledger_balance_after: Some(0.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("pending_acceptance".to_string()),
            buyer_consume_entry_id: None,
            buyer_consume_balance_after: None,
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_960,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: company_id.to_string(),
            status: "delivered".to_string(),
            brief: "Delivered work awaiting rejection refund".to_string(),
            value_score: 70,
            created_at_epoch: 1_777_897_960,
        });
    }

    let (status, rejection) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/reject"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "body": "Buyer rejects the work, but omits room context so ledger refund cannot happen."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reject response: {rejection}");
    assert_eq!(rejection["buyer_refund_status"], "skipped_missing_room");
    assert_eq!(
        rejection["seller_chargeback_status"],
        "skipped_buyer_not_refunded"
    );
    assert_eq!(rejection["purchase"]["status"], "rejected_refund_hold");
    assert_eq!(rejection["work_order"]["status"], "rejected_refund_hold");
    assert_eq!(rejection["rejection"]["status"], "rejected_refund_hold");
    assert!(rejection["economy_event"].is_null());
    assert!(rejection["standing"].is_null());

    let league = state.inner.league_state.lock().await;
    assert!(!league
        .world
        .world_economy_events
        .iter()
        .any(|event| { event.subject_id == work_order_id && event.event_kind == "work_rejected" }));
    assert!(!league
        .world
        .world_faction_standings
        .iter()
        .any(|standing| standing.matrix_user_id == buyer_matrix_user_id));
}

#[tokio::test]
async fn world_cancel_does_not_emit_refund_economy_without_buyer_refund() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let buyer_matrix_user_id = "@world-uncancel-refund-buyer:local.dev";
    let seller_matrix_user_id = "@world-uncancel-refund-seller:local.dev";
    let company_id = "company-uncancel-refund";
    let shop_id = "shop-uncancel-refund";
    let listing_id = "listing-uncancel-refund";
    let purchase_id = "purchase-uncancel-refund";
    let work_order_id = "work-uncancel-refund";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-uncancel-refund".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unrefunded Cancel Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 140,
            reputation_score: 22,
            level: 2,
            created_at_epoch: 1_777_897_970,
        });
        league.world.world_shops.push(WorldShop {
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            name: "Unrefunded Cancel Guard Storefront".to_string(),
            shop_kind: "studio".to_string(),
            status: "operating".to_string(),
            listing_count: 1,
            gross_merchandise_score: 140,
            created_at_epoch: 1_777_897_970,
        });
        league.world.world_listings.push(WorldListing {
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-uncancel-refund".to_string(),
            title: "Unrefunded cancel guard offer".to_string(),
            listing_kind: "service_offer".to_string(),
            status: "listed".to_string(),
            price_credits: 65,
            quality_score: 68,
            created_at_epoch: 1_777_897_970,
        });
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 65,
            status: "reserved".to_string(),
            ledger_status: Some("settled".to_string()),
            ledger_account_id: Some("seller-account".to_string()),
            ledger_entry_id: Some("seller-grant-entry".to_string()),
            ledger_balance_after: Some(65.0),
            ledger_error: None,
            buyer_ledger_status: Some("reserved".to_string()),
            buyer_ledger_account_id: Some("buyer-account".to_string()),
            buyer_ledger_entry_id: Some("buyer-reserve-entry".to_string()),
            buyer_ledger_balance_after: Some(0.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("pending_acceptance".to_string()),
            buyer_consume_entry_id: None,
            buyer_consume_balance_after: None,
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_970,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: company_id.to_string(),
            status: "open".to_string(),
            brief: "Open work awaiting cancellation refund".to_string(),
            value_score: 65,
            created_at_epoch: 1_777_897_970,
        });
    }

    let (status, cancellation) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/cancel"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "body": "Buyer cancels the work, but omits room context so ledger refund cannot happen."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cancel response: {cancellation}");
    assert_eq!(
        cancellation["buyer_cancel_refund_status"],
        "skipped_missing_room"
    );
    assert_eq!(
        cancellation["seller_chargeback_status"],
        "skipped_buyer_not_refunded"
    );
    assert_eq!(cancellation["purchase"]["status"], "cancelled_refund_hold");
    assert_eq!(
        cancellation["work_order"]["status"],
        "cancelled_refund_hold"
    );
    assert_eq!(
        cancellation["cancellation"]["status"],
        "cancelled_refund_hold"
    );
    assert!(cancellation["economy_event"].is_null());
    assert!(cancellation["standing"].is_null());

    let league = state.inner.league_state.lock().await;
    assert!(!league.world.world_economy_events.iter().any(|event| {
        event.subject_id == work_order_id && event.event_kind == "work_cancelled"
    }));
    assert!(!league
        .world
        .world_faction_standings
        .iter()
        .any(|standing| standing.matrix_user_id == buyer_matrix_user_id));
}

#[tokio::test]
async fn world_reopen_does_not_emit_progression_without_buyer_reserve() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let buyer_matrix_user_id = "@world-unreserved-reopen-buyer:local.dev";
    let seller_matrix_user_id = "@world-unreserved-reopen-seller:local.dev";
    let company_id = "company-unreserved-reopen";
    let shop_id = "shop-unreserved-reopen";
    let listing_id = "listing-unreserved-reopen";
    let purchase_id = "purchase-unreserved-reopen";
    let work_order_id = "work-unreserved-reopen";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 55,
            status: "rejected_refunded".to_string(),
            ledger_status: Some("seller_chargeback_consumed".to_string()),
            ledger_account_id: Some("seller-account".to_string()),
            ledger_entry_id: Some("seller-chargeback-entry".to_string()),
            ledger_balance_after: Some(0.0),
            ledger_error: None,
            buyer_ledger_status: Some("refunded".to_string()),
            buyer_ledger_account_id: Some("buyer-account".to_string()),
            buyer_ledger_entry_id: Some("buyer-refund-entry".to_string()),
            buyer_ledger_balance_after: Some(55.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("refunded".to_string()),
            buyer_consume_entry_id: Some("buyer-refund-consume-entry".to_string()),
            buyer_consume_balance_after: Some(55.0),
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_980,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: company_id.to_string(),
            status: "rejected_refunded".to_string(),
            brief: "Rejected work awaiting paid reopen reserve".to_string(),
            value_score: 55,
            created_at_epoch: 1_777_897_980,
        });
    }

    let (status, reopen) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/reopen"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "body": "Buyer requests a revision reopen, but omits room context so the new reserve cannot settle."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reopen response: {reopen}");
    assert_eq!(
        reopen["buyer_reopen_reserve_status"],
        "skipped_missing_room"
    );
    assert_eq!(
        reopen["seller_reopen_settlement_status"],
        "skipped_buyer_reopen_reserve"
    );
    assert_eq!(reopen["purchase"]["status"], "reopen_reserve_hold");
    assert_eq!(reopen["work_order"]["status"], "reopen_reserve_hold");
    assert_eq!(reopen["reopen"]["status"], "reopen_reserve_hold");
    assert!(reopen["economy_event"].is_null());
    assert!(reopen["standing"].is_null());

    let league = state.inner.league_state.lock().await;
    assert!(!league
        .world
        .world_economy_events
        .iter()
        .any(|event| { event.subject_id == work_order_id && event.event_kind == "work_reopened" }));
    assert!(!league
        .world
        .world_faction_standings
        .iter()
        .any(|standing| standing.matrix_user_id == buyer_matrix_user_id));
}

#[tokio::test]
async fn world_reopen_does_not_open_without_seller_resettlement() {
    let (ledger_base_url, ledger_admin_token) = start_real_ledger_service_for_world_e2e().await;
    let http = Client::new();
    let buyer_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 250.0).await;

    let buyer_matrix_user_id = "@world-reopen-unsettled-buyer:local.dev";
    let seller_matrix_user_id = "@world-reopen-unsettled-seller:local.dev";
    let room_id = "!world-reopen-unsettled:local.dev";
    let mut bindings = IdentityBindings::default();
    bindings.matrix_users.insert(
        buyer_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-reopen-unsettled-org".to_string()),
            account_id: Some(buyer_account_id.clone()),
        },
    );
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    config.ledger_base_url = ledger_base_url.clone();
    config.ledger_admin_token = Some(ledger_admin_token.clone());
    let state = test_state(config, bindings, HashMap::new());
    let app = build_router(state.clone());

    let company_id = "company-reopen-unsettled";
    let shop_id = "shop-reopen-unsettled";
    let listing_id = "listing-reopen-unsettled";
    let purchase_id = "purchase-reopen-unsettled";
    let work_order_id = "work-reopen-unsettled";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 75,
            status: "rejected_refunded".to_string(),
            ledger_status: Some("seller_chargeback_consumed".to_string()),
            ledger_account_id: Some("seller-account".to_string()),
            ledger_entry_id: Some("seller-chargeback-entry".to_string()),
            ledger_balance_after: Some(0.0),
            ledger_error: None,
            buyer_ledger_status: Some("refunded".to_string()),
            buyer_ledger_account_id: Some(buyer_account_id.clone()),
            buyer_ledger_entry_id: Some("buyer-refund-entry".to_string()),
            buyer_ledger_balance_after: Some(250.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("refunded".to_string()),
            buyer_consume_entry_id: Some("buyer-refund-consume-entry".to_string()),
            buyer_consume_balance_after: Some(250.0),
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_985,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: company_id.to_string(),
            status: "rejected_refunded".to_string(),
            brief: "Rejected work should not reopen until seller is resettled".to_string(),
            value_score: 75,
            created_at_epoch: 1_777_897_985,
        });
    }

    let (status, reopen) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/reopen"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Buyer reopens with customer deliverable revisions, evidence package, risk controls, next action, and self-review."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reopen response: {reopen}");
    assert_eq!(reopen["buyer_reopen_reserve_status"], "reserved");
    assert_eq!(
        reopen["seller_reopen_settlement_status"],
        "skipped_missing_account"
    );
    assert_eq!(
        reopen["purchase"]["status"],
        "reopen_seller_settlement_pending"
    );
    assert_eq!(
        reopen["work_order"]["status"],
        "reopen_seller_settlement_pending"
    );
    assert!(reopen["economy_event"].is_null());
    assert!(reopen["standing"].is_null());

    let (status, delivery) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/deliver"),
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "room_id": room_id,
            "body": "Seller delivery includes customer deliverable, evidence package, risk controls, next action, and self-review."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "delivery response: {delivery}"
    );
    assert_eq!(delivery["error"], "work order is not deliverable");

    let (status, cancel) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/cancel"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Buyer cancels after seller settlement did not open: customer deliverable status, evidence package, risk controls, next action, and self-review."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cancel response: {cancel}");
    assert_eq!(cancel["buyer_cancel_refund_status"], "refunded");
    assert_eq!(
        cancel["seller_chargeback_status"],
        "skipped_seller_not_settled"
    );
    assert_eq!(cancel["purchase"]["status"], "cancelled_refunded");

    let buyer_account = get_real_ledger_account(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        &buyer_account_id,
    )
    .await;
    assert_eq!(buyer_account["reserved"].as_f64().unwrap(), 0.0);
    assert_eq!(buyer_account["balance"].as_f64().unwrap(), 250.0);
}

#[tokio::test]
async fn world_reopen_requires_rejection_chargeback_settlement() {
    let (ledger_base_url, ledger_admin_token) = start_real_ledger_service_for_world_e2e().await;
    let http = Client::new();
    let buyer_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 250.0).await;
    let seller_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 0.0).await;

    let buyer_matrix_user_id = "@world-reopen-chargeback-buyer:local.dev";
    let seller_matrix_user_id = "@world-reopen-chargeback-seller:local.dev";
    let room_id = "!world-reopen-chargeback:local.dev";
    let mut bindings = IdentityBindings::default();
    bindings.matrix_users.insert(
        buyer_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-reopen-chargeback-org".to_string()),
            account_id: Some(buyer_account_id.clone()),
        },
    );
    bindings.matrix_users.insert(
        seller_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-reopen-chargeback-org".to_string()),
            account_id: Some(seller_account_id.clone()),
        },
    );
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    config.ledger_base_url = ledger_base_url.clone();
    config.ledger_admin_token = Some(ledger_admin_token.clone());
    let state = test_state(config, bindings, HashMap::new());
    let app = build_router(state.clone());

    let purchase_id = "purchase-reopen-chargeback-blocked";
    let work_order_id = "work-reopen-chargeback-blocked";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: "listing-reopen-chargeback-blocked".to_string(),
            shop_id: "shop-reopen-chargeback-blocked".to_string(),
            company_id: "company-reopen-chargeback-blocked".to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 90,
            status: "rejected_refunded".to_string(),
            ledger_status: Some("seller_chargeback_failed".to_string()),
            ledger_account_id: Some(seller_account_id.clone()),
            ledger_entry_id: Some("seller-original-settlement-entry".to_string()),
            ledger_balance_after: Some(81.0),
            ledger_error: Some("seller chargeback reserve did not complete".to_string()),
            buyer_ledger_status: Some("reserved".to_string()),
            buyer_ledger_account_id: Some(buyer_account_id.clone()),
            buyer_ledger_entry_id: Some("buyer-original-reserve-entry".to_string()),
            buyer_ledger_balance_after: Some(250.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("refunded".to_string()),
            buyer_consume_entry_id: Some("buyer-refund-entry".to_string()),
            buyer_consume_balance_after: Some(250.0),
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_986,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: "listing-reopen-chargeback-blocked".to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: "company-reopen-chargeback-blocked".to_string(),
            status: "rejected_refunded".to_string(),
            brief: "Rejected work whose seller chargeback failed must not reopen".to_string(),
            value_score: 90,
            created_at_epoch: 1_777_897_986,
        });
    }

    let (status, reopen) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/reopen"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Buyer tries to reopen after a refund but before seller chargeback settlement, with customer deliverable revisions, evidence package, risk controls, next action, and self-review."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "reopen response: {reopen}");
    assert_eq!(
        reopen["error"],
        "world work rejection settlement is not complete"
    );
    assert_eq!(
        reopen["seller_chargeback_status"],
        "seller_chargeback_failed"
    );

    let league = state.inner.league_state.lock().await;
    assert!(league.world.world_work_reopens.is_empty());
    let work_order = league
        .world
        .world_work_orders
        .iter()
        .find(|work_order| work_order.work_order_id == work_order_id)
        .expect("stored work order");
    assert_eq!(work_order.status, "rejected_refunded");
    drop(league);

    let buyer_account = get_real_ledger_account(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        &buyer_account_id,
    )
    .await;
    assert_eq!(buyer_account["reserved"].as_f64().unwrap(), 0.0);
    assert_eq!(buyer_account["balance"].as_f64().unwrap(), 250.0);
    let seller_account = get_real_ledger_account(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        &seller_account_id,
    )
    .await;
    assert_eq!(seller_account["balance"].as_f64().unwrap(), 0.0);
}

#[tokio::test]
async fn world_delivery_review_hold_does_not_grant_faction_or_seller_progress() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let buyer_matrix_user_id = "@world-delivery-review-buyer:local.dev";
    let seller_matrix_user_id = "@world-delivery-review-seller:local.dev";
    let company_id = "company-delivery-review-hold";
    let listing_id = "listing-delivery-review-hold";
    let purchase_id = "purchase-delivery-review-hold";
    let work_order_id = "work-delivery-review-hold";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: company_id.to_string(),
            owner_matrix_user_id: seller_matrix_user_id.to_string(),
            asset_id: "asset-delivery-review-hold".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Delivery Review Hold Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 120,
            reputation_score: 30,
            level: 2,
            created_at_epoch: 1_777_897_990,
        });
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            shop_id: "shop-delivery-review-hold".to_string(),
            company_id: company_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 80,
            status: "reserved".to_string(),
            ledger_status: Some("settled".to_string()),
            ledger_account_id: Some("seller-account".to_string()),
            ledger_entry_id: Some("seller-grant-entry".to_string()),
            ledger_balance_after: Some(80.0),
            ledger_error: None,
            buyer_ledger_status: Some("reserved".to_string()),
            buyer_ledger_account_id: Some("buyer-account".to_string()),
            buyer_ledger_entry_id: Some("buyer-reserve-entry".to_string()),
            buyer_ledger_balance_after: Some(0.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("pending_acceptance".to_string()),
            buyer_consume_entry_id: None,
            buyer_consume_balance_after: None,
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_990,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: listing_id.to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: company_id.to_string(),
            status: "open".to_string(),
            brief: "Open work awaiting seller delivery".to_string(),
            value_score: 80,
            created_at_epoch: 1_777_897_990,
        });
    }

    let (status, delivery) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/deliver"),
        &[],
        json!({
            "matrix_user_id": seller_matrix_user_id,
            "body": "copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "delivery response: {delivery}");
    assert_eq!(delivery["delivery"]["status"], "review_hold");
    assert_eq!(delivery["work_order"]["status"], "delivery_review_hold");
    assert!(delivery["economy_event"].is_null());
    assert!(delivery["standing"].is_null());

    let league = state.inner.league_state.lock().await;
    let company = league
        .world
        .world_companies
        .iter()
        .find(|company| company.company_id == company_id)
        .expect("company should remain present");
    assert_eq!(company.reputation_score, 30);
    assert!(!league
        .players_by_matrix_user
        .contains_key(seller_matrix_user_id));
    assert!(!league
        .world
        .world_economy_events
        .iter()
        .any(|event| event.subject_id == work_order_id
            || event.matrix_user_id == seller_matrix_user_id));
    assert!(!league
        .world
        .world_faction_standings
        .iter()
        .any(|standing| standing.matrix_user_id == seller_matrix_user_id));
}

#[tokio::test]
async fn world_asset_upgrade_review_hold_does_not_grant_player_progress() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@world-asset-review-hold:local.dev";
    let asset_id = "asset-review-hold-upgrade";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_assets.push(WorldAsset {
            asset_id: asset_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            asset_kind: "studio".to_string(),
            name: "Review Hold Upgrade Guard Asset".to_string(),
            status: "seeded".to_string(),
            value_score: 40,
            upgrade_level: 1,
            upgrade_points: 0,
            last_upgrade_kind: None,
            created_at_epoch: 1_777_901_000,
        });
    }

    let (status, upgrade) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/assets/{asset_id}/upgrade"),
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "body": "copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "asset upgrade response: {upgrade}");
    assert_eq!(upgrade["upgrade"]["status"], "review_hold");
    assert_eq!(upgrade["upgrade"]["value_delta"], 0);
    assert_eq!(upgrade["asset"]["value_score"], 40);
    assert_eq!(upgrade["asset"]["upgrade_points"], 0);

    let league = state.inner.league_state.lock().await;
    let asset = league
        .world
        .world_assets
        .iter()
        .find(|asset| asset.asset_id == asset_id)
        .expect("asset should remain present");
    assert_eq!(asset.value_score, 40);
    assert_eq!(asset.upgrade_points, 0);
    assert_eq!(asset.upgrade_level, 1);
    assert!(!league.players_by_matrix_user.contains_key(matrix_user_id));
}

#[tokio::test]
async fn world_company_review_hold_does_not_release_commercial_progression_or_followup_listings() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@world-company-review-hold:local.dev";
    let asset_id = "asset-review-hold-company";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_assets.push(WorldAsset {
            asset_id: asset_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            asset_kind: "studio".to_string(),
            name: "Review Hold Company Guard Asset".to_string(),
            status: "seeded".to_string(),
            value_score: 120,
            upgrade_level: 2,
            upgrade_points: 90,
            last_upgrade_kind: Some("manual_upgrade".to_string()),
            created_at_epoch: 1_777_901_010,
        });
    }

    let (status, company_response) = send_json_request(
        &app,
        "POST",
        "/v1/world/companies",
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "asset_id": asset_id,
            "body": "copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "company review hold response: {company_response}"
    );
    assert_eq!(company_response["payout_status"], "review_hold");
    assert_eq!(company_response["company"]["status"], "review_hold");
    assert_eq!(company_response["company"]["revenue_score"], 0);
    assert_eq!(company_response["company"]["reputation_score"], 0);
    assert_eq!(company_response["shop"]["listing_count"], 0);
    assert_eq!(company_response["shop"]["gross_merchandise_score"], 0);
    assert_eq!(company_response["listing"]["status"], "review_hold");
    assert_eq!(company_response["listing"]["price_credits"], 0);
    assert_eq!(company_response["listing"]["quality_score"], 0);
    assert!(company_response["economy_event"].is_null());

    let company_id = company_response["company"]["company_id"]
        .as_str()
        .expect("company id")
        .to_string();
    let (listing_status, listing_response) = send_json_request(
        &app,
        "POST",
        "/v1/world/listings",
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "company_id": company_id,
            "body": "Publish a legitimate deliverable with customer evidence, risk controls, next action, self review, and market promise."
        }),
    )
    .await;
    assert_eq!(
        listing_status,
        StatusCode::CONFLICT,
        "review-hold company should not publish listings: {listing_response}"
    );
    assert_eq!(listing_response["status"], "review_hold");

    let league = state.inner.league_state.lock().await;
    assert!(!league.players_by_matrix_user.contains_key(matrix_user_id));
    assert!(!league
        .world
        .world_economy_events
        .iter()
        .any(|event| { event.subject_id == company_id || event.matrix_user_id == matrix_user_id }));
    assert!(
        !league.world.world_relationships.iter().any(|relationship| {
            relationship.from_id == matrix_user_id
                && relationship.to_id == company_id
                && relationship.relation_kind == "owner"
        })
    );
    assert_eq!(
        league
            .world
            .world_listings
            .iter()
            .filter(|listing| listing.company_id == company_id)
            .count(),
        1,
        "only the held bootstrap listing should exist"
    );
}

#[tokio::test]
async fn world_listing_review_hold_does_not_release_market_artifacts_or_events() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@world-listing-review-hold:local.dev";
    let company_id = "company-review-hold-listing";
    let shop_id = "shop-review-hold-listing";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: company_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            asset_id: "asset-review-hold-listing".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Listing Review Hold Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 220,
            reputation_score: 44,
            level: 3,
            created_at_epoch: 1_777_901_020,
        });
        league.world.world_shops.push(WorldShop {
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            name: "Listing Review Hold Guard Storefront".to_string(),
            shop_kind: "studio".to_string(),
            status: "operating".to_string(),
            listing_count: 2,
            gross_merchandise_score: 220,
            created_at_epoch: 1_777_901_020,
        });
    }

    let (status, listing_response) = send_json_request(
        &app,
        "POST",
        "/v1/world/listings",
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "company_id": company_id,
            "body": "copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "listing review-hold response: {listing_response}"
    );
    assert_eq!(listing_response["payout_status"], "review_hold");
    assert_eq!(listing_response["listing"]["status"], "review_hold");
    assert_eq!(listing_response["listing"]["price_credits"], 0);
    assert_eq!(listing_response["listing"]["quality_score"], 0);
    assert!(listing_response["economy_event"].is_null());

    let listing_id = listing_response["listing"]["listing_id"]
        .as_str()
        .expect("listing id")
        .to_string();
    let league = state.inner.league_state.lock().await;
    let company = league
        .world
        .world_companies
        .iter()
        .find(|company| company.company_id == company_id)
        .expect("company should remain present");
    assert_eq!(company.revenue_score, 220);
    assert_eq!(company.reputation_score, 44);
    assert_eq!(company.level, 3);
    let shop = league
        .world
        .world_shops
        .iter()
        .find(|shop| shop.shop_id == shop_id)
        .expect("shop should remain present");
    assert_eq!(shop.listing_count, 2);
    assert_eq!(shop.gross_merchandise_score, 220);
    assert!(!league.players_by_matrix_user.contains_key(matrix_user_id));
    assert!(!league
        .world
        .world_economy_events
        .iter()
        .any(|event| event.subject_id == listing_id || event.matrix_user_id == matrix_user_id));
}

#[tokio::test]
async fn world_contract_review_hold_does_not_upgrade_asset_or_player_progress() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let matrix_user_id = "@world-contract-review-hold:local.dev";
    let contract_id = "world-contract-review-hold-guard";
    let asset_id = "asset-contract-review-hold-guard";
    {
        let mut league = state.inner.league_state.lock().await;
        let starter_location_id = league
            .world
            .world_map_nodes
            .get("starter-studio")
            .map(|node| node.location_id.clone())
            .unwrap_or_else(|| "starter-studio".to_string());
        league.world.world_events.push(WorldEvent {
            event_id: "world-event-review-hold-contract".to_string(),
            actor_matrix_user_id: matrix_user_id.to_string(),
            room_id: Some("!room:local.dev".to_string()),
            location_id: starter_location_id.clone(),
            event_kind: "world_contract".to_string(),
            body: "Review hold contract should not release progression".to_string(),
            result: "queued".to_string(),
            impact_score: 7,
            cex_task_id: Some("task-review-hold-contract".to_string()),
            cex_status: Some("Running".to_string()),
            created_at_epoch: 1_777_902_000,
        });
        league.world.world_contracts.push(WorldContract {
            contract_id: contract_id.to_string(),
            event_id: "world-event-review-hold-contract".to_string(),
            actor_matrix_user_id: matrix_user_id.to_string(),
            location_id: starter_location_id.clone(),
            task_id: "task-review-hold-contract".to_string(),
            title: "Review hold contract progression guard".to_string(),
            body: "Completion must pass review before upgrading assets or players".to_string(),
            status: "open".to_string(),
            cex_status: Some("Running".to_string()),
            value_score: 64,
            created_at_epoch: 1_777_902_001,
        });
        league.world.world_assets.push(WorldAsset {
            asset_id: asset_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            location_id: starter_location_id,
            asset_kind: "contract_proof".to_string(),
            name: "Review Hold Contract Asset".to_string(),
            status: "active".to_string(),
            value_score: 70,
            upgrade_level: 2,
            upgrade_points: 10,
            last_upgrade_kind: None,
            created_at_epoch: 1_777_902_002,
        });
    }
    let app = build_router(state.clone());
    let (status, completion) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/contracts/{contract_id}/complete"),
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "body": "copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "contract review-hold completion failed: {completion}"
    );
    assert_eq!(completion["completion"]["payout_status"], "review_hold");
    assert_eq!(completion["completion"]["ledger_status"], "held_review");
    assert_eq!(completion["contract"]["status"], "review_hold");
    assert_eq!(completion["contract"]["cex_status"], "review_hold");
    assert_eq!(completion["contract"]["value_score"], 64);

    let league = state.inner.league_state.lock().await;
    assert!(!league.players_by_matrix_user.contains_key(matrix_user_id));
    let contract = league
        .world
        .world_contracts
        .iter()
        .find(|contract| contract.contract_id == contract_id)
        .expect("contract should remain present");
    assert_eq!(contract.status, "review_hold");
    assert_eq!(contract.cex_status.as_deref(), Some("review_hold"));
    assert_eq!(contract.value_score, 64);
    let asset = league
        .world
        .world_assets
        .iter()
        .find(|asset| asset.asset_id == asset_id)
        .expect("asset should remain present");
    assert_eq!(asset.status, "active");
    assert_eq!(asset.value_score, 70);
    assert_eq!(asset.upgrade_level, 2);
    assert_eq!(asset.upgrade_points, 10);
    assert!(asset.last_upgrade_kind.is_none());
}

#[tokio::test]
async fn league_submission_review_hold_does_not_release_progression_or_success_count() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@league-review-hold:local.dev";
    let (status, submission) = send_json_request(
        &app,
        "POST",
        "/v1/league/matches/daily-dungeon-001/submit",
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "room_id": "!league-review-hold:local.dev",
            "body": "copy copy copy final customer deliverable with evidence package, risk controls, next action, self-review, and clear reward settlement proof."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "league submit failed: {submission}");
    assert_eq!(submission["submission"]["payout_status"], "review_hold");
    assert_eq!(submission["reward"]["ledger_status"], "held_review");
    assert!(submission["submission"]["score"].as_f64().unwrap_or(0.0) >= 60.0);
    assert_eq!(submission["player"]["submissions"], 0);
    assert_eq!(submission["player"]["wins"], 0);
    assert_eq!(submission["player"]["xp"], 0);
    assert_eq!(submission["player"]["reputation"], 0);
    assert_eq!(submission["player"]["rating"], 1000);
    assert_eq!(submission["entry"]["submissions"], 0);
    assert_eq!(submission["entry"]["best_score"], 0.0);
    assert_eq!(submission["entry"]["rewards_earned"], 0.0);

    let (progression_status, progression) = send_json_request(
        &app,
        "GET",
        &format!("/v1/league/players/{matrix_user_id}/progression"),
        &[],
        json!({}),
    )
    .await;
    assert_eq!(
        progression_status,
        StatusCode::OK,
        "progression failed: {progression}"
    );
    assert_eq!(progression["successful_task_count"], 0);
    assert_eq!(progression["level"], 1);

    let league = state.inner.league_state.lock().await;
    let player = league
        .players_by_matrix_user
        .get(matrix_user_id)
        .expect("player should exist for audit trail");
    assert_eq!(player.submissions, 0);
    assert_eq!(player.wins, 0);
    assert_eq!(player.xp, 0);
    assert_eq!(player.reputation, 0);
    assert_eq!(player.rating, 1000);
    let entry = league
        .entries
        .values()
        .find(|entry| {
            entry.match_id == "daily-dungeon-001" && entry.matrix_user_id == matrix_user_id
        })
        .expect("entry should exist for audit trail");
    assert_eq!(entry.submissions, 0);
    assert_eq!(entry.best_score, 0.0);
    assert_eq!(entry.rewards_earned, 0.0);
    assert!(league.inventory_items.is_empty());
}

#[tokio::test]
async fn league_web_submit_review_hold_does_not_release_progression() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@league-web-review-hold:local.dev";
    let body = "action=submit&matrix_user_id=%40league-web-review-hold%3Alocal.dev&match_id=daily-dungeon-001&body=copy+copy+copy+final+customer+deliverable+with+evidence+package+risk+controls+self-review+next+action+and+settlement+proof";
    let request = Request::builder()
        .method("POST")
        .uri("/league/web/action")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("build league web review hold submit request");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("league web review hold submit response");
    assert!(
        response.status().is_redirection(),
        "league web submit should redirect back to shell"
    );

    let league = state.inner.league_state.lock().await;
    let player = league
        .players_by_matrix_user
        .get(matrix_user_id)
        .expect("player should exist for audit trail");
    assert_eq!(player.submissions, 0);
    assert_eq!(player.wins, 0);
    assert_eq!(player.xp, 0);
    assert_eq!(player.reputation, 0);
    assert_eq!(player.rating, 1000);
    let entry = league
        .entries
        .values()
        .find(|entry| {
            entry.match_id == "daily-dungeon-001" && entry.matrix_user_id == matrix_user_id
        })
        .expect("entry should exist for audit trail");
    assert_eq!(entry.submissions, 0);
    assert_eq!(entry.best_score, 0.0);
    assert_eq!(entry.rewards_earned, 0.0);
    let submission = league
        .submissions
        .values()
        .find(|submission| submission.matrix_user_id == matrix_user_id)
        .expect("submission should be recorded for review");
    assert_eq!(submission.payout_status.as_deref(), Some("review_hold"));
    assert!(submission.score >= 60.0);
    let reward = league
        .rewards
        .iter()
        .find(|reward| reward.matrix_user_id == matrix_user_id)
        .expect("reward should be recorded for review");
    assert_eq!(reward.ledger_status.as_deref(), Some("held_review"));
    assert_eq!(reward.review_status.as_deref(), Some("pending_review"));
    assert!(league.inventory_items.is_empty());
}

#[tokio::test]
async fn league_raid_review_hold_does_not_release_progression_or_progress() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@league-raid-review-hold:local.dev";
    let (status, response) = send_json_request(
        &app,
        "POST",
        "/v1/league/raids/guild-raid-001/contribute",
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "room_id": "!league-raid-review-hold:local.dev",
            "role": "scout",
            "body": "copy copy copy final customer deliverable with evidence package, risk gate, next action, self-review, scout team coordination, and clear raid proof."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "raid contribution failed: {response}"
    );
    assert_eq!(response["contribution"]["payout_status"], "review_hold");
    assert!(response["contribution"]["anti_cheat_flags"]
        .as_array()
        .is_some_and(|flags| !flags.is_empty()));
    assert_eq!(response["contribution"]["progress_delta"], 0.0);
    assert_eq!(response["progress"]["progress_percent"], 0.0);
    assert_eq!(response["progress"]["contribution_count"], 0);
    assert_eq!(response["progress"]["average_score"], 0.0);

    let league = state.inner.league_state.lock().await;
    let contribution = league
        .raid_contributions
        .iter()
        .find(|contribution| contribution.matrix_user_id == matrix_user_id)
        .expect("held contribution should remain as an audit record");
    assert_eq!(contribution.payout_status.as_deref(), Some("review_hold"));
    assert_eq!(contribution.progress_delta, 0.0);
    assert!(!contribution.anti_cheat_flags.is_empty());
    let player = league
        .players_by_matrix_user
        .get(matrix_user_id)
        .expect("player shell should exist for audit trail");
    assert_eq!(player.xp, 0);
    assert_eq!(player.reputation, 0);
    assert_eq!(player.rating, 1000);
    let entry = league
        .entries
        .values()
        .find(|entry| entry.match_id == "guild-raid-001" && entry.matrix_user_id == matrix_user_id)
        .expect("entry shell should exist for audit trail");
    assert_eq!(entry.battles_started, 0);
}

#[tokio::test]
async fn league_submission_requires_ledger_settlement_before_earned_rewards() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let (status, submission) = send_json_request(
        &app,
        "POST",
        "/v1/league/matches/daily-dungeon-001/submit",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "body": "Submit a final deliverable with evidence package, risk controls, next action, cost-aware strategy, self-review, and clear reward settlement proof."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "league submit failed: {submission}");
    assert_eq!(
        submission["reward"]["ledger_status"],
        "skipped_missing_room"
    );
    assert!(submission["reward"]["amount"].as_f64().unwrap_or(0.0) > 0.0);
    assert_eq!(submission["player"]["earned_credits"], 0.0);
    assert_eq!(submission["player"]["submissions"], 0);
    assert_eq!(submission["player"]["xp"], 0);
    assert_eq!(submission["player"]["reputation"], 0);
    assert_eq!(submission["player"]["rating"], 1000);
    assert_eq!(submission["entry"]["rewards_earned"], 0.0);
    assert_eq!(submission["entry"]["submissions"], 0);
    assert_eq!(submission["entry"]["best_score"], 0.0);

    let league = state.inner.league_state.lock().await;
    let player = league
        .players_by_matrix_user
        .get("@alice:local.dev")
        .expect("player should be created by league submission");
    assert_eq!(player.earned_credits, 0.0);
    assert_eq!(player.submissions, 0);
    assert_eq!(player.xp, 0);
    assert_eq!(player.reputation, 0);
    assert_eq!(player.rating, 1000);
    let entry = league
        .entries
        .values()
        .find(|entry| {
            entry.match_id == "daily-dungeon-001" && entry.matrix_user_id == "@alice:local.dev"
        })
        .expect("entry should be created by league submission");
    assert_eq!(entry.rewards_earned, 0.0);
    assert_eq!(entry.submissions, 0);
    assert_eq!(entry.best_score, 0.0);
    assert!(league.inventory_items.is_empty());
    drop(league);

    let (status, progression) = send_json_request(
        &app,
        "GET",
        "/v1/league/players/@alice:local.dev/progression",
        &[],
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "progression failed: {progression}");
    assert_eq!(progression["successful_task_count"], 0);
    assert_eq!(progression["level"], 1);

    let (status, rewards) = send_json_request(
        &app,
        "GET",
        "/v1/league/players/@alice:local.dev/rewards",
        &[],
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "league rewards failed: {rewards}");
    assert_eq!(rewards["total_earned"], 0.0);
}

#[tokio::test]
async fn league_web_submit_requires_ledger_settlement_before_earned_rewards() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let body = "action=submit&matrix_user_id=%40alice%3Alocal.dev&match_id=daily-dungeon-001&body=Web+submit+final+deliverable+with+evidence+risk+controls+self-review+next+action+and+settlement+proof";
    let request = Request::builder()
        .method("POST")
        .uri("/league/web/action")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("build league web submit request");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("league web submit response");
    assert!(
        response.status().is_redirection(),
        "league web submit should redirect back to shell"
    );

    let league = state.inner.league_state.lock().await;
    let player = league
        .players_by_matrix_user
        .get("@alice:local.dev")
        .expect("player should be created by league web submit");
    assert_eq!(player.earned_credits, 0.0);
    assert_eq!(player.submissions, 0);
    assert_eq!(player.xp, 0);
    assert_eq!(player.reputation, 0);
    assert_eq!(player.rating, 1000);
    let entry = league
        .entries
        .values()
        .find(|entry| {
            entry.match_id == "daily-dungeon-001" && entry.matrix_user_id == "@alice:local.dev"
        })
        .expect("entry should be created by league web submit");
    assert_eq!(entry.rewards_earned, 0.0);
    assert_eq!(entry.submissions, 0);
    assert_eq!(entry.best_score, 0.0);
    assert!(league.inventory_items.is_empty());
    let reward = league
        .rewards
        .iter()
        .find(|reward| reward.matrix_user_id == "@alice:local.dev")
        .expect("reward should be recorded for audit/review");
    assert_eq!(
        reward.ledger_status.as_deref(),
        Some("skipped_missing_account")
    );
    assert!(reward.amount > 0.0);
}

#[tokio::test]
async fn league_review_approval_failed_does_not_release_progression_or_success_count() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let submission_id = "submission-review-approval-failed-no-progression".to_string();
    let reward_id = league_hash_id("reward", &submission_id);
    let match_id = "daily-dungeon-001".to_string();
    let matrix_user_id = "@approval-failed-no-progress:local.dev".to_string();
    let player_id = "player-approval-failed-no-progress".to_string();
    let entry_id = "entry-approval-failed-no-progress".to_string();
    {
        let mut league = state.inner.league_state.lock().await;
        league.players_by_matrix_user.insert(
            matrix_user_id.clone(),
            LeaguePlayer {
                player_id: player_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                display_name: "Approval Failed".to_string(),
                class_tag: "scout".to_string(),
                rank_tier: "bronze".to_string(),
                rating: 1000,
                xp: 0,
                reputation: 0,
                battles: 0,
                submissions: 0,
                wins: 0,
                earned_credits: 0.0,
                created_at_epoch: 1_777_898_000,
            },
        );
        league.entries.insert(
            format!("{match_id}\u{1f}{matrix_user_id}"),
            LeagueMatchEntry {
                entry_id: entry_id.clone(),
                match_id: match_id.clone(),
                player_id: player_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                status: "active".to_string(),
                battles_started: 0,
                submissions: 0,
                best_score: 0.0,
                rewards_earned: 0.0,
                joined_at_epoch: 1_777_898_000,
            },
        );
        league.submissions.insert(
            submission_id.clone(),
            LeagueSubmission {
                submission_id: submission_id.clone(),
                match_id: match_id.clone(),
                entry_id: entry_id.clone(),
                player_id: player_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                task_id: None,
                body: "held review submission pending human approval".to_string(),
                score: 86.0,
                grade: "A".to_string(),
                reward_amount: 7.0,
                judge_status: Some("rubric_hidden_pipeline_v2".to_string()),
                payout_status: Some("review_hold".to_string()),
                anti_cheat_flags: vec!["repetition_suspected".to_string()],
                score_events: Vec::new(),
                created_at_epoch: 1_777_898_001,
            },
        );
        league.rewards.push(LeagueReward {
            reward_id: reward_id.clone(),
            match_id: match_id.clone(),
            entry_id: entry_id.clone(),
            player_id: player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            amount: 7.0,
            currency_unit: "credit".to_string(),
            reason: "held_review_retry_without_account".to_string(),
            ledger_status: Some("held_review".to_string()),
            ledger_account_id: None,
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: Some("held for review".to_string()),
            review_status: Some("pending_review".to_string()),
            reviewed_by: None,
            review_note: None,
            reviewed_at_epoch: None,
            created_at_epoch: 1_777_898_001,
        });
    }

    let (status, approval) = send_json_request(
        &app,
        "POST",
        &format!("/v1/league/reviews/{reward_id}/approve"),
        &[],
        json!({"reviewer_id": "ops", "note": "approve but ledger cannot release"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "approval response: {approval}");
    assert_eq!(approval["review_status"], "approval_failed");
    assert_ne!(approval["ledger_status"], "settled");

    let (status, progression) = send_json_request(
        &app,
        "GET",
        &format!("/v1/league/players/{matrix_user_id}/progression"),
        &[],
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "progression response: {progression}"
    );
    assert_eq!(progression["successful_task_count"], 0);
    assert_eq!(progression["level"], 1);

    let league = state.inner.league_state.lock().await;
    let stored_submission = league
        .submissions
        .get(&submission_id)
        .expect("submission remains for retry");
    assert_eq!(
        stored_submission.payout_status.as_deref(),
        Some("review_hold")
    );
    assert!(stored_submission
        .score_events
        .iter()
        .any(|event| event.dimension == "human_review_release_failed"));
    let player = league
        .players_by_matrix_user
        .get(&matrix_user_id)
        .expect("player should remain present");
    assert_eq!(player.submissions, 0);
    assert_eq!(player.wins, 0);
    assert_eq!(player.xp, 0);
    assert_eq!(player.reputation, 0);
    assert_eq!(player.rating, 1000);
    assert_eq!(player.earned_credits, 0.0);
    let entry = league
        .entries
        .values()
        .find(|entry| entry.entry_id == entry_id)
        .expect("entry should remain present");
    assert_eq!(entry.submissions, 0);
    assert_eq!(entry.best_score, 0.0);
    assert_eq!(entry.rewards_earned, 0.0);
    assert!(league.inventory_items.is_empty());
}

#[tokio::test]
async fn league_review_cannot_reapprove_or_reject_released_reward() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let submission_id = "submission-review-release-guard".to_string();
    let reward_id = league_hash_id("reward", &submission_id);
    let match_id = "daily-dungeon-001".to_string();
    let matrix_user_id = "@alice:local.dev".to_string();
    let player_id = "player-review-release-guard".to_string();
    let entry_id = "entry-review-release-guard".to_string();
    {
        let mut league = state.inner.league_state.lock().await;
        league.players_by_matrix_user.insert(
            matrix_user_id.clone(),
            LeaguePlayer {
                player_id: player_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                display_name: "Alice".to_string(),
                class_tag: "scout".to_string(),
                rank_tier: "bronze".to_string(),
                rating: 1000,
                xp: 100,
                reputation: 10,
                battles: 0,
                submissions: 1,
                wins: 1,
                earned_credits: 7.0,
                created_at_epoch: 1_777_897_900,
            },
        );
        league.entries.insert(
            format!("{match_id}\u{1f}{matrix_user_id}"),
            LeagueMatchEntry {
                entry_id: entry_id.clone(),
                match_id: match_id.clone(),
                player_id: player_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                status: "active".to_string(),
                battles_started: 0,
                submissions: 1,
                best_score: 86.0,
                rewards_earned: 7.0,
                joined_at_epoch: 1_777_897_900,
            },
        );
        league.submissions.insert(
            submission_id.clone(),
            LeagueSubmission {
                submission_id: submission_id.clone(),
                match_id: match_id.clone(),
                entry_id: entry_id.clone(),
                player_id: player_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                task_id: None,
                body: "already released review reward".to_string(),
                score: 86.0,
                grade: "A".to_string(),
                reward_amount: 7.0,
                judge_status: Some("accepted".to_string()),
                payout_status: Some("approved_release".to_string()),
                anti_cheat_flags: Vec::new(),
                score_events: Vec::new(),
                created_at_epoch: 1_777_897_901,
            },
        );
        league.rewards.push(LeagueReward {
            reward_id: reward_id.clone(),
            match_id: match_id.clone(),
            entry_id: entry_id.clone(),
            player_id: player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            amount: 7.0,
            currency_unit: "credit".to_string(),
            reason: "already_released_duplicate".to_string(),
            ledger_status: Some("duplicate".to_string()),
            ledger_account_id: Some("acct-alice".to_string()),
            ledger_entry_id: Some("ledger-entry-once".to_string()),
            ledger_balance_after: Some(7.0),
            ledger_error: None,
            review_status: Some("approved".to_string()),
            reviewed_by: Some("ops".to_string()),
            review_note: Some("released once".to_string()),
            reviewed_at_epoch: Some(1_777_897_902),
            created_at_epoch: 1_777_897_901,
        });
    }

    let (status, approval) = send_json_request(
        &app,
        "POST",
        &format!("/v1/league/reviews/{reward_id}/approve"),
        &[],
        json!({"reviewer_id": "ops", "note": "approve again"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "approval response: {approval}"
    );
    assert_eq!(approval["error"], "league reward already released");

    let (status, rejection) = send_json_request(
        &app,
        "POST",
        &format!("/v1/league/reviews/{reward_id}/reject"),
        &[],
        json!({"reviewer_id": "ops", "note": "reject after release"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "reject response: {rejection}");
    assert_eq!(rejection["error"], "league reward already released");

    let league = state.inner.league_state.lock().await;
    let player = league
        .players_by_matrix_user
        .get(&matrix_user_id)
        .expect("released player should remain present");
    assert_eq!(player.earned_credits, 7.0);
    let entry = league
        .entries
        .values()
        .find(|entry| entry.entry_id == entry_id)
        .expect("released entry should remain present");
    assert_eq!(entry.rewards_earned, 7.0);
    let reward = league
        .rewards
        .iter()
        .find(|reward| reward.reward_id == reward_id)
        .expect("released reward should remain present");
    assert_eq!(reward.ledger_status.as_deref(), Some("duplicate"));
    assert_eq!(reward.review_status.as_deref(), Some("approved"));
}

#[tokio::test]
async fn league_review_queue_keeps_approval_failed_rewards_visible() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let submission_id = "submission-approval-failed-visible".to_string();
    let reward_id = league_hash_id("reward", &submission_id);
    {
        let mut league = state.inner.league_state.lock().await;
        league.submissions.insert(
            submission_id.clone(),
            LeagueSubmission {
                submission_id: submission_id.clone(),
                match_id: "daily-dungeon-001".to_string(),
                entry_id: "entry-approval-failed-visible".to_string(),
                player_id: "player-approval-failed-visible".to_string(),
                matrix_user_id: "@alice:local.dev".to_string(),
                task_id: None,
                body: "approval failed should remain visible".to_string(),
                score: 82.0,
                grade: "A".to_string(),
                reward_amount: 6.0,
                judge_status: Some("accepted".to_string()),
                payout_status: Some("review_hold".to_string()),
                anti_cheat_flags: vec!["approval_failed_retry".to_string()],
                score_events: Vec::new(),
                created_at_epoch: 1_777_897_930,
            },
        );
        league.rewards.push(LeagueReward {
            reward_id: reward_id.clone(),
            match_id: "daily-dungeon-001".to_string(),
            entry_id: "entry-approval-failed-visible".to_string(),
            player_id: "player-approval-failed-visible".to_string(),
            matrix_user_id: "@alice:local.dev".to_string(),
            amount: 6.0,
            currency_unit: "credit".to_string(),
            reason: "approval_failed_retry_needed".to_string(),
            ledger_status: Some("failed_ledger".to_string()),
            ledger_account_id: Some("acct-alice".to_string()),
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: Some("transient ledger failure".to_string()),
            review_status: Some("approval_failed".to_string()),
            reviewed_by: Some("ops".to_string()),
            review_note: Some("retry after ledger recovers".to_string()),
            reviewed_at_epoch: Some(1_777_897_931),
            created_at_epoch: 1_777_897_930,
        });
    }

    let (status, queue) =
        send_json_request(&app, "GET", "/v1/league/reviews/held", &[], json!({})).await;
    assert_eq!(status, StatusCode::OK, "held reviews response: {queue}");
    assert_eq!(queue["held_count"], 1);
    assert_eq!(queue["held"][0]["reward"]["reward_id"], reward_id);
    assert_eq!(
        queue["held"][0]["reward"]["review_status"],
        "approval_failed"
    );
}

#[tokio::test]
async fn world_contract_completion_requires_ledger_settlement_before_progression() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let initial_asset_count;
    let initial_contract_value = 64;
    {
        let mut league = state.inner.league_state.lock().await;
        initial_asset_count = league.world.world_assets.len();
        let starter_location_id = league
            .world
            .world_map_nodes
            .get("starter-studio")
            .map(|node| node.location_id.clone())
            .unwrap_or_else(|| "starter-studio".to_string());
        league.world.world_events.push(WorldEvent {
            event_id: "world-event-unsettled-contract".to_string(),
            actor_matrix_user_id: "@alice:local.dev".to_string(),
            room_id: Some("!room:local.dev".to_string()),
            location_id: starter_location_id.clone(),
            event_kind: "world_contract".to_string(),
            body: "Contract completion payout settlement guard".to_string(),
            result: "queued".to_string(),
            impact_score: 7,
            cex_task_id: Some("task-unsettled-contract".to_string()),
            cex_status: Some("Running".to_string()),
            created_at_epoch: 1_777_895_900,
        });
        league.world.world_contracts.push(WorldContract {
            contract_id: "world-contract-unsettled-payout".to_string(),
            event_id: "world-event-unsettled-contract".to_string(),
            actor_matrix_user_id: "@alice:local.dev".to_string(),
            location_id: starter_location_id,
            task_id: "task-unsettled-contract".to_string(),
            title: "Unsettled payout guard".to_string(),
            body: "Complete only after real ledger settlement".to_string(),
            status: "open".to_string(),
            cex_status: Some("Running".to_string()),
            value_score: initial_contract_value,
            created_at_epoch: 1_777_895_901,
        });
    }
    let app = build_router(state.clone());
    let (status, completion) = send_json_request(
        &app,
        "POST",
        "/v1/world/contracts/world-contract-unsettled-payout/complete",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "body": "Completion includes final deliverable, evidence package, risk controls, next action, rubric self review, and clear settlement proof."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "contract completion failed: {completion}"
    );
    assert_eq!(
        completion["completion"]["ledger_status"],
        "skipped_missing_room"
    );
    assert!(
        completion["completion"]["reward_amount"]
            .as_f64()
            .unwrap_or(0.0)
            > 0.0
    );
    let league = state.inner.league_state.lock().await;
    assert!(
        !league
            .players_by_matrix_user
            .contains_key("@alice:local.dev"),
        "contract completion must not create/reward player progression before ledger settlement"
    );
    assert_eq!(
        league.world.world_assets.len(),
        initial_asset_count,
        "contract completion must not mint or upgrade assets before ledger settlement"
    );
    let stored_contract = league
        .world
        .world_contracts
        .iter()
        .find(|contract| contract.contract_id == "world-contract-unsettled-payout")
        .expect("stored contract");
    assert_eq!(stored_contract.value_score, initial_contract_value);
    assert_eq!(stored_contract.status, "completed_skipped_missing_room");
    assert_eq!(
        stored_contract.cex_status.as_deref(),
        Some("settlement_blocked")
    );
}

#[tokio::test]
async fn world_contract_completion_cannot_release_twice() {
    let (ledger_base_url, ledger_admin_token) = start_real_ledger_service_for_world_e2e().await;
    let http = Client::new();
    let account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 0.0).await;
    let matrix_user_id = "@world-contract-repeat:local.dev";
    let room_id = "!world-contract-repeat:local.dev";
    let mut bindings = IdentityBindings::default();
    bindings.matrix_users.insert(
        matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-contract-repeat-org".to_string()),
            account_id: Some(account_id.clone()),
        },
    );
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    config.ledger_base_url = ledger_base_url.clone();
    config.ledger_admin_token = Some(ledger_admin_token.clone());
    let state = test_state(config, bindings, HashMap::new());
    let initial_asset_count;
    {
        let mut league = state.inner.league_state.lock().await;
        initial_asset_count = league.world.world_assets.len();
        league.world.world_events.push(WorldEvent {
            event_id: "world-event-repeat-contract".to_string(),
            actor_matrix_user_id: matrix_user_id.to_string(),
            room_id: Some(room_id.to_string()),
            location_id: "starter-studio".to_string(),
            event_kind: "world_contract".to_string(),
            body: "Repeat completion guard event".to_string(),
            result: "queued".to_string(),
            impact_score: 9,
            cex_task_id: Some("task-repeat-contract".to_string()),
            cex_status: Some("Running".to_string()),
            created_at_epoch: 1_777_895_910,
        });
        league.world.world_contracts.push(WorldContract {
            contract_id: "world-contract-repeat-release".to_string(),
            event_id: "world-event-repeat-contract".to_string(),
            actor_matrix_user_id: matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            task_id: "task-repeat-contract".to_string(),
            title: "Repeat release guard".to_string(),
            body: "A settled contract must not mint a second reward.".to_string(),
            status: "open".to_string(),
            cex_status: Some("Running".to_string()),
            value_score: 42,
            created_at_epoch: 1_777_895_911,
        });
    }
    let app = build_router(state.clone());
    let first_body = json!({
        "matrix_user_id": matrix_user_id,
        "room_id": room_id,
        "body": "Final customer deliverable with evidence package, risk controls, next action, self-review, settlement proof, measurable acceptance checklist, and remediation notes."
    });
    let (status, first_completion) = send_json_request(
        &app,
        "POST",
        "/v1/world/contracts/world-contract-repeat-release/complete",
        &[],
        first_body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "first completion: {first_completion}"
    );
    assert_eq!(first_completion["completion"]["ledger_status"], "settled");
    let first_reward = first_completion["completion"]["reward_amount"]
        .as_f64()
        .expect("first reward amount");
    assert!(first_reward > 0.0);

    let (status, second_completion) = send_json_request(
        &app,
        "POST",
        "/v1/world/contracts/world-contract-repeat-release/complete",
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "room_id": room_id,
            "body": "Second completion tries to repeat the settled payout with evidence, risk controls, next action, self-review, and settlement proof."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "second completion should be blocked: {second_completion}"
    );
    assert_eq!(
        second_completion["error"],
        "world contract is already completed"
    );

    let league = state.inner.league_state.lock().await;
    assert_eq!(
        league
            .world
            .world_contract_completions
            .iter()
            .filter(|completion| completion.contract_id == "world-contract-repeat-release")
            .count(),
        1,
        "a settled contract must not append a second completion record"
    );
    let player = league
        .players_by_matrix_user
        .get(matrix_user_id)
        .expect("settled contract should create player progression once");
    assert_eq!(player.earned_credits, first_reward);
    assert_eq!(league.world.world_assets.len(), initial_asset_count + 1);
    let stored_contract = league
        .world
        .world_contracts
        .iter()
        .find(|contract| contract.contract_id == "world-contract-repeat-release")
        .expect("stored repeat contract");
    assert_eq!(stored_contract.status, "completed_settled");
    assert_eq!(stored_contract.cex_status.as_deref(), Some("completed"));
    drop(league);

    let account =
        get_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, &account_id).await;
    assert_eq!(account["balance"].as_f64().unwrap(), first_reward);
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

async fn send_json_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    for (name, value) in headers {
        request = request.header(*name, *value);
    }

    let request = request
        .body(Body::from(
            serde_json::to_vec(&body).expect("serialize request body"),
        ))
        .expect("build request body");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("request response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body bytes");
    let body: Value = serde_json::from_slice(&bytes).expect("decode json response body");

    (status, body)
}

async fn start_real_ledger_service_for_world_e2e() -> (String, String) {
    let admin_token = "world-commerce-real-ledger-token".to_string();
    let state = LedgerAppState::new_for_tests(
        PostgresLedgerRepository::new_placeholder(),
        false,
        Some(admin_token.clone()),
        vec!["ledger:manage".to_string(), "ledger:read".to_string()],
        Vec::new(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind real ledger test service");
    let addr = listener.local_addr().expect("ledger test local addr");
    tokio::spawn(async move {
        axum::serve(listener, build_ledger_router(state))
            .await
            .expect("serve ledger test router");
    });
    (format!("http://{addr}"), admin_token)
}

async fn create_real_ledger_account(
    http: &Client,
    ledger_base_url: &str,
    admin_token: &str,
    initial_balance: f64,
) -> String {
    let response = http
        .post(format!("{}/v1/accounts", ledger_base_url))
        .header("x-admin-token", admin_token)
        .json(&json!({
            "org_id": "world-commerce-org",
            "account_type": "world_player",
            "currency_unit": "credit",
            "initial_balance": initial_balance,
        }))
        .send()
        .await
        .expect("create real ledger account");
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    response
        .json::<Value>()
        .await
        .expect("decode created ledger account")["account_id"]
        .as_str()
        .expect("created ledger account id")
        .to_string()
}

async fn get_real_ledger_account(
    http: &Client,
    ledger_base_url: &str,
    admin_token: &str,
    account_id: &str,
) -> Value {
    let response = http
        .get(format!("{}/v1/accounts/{account_id}", ledger_base_url))
        .header("x-admin-token", admin_token)
        .send()
        .await
        .expect("get real ledger account");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response
        .json::<Value>()
        .await
        .expect("decode real ledger account")
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
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["contract_version"],
        "trillionnium_world_playability_scorecard_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["target"],
        "all_5_user_playability_metrics_score_10_of_10"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["diagnostic_target"],
        "all_10_playability_sub_axes_score_10_of_10"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["score_unit"],
        "0_to_10"
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axis_order"]
            .as_array()
            .is_some_and(|axes| axes.len() == 10)
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["user_metric_order"]
            .as_array()
            .is_some_and(|axes| axes.len() == 5)
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["user_metric_overall_score"].is_number()
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["intent_mapping"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "six_core_intents_covered")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["user_metric_axes"]
            ["real_player_comprehension_cost"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "league_score_breakdown_explainable")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["surface_feedback"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "playability_coach_visible")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["user_metric_axes"]
            ["economy_social_strategy_depth"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "coach_strategy_depth_visible")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["observability_gates"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "scorecard_has_runtime_funnel_data")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["user_metric_axes"]
            ["long_term_replayability"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "coach_retention_ops_visible")
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
    assert!(
        body.contains("cex_consumer_entry_trillionnium_world_playability_scorecard_overall_score")
    );
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_playability_scorecard_onboarding_3_minute_loop_score"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_playability_scorecard_intent_mapping_score"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_playability_scorecard_observability_gates_score"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_playability_scorecard_user_metric_overall_score"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_playability_scorecard_technical_reliability_score"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_playability_scorecard_real_player_comprehension_cost_score"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_playability_scorecard_economy_social_strategy_depth_score"
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

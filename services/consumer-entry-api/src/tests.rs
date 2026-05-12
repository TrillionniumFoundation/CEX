use super::{
    authorize_league_web_session, authorize_league_web_session_readonly, authorize_user_session,
    build_chat_identity_scope, build_chat_org_rate_limit_key, build_chat_rate_limit_key,
    build_chat_replay_key, build_chat_request_fingerprint, build_chat_room_rate_limit_key,
    build_chat_session_rate_limit_key, build_chat_user_rate_limit_key, build_matrix_identity_scope,
    build_matrix_org_rate_limit_key, build_matrix_rate_limit_key, build_matrix_replay_key,
    build_matrix_room_rate_limit_key, build_matrix_session_rate_limit_key,
    build_matrix_user_rate_limit_key, build_router, build_world_indexes,
    build_world_route_artifacts, client_app_json, client_feed_json, default_league_state,
    default_world_node_id, encode_league_web_session, evaluate_identity_binding_reload_governance,
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
    openstreetmap_fixture_identity_for, openstreetmap_geodata_v1_json,
    openstreetmap_provider_mode_status_json, parse_csv_list, project_consumer_status,
    prune_rate_limit_cache, real_world_map_engine_json, resolve_chat_identity,
    session_auth_issuer_registry_active_key_diff_json, sign_user_session_assertion,
    validate_text_payload, world_home_json, world_map_delta_json, world_map_json,
    world_map_viewport_json, world_route_ui_contract_json, world_tactics_board_projection_json,
    world_trillionnium_character_projection_json, AppState, AppStateInner, ConsumerEntryConfig,
    ConsumerEntryMetrics, CreateChatTaskRequest, IdentityBindingAuditState, IdentityBindingEntry,
    IdentityBindingMetadata, IdentityBindingRevisionApprovalState, IdentityBindingStore,
    IdentityBindings, LeagueMatchEntry, LeaguePlayer, LeagueReward, LeagueStateRepositorySnapshot,
    LeagueSubmission, LeagueWebSessionClaims, MatrixMessageRequest, ProductUserIdentity,
    RateLimitCache, ReplayCache, RuntimeProfile, SessionAuthIssuerRegistryIssuer,
    SessionAuthIssuerRegistryMetadata, SessionAuthIssuerRegistryRuntimeState,
    UserSessionAuthClaims, WorldAsset, WorldCompany, WorldContract, WorldContractCompletion,
    WorldEconomyEvent, WorldEvent, WorldListing, WorldMapNode, WorldPlayerPosition, WorldPurchase,
    WorldRelationship, WorldShop, WorldTrillionniumCharacter, WorldWorkCancellation,
    WorldWorkOrder, WorldWorkRejection, DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS,
    DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS, DEFAULT_MAX_TEXT_CHARS,
    TRILLIONNIUM_REPOSITORY_MIGRATION_FLOOR, USER_SESSION_ASSERTION_HEADER,
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
    fs,
    path::Path,
    sync::{atomic::AtomicU64, Arc, RwLock as StdRwLock},
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
fn trillionnium_terminology_guard_blocks_legacy_and_double_rename_terms() {
    fn scan_file(path: &Path, failures: &mut Vec<String>) {
        let Ok(raw) = fs::read(path) else {
            return;
        };
        if raw.contains(&0) {
            return;
        }
        let Ok(text) = String::from_utf8(raw) else {
            return;
        };
        let forbidden_terms = [
            format!("{}{}", "Jiang", "hu"),
            format!("{}{}", "jiang", "hu"),
            format!("{}{}", '江', '湖'),
            format!("{}{}", "TRILLIONNIUM_", "TRILLIONNIUM"),
            format!("{}{}", "trillionnium_", "trillionnium"),
            format!("{}{}", "Trillionnium ", "Trillionnium"),
            format!("{}{}", "trillionnium-", "trillionnium"),
            format!("{}{}", "trillionnium", "Trillionnium"),
        ];
        for forbidden in forbidden_terms {
            if text.contains(&forbidden) {
                failures.push(format!("{} contains {forbidden}", path.display()));
            }
        }
    }

    fn scan_dir(path: &Path, failures: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if name == "target" || name == ".git" || name == "run" {
                continue;
            }
            if path.is_dir() {
                scan_dir(&path, failures);
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("rs" | "sh" | "mjs" | "md" | "sql" | "json" | "toml" | "yml" | "yaml")
            ) {
                scan_file(&path, failures);
            }
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut failures = Vec::new();
    for relative in ["docs", "migrations", "scripts", "services"] {
        scan_dir(&root.join(relative), &mut failures);
    }
    assert!(
        failures.is_empty(),
        "legacy/double-renamed Trillionnium terminology found: {failures:#?}"
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
            write_set.get("command").and_then(Value::as_str) == Some("world_tactics_command")
                && write_set
                    .get("tables")
                    .and_then(Value::as_array)
                    .is_some_and(|tables| {
                        tables
                            .iter()
                            .any(|table| table.as_str() == Some("world_trillionnium_characters"))
                            && tables
                                .iter()
                                .any(|table| table.as_str() == Some("world_tactics_sessions"))
                            && tables.iter().any(|table| {
                                table.as_str() == Some("world_tactics_simulation_ticks")
                            })
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
        Some(13)
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
    assert!(tables.iter().any(|table| {
        table.get("table_name").and_then(Value::as_str) == Some("world_tactics_sessions")
            && table.get("source_path").and_then(Value::as_str)
                == Some("world.world_tactics_sessions")
            && table.get("primary_key").and_then(Value::as_str) == Some("session_id")
    }));
    assert!(tables.iter().any(|table| {
        table.get("table_name").and_then(Value::as_str) == Some("world_tactics_simulation_ticks")
            && table.get("primary_key").and_then(Value::as_str) == Some("tick_id")
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
    assert!(shadow_sql_contract
        .get("tables")
        .and_then(Value::as_array)
        .is_some_and(
            |tables| tables.iter().any(|table| table == "world_tactics_sessions")
                && tables
                    .iter()
                    .any(|table| table == "world_tactics_simulation_ticks")
        ));
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
    .expect("repository migration floor should exist");
    assert!(repository_migration.contains("world_trillionnium_characters"));
    assert!(repository_migration.contains("combat_numerics_state"));
    let region_story_migration = std::fs::read_to_string(format!(
        "{}/../../migrations/0024_add_trillionnium_region_story_unlock_runtime_column.sql",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("region/story unlock runtime migration should exist");
    assert!(region_story_migration.contains("world_trillionnium_characters"));
    assert!(region_story_migration.contains("region_story_unlock_state"));
    let resource_pressure_migration = std::fs::read_to_string(format!(
        "{}/../../migrations/0023_add_trillionnium_resource_pressure_runtime_column.sql",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("resource pressure runtime migration should exist");
    assert!(resource_pressure_migration.contains("resource_pressure_state"));
    let tactics_migration = std::fs::read_to_string(format!(
        "{}/../../migrations/0021_add_trillionnium_tactics_storage_tables.sql",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("tactics storage migration should exist");
    assert!(tactics_migration.contains("world_tactics_sessions"));
    assert!(tactics_migration.contains("world_tactics_simulation_ticks"));
    assert!(tactics_migration.contains("objective_progress"));
    assert!(tactics_migration.contains("reward_status"));
    let equipment_migration = std::fs::read_to_string(format!(
        "{}/../../migrations/0022_add_trillionnium_item_equipment_runtime_columns.sql",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("equipment runtime migration should exist");
    assert!(equipment_migration.contains("inventory_items"));
    assert!(equipment_migration.contains("equipment_slots"));
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
    league.world.world_trillionnium_characters.insert(
        "@map-move:local.dev".to_string(),
        WorldTrillionniumCharacter::default_for("@map-move:local.dev"),
    );
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
    assert!(tables
        .iter()
        .any(|table| table.as_str() == Some("world_trillionnium_characters")));

    let command_sql = normalized_repository_command_shadow_sql(&league.world, "world_map_move")
        .unwrap()
        .expect("world_map_move should produce command-scoped SQL");
    assert!(command_sql.contains("trillionnium_normalized_repository_command_shadow_sql_v1"));
    assert!(command_sql.contains("\"command\":\"world_map_move\""));
    assert!(command_sql.contains("insert into world_player_positions"));
    assert!(command_sql.contains("insert into world_economy_events"));
    assert!(command_sql.contains("insert into world_trillionnium_characters"));
    assert!(command_sql.contains("resource_pressure_state"));
    assert!(command_sql.contains("region_story_unlock_state"));
    assert!(command_sql.contains("combat_numerics_state"));
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
    league.world.world_trillionnium_characters.insert(
        "@map-move:local.dev".to_string(),
        WorldTrillionniumCharacter::default_for("@map-move:local.dev"),
    );
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
    assert!(runtime_sql.contains("insert into world_trillionnium_characters"));
    assert!(runtime_sql.contains("resource_pressure_state"));
    assert!(runtime_sql.contains("region_story_unlock_state"));
    assert!(runtime_sql.contains("combat_numerics_state"));
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
        .any(|command| command.as_str() == Some("world_tactics_command")));
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
            health_world_readiness_cache_generation: AtomicU64::new(0),
            health_world_readiness_cache: Mutex::new(None),
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
async fn term_exchange_kernel_manifest_declares_cex_as_first_backend() {
    let app = build_router(AppState::new(test_config()));
    let (status, body) = send_identity_request(
        &app,
        "GET",
        "/v1/trillionnium/term-exchange/kernel/manifest",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "trillionnium_term_exchange_kernel_manifest");
    assert_eq!(
        body["contract_version"],
        "trillionnium_term_exchange_kernel_v1"
    );
    assert_eq!(body["kernel_id"], "term-exchange-kernel");
    assert_eq!(body["active_backend_id"], "cex-settlement-backend");
    assert_eq!(body["active_backend_kind"], "cex");
    assert_eq!(
        body["protocol"]["protocol_version"],
        "term_exchange_protocol_v1"
    );
    assert!(body["ownership"]["term_exchange_kernel_owns"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value.as_str() == Some("economic_intent_schema")));
    assert!(body["ownership"]["cex_backend_owns"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value.as_str() == Some("seller_chargeback")));
    assert!(body["ownership"]["trillionnium_world_term_owns"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value.as_str() == Some("world_event_to_economic_intent_mapping")));
    assert_eq!(
        body["integration_model"]["world_progression_gate"],
        "domain_state_advances_only_after_economic_receipt_allows_progression_or_terminal_skip"
    );
    assert_eq!(body["runtime_requirements"]["fail_closed"], true);
    assert_eq!(
        body["legacy_compatibility"]["legacy_contract_version"],
        "trillionnium_cex_runtime_plugin_v1"
    );
    assert_eq!(
        body["migration_status"]["split_strategy"],
        "protocol_first_then_backend_adapter_then_storage_boundary"
    );
}

#[tokio::test]
async fn legacy_cex_runtime_manifest_endpoint_serves_term_exchange_kernel_manifest() {
    let app = build_router(AppState::new(test_config()));
    let (status, body) =
        send_identity_request(&app, "GET", "/v1/trillionnium/runtime/cex/manifest", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["contract_version"],
        "trillionnium_term_exchange_kernel_v1"
    );
    assert_eq!(
        body["legacy_compatibility"]["status"],
        "upgraded_to_term_exchange_kernel_manifest"
    );
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
        location_id: starter_location_id.clone(),
        event_kind: "world_contract".to_string(),
        body: "Test viewport stream event".to_string(),
        result: "queued".to_string(),
        impact_score: 7,
        cex_task_id: Some("task-viewport-1".to_string()),
        cex_status: Some("Queued".to_string()),
        created_at_epoch: 1_777_230_001,
    });
    league.world.world_player_positions.insert(
        "@alice:local.dev".to_string(),
        WorldPlayerPosition {
            matrix_user_id: "@alice:local.dev".to_string(),
            node_id: "starter-studio".to_string(),
            location_id: starter_location_id,
            updated_at_epoch: 1_777_230_003,
        },
    );

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
    assert!(viewport["player_avatar_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(viewport["player_density"]["mode"], "dense");
    assert_eq!(
        viewport["gameplay_layer_contract"]["product_name"],
        "Trillionnium World Map"
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["openstreetmap_base_tiles"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["avatar_task_route_overlays"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["avatar_route_runners"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["checkpoint_reward_history"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["route_runner_reward_claim_actions"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["route_runner_next_route_actions"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["route_mastery_progression"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["route_mastery_contract_version"],
        "trillionnium_route_mastery_v1"
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["map_readability_lod"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["map_readability_lod_contract_version"],
        "trillionnium_world_map_readability_lod_v1"
    );
    assert_eq!(
        viewport["map_readability_lod"]["contract_version"],
        "trillionnium_world_map_readability_lod_v1"
    );
    assert_eq!(
        viewport["map_readability_lod"]["primary_cta_budget"]["max_primary_cta_count"],
        1
    );
    assert_eq!(
        viewport["map_readability_lod"]["copy_budget"]["details_default_state"],
        "collapsed"
    );
    assert_eq!(
        viewport["map_readability_lod"]["object_budget"]["within_budget"],
        true
    );
    assert_eq!(
        viewport["runtime_performance_budget"]["contract_version"],
        "trillionnium_world_map_runtime_performance_budget_v1"
    );
    assert_eq!(
        viewport["map_readability_lod"]["runtime_performance_budget"]["contract_version"],
        "trillionnium_world_map_runtime_performance_budget_v1"
    );
    assert!(viewport["runtime_performance_budget"]["degrade_strategy"]
        ["delta_viewport_updates_required"]
        .as_bool()
        .unwrap_or(false));
    assert_eq!(
        viewport["transport_delta_contract"]["contract_version"],
        "trillionnium_world_map_transport_delta_v1"
    );
    assert_eq!(
        viewport["transport_delta_contract"]["entity_delta_cache"]["mode"],
        "entity_group_versioned_delta_v1"
    );
    assert!(
        viewport["transport_delta_contract"]["presence_payload"]["presence_delta_required"]
            .as_bool()
            .unwrap_or(false)
    );
    assert!(viewport["transport_delta_contract"]["transport_boundaries"]
        .as_object()
        .is_some());
    assert_eq!(
        viewport["renderer_shadow_parity"]["contract_version"],
        "trillionnium_world_map_renderer_shadow_v1"
    );
    assert_eq!(
        viewport["renderer_shadow_parity"]["shadow_engine_id"],
        "maplibre_gl_v1"
    );
    assert_eq!(
        viewport["renderer_shadow_parity"]["status"],
        "shadow_only_not_user_facing"
    );
    assert_eq!(
        viewport["rum_slo_contract"]["contract_version"],
        "trillionnium_world_map_rum_slo_v1"
    );
    assert_eq!(
        viewport["weak_network_resilience"]["contract_version"],
        "trillionnium_world_map_weak_network_resilience_v1"
    );
    assert_eq!(
        viewport["location_privacy_contract"]["contract_version"],
        "trillionnium_world_map_location_privacy_v1"
    );
    assert!(
        viewport["renderer_shadow_parity"]["parity_result"]["counts_match"]
            .as_bool()
            .unwrap_or(false)
    );
    assert!(viewport["delta_cursor"]
        .as_str()
        .is_some_and(|cursor| !cursor.is_empty() && cursor.contains(";gv=")));
    assert!(viewport["entity_group_versions"]
        .as_object()
        .is_some_and(|versions| versions.contains_key("avatar_route_runners")));
    assert!(viewport["delta_path"]
        .as_str()
        .unwrap_or_default()
        .contains("/delta"));
    assert!(viewport["web_session_delta_path"]
        .as_str()
        .unwrap_or_default()
        .contains("/world/web/map-delta"));
    assert_eq!(
        viewport["viewport_contract"]["supports_transport_delta_endpoint"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_renderer_shadow_parity"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_rum_slo_quantiles"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_weak_network_resilience"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_location_privacy"],
        true
    );
    let initial_cursor = viewport["delta_cursor"].as_str().unwrap().to_string();
    let noop_delta = world_map_delta_json(
        &league.world,
        "@alice:local.dev",
        None,
        None,
        Some(15),
        None,
        None,
        Some(initial_cursor.clone()),
    );
    assert_eq!(noop_delta["changed"], false);
    assert_eq!(noop_delta["snapshot_fallback_required"], true);
    assert_eq!(noop_delta["snapshot_fallback_is_failure"], false);
    assert_eq!(
        noop_delta["entity_delta"]["mode"],
        "entity_group_versioned_delta_v1"
    );
    assert_eq!(noop_delta["entity_delta"]["changed_group_count"], 0);
    assert_eq!(noop_delta["next_cursor"], initial_cursor);
    let changed_delta = world_map_delta_json(
        &league.world,
        "@alice:local.dev",
        None,
        None,
        Some(15),
        None,
        None,
        Some("stale-cursor".to_string()),
    );
    assert_eq!(changed_delta["changed"], true);
    assert_eq!(
        changed_delta["delta_mode"],
        "entity_group_versioned_delta_v1"
    );
    assert!(changed_delta["entity_delta"]["changed_group_count"]
        .as_u64()
        .is_some_and(|count| count > 0));
    assert!(changed_delta["delta"]["live_event_stream"]
        .as_array()
        .is_some());
    assert_eq!(
        changed_delta["renderer_shadow_parity"]["shadow_engine_id"],
        "maplibre_gl_v1"
    );
    assert!(viewport["map_readability_lod"]["readiness_checks"]
        .as_array()
        .is_some_and(|checks| checks
            .iter()
            .any(|check| check == "visible_marker_budget_enforced")));
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["agent_party_state"],
        true
    );
    assert_eq!(
        viewport["gameplay_layer_contract"]["supports"]["agent_party_handoff_actions"],
        true
    );
    assert_eq!(
        viewport["player_avatars"][0]["movement_status"],
        "ready_to_run_task"
    );
    assert_eq!(
        viewport["player_avatars"][0]["agent_party_layer_id"],
        "trillionnium_avatar_agent_party_state_layer"
    );
    assert!(
        viewport["player_avatars"][0]["agent_party"]
            .as_array()
            .map(|members| members.len())
            .unwrap_or(0)
            >= 4
    );
    assert!(viewport["player_avatars"][0]["agent_party_summary"]
        .as_str()
        .unwrap_or_default()
        .contains("audit risk"));
    assert!(viewport["player_avatars"][0]["agent_party_action_summary"]
        .as_str()
        .unwrap_or_default()
        .contains("Tap a party role"));
    assert!(viewport["avatar_task_route_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(
        viewport["avatar_task_routes"][0]["route_layer_id"],
        "trillionnium_avatar_task_route_overlay"
    );
    assert_eq!(
        viewport["avatar_task_routes"][0]["agent_party_layer_id"],
        "trillionnium_avatar_agent_party_state_layer"
    );
    assert_eq!(
        viewport["avatar_task_routes"][0]["reward_loop"],
        "move avatar → complete task → submit evidence → rating/reward → next route"
    );
    assert!(
        viewport["avatar_task_routes"][0]["agent_party_handoff_hint"]
            .as_str()
            .unwrap_or_default()
            .contains("self-review")
    );
    assert!(
        viewport["avatar_task_routes"][0]["agent_party_action_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("world action")
    );
    assert!(viewport["avatar_route_runner_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(
        viewport["avatar_route_runners"][0]["route_layer_id"],
        "trillionnium_avatar_route_runner_layer"
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["telemetry_layer_id"],
        "trillionnium_avatar_route_runner_telemetry_layer"
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["checkpoint_layer_id"],
        "trillionnium_avatar_route_reward_checkpoint_layer"
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["agent_party_layer_id"],
        "trillionnium_avatar_agent_party_state_layer"
    );
    assert!(
        viewport["avatar_route_runners"][0]["agent_party"]
            .as_array()
            .map(|members| members.len())
            .unwrap_or(0)
            >= 4
    );
    assert!(viewport["avatar_route_runners"][0]["agent_party_summary"]
        .as_str()
        .unwrap_or_default()
        .contains("close reward"));
    assert!(
        viewport["avatar_route_runners"][0]["agent_party_action_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("world action")
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["agent_party"][0]["action_label"],
        "Scout route / 侦察路线"
    );
    assert!(
        viewport["avatar_route_runners"][0]["agent_party"][0]["action_body"]
            .as_str()
            .unwrap_or_default()
            .contains("evidence gaps")
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["agent_party"][0]["handoff_action"]["panel_id"],
        "world-action-console"
    );
    assert!(
        viewport["avatar_route_runners"][0]["agent_party"][0]["handoff_action"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("self-review")
    );
    assert!(viewport["avatar_route_runners"][0]["current"]["lat"].is_number());
    assert!(
        viewport["avatar_route_runners"][0]["runner_trace_points"]
            .as_array()
            .map(|points| points.len())
            .unwrap_or(0)
            >= 3
    );
    assert!(
        viewport["avatar_route_runners"][0]["progress_percent"]
            .as_i64()
            .unwrap_or(0)
            > 0
    );
    assert!(
        viewport["avatar_route_runners"][0]["remaining_distance_meters"]
            .as_i64()
            .unwrap_or(0)
            >= 0
    );
    assert!(
        viewport["avatar_route_runners"][0]["eta_seconds"]
            .as_i64()
            .unwrap_or(-1)
            >= 0
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["lifecycle_contract_version"],
        "trillionnium_route_runner_lifecycle_v1"
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["lifecycle"]["contract_version"],
        "trillionnium_route_runner_lifecycle_v1"
    );
    assert!(viewport["avatar_route_runners"][0]["lifecycle_source"]
        .as_str()
        .is_some_and(|source| !source.is_empty()));
    assert!(viewport["avatar_route_runners"][0]["lifecycle_stage"]
        .as_str()
        .is_some_and(|stage| !stage.is_empty()));
    assert!(viewport["avatar_route_runners"][0]["lifecycle_status"]
        .as_str()
        .is_some_and(|status| !status.is_empty()));
    assert!(
        viewport["avatar_route_runners"][0]["lifecycle"]["route_duration_seconds"]
            .as_i64()
            .unwrap_or(0)
            >= 300
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["route_mastery_contract_version"],
        "trillionnium_route_mastery_v1"
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["route_mastery"]["contract_version"],
        "trillionnium_route_mastery_v1"
    );
    assert!(
        viewport["avatar_route_runners"][0]["route_mastery_xp"]
            .as_i64()
            .unwrap_or(0)
            > 0
    );
    assert!(viewport["avatar_route_runners"][0]["route_mastery_tier"]
        .as_str()
        .is_some_and(|tier| !tier.is_empty()));
    assert!(viewport["avatar_route_runners"][0]["route_mastery_summary"]
        .as_str()
        .unwrap_or_default()
        .contains("Route mastery"));
    assert!(
        viewport["avatar_route_runners"][0]["route_mastery_next_goal"]
            .as_str()
            .unwrap_or_default()
            .contains("evidence")
    );
    assert!(viewport["avatar_route_runners"][0]["completion_command"]
        .as_str()
        .unwrap_or_default()
        .contains("evidence"));
    assert!(
        viewport["avatar_route_runners"][0]["completion_action_body"]
            .as_str()
            .unwrap_or_default()
            .contains("self-review")
    );
    assert!(viewport["avatar_route_runners"][0]["completion_prompt"]
        .as_str()
        .unwrap_or_default()
        .contains("risk controls"));
    assert!(
        viewport["avatar_route_runners"][0]["reward_claim_action_body"]
            .as_str()
            .unwrap_or_default()
            .contains("evidence package")
    );
    assert!(
        viewport["avatar_route_runners"][0]["reward_claim_action_body"]
            .as_str()
            .unwrap_or_default()
            .contains("self-review")
    );
    assert!(
        viewport["avatar_route_runners"][0]["reward_claim_action_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("rating/reward settlement")
    );
    assert!(
        viewport["avatar_route_runners"][0]["next_route_action_body"]
            .as_str()
            .unwrap_or_default()
            .contains("evidence")
    );
    assert!(
        viewport["avatar_route_runners"][0]["next_route_action_body"]
            .as_str()
            .unwrap_or_default()
            .contains("self-review")
    );
    assert!(
        viewport["avatar_route_runners"][0]["next_route_action_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("post-reward loop")
    );
    assert!(
        viewport["avatar_route_runners"][0]["next_route_sequence_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("Trillionnium World Map route")
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["checkpoint_history_layer_id"],
        "trillionnium_avatar_route_reward_history_layer"
    );
    assert!(
        viewport["avatar_route_runners"][0]["checkpoint_history"]
            .as_array()
            .map(|items| items.len())
            .unwrap_or(0)
            >= 3
    );
    assert!(
        viewport["avatar_route_runners"][0]["checkpoint_history_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("rating/reward settlement")
    );
    assert!(
        viewport["avatar_route_runners"][0]["reward_history_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("Reward history")
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["reward_checkpoint"]["layer_id"],
        "trillionnium_avatar_route_reward_checkpoint_layer"
    );
    assert!(
        viewport["avatar_route_runners"][0]["reward_checkpoint"]["reward_claim_label"]
            .as_str()
            .unwrap_or_default()
            .contains("rating/reward")
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["reward_checkpoint"]["reward_claim_action"]["panel_id"],
        "world-action-console"
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["reward_checkpoint"]["reward_claim_action"]
            ["textarea_id"],
        "world-action-body"
    );
    assert!(
        viewport["avatar_route_runners"][0]["reward_checkpoint"]["reward_claim_action"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("risk controls")
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["reward_checkpoint"]["next_route_action"]["panel_id"],
        "world-action-console"
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["reward_checkpoint"]["next_route_action"]
            ["textarea_id"],
        "world-action-body"
    );
    assert!(
        viewport["avatar_route_runners"][0]["reward_checkpoint"]["next_route_action"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("risk controls")
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["animation_hint"],
        "animate_avatar_marker_between_route_endpoints"
    );
    assert_eq!(
        viewport["avatar_route_runners"][0]["movement_state"],
        "en_route_to_task_reward"
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_player_avatars"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_avatar_task_routes"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_avatar_route_runners"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_checkpoint_reward_history"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_route_runner_lifecycle"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["route_runner_lifecycle_contract_version"],
        "trillionnium_route_runner_lifecycle_v1"
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_route_mastery_progression"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["route_mastery_contract_version"],
        "trillionnium_route_mastery_v1"
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_route_runner_reward_claim_actions"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_route_runner_next_route_actions"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_agent_party_state"],
        true
    );
    assert_eq!(
        viewport["viewport_contract"]["supports_agent_party_handoff_actions"],
        true
    );
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
        map["openstreetmap_geodata"]["contract_version"],
        "openstreetmap_geodata_v1"
    );
    assert_eq!(
        map["openstreetmap_geodata"]["provider_contract"],
        "OpenStreetMapDataProvider"
    );
    assert_eq!(
        map["openstreetmap_geodata"]["source_of_truth"],
        "rust_openstreetmap_data_provider"
    );
    assert_eq!(
        map["openstreetmap_geodata"]["web_role"],
        "visualization_input_only"
    );
    assert_eq!(
        map["openstreetmap_geodata"]["production_ingestion_plan"]["live_overpass_enabled"],
        false
    );
    let osm_features = map["openstreetmap_geodata"]["features"].as_array().unwrap();
    assert!(osm_features.len() >= 8);
    assert!(osm_features
        .iter()
        .all(|feature| feature.get("osm_id").and_then(Value::as_i64).is_some()));
    assert!(osm_features.iter().all(|feature| matches!(
        feature.get("osm_type").and_then(Value::as_str),
        Some("node" | "way" | "relation")
    )));
    assert!(osm_features.iter().all(|feature| feature
        .get("tags")
        .and_then(|tags| tags.get("trillionnium:node_id"))
        .and_then(Value::as_str)
        .is_some()));
    assert!(osm_features.iter().all(|feature| feature
        .get("game_overlay_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .starts_with("trillionnium-world-node:")));
    assert_eq!(
        map["route_preview"]["projection_layer"],
        "world_route_projection_v1"
    );
    assert_eq!(
        map["route_preview"]["index_layer"],
        "WorldIndexes::recent_route_indices_v1"
    );
    assert_eq!(
        map["route_preview"]["ranker"],
        "commercial_quality_weighted_route_ranker_v1"
    );
    assert_eq!(
        map["route_preview"]["ranker_contract_version"],
        "trillionnium_world_route_recommendation_policy_v1"
    );
    let route_preview_items = map["route_preview"]["items"].as_array().unwrap();
    assert!(route_preview_items.iter().all(|item| item
        .get("route_recommendation_score")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        > 0));
    assert!(route_preview_items.windows(2).all(|pair| pair[0]
        .get("route_recommendation_score")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        >= pair[1]
            .get("route_recommendation_score")
            .and_then(Value::as_i64)
            .unwrap_or(0)));
    assert!(route_preview_items.iter().all(|item| item
        .get("route_recommendation_reasons")
        .and_then(|reasons| reasons.get("low_dispute_risk"))
        .and_then(Value::as_i64)
        .is_some()));
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

    fn collect_slash_commands(value: &Value, commands: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    if key.ends_with("command") || key.ends_with("commands") {
                        match child {
                            Value::String(command) if command.trim_start().starts_with('/') => {
                                commands.push(command.clone());
                            }
                            Value::Array(items) => {
                                for item in items {
                                    if let Some(command) = item.as_str() {
                                        if command.trim_start().starts_with('/') {
                                            commands.push(command.to_string());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    collect_slash_commands(child, commands);
                }
            }
            Value::Array(items) => {
                for item in items {
                    collect_slash_commands(item, commands);
                }
            }
            _ => {}
        }
    }

    let mut slash_commands = Vec::new();
    collect_slash_commands(&app, &mut slash_commands);
    assert!(slash_commands.len() >= 20);
    for command in &slash_commands {
        assert!(
            !command.contains('|') && !command.contains('<') && !command.contains('>'),
            "first-session/player command should be concrete, not syntax shorthand: {command}"
        );
        if command.starts_with("/work ")
            || command.starts_with("/world action ")
            || command.starts_with("/sell ")
            || command.starts_with("/contract ")
        {
            assert_hidden_test_ready_prompt(command, &route_command_body(command));
        }
    }
}

#[test]
fn playability_coach_separates_settlement_recovery_from_reopenable_work() {
    let mut league = default_league_state();
    league.world.world_work_orders.clear();
    let mut push_work_order = |work_order_id: &str, status: &str, created_at_epoch: i64| {
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: format!("purchase-{work_order_id}"),
            listing_id: format!("listing-{work_order_id}"),
            buyer_matrix_user_id: "@alice:local.dev".to_string(),
            seller_matrix_user_id: "@merchant:local.dev".to_string(),
            company_id: "company-playability-recovery".to_string(),
            status: status.to_string(),
            brief: format!("{status} work order for playability coach"),
            value_score: 60,
            created_at_epoch,
        });
    };
    push_work_order(
        "rejected-chargeback",
        "rejected_chargeback_failed",
        1_777_231_001,
    );
    push_work_order("cancel-refund", "cancelled_refund_hold", 1_777_231_002);
    push_work_order("reopenable", "rejected_refunded", 1_777_231_003);
    push_work_order("reviewable", "delivery_review_hold", 1_777_231_004);
    push_work_order("open", "open", 1_777_231_005);

    let app = client_app_json(&league, "@alice:local.dev");
    let recovery = &app["playability_coach"]["failure_recovery"];
    assert_eq!(recovery["settlement_recovery_work_count"], 2);
    assert_eq!(recovery["reopenable_work_count"], 1);
    assert_eq!(recovery["reviewable_work_count"], 1);
    assert_eq!(recovery["open_work_count"], 1);
    let recovery_states = recovery["states"].as_array().unwrap();
    for expected_state in [
        "rejected_chargeback_failed",
        "cancelled_refund_hold",
        "cancelled_chargeback_failed",
        "rejected_refunded",
    ] {
        assert!(recovery_states.iter().any(|state| state == expected_state));
    }
    let settlement_recovery_command = recovery["settlement_recovery_command"]
        .as_str()
        .unwrap_or("");
    assert!(settlement_recovery_command.starts_with("/work reject latest"));
    assert!(!settlement_recovery_command.contains('|'));
    assert_hidden_test_ready_prompt(
        "settlement_recovery_command",
        &route_command_body(settlement_recovery_command),
    );
    let settlement_recovery_alternatives = recovery["settlement_recovery_alternative_commands"]
        .as_array()
        .unwrap();
    assert!(settlement_recovery_alternatives
        .iter()
        .any(|command| command
            .as_str()
            .unwrap_or("")
            .starts_with("/work cancel latest")));
    assert!(recovery["player_copy"]
        .as_str()
        .unwrap_or("")
        .contains("settlement retry must happen before reopen"));
}

#[test]
fn latest_reopenable_work_order_skips_unsettled_rejection_recovery() {
    let mut league = default_league_state();
    league.world.world_work_orders.clear();
    let mut push_work_order = |work_order_id: &str, status: &str, created_at_epoch: i64| {
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: format!("purchase-{work_order_id}"),
            listing_id: format!("listing-{work_order_id}"),
            buyer_matrix_user_id: "@alice:local.dev".to_string(),
            seller_matrix_user_id: "@merchant:local.dev".to_string(),
            company_id: "company-playability-recovery".to_string(),
            status: status.to_string(),
            brief: format!("{status} work order for latest reopen selection"),
            value_score: 60,
            created_at_epoch,
        });
    };
    push_work_order("settled-reopenable", "rejected_refunded", 1_777_231_001);
    push_work_order("blocked-refund", "rejected_refund_failed", 1_777_231_002);
    push_work_order(
        "blocked-chargeback",
        "rejected_chargeback_failed",
        1_777_231_003,
    );

    let indexes = build_world_indexes(&league.world);
    let selected_index = indexes
        .resolve_reopenable_work_order_index("latest", "@alice:local.dev")
        .expect("latest reopenable work should resolve to the settled rejection");
    assert_eq!(
        league.world.world_work_orders[selected_index].work_order_id,
        "settled-reopenable"
    );
}

#[test]
fn real_world_map_engine_declares_shared_renderer_adapter() {
    let league = default_league_state();
    let nodes: Vec<WorldMapNode> = league.world.world_map_nodes.values().cloned().collect();
    let engine = real_world_map_engine_json(&nodes, None);
    let geodata = openstreetmap_geodata_v1_json(&nodes, None);
    assert_eq!(
        engine["geodata_provider_contract"]["provider_contract"],
        "OpenStreetMapDataProvider"
    );
    assert_eq!(geodata["contract_version"], "openstreetmap_geodata_v1");
    assert_eq!(
        geodata["provider_id"],
        "fixture_openstreetmap_data_provider_v1"
    );
    assert_eq!(
        geodata["production_ingestion_plan"]
            ["cache_or_self_host_required_before_production_traffic"],
        true
    );
    let first_feature = geodata["features"]
        .as_array()
        .and_then(|features| features.first())
        .expect("OSM geodata fixture should project world nodes");
    assert!(first_feature["osm_id"].as_i64().unwrap_or(0) > 0);
    assert!(first_feature["lat"].as_f64().is_some());
    assert!(first_feature["lng"].as_f64().is_some());
    assert!(first_feature["game_overlay_id"]
        .as_str()
        .unwrap_or_default()
        .starts_with("trillionnium-world-node:"));
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
        engine["planned_upgrade_engine"]["readiness_contract_version"],
        "trillionnium_world_future_engine_readiness_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["future_engine_readiness"]["contract_version"],
        "trillionnium_world_future_engine_readiness_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["future_engine_readiness"]["active_engine_id"],
        "leaflet_openstreetmap_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["future_engine_readiness"]["candidate_engine_id"],
        "maplibre_gl_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["future_engine_readiness"]["rollback_plan"]
            ["candidate_is_shadow_only"],
        true
    );
    assert_eq!(
        engine["renderer_adapter"]["future_engine_readiness"]["shadow_renderer_contract"]
            ["contract_version"],
        "trillionnium_world_map_renderer_shadow_v1"
    );
    assert_eq!(
        engine["planned_upgrade_engine"]["shadow_renderer_contract_version"],
        "trillionnium_world_map_renderer_shadow_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["adapter_contract"]["supports_future_engine_swap"],
        true
    );
    assert_eq!(engine["product_name"], "Trillionnium World Map");
    assert_eq!(
        engine["gameplay_layer_contract"]["supports"]["player_avatars"],
        true
    );
    assert_eq!(
        engine["renderer_adapter"]["adapter_contract"]["supports_player_avatar_layer"],
        true
    );
    assert_eq!(
        engine["renderer_adapter"]["adapter_contract"]["supports_avatar_route_runner_layer"],
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
    assert!(adapter_methods
        .iter()
        .any(|method| method == "renderPlayerAvatar"));
    assert!(adapter_methods
        .iter()
        .any(|method| method == "renderMovingAvatar"));
    assert!(adapter_methods.iter().any(|method| method == "getCenter"));
    assert!(adapter_methods.iter().any(|method| method == "getZoom"));
    assert!(adapter_methods
        .iter()
        .any(|method| method == "onViewportChange"));
    assert!(adapter_methods.iter().any(|method| method == "focus"));
}

#[test]
fn openstreetmap_geodata_provider_uses_stable_fixture_identities() {
    let league = default_league_state();
    let nodes: Vec<WorldMapNode> = league.world.world_map_nodes.values().cloned().collect();
    let geodata = openstreetmap_geodata_v1_json(&nodes, None);

    assert_eq!(
        geodata["fixture_identity_mode"],
        "stable_fixture_table_with_deterministic_hash_fallback"
    );
    assert_eq!(
        geodata["fixture_layers_contract_version"],
        "openstreetmap_fixture_layers_v1"
    );
    assert_eq!(geodata["provider_mode"], "fixture");
    assert_eq!(
        geodata["provider_mode_contract_version"],
        "openstreetmap_provider_mode_v1"
    );
    assert_eq!(geodata["provider_mode_status"]["enabled"], true);
    assert_eq!(geodata["provider_mode_status"]["fail_closed"], false);
    assert_eq!(
        geodata["provider_readiness_contract_version"],
        "openstreetmap_provider_readiness_v1"
    );
    assert_eq!(
        geodata["provider_readiness"]["contract_version"],
        "openstreetmap_provider_readiness_v1"
    );
    assert_eq!(
        geodata["provider_readiness"]["readiness_status"],
        "fixture_ready_live_fail_closed"
    );
    assert_eq!(geodata["provider_readiness"]["fixture_mode_green"], true);
    assert_eq!(
        geodata["provider_readiness"]["live_modes_fail_closed"],
        true
    );
    assert_eq!(
        geodata["provider_readiness"]["live_network_ingestion_enabled"],
        false
    );
    assert_eq!(
        geodata["provider_readiness"]["production_ingestion_enabled"],
        false
    );
    assert_eq!(
        geodata["provider_readiness"]["stable_fixture_identity_coverage_complete"],
        true
    );
    assert_eq!(
        geodata["provider_readiness"]["overpass_bbox_cache_fail_closed"],
        true
    );
    assert_eq!(
        geodata["provider_readiness"]["geofabrik_extract_import_fail_closed"],
        true
    );
    assert_eq!(
        geodata["provider_readiness"]["vendor_tile_cache_fail_closed"],
        true
    );
    assert_eq!(
        geodata["provider_readiness"]["unknown_mode_fail_closed"],
        true
    );
    assert_eq!(
        geodata["freshness_contract_version"],
        "openstreetmap_geodata_freshness_v1"
    );
    assert_eq!(
        geodata["freshness"]["contract_version"],
        "openstreetmap_geodata_freshness_v1"
    );
    assert_eq!(
        geodata["freshness"]["freshness_status"],
        "fixture_static_fresh_live_stale_blocked"
    );
    assert_eq!(geodata["freshness"]["fixture_static_snapshot"], true);
    assert_eq!(geodata["freshness"]["wall_clock_freshness_applies"], false);
    assert_eq!(geodata["freshness"]["live_data_freshness_applies"], false);
    assert_eq!(geodata["freshness"]["fixture_snapshot_age_seconds"], 0);
    assert_eq!(
        geodata["freshness"]["fixture_snapshot_age_within_policy"],
        true
    );
    assert_eq!(geodata["freshness"]["live_ingestion_enabled"], false);
    assert_eq!(
        geodata["freshness"]["live_snapshot_age_unknown_blocked"],
        true
    );
    assert_eq!(geodata["freshness"]["staleness_alarm_active"], false);
    assert_eq!(geodata["freshness"]["stale_live_ingestion_blocked"], true);
    assert_eq!(
        geodata["freshness"]["requires_fresh_import_before_live"],
        true
    );
    assert_eq!(
        geodata["attribution_presence_contract_version"],
        "openstreetmap_attribution_presence_v1"
    );
    assert_eq!(
        geodata["attribution_presence"]["contract_version"],
        "openstreetmap_attribution_presence_v1"
    );
    assert_eq!(
        geodata["attribution_presence"]["attribution"],
        "© OpenStreetMap contributors"
    );
    assert_eq!(
        geodata["attribution_presence"]["database_license"],
        "ODbL-1.0"
    );
    assert_eq!(
        geodata["attribution_presence"]["attribution_visible_required"],
        true
    );
    assert_eq!(
        geodata["attribution_presence"]["derived_database_tracking_required"],
        true
    );
    assert_eq!(
        geodata["attribution_presence"]["odbl_database_obligations"],
        true
    );
    assert_eq!(
        geodata["attribution_presence"]["live_ingestion_blocked_until_attribution_manifest"],
        true
    );
    let attribution_presence_checks = geodata["attribution_presence"]["presence_checks"]
        .as_array()
        .unwrap();
    assert!(attribution_presence_checks
        .iter()
        .any(|check| check == "app_shell_static_attribution_node_present"));
    assert!(attribution_presence_checks
        .iter()
        .any(|check| check == "world_shell_static_attribution_node_present"));
    assert!(attribution_presence_checks
        .iter()
        .any(|check| check == "leaflet_runtime_attribution_configured"));
    assert!(attribution_presence_checks
        .iter()
        .any(|check| check == "odbl_database_obligations_visible"));
    assert_eq!(
        geodata["freshness"]["derived_database_metadata_contract_version"],
        "openstreetmap_derived_database_metadata_v1"
    );
    assert_eq!(
        geodata["derived_database_metadata_contract_version"],
        "openstreetmap_derived_database_metadata_v1"
    );
    assert_eq!(
        geodata["derived_database_metadata"]["source_of_truth"],
        "rust_openstreetmap_data_provider"
    );
    assert_eq!(
        geodata["derived_database_metadata"]["odbl"]["database_license"],
        "ODbL-1.0"
    );
    assert_eq!(
        geodata["derived_database_metadata"]["odbl"]["derived_database_tracking_required"],
        true
    );
    for mode in [
        "overpass_bbox_cache",
        "geofabrik_extract_import",
        "vendor_tile_cache",
        "unknown-live-mode",
    ] {
        let status = openstreetmap_provider_mode_status_json(mode);
        assert_eq!(status["enabled"], false);
        assert_eq!(status["fail_closed"], true);
        assert_eq!(status["network_ingestion_enabled"], false);
    }
    assert_eq!(
        geodata["fixture_layers"]["source_of_truth"],
        "rust_openstreetmap_data_provider"
    );
    assert!(
        geodata["fixture_layers"]["layer_feature_counts"]["roads"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        geodata["fixture_layers"]["layer_feature_counts"]["buildings"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        geodata["fixture_layers"]["layer_feature_counts"]["areas"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        geodata["fixture_layers"]["layer_feature_counts"]["admin_boundaries"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(geodata["semantic_role_mapping"]
        .as_array()
        .unwrap()
        .iter()
        .any(|mapping| mapping["semantic_role"] == "mentor_home"
            && mapping["game_system_role"] == "mentor_training_anchor"));
    assert_eq!(
        geodata["stable_fixture_count"].as_u64().unwrap_or_default(),
        nodes.len() as u64
    );
    let market_node = league
        .world
        .world_map_nodes
        .get("zbj-market-gate")
        .expect("market gate fixture node should exist");
    let identity = openstreetmap_fixture_identity_for(market_node);
    assert_eq!(identity.osm_type, "way");
    assert_eq!(identity.osm_id, 31_230_416_201);
    assert_eq!(identity.semantic_role, "market");
    let market_feature = geodata["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|feature| feature["game_binding"]["node_id"] == "zbj-market-gate")
        .expect("market gate should have a projected OSM fixture feature");
    assert_eq!(market_feature["stable_fixture_identity"], true);
    assert_eq!(market_feature["semantic_role"], "market");
    assert_eq!(market_feature["source_layer"], "pois");
    assert_eq!(market_feature["game_system_role"], "bounty_market_anchor");
    assert_eq!(
        market_feature["completion_owner"],
        "rust_market_command_handler"
    );
    assert_eq!(market_feature["osm_id"], 31_230_416_201_i64);
    assert_eq!(
        market_feature["tags"]["trillionnium:semantic_role"],
        "market"
    );
    assert!(market_feature["objective_seed"]
        .as_str()
        .unwrap_or_default()
        .contains("way:31230416201"));
}

#[test]
fn world_tactics_projection_binds_trillionnium_state_to_osm_objectives() {
    let league = default_league_state();
    let nodes: Vec<WorldMapNode> = league.world.world_map_nodes.values().cloned().collect();
    let current_node = league.world.world_map_nodes.get(default_world_node_id());
    let geodata = openstreetmap_geodata_v1_json(&nodes, current_node);
    let tactics = world_tactics_board_projection_json(
        &league.world,
        "@alice:local.dev",
        current_node,
        &geodata,
    );
    let trillionnium =
        world_trillionnium_character_projection_json(&league.world, "@alice:local.dev");

    assert_eq!(
        tactics["contract_version"],
        "trillionnium_world_tactics_board_v1"
    );
    assert_eq!(tactics["source_of_truth"], "rust_trillionnium_game_state");
    assert_eq!(tactics["web_role"], "visualization_input_only");
    assert_eq!(
        tactics["unit_contract_version"],
        "trillionnium_world_tactics_unit_v1"
    );
    assert_eq!(
        tactics["command_contract_version"],
        "trillionnium_world_tactics_command_v1"
    );
    assert_eq!(
        tactics["trillionnium_skill_contract_version"],
        "trillionnium_skill_v1"
    );
    assert_eq!(
        tactics["command_outcome_contract_version"],
        "trillionnium_world_tactics_command_outcome_v1"
    );
    assert_eq!(
        tactics["trillionnium_training_contract_version"],
        "trillionnium_training_command_v1"
    );
    assert_eq!(
        tactics["trillionnium_sect_contract_version"],
        "trillionnium_sect_v1"
    );
    assert_eq!(
        tactics["trillionnium_npc_contract_version"],
        "trillionnium_npc_v1"
    );
    assert_eq!(
        tactics["trillionnium_sect_osm_binding_contract_version"],
        "trillionnium_sect_osm_binding_v1"
    );
    assert_eq!(
        tactics["trillionnium_npc_spawn_contract_version"],
        "trillionnium_npc_spawn_anchor_v1"
    );
    assert_eq!(
        tactics["trillionnium_npc_command_descriptor_contract_version"],
        "trillionnium_npc_command_descriptor_v1"
    );
    assert_eq!(
        tactics["mentor_training_task_contract_version"],
        "trillionnium_mentor_training_task_v1"
    );
    assert_eq!(
        tactics["trillionnium_task_archetype_contract_version"],
        "trillionnium_task_archetype_v1"
    );
    assert_eq!(
        tactics["trillionnium_task_completion_contract_version"],
        "trillionnium_task_completion_v1"
    );
    assert_eq!(
        tactics["trillionnium_reward_gate_contract_version"],
        "trillionnium_reward_gate_v1"
    );
    assert_eq!(
        tactics["trillionnium_battle_log_style_contract_version"],
        "trillionnium_battle_log_style_v1"
    );
    assert_eq!(
        tactics["trillionnium_combat_log_contract_version"],
        "trillionnium_combat_log_v1"
    );
    assert_eq!(
        tactics["trillionnium_npc_relationship_contract_version"],
        "trillionnium_npc_relationship_v1"
    );
    assert_eq!(
        tactics["trillionnium_osm_objective_contract_version"],
        "trillionnium_osm_objective_v1"
    );
    assert_eq!(
        tactics["tactics_combat_resolution_contract_version"],
        "trillionnium_tactics_combat_resolution_v1"
    );
    assert_eq!(
        tactics["tactics_game_session_contract_version"],
        "trillionnium_tactics_game_session_v1"
    );
    assert_eq!(
        tactics["tactics_simulation_tick_contract_version"],
        "trillionnium_tactics_simulation_tick_v1"
    );
    assert_eq!(
        tactics["tactics_reward_settlement_contract_version"],
        "trillionnium_tactics_reward_settlement_v1"
    );
    assert_eq!(
        tactics["tactics_board_cell_interaction_contract_version"],
        "trillionnium_tactics_board_cell_interaction_v1"
    );
    assert_eq!(
        tactics["tactics_unit_selection_contract_version"],
        "trillionnium_tactics_unit_selection_v1"
    );
    assert_eq!(
        tactics["tactics_command_intent_draft_contract_version"],
        "trillionnium_tactics_command_intent_draft_v1"
    );
    assert_eq!(
        tactics["tactics_accessibility_contract_version"],
        "trillionnium_tactics_accessibility_v1"
    );
    assert_eq!(
        tactics["intent_draft_policy"]["validation_owner"],
        "rust_tactics_command_validator"
    );
    assert_eq!(
        tactics["intent_draft_policy"]["command_handler_owner"],
        "rust_world_tactics_command_handler"
    );
    assert_eq!(
        tactics["intent_draft_policy"]["web_role"],
        "intent_only_visualization_input"
    );
    assert_eq!(
        tactics["accessibility_policy"]["contract_version"],
        "trillionnium_tactics_accessibility_v1"
    );
    assert_eq!(
        tactics["accessibility_policy"]["keyboard_traversal"],
        "roving_grid_focus"
    );
    assert_eq!(
        tactics["accessibility_policy"]["low_motion_support"],
        "prefers_reduced_motion"
    );
    assert_eq!(
        tactics["map_overlay_identity_contract_version"],
        "trillionnium_map_overlay_identity_v1"
    );
    assert_eq!(
        tactics["open_source_base"]["repo"],
        "tranchikhang/MedievalWar"
    );
    assert_eq!(tactics["board"]["cells"].as_array().unwrap().len(), 64);
    assert_eq!(
        tactics["trillionnium_character"]["contract_version"],
        "trillionnium_character_v1"
    );
    assert_eq!(
        tactics["units"][0]["contract_version"],
        "trillionnium_world_tactics_unit_v1"
    );
    assert_eq!(
        tactics["units"][0]["source_of_truth"],
        "rust_tactics_unit_model"
    );
    assert_eq!(
        tactics["units"][0]["unit_selection_contract_version"],
        "trillionnium_tactics_unit_selection_v1"
    );
    assert_eq!(
        tactics["units"][0]["command_intent_draft_contract_version"],
        "trillionnium_tactics_command_intent_draft_v1"
    );
    assert_eq!(
        tactics["units"][0]["accessibility_contract_version"],
        "trillionnium_tactics_accessibility_v1"
    );
    assert_eq!(tactics["units"][0]["draft_input_name"], "unit_id");
    assert_eq!(tactics["units"][0]["selection_role"], "active_unit");
    assert_eq!(
        tactics["units"][0]["keyboard_focus_role"],
        "active_unit_button"
    );
    assert_eq!(tactics["units"][0]["owner"], "player");
    assert!(tactics["units"][0]["max_hp"].as_i64().unwrap_or_default() >= 100);
    assert_eq!(
        tactics["available_commands"][2]["contract_version"],
        "trillionnium_world_tactics_command_v1"
    );
    assert_eq!(tactics["available_commands"][2]["command"], "attack");
    assert_eq!(
        tactics["available_commands"][2]["validation_owner"],
        "rust_tactics_combat_handler"
    );
    assert_eq!(
        tactics["available_commands"][2]["command_intent_draft_contract_version"],
        "trillionnium_tactics_command_intent_draft_v1"
    );
    assert_eq!(
        tactics["available_commands"][2]["accessibility_contract_version"],
        "trillionnium_tactics_accessibility_v1"
    );
    assert_eq!(
        tactics["available_commands"][2]["draft_owner"],
        "browser_tactics_intent_builder"
    );
    assert_eq!(
        tactics["available_commands"][2]["target_tile_required"],
        true
    );
    assert_eq!(
        tactics["board"]["cells"][0]["board_cell_interaction_contract_version"],
        "trillionnium_tactics_board_cell_interaction_v1"
    );
    assert_eq!(
        tactics["board"]["cells"][0]["command_intent_draft_contract_version"],
        "trillionnium_tactics_command_intent_draft_v1"
    );
    assert_eq!(
        tactics["board"]["cells"][0]["accessibility_contract_version"],
        "trillionnium_tactics_accessibility_v1"
    );
    assert_eq!(
        tactics["board"]["cells"][0]["draft_input_name"],
        "target_tile"
    );
    assert_eq!(
        tactics["board"]["cells"][0]["keyboard_focus_role"],
        "target_tile_gridcell"
    );
    assert_eq!(
        tactics["board"]["cells"][0]["validation_owner"],
        "rust_tactics_command_validator"
    );
    assert!(tactics["available_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["command"] == "end_turn"));
    assert!(tactics["available_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["command"] == "train_skill"
            && command["validation_owner"] == "rust_mentor_training_validator"));
    assert!(tactics["available_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["command"] == "talk_npc"
            && command["validation_owner"] == "rust_trillionnium_npc_interaction_validator"));
    assert!(tactics["available_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["command"] == "offer_task"
            && command["validation_owner"] == "rust_trillionnium_task_offer_validator"));
    assert!(tactics["available_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["command"] == "complete_task"
            && command["validation_owner"] == "rust_trillionnium_task_completion_handler"
            && command["required_skill_id"] == "reading_and_contracts"));
    assert_eq!(
        tactics["turn_state"]["source_of_truth"],
        "rust_tactics_turn_handler"
    );
    assert!(tactics["skill_definitions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["skill_id"] == "reading_and_contracts"
            && skill["training_anchor_role"] == "ledger_hall"));
    assert_eq!(
        trillionnium["mechanics_reference_layer"],
        "gmud_rmxp_hero_yxts_llm_reference_only"
    );
    assert_eq!(trillionnium["attributes"]["physique"], 12);
    assert_eq!(trillionnium["attributes"]["force"], 11);
    assert_eq!(trillionnium["attributes"]["agility"], 12);
    assert_eq!(trillionnium["attributes"]["insight"], 13);
    assert!(
        trillionnium["attributes"]["derived_stats"]["max_hp"]
            .as_i64()
            .unwrap_or_default()
            >= 100
    );
    assert_eq!(
        trillionnium["skill_definition_contract"],
        "trillionnium_skill_v1"
    );
    assert!(trillionnium["known_skills"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["skill_id"] == "basic_inner_power"));
    assert_eq!(
        tactics["osm_objective_source"]["provider_contract"],
        "OpenStreetMapDataProvider"
    );
    assert_eq!(
        tactics["osm_objective_source"]["rust_command_handler_decides_completion"],
        true
    );
    let objective_overlay = tactics["objectives"][0]["osm_game_overlay_id"]
        .as_str()
        .unwrap_or_default();
    assert!(objective_overlay.starts_with("trillionnium-world-node:"));
    assert!(tactics["available_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["command"] == "move_unit"));
    assert!(tactics["training_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["skill_id"] == "basic_unarmed"
            && command["required_semantic_role"] == "civic_square"
            && command["validation_owner"] == "rust_mentor_training_validator"
            && command["required_osm_game_overlay_id"]
                .as_str()
                .unwrap_or_default()
                .starts_with("trillionnium-world-node:")));
    assert!(tactics["sects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|sect| sect["sect_id"] == "cloud-ledger-hall"
            && sect["contract_version"] == "trillionnium_sect_v1"
            && sect["osm_anchor_binding"]["contract_version"]
                == "trillionnium_sect_osm_binding_v1"
            && sect["osm_anchor_binding"]["source_of_truth"]
                == "rust_openstreetmap_data_provider"
            && sect["title_ladder"].as_array().unwrap().len() >= 3));
    assert!(tactics["npcs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|npc| npc["npc_id"] == "npc-street-compass-sifu"
            && npc["contract_version"] == "trillionnium_npc_v1"
            && npc["spawn_anchor"]["contract_version"] == "trillionnium_npc_spawn_anchor_v1"
            && npc["command_descriptors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|descriptor| descriptor["command"] == "train_skill"
                    && descriptor["contract_version"]
                        == "trillionnium_npc_command_descriptor_v1")));
    assert!(tactics["npc_spawn_anchors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|anchor| anchor["binding_kind"] == "npc_spawn"
            && anchor["osm_game_overlay_id"]
                .as_str()
                .unwrap_or_default()
                .starts_with("trillionnium-world-node:")));
    assert!(tactics["npc_command_descriptors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|descriptor| descriptor["command"] == "offer_task"
            && descriptor["validation_owner"] == "rust_trillionnium_task_offer_validator"));
    assert!(tactics["mentor_training_task_flows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|flow| flow["skill_id"] == "basic_unarmed"
            && flow["contract_version"] == "trillionnium_mentor_training_task_v1"
            && flow["steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|step| step == "rust_validate_skill_mentor_place_cost_cooldown")));
    assert!(tactics["task_archetypes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["task_archetype_id"] == "sect_training_trial"
            && task["contract_version"] == "trillionnium_task_archetype_v1"));
    assert!(tactics["task_archetypes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["task_archetype_id"] == "witness_archive_case"
            && task["source_semantic_roles"]
                .as_array()
                .unwrap()
                .iter()
                .any(|role| role == "archive")
            && task["content_policy"]
                == "trillionnium_native_no_copied_hero_tan_text_assets_or_tables"));
    assert!(tactics["task_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |candidate| candidate["task_archetype_id"] == "market_settlement"
                && candidate["completion_command"] == "complete_task"
                && candidate["completion_contract_version"] == "trillionnium_task_completion_v1"
                && candidate["reward_gate_contract_version"] == "trillionnium_reward_gate_v1"
                && candidate["ledger_reward_requires_settlement"] == true
                && candidate["review_hold_gate_enforced"] == true
                && candidate["anti_cheese_gate_enforced"] == true
        ));
    assert!(tactics["task_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |candidate| candidate["task_archetype_id"] == "cistern_ration_run"
                && candidate["source_semantic_role"] == "water_supply"
        ));
    assert_eq!(
        tactics["authored_quest_chain_contract_version"],
        "trillionnium_world_authored_quest_chain_v1"
    );
    let authored_quest_chains = &tactics["authored_quest_chains"];
    assert_eq!(
        authored_quest_chains["contract_version"],
        "trillionnium_world_authored_quest_chain_v1"
    );
    assert_eq!(
        authored_quest_chains["forbidden_intermediate"],
        "no_full_hero_tan_replica_then_replace_workflow"
    );
    assert!(
        authored_quest_chains["chain_count"]
            .as_u64()
            .unwrap_or_default()
            >= 6
    );
    assert!(
        authored_quest_chains["total_step_count"]
            .as_u64()
            .unwrap_or_default()
            >= 18
    );
    assert!(
        authored_quest_chains["covered_node_count"]
            .as_u64()
            .unwrap_or_default()
            >= 16
    );
    assert!(authored_quest_chains["chains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|chain| chain["chain_id"] == "cistern_ration_relief"
            && chain["survival_pressure"] == "food_water_decay_visible"
            && chain["task_archetype_ids"]
                .as_array()
                .unwrap()
                .iter()
                .any(|task| task == "field_infirmary_round")));
    assert!(tactics["osm_objectives"].as_array().unwrap().len() >= 5);
    assert!(tactics["osm_objectives"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |objective| objective["contract_version"] == "trillionnium_osm_objective_v1"
                && objective["source_of_truth"] == "rust_trillionnium_osm_objective_generator"
                && objective["osm_can_suggest_objectives"] == true
                && objective["rust_command_handler_decides_completion"] == true
                && objective["objective_seed"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("trillionnium-objective-seed-")
        ));
    assert!(tactics["osm_objectives"]
        .as_array()
        .unwrap()
        .iter()
        .any(|objective| objective["source_semantic_role"] == "archive"
            && objective["task_archetype_id"] == "witness_archive_case"
            && objective["completion_owner"] == "rust_trillionnium_task_completion_handler"));
    assert_eq!(tactics["objectives"], tactics["osm_objectives"]);
    assert_eq!(
        tactics["world_objective_travel_contract_version"],
        "trillionnium_world_objective_travel_v1"
    );
    let objective_travel = &tactics["world_objective_travel"];
    assert_eq!(
        objective_travel["contract_version"],
        "trillionnium_world_objective_travel_v1"
    );
    assert_eq!(
        objective_travel["source_of_truth"],
        "rust_world_graph_objective_travel"
    );
    assert_eq!(
        objective_travel["movement_source_of_truth"],
        "rust_world_map_move"
    );
    assert_eq!(
        objective_travel["transition_source_of_truth"],
        "rust_world_map_transition_rules"
    );
    assert_eq!(
        objective_travel["graph_owner"],
        "world_state.world_map_nodes.exits"
    );
    assert_eq!(
        objective_travel["active_route"]["contract_version"],
        "trillionnium_world_objective_travel_v1"
    );
    assert_eq!(
        objective_travel["active_route"]["source_of_truth"],
        "rust_world_graph_objective_travel"
    );
    assert_eq!(
        objective_travel["active_route"]["current_node_id"],
        default_world_node_id()
    );
    assert!(objective_travel["active_route"]["target_node_id"]
        .as_str()
        .is_some_and(|target| !target.is_empty()));
    let travel_path = objective_travel["active_route"]["path_node_ids"]
        .as_array()
        .unwrap();
    assert!(travel_path.len() >= 2);
    assert_eq!(travel_path.first().unwrap(), default_world_node_id());
    assert_eq!(
        travel_path.last().unwrap(),
        &objective_travel["active_route"]["target_node_id"]
    );
    assert!(objective_travel["active_route"]["next_step_direction"]
        .as_str()
        .is_some_and(|direction| direction != "wait"));
    assert!(objective_travel["party_members"]
        .as_array()
        .unwrap()
        .iter()
        .any(|member| member["member_id"] == "lord"
            && member["source_of_truth"] == "rust_world_player_positions"));
    assert!(objective_travel["route_tracks"].as_array().unwrap().len() >= 2);
    assert!(tactics["map_overlay_identity_index"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |identity| identity["contract_version"] == "trillionnium_map_overlay_identity_v1"
                && identity["game_overlay_id"] == objective_overlay
                && identity["source_of_truth"] == "rust_openstreetmap_data_provider"
        ));
    assert_eq!(
        tactics["osm_objective_source"]["map_overlay_identity_contract_version"],
        "trillionnium_map_overlay_identity_v1"
    );
    assert!(
        tactics["osm_objective_source"]["map_overlay_identity_count"]
            .as_i64()
            .unwrap_or_default()
            >= tactics["osm_objective_source"]["objective_count"]
                .as_i64()
                .unwrap_or_default()
    );
    assert_eq!(
        tactics["game_session"]["contract_version"],
        "trillionnium_tactics_game_session_v1"
    );
    assert_eq!(
        tactics["game_session"]["objective_id"],
        "defeat_market_bandit"
    );
    assert_eq!(tactics["game_session"]["objective_progress"], 0);
    assert_eq!(tactics["game_session"]["objective_goal"], 1);
    assert_eq!(tactics["game_session"]["victory_state"], "active");
    assert_eq!(tactics["game_session"]["reward_status"], "not_eligible");
    assert_eq!(
        tactics["game_session"]["persistence_status"],
        "projected_default_until_first_command"
    );
    assert_eq!(
        tactics["simulation_tick_source"]["tick_contract_version"],
        "trillionnium_tactics_simulation_tick_v1"
    );
    assert_eq!(
        tactics["simulation_tick_source"]["persistence_owner"],
        "world_state.world_tactics_simulation_ticks"
    );
    let tactics_again = world_tactics_board_projection_json(
        &league.world,
        "@alice:local.dev",
        current_node,
        &geodata,
    );
    assert_eq!(tactics["osm_objectives"], tactics_again["osm_objectives"]);
    assert_eq!(
        tactics["battle_log_style"]["contract_version"],
        "trillionnium_battle_log_style_v1"
    );
    assert_eq!(
        tactics["combat_log"]["contract_version"],
        "trillionnium_combat_log_v1"
    );
    assert_eq!(
        tactics["combat_log"]["template_pack"],
        "trillionnium_native_combat_task_templates_v1"
    );
    assert_eq!(
        tactics["combat_log"]["source_reference_safety"]["test_gate"],
        "forbid_source_reference_strings_in_generated_beats"
    );
    assert_eq!(
        tactics["full_content_alignment_contract_version"],
        "trillionnium_hero_tan_full_content_alignment_v1"
    );
    let full_content_alignment = &tactics["full_content_alignment"];
    assert_eq!(
        full_content_alignment["contract_version"],
        "trillionnium_hero_tan_full_content_alignment_v1"
    );
    assert_eq!(
        full_content_alignment["source_of_truth"],
        "rust_trillionnium_full_content_volume_alignment_gate"
    );
    assert_eq!(
        full_content_alignment["reference_policy"]["implementation_rule"],
        "trillionnium_native_content_only"
    );
    assert_eq!(
        full_content_alignment["reference_policy"]["copy_policy"],
        "no_copied_hero_tan_text_assets_code_tables_or_data"
    );
    assert_eq!(
        full_content_alignment["thresholds_green"], true,
        "full content coverage counts: {}",
        full_content_alignment["coverage_counts"]
    );
    assert_eq!(
        full_content_alignment["status"],
        "content_volume_catalog_gate_green"
    );
    let coverage_counts = &full_content_alignment["coverage_counts"];
    assert!(
        coverage_counts["skill_definitions"]
            .as_u64()
            .unwrap_or_default()
            >= 18
    );
    assert!(
        coverage_counts["skill_families"]
            .as_u64()
            .unwrap_or_default()
            >= 14
    );
    assert!(
        coverage_counts["training_commands"]
            .as_u64()
            .unwrap_or_default()
            >= 18
    );
    assert!(coverage_counts["sects"].as_u64().unwrap_or_default() >= 8);
    assert!(coverage_counts["npcs"].as_u64().unwrap_or_default() >= 18);
    assert!(
        coverage_counts["npc_command_descriptors"]
            .as_u64()
            .unwrap_or_default()
            >= 28
    );
    assert!(
        coverage_counts["task_archetypes"]
            .as_u64()
            .unwrap_or_default()
            >= 12
    );
    assert!(
        coverage_counts["world_map_nodes"]
            .as_u64()
            .unwrap_or_default()
            >= 24
    );
    assert_eq!(
        full_content_alignment["minimum_thresholds"]["world_map_nodes"],
        24
    );
    assert_eq!(
        full_content_alignment["clean_room_content_scale"]["contract_version"],
        "trillionnium_clean_room_content_scale_v1"
    );
    assert_eq!(
        full_content_alignment["clean_room_content_scale"]["status"],
        "clean_room_scale_scaffold_green"
    );
    assert_eq!(
        full_content_alignment["clean_room_content_scale"]["forbidden_intermediate"],
        "no_full_hero_tan_replica_then_replace_workflow"
    );
    assert_eq!(
        full_content_alignment["clean_room_content_scale"]["copy_policy"],
        "no_copied_hero_tan_text_assets_code_tables_or_data"
    );
    assert!(
        coverage_counts["item_equipment_catalog"]
            .as_u64()
            .unwrap_or_default()
            >= 12
    );
    assert!(
        coverage_counts["resource_pressure_loops"]
            .as_u64()
            .unwrap_or_default()
            >= 6
    );
    assert!(
        coverage_counts["resource_pressure_runtime_tracked_domains"]
            .as_u64()
            .unwrap_or_default()
            >= 4
    );
    assert!(
        coverage_counts["resource_pressure_runtime_mutation_sources"]
            .as_u64()
            .unwrap_or_default()
            >= 3
    );
    assert_eq!(
        coverage_counts["resource_pressure_runtime_contract_green"],
        true
    );
    assert_eq!(
        coverage_counts["food_water_age_survival_runtime_contract_green"],
        true
    );
    assert!(
        coverage_counts["food_water_age_survival_runtime_tracked_domains"]
            .as_u64()
            .unwrap_or_default()
            >= 5
    );
    assert_eq!(
        coverage_counts["dynamic_social_simulation_contract_green"],
        true
    );
    assert!(
        coverage_counts["dynamic_social_simulation_tracked_domains"]
            .as_u64()
            .unwrap_or_default()
            >= 5
    );
    assert!(
        coverage_counts["dynamic_social_simulation_factions"]
            .as_u64()
            .unwrap_or_default()
            >= 8
    );
    assert_eq!(coverage_counts["authored_quest_chain_contract_green"], true);
    assert!(
        coverage_counts["authored_quest_chains"]
            .as_u64()
            .unwrap_or_default()
            >= 6
    );
    assert!(
        coverage_counts["authored_quest_chain_steps"]
            .as_u64()
            .unwrap_or_default()
            >= 18
    );
    assert!(
        coverage_counts["authored_quest_chain_node_coverage"]
            .as_u64()
            .unwrap_or_default()
            >= 16
    );
    assert_eq!(
        coverage_counts["combat_numerics_runtime_contract_green"],
        true
    );
    assert!(
        coverage_counts["combat_numerics_runtime_tracked_domains"]
            .as_u64()
            .unwrap_or_default()
            >= 8
    );
    assert!(
        coverage_counts["combat_numerics_runtime_mutation_sources"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert!(coverage_counts["story_arcs"].as_u64().unwrap_or_default() >= 6);
    assert_eq!(coverage_counts["region_story_runtime_contract_green"], true);
    assert!(
        coverage_counts["region_story_unlocked_regions"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert!(
        coverage_counts["region_story_unlocked_arcs"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert!(
        coverage_counts["region_story_runtime_mutation_sources"]
            .as_u64()
            .unwrap_or_default()
            >= 3
    );
    assert!(full_content_alignment["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|domain| domain["domain"] == "items_and_equipment"
            && domain["status"] == "rust_runtime_backed"));
    assert!(full_content_alignment["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |domain| domain["domain"] == "survival_time_resource_pressure"
                && domain["status"] == "rust_runtime_backed"
                && domain["gate_field"] == "resource_pressure_runtime"
        ));
    assert!(full_content_alignment["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|domain| domain["domain"] == "food_water_age_survival"
            && domain["status"] == "rust_runtime_backed"
            && domain["gate_field"] == "survival_runtime"));
    assert!(full_content_alignment["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|domain| domain["domain"] == "npc_social_relationships"
            && domain["status"] == "rust_runtime_backed"
            && domain["gate_field"] == "dynamic_social_simulation"));
    assert!(full_content_alignment["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|domain| domain["domain"] == "authored_quest_chains"
            && domain["status"] == "native_catalog_expanded"
            && domain["gate_field"] == "authored_quest_chains"));
    assert_eq!(
        tactics["item_equipment_runtime_contract_version"],
        "trillionnium_world_item_equipment_runtime_v1"
    );
    assert_eq!(
        tactics["item_equipment_runtime"]["source_of_truth"],
        "rust_trillionnium_item_equipment_runtime_state"
    );
    assert!(
        tactics["item_equipment_runtime"]["inventory_count"]
            .as_u64()
            .unwrap_or_default()
            >= 3
    );
    assert!(tactics["item_equipment_runtime"]["equipment_slots"]
        .as_object()
        .is_some_and(|slots| slots.len() >= 3));
    assert!(full_content_alignment["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|domain| domain["domain"] == "combat_entry_and_return"
            && domain["status"] == "rust_runtime_backed"));
    assert!(full_content_alignment["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|domain| domain["domain"] == "combat_numerics"
            && domain["status"] == "rust_runtime_backed"
            && domain["gate_field"] == "combat_numerics_runtime"));
    assert_eq!(
        tactics["trillionnium_combat_numerics_runtime_contract_version"],
        "trillionnium_world_combat_numerics_runtime_v1"
    );
    assert_eq!(
        tactics["combat_numerics_runtime"]["contract_version"],
        "trillionnium_world_combat_numerics_runtime_v1"
    );
    assert_eq!(
        tactics["combat_numerics_runtime"]["source_of_truth"],
        "rust_trillionnium_combat_numerics_runtime_state"
    );
    assert_eq!(
        tactics["combat_numerics_runtime"]["mutation_sources"],
        json!(["tactics_attack"])
    );
    assert_eq!(
        tactics["combat_numerics_runtime"]["health"]["status"],
        "combat_ready"
    );
    assert_eq!(
        tactics["item_equipment_catalog"]["contract_version"],
        "trillionnium_native_item_equipment_catalog_v1"
    );
    assert!(tactics["item_equipment_catalog"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["item_id"] == "raid-signal-drum" && item["family"] == "raid_command"));
    assert_eq!(
        tactics["resource_pressure_loops"]["contract_version"],
        "trillionnium_native_resource_pressure_loop_v1"
    );
    assert!(tactics["resource_pressure_loops"]["loops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|loop_def| loop_def["loop_id"] == "evidence_integrity"
            && loop_def["failure_mode"] == "review_hold_or_reward_delay"));
    assert_eq!(
        tactics["resource_pressure_loops"]["survival_runtime_contract_version"],
        "trillionnium_world_food_water_age_survival_v1"
    );
    for loop_id in ["food_supply", "water_supply", "age_pressure"] {
        assert!(
            tactics["resource_pressure_loops"]["loops"]
                .as_array()
                .unwrap()
                .iter()
                .any(|loop_def| loop_def["loop_id"] == loop_id),
            "missing survival resource loop {loop_id}"
        );
    }
    assert_eq!(
        tactics["trillionnium_resource_pressure_runtime_contract_version"],
        "trillionnium_world_resource_pressure_runtime_v1"
    );
    assert_eq!(
        tactics["resource_pressure_runtime"]["contract_version"],
        "trillionnium_world_resource_pressure_runtime_v1"
    );
    assert_eq!(
        tactics["resource_pressure_runtime"]["source_of_truth"],
        "rust_trillionnium_resource_pressure_runtime_state"
    );
    assert_eq!(
        tactics["resource_pressure_runtime"]["mutation_sources"],
        json!(["world_map_move", "tactics_attack", "tactics_complete_task"])
    );
    assert_eq!(
        tactics["resource_pressure_runtime"]["time"]["clock_label"],
        "08:00"
    );
    assert_eq!(
        tactics["resource_pressure_runtime"]["stamina"]["status"],
        "route_ready"
    );
    assert_eq!(
        tactics["resource_pressure_runtime"]["evidence_integrity"]["status"],
        "draft_evidence_bundle"
    );
    assert_eq!(
        tactics["food_water_age_survival_runtime_contract_version"],
        "trillionnium_world_food_water_age_survival_v1"
    );
    assert_eq!(
        tactics["survival_runtime"]["contract_version"],
        "trillionnium_world_food_water_age_survival_v1"
    );
    assert_eq!(
        tactics["survival_runtime"]["source_of_truth"],
        "rust_trillionnium_food_water_age_survival_state"
    );
    assert_eq!(tactics["survival_runtime"]["food"]["status"], "fed");
    assert_eq!(tactics["survival_runtime"]["water"]["status"], "hydrated");
    assert_eq!(tactics["survival_runtime"]["age"]["stage"], "young_adult");
    assert_eq!(
        tactics["dynamic_social_simulation_contract_version"],
        "trillionnium_world_dynamic_social_simulation_v1"
    );
    assert_eq!(
        tactics["dynamic_social_simulation"]["contract_version"],
        "trillionnium_world_dynamic_social_simulation_v1"
    );
    assert_eq!(
        tactics["dynamic_social_simulation"]["source_of_truth"],
        "rust_world_relationships_dynamic_social_state"
    );
    assert!(
        tactics["dynamic_social_simulation"]["active_npc_count"]
            .as_u64()
            .unwrap_or_default()
            >= 18
    );
    assert!(
        tactics["dynamic_social_simulation"]["faction_standings"]
            .as_array()
            .unwrap()
            .len()
            >= 8
    );
    assert_eq!(
        tactics["story_arc_catalog"]["contract_version"],
        "trillionnium_native_story_arc_catalog_v1"
    );
    assert_eq!(
        tactics["trillionnium_region_story_unlock_runtime_contract_version"],
        "trillionnium_world_region_story_unlock_runtime_v1"
    );
    assert_eq!(
        tactics["region_story_unlock_runtime"]["contract_version"],
        "trillionnium_world_region_story_unlock_runtime_v1"
    );
    assert_eq!(
        tactics["region_story_unlock_runtime"]["source_of_truth"],
        "rust_trillionnium_region_story_unlock_runtime_state"
    );
    assert_eq!(
        tactics["region_story_unlock_runtime"]["mutation_sources"],
        json!(["world_map_move", "tactics_attack", "tactics_complete_task"])
    );
    assert!(
        tactics["region_story_unlock_runtime"]["unlocked_region_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|region| region == "reality-mirror-city")
    );
    assert!(full_content_alignment["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|domain| domain["domain"] == "story_arcs"
            && domain["status"] == "rust_runtime_backed"
            && domain["gate_field"] == "region_story_unlock_runtime"));
    assert!(tactics["story_arc_catalog"]["arcs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arc| arc["arc_id"] == "jade_route_patrol"
            && arc["entry_task_archetypes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry == "map_survey")));
    let combat_log_text = tactics["combat_log"]["beats"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|beat| beat["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(combat_log_text.contains("镜城风从巷口压低"));
    for forbidden in [
        "gmud",
        "RMXP-Hero",
        "yxts-llm",
        "Hero Tan",
        "tranchikhang/MedievalWar",
        "Phaser 3",
    ] {
        assert!(
            !combat_log_text.contains(forbidden),
            "generated combat log copied forbidden source reference string: {forbidden}"
        );
    }
    assert!(tactics["battle_log"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |entry| entry["style_contract"] == "trillionnium_battle_log_style_v1"
                && entry["source_of_truth"] == "rust_trillionnium_combat_log_generator"
        ));
    let app = client_app_json(&league, "@alice:local.dev");
    assert_eq!(
        app["trillionnium_combat_log"]["contract_version"],
        "trillionnium_combat_log_v1"
    );
    assert_eq!(
        app["map"]["tactics_board"]["combat_log"]["app_projection"]["json_field"],
        "trillionnium_combat_log"
    );
    assert_eq!(
        tactics["npc_relationship_model"]["source_of_truth"],
        "rust_world_relationships_persistent_state"
    );
    assert_eq!(
        tactics["npc_relationship_model"]["contract_version"],
        "trillionnium_npc_relationship_v1"
    );
}

#[tokio::test]
async fn world_tactics_command_endpoint_validates_training_place_and_mutates_character() {
    let state = AppState::new(test_config());
    let app = build_router(state.clone());

    let (blocked_status, blocked) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "train_skill",
            "unit_id": "lord",
            "target_tile": "G8",
            "skill_id": "basic_unarmed",
            "osm_game_overlay_id": "trillionnium-world-node:starter-studio",
            "body": "try training at the wrong OSM place"
        }),
    )
    .await;
    assert_eq!(blocked_status, StatusCode::OK);
    assert_eq!(blocked["outcome"]["accepted"], false);
    assert_eq!(blocked["outcome"]["result"], "training_place_mismatch");
    assert_eq!(
        blocked["outcome"]["source_of_truth"],
        "rust_mentor_training_validator"
    );

    let (wrong_mentor_status, wrong_mentor) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "train_skill",
            "unit_id": "lord",
            "target_tile": "G8",
            "skill_id": "basic_unarmed",
            "npc_id": "npc-cloud-ledger-mentor",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "try training with the wrong mentor"
        }),
    )
    .await;
    assert_eq!(wrong_mentor_status, StatusCode::OK);
    assert_eq!(wrong_mentor["outcome"]["accepted"], false);
    assert_eq!(wrong_mentor["outcome"]["result"], "mentor_mismatch");
    assert_eq!(
        wrong_mentor["outcome"]["world_skill_practice_loop_contract_version"],
        "trillionnium_world_skill_practice_loop_v1"
    );
    assert_eq!(
        wrong_mentor["outcome"]["source_of_truth"],
        "rust_mentor_training_validator"
    );

    let (trained_status, trained) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "train_skill",
            "unit_id": "lord",
            "target_tile": "G8",
            "skill_id": "basic_unarmed",
            "npc_id": "npc-street-compass-sifu",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "mentor training with the Street Compass Sifu"
        }),
    )
    .await;
    assert_eq!(trained_status, StatusCode::OK);
    assert_eq!(trained["kind"], "trillionnium_world_tactics_command");
    assert_eq!(trained["outcome"]["accepted"], true);
    assert_eq!(trained["outcome"]["result"], "skill_trained");
    assert_eq!(trained["outcome"]["skill_id"], "basic_unarmed");
    assert_eq!(
        trained["outcome"]["mentor_npc_id"],
        "npc-street-compass-sifu"
    );
    assert_eq!(trained["outcome"]["required_semantic_role"], "civic_square");
    assert_eq!(
        trained["outcome"]["world_skill_practice_loop_contract_version"],
        "trillionnium_world_skill_practice_loop_v1"
    );
    assert_eq!(
        trained["outcome"]["source_of_truth"],
        "rust_mentor_training_validator"
    );

    let (wrong_slot_status, wrong_slot) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "equip_item",
            "unit_id": "lord",
            "item_id": "route-guard-staff",
            "target_slot": "pack",
            "body": "try equipping a staff into the wrong slot"
        }),
    )
    .await;
    assert_eq!(wrong_slot_status, StatusCode::OK);
    assert_eq!(wrong_slot["outcome"]["accepted"], false);
    assert_eq!(wrong_slot["outcome"]["result"], "equipment_slot_mismatch");
    assert_eq!(wrong_slot["outcome"]["expected_slot"], "weapon");
    assert_eq!(
        wrong_slot["outcome"]["item_equipment_runtime_contract_version"],
        "trillionnium_world_item_equipment_runtime_v1"
    );
    assert_eq!(
        wrong_slot["outcome"]["source_of_truth"],
        "rust_trillionnium_item_equipment_runtime_state"
    );

    let (equipped_status, equipped) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "equip_item",
            "unit_id": "lord",
            "item_id": "street-compass-bracer",
            "target_slot": "wrist",
            "body": "equip the street compass bracer from Rust-owned inventory"
        }),
    )
    .await;
    assert_eq!(equipped_status, StatusCode::OK);
    assert_eq!(equipped["outcome"]["accepted"], true);
    assert_eq!(equipped["outcome"]["result"], "item_equipped");
    assert_eq!(equipped["outcome"]["equipped_slot"], "wrist");
    assert_eq!(
        equipped["outcome"]["item_equipment_runtime_contract_version"],
        "trillionnium_world_item_equipment_runtime_v1"
    );
    assert_eq!(
        equipped["outcome"]["item_equipment_runtime"]["runtime_status"],
        "rust_owned_inventory_and_equip_slots_live"
    );
    assert!(
        equipped["outcome"]["item_equipment_runtime"]["inventory_count"]
            .as_u64()
            .unwrap_or_default()
            >= 3
    );
    assert!(
        equipped["outcome"]["item_equipment_runtime"]["equipment_slots"]
            .get("wrist")
            .and_then(Value::as_str)
            .is_some_and(|slot_item| !slot_item.is_empty())
    );

    let (wrong_combat_node_status, wrong_combat_node) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "attack",
            "unit_id": "lord",
            "target_tile": "F5",
            "skill_id": "basic_unarmed",
            "osm_game_overlay_id": "trillionnium-world-node:starter-studio",
            "body": "try entering a local combat encounter from the wrong exploration node"
        }),
    )
    .await;
    assert_eq!(wrong_combat_node_status, StatusCode::OK);
    assert_eq!(wrong_combat_node["outcome"]["accepted"], false);
    assert_eq!(
        wrong_combat_node["outcome"]["result"],
        "combat_encounter_node_mismatch"
    );
    assert_eq!(
        wrong_combat_node["outcome"]["world_combat_encounter_loop_contract_version"],
        "trillionnium_world_combat_encounter_loop_v1"
    );
    assert_eq!(
        wrong_combat_node["outcome"]["source_of_truth"],
        "rust_world_combat_encounter_validator"
    );

    let (wrong_combat_target_status, wrong_combat_target) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "attack",
            "unit_id": "lord",
            "target_tile": "G7",
            "skill_id": "basic_unarmed",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "try entering the wrong target tile for the current exploration node"
        }),
    )
    .await;
    assert_eq!(wrong_combat_target_status, StatusCode::OK);
    assert_eq!(wrong_combat_target["outcome"]["accepted"], false);
    assert_eq!(
        wrong_combat_target["outcome"]["result"],
        "combat_encounter_target_mismatch"
    );
    assert_eq!(wrong_combat_target["outcome"]["expected_target_tile"], "F5");
    assert_eq!(
        wrong_combat_target["outcome"]["source_of_truth"],
        "rust_world_combat_encounter_validator"
    );

    let (attack_status, attack) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "attack",
            "unit_id": "lord",
            "target_tile": "F5",
            "skill_id": "basic_unarmed",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "resolve a deterministic street duel against the market bandit"
        }),
    )
    .await;
    assert_eq!(attack_status, StatusCode::OK);
    assert_eq!(attack["outcome"]["accepted"], true);
    assert_eq!(attack["outcome"]["result"], "tactics_combat_resolved");
    assert_eq!(
        attack["outcome"]["world_combat_encounter_loop_contract_version"],
        "trillionnium_world_combat_encounter_loop_v1"
    );
    assert_eq!(
        attack["outcome"]["combat_encounter"]["source_of_truth"],
        "rust_world_combat_encounter_projection"
    );
    assert_eq!(
        attack["outcome"]["combat_encounter"]["current_node_id"],
        "mirror-city-square"
    );
    assert_eq!(
        attack["outcome"]["return_to_map"]["return_state"],
        "map_ready_after_resolution"
    );
    assert_eq!(
        attack["outcome"]["return_to_map"]["source_of_truth"],
        "rust_world_combat_encounter_return_state"
    );
    assert_eq!(
        attack["outcome"]["combat_resolution_contract_version"],
        "trillionnium_tactics_combat_resolution_v1"
    );
    assert_eq!(
        attack["outcome"]["combat_resolution"]["source_of_truth"],
        "rust_tactics_combat_handler"
    );
    assert_eq!(
        attack["outcome"]["combat_resolution"]["defender_unit_id"],
        "market-bandit"
    );
    assert_eq!(
        attack["outcome"]["combat_resolution"]["state_persistence"],
        "world_state.world_trillionnium_characters.combat_numerics_state"
    );
    assert_eq!(
        attack["outcome"]["combat_numerics_runtime_contract_version"],
        "trillionnium_world_combat_numerics_runtime_v1"
    );
    assert_eq!(
        attack["outcome"]["combat_numerics_mutation"]["mutation_event"],
        "tactics_attack"
    );
    assert_eq!(
        attack["outcome"]["combat_numerics_mutation"]["mutation"]["attacker_unit_id"],
        "lord"
    );
    assert_eq!(
        attack["outcome"]["combat_numerics_runtime"]["source_of_truth"],
        "rust_trillionnium_combat_numerics_runtime_state"
    );
    assert_eq!(
        attack["outcome"]["combat_numerics_runtime"]["mutation_count"],
        1
    );
    assert!(
        attack["outcome"]["combat_numerics_runtime"]["health"]["current"]
            .as_i64()
            .unwrap_or_default()
            < attack["outcome"]["combat_numerics_runtime"]["health"]["max"]
                .as_i64()
                .unwrap_or_default()
    );
    assert_eq!(
        attack["outcome"]["resource_pressure_runtime_contract_version"],
        "trillionnium_world_resource_pressure_runtime_v1"
    );
    assert_eq!(
        attack["outcome"]["resource_pressure_mutation"]["mutation_event"],
        "tactics_attack"
    );
    assert_eq!(
        attack["outcome"]["resource_pressure_mutation"]["command"],
        "attack"
    );
    assert_eq!(
        attack["outcome"]["resource_pressure_runtime"]["source_of_truth"],
        "rust_trillionnium_resource_pressure_runtime_state"
    );
    assert_eq!(
        attack["outcome"]["resource_pressure_runtime"]["mutation_count"],
        1
    );
    assert_eq!(
        attack["outcome"]["resource_pressure_runtime"]["stamina"]["current"],
        86
    );
    assert_eq!(
        attack["outcome"]["resource_pressure_runtime"]["last_mutation_event"],
        "tactics_attack"
    );
    assert_eq!(
        attack["outcome"]["food_water_age_survival_runtime_contract_version"],
        "trillionnium_world_food_water_age_survival_v1"
    );
    assert_eq!(
        attack["outcome"]["survival_mutation"]["event_kind"],
        "tactics_attack"
    );
    assert_eq!(attack["outcome"]["survival_mutation"]["food_delta"], -3);
    assert_eq!(attack["outcome"]["survival_mutation"]["water_delta"], -5);
    assert_eq!(
        attack["outcome"]["survival_runtime"]["source_of_truth"],
        "rust_trillionnium_food_water_age_survival_state"
    );
    assert_eq!(attack["outcome"]["survival_runtime"]["food"]["current"], 73);
    assert_eq!(
        attack["outcome"]["survival_runtime"]["water"]["current"],
        77
    );
    assert_eq!(
        attack["outcome"]["dynamic_social_simulation_contract_version"],
        "trillionnium_world_dynamic_social_simulation_v1"
    );
    assert_eq!(
        attack["outcome"]["dynamic_social_mutation"]["source_of_truth"],
        "rust_world_relationships_dynamic_social_state"
    );
    assert_eq!(
        attack["outcome"]["dynamic_social_mutation"]["event_kind"],
        "tactics_attack"
    );
    assert!(
        attack["outcome"]["dynamic_social_simulation"]["relationship_event_count"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert_eq!(
        attack["outcome"]["region_story_unlock_runtime_contract_version"],
        "trillionnium_world_region_story_unlock_runtime_v1"
    );
    assert_eq!(
        attack["outcome"]["region_story_unlock_mutation"]["mutation_event"],
        "tactics_attack"
    );
    assert_eq!(
        attack["outcome"]["region_story_unlock_runtime"]["source_of_truth"],
        "rust_trillionnium_region_story_unlock_runtime_state"
    );
    assert!(
        attack["outcome"]["region_story_unlock_runtime"]["unlocked_story_arc_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arc| arc == "raid_signal_return")
    );
    assert_eq!(
        attack["outcome"]["tactics_game_session_contract_version"],
        "trillionnium_tactics_game_session_v1"
    );
    assert_eq!(
        attack["outcome"]["tactics_simulation_tick_contract_version"],
        "trillionnium_tactics_simulation_tick_v1"
    );
    assert_eq!(
        attack["outcome"]["tactics_reward_settlement_contract_version"],
        "trillionnium_tactics_reward_settlement_v1"
    );
    assert_eq!(
        attack["tactics_session"]["contract_version"],
        "trillionnium_tactics_game_session_v1"
    );
    assert_eq!(
        attack["tactics_session"]["persistence_owner"],
        "world_state.world_tactics_sessions"
    );
    assert_eq!(attack["tactics_session"]["objective_progress"], 1);
    assert_eq!(attack["tactics_session"]["objective_goal"], 1);
    assert_eq!(attack["tactics_session"]["victory_state"], "victory");
    assert_eq!(attack["tactics_session"]["reward_status"], "settled");
    assert_eq!(attack["tactics_session"]["status"], "completed");
    assert_eq!(
        attack["simulation_tick"]["contract_version"],
        "trillionnium_tactics_simulation_tick_v1"
    );
    assert_eq!(
        attack["simulation_tick"]["simulation_effect"],
        "deterministic_combat_resolved"
    );
    assert_eq!(attack["simulation_tick"]["outcome_accepted"], true);
    assert_eq!(attack["simulation_tick"]["action_points_after"], 0);
    assert_eq!(attack["simulation_tick"]["objective_delta"], 1);
    assert_eq!(attack["simulation_tick"]["victory_state_after"], "victory");
    assert_eq!(
        attack["simulation_tick"]["reward_status_after"],
        "pending_settlement"
    );
    assert_eq!(
        attack["tactics_reward_settlement"]["contract_version"],
        "trillionnium_tactics_reward_settlement_v1"
    );
    assert_eq!(attack["tactics_reward_settlement"]["status"], "settled");
    assert_eq!(attack["tactics_reward_settlement"]["credits_delta"], 5);
    assert_eq!(attack["tactics_reward_settlement"]["xp_delta"], 12);
    assert_eq!(attack["tactics_reward_settlement"]["reputation_delta"], 1);
    assert!(
        attack["outcome"]["combat_resolution"]["damage"]
            .as_i64()
            .unwrap_or_default()
            > 0
    );

    let (repeat_attack_status, repeat_attack) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "attack",
            "unit_id": "lord",
            "target_tile": "F5",
            "skill_id": "basic_unarmed",
            "body": "try to farm the already settled market bandit reward again"
        }),
    )
    .await;
    assert_eq!(repeat_attack_status, StatusCode::OK);
    assert_eq!(repeat_attack["outcome"]["accepted"], false);
    assert_eq!(repeat_attack["outcome"]["result"], "repeat_farming_blocked");
    assert_eq!(
        repeat_attack["outcome"]["anti_cheese_contract_version"],
        "trillionnium_tactics_repeat_farming_anti_cheese_v1"
    );
    assert_eq!(repeat_attack["outcome"]["anti_cheese_gate_enforced"], true);
    assert_eq!(
        repeat_attack["simulation_tick"]["outcome_result"],
        "repeat_farming_blocked"
    );
    assert_eq!(
        repeat_attack["simulation_tick"]["reward_status_after"],
        "settled"
    );

    let app_tactics_html =
        get_client_app_web_shell(axum::extract::State(state.clone()), HeaderMap::new())
            .await
            .0;
    assert!(app_tactics_html.contains("app-tactics-player-hud"));
    assert!(app_tactics_html.contains("app-tactics-objective-card"));
    assert!(app_tactics_html.contains("app-tactics-current-session-card"));
    assert!(app_tactics_html.contains("app-tactics-reward-history-handoff"));
    assert!(app_tactics_html.contains("app-tactics-repeat-farming-copy"));
    assert!(app_tactics_html.contains("data-victory-state=\"victory\""));
    assert!(app_tactics_html.contains("data-reward-status=\"settled\""));
    assert!(app_tactics_html.contains("data-repeat-farming-block-count=\"1\""));
    assert!(app_tactics_html.contains("data-result=\"repeat_farming_blocked\""));
    assert!(app_tactics_html.contains(
        "Repeat farming blocked: 1 extra attack intent(s) were rejected after the settled reward."
    ));
    assert!(app_tactics_html.contains("trillionnium_tactics_reward_history_v1"));

    let world_tactics_html = get_world_web_shell(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::extract::Query(HashMap::new()),
    )
    .await
    .0;
    assert!(world_tactics_html.contains("world-tactics-player-hud"));
    assert!(world_tactics_html.contains("world-tactics-objective-card"));
    assert!(world_tactics_html.contains("world-tactics-current-session-card"));
    assert!(world_tactics_html.contains("world-tactics-reward-history-handoff"));
    assert!(world_tactics_html.contains("world-tactics-repeat-farming-copy"));
    assert!(world_tactics_html.contains("trillionnium_world_combat_encounter_loop_v1"));
    assert!(world_tactics_html.contains("world-local-combat-encounter"));
    assert!(world_tactics_html.contains("world-local-combat-encounter-form"));
    assert!(world_tactics_html.contains("rust_world_combat_encounter_projection"));
    assert!(world_tactics_html.contains("rust_world_combat_encounter_validator"));
    assert!(world_tactics_html.contains("rust_world_combat_encounter_return_state"));
    assert!(world_tactics_html.contains("world-local-combat-return"));
    assert!(world_tactics_html.contains("data-victory-state=\"victory\""));
    assert!(world_tactics_html.contains("data-reward-status=\"settled\""));
    assert!(world_tactics_html.contains("data-repeat-farming-block-count=\"1\""));
    assert!(world_tactics_html.contains("data-result=\"repeat_farming_blocked\""));
    assert!(world_tactics_html.contains(
        "Repeat farming blocked: 1 extra attack intent(s) were rejected after the settled reward."
    ));
    assert!(world_tactics_html.contains("trillionnium_tactics_reward_history_v1"));

    let (miss_status, miss) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "attack",
            "unit_id": "lord",
            "target_tile": "A1",
            "skill_id": "basic_unarmed",
            "body": "try attacking an empty tile"
        }),
    )
    .await;
    assert_eq!(miss_status, StatusCode::OK);
    assert_eq!(miss["outcome"]["accepted"], false);
    assert_eq!(
        miss["outcome"]["rejection_reason"],
        "no_target_unit_at_tile"
    );
    assert_eq!(
        miss["simulation_tick"]["simulation_effect"],
        "rejected_no_state_advance"
    );
    assert_eq!(miss["simulation_tick"]["outcome_accepted"], false);

    let (wrong_npc_status, wrong_npc) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "talk_npc",
            "unit_id": "lord",
            "target_tile": "G8",
            "npc_id": "npc-street-compass-sifu",
            "osm_game_overlay_id": "trillionnium-world-node:starter-studio",
            "body": "try talking to the mentor at the wrong OSM anchor"
        }),
    )
    .await;
    assert_eq!(wrong_npc_status, StatusCode::OK);
    assert_eq!(wrong_npc["outcome"]["accepted"], false);
    assert_eq!(wrong_npc["outcome"]["result"], "npc_place_mismatch");
    assert_eq!(
        wrong_npc["outcome"]["source_of_truth"],
        "rust_trillionnium_npc_interaction_validator"
    );

    let (task_status, task_offer) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "offer_task",
            "unit_id": "lord",
            "target_tile": "G8",
            "npc_id": "npc-street-compass-sifu",
            "task_archetype_id": "courier_letter",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "ask Compass Sifu Luo for a local courier task"
        }),
    )
    .await;
    assert_eq!(task_status, StatusCode::OK);
    assert_eq!(task_offer["outcome"]["accepted"], true);
    assert_eq!(task_offer["outcome"]["result"], "task_offer_recorded");
    assert_eq!(task_offer["outcome"]["npc_id"], "npc-street-compass-sifu");
    assert_eq!(task_offer["outcome"]["task_archetype_id"], "courier_letter");
    assert_eq!(
        task_offer["outcome"]["task_archetype_contract_version"],
        "trillionnium_task_archetype_v1"
    );
    assert_eq!(task_offer["outcome"]["completion_command"], "complete_task");
    assert_eq!(
        task_offer["outcome"]["ledger_reward_requires_settlement"],
        true
    );
    assert_eq!(
        task_offer["trillionnium_task_contract"]["status"],
        "trillionnium_task_offered"
    );
    assert_eq!(
        task_offer["trillionnium_task_contract"]["task_id"],
        "trillionnium-task:courier_letter"
    );

    let (completion_status, completion) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "complete_task",
            "unit_id": "lord",
            "target_tile": "G8",
            "task_archetype_id": "courier_letter",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "Trillionnium task report: deliverable is the courier receipt, evidence source is the OSM plaza marker, risk controls are checked, next action is queued, and self-review is complete."
        }),
    )
    .await;
    assert_eq!(completion_status, StatusCode::OK);
    assert_eq!(completion["outcome"]["accepted"], true);
    assert_eq!(completion["outcome"]["result"], "task_completion_validated");
    assert_eq!(
        completion["outcome"]["resource_pressure_runtime_contract_version"],
        "trillionnium_world_resource_pressure_runtime_v1"
    );
    assert_eq!(
        completion["outcome"]["resource_pressure_mutation"]["mutation_event"],
        "tactics_complete_task"
    );
    assert_eq!(
        completion["outcome"]["resource_pressure_runtime"]["mutation_count"],
        2
    );
    assert_eq!(
        completion["outcome"]["resource_pressure_runtime"]["evidence_integrity"]["status"],
        "review_ready"
    );
    assert_eq!(
        completion["outcome"]["resource_pressure_runtime"]["evidence_integrity"]["fragments"],
        3
    );
    assert_eq!(
        completion["outcome"]["food_water_age_survival_runtime_contract_version"],
        "trillionnium_world_food_water_age_survival_v1"
    );
    assert_eq!(
        completion["outcome"]["survival_mutation"]["event_kind"],
        "tactics_complete_task"
    );
    assert_eq!(completion["outcome"]["survival_mutation"]["food_delta"], -2);
    assert_eq!(
        completion["outcome"]["survival_mutation"]["water_delta"],
        -3
    );
    assert_eq!(
        completion["outcome"]["survival_runtime"]["survival_pressure_status"],
        "stable_survival_loop"
    );
    assert_eq!(
        completion["outcome"]["dynamic_social_simulation_contract_version"],
        "trillionnium_world_dynamic_social_simulation_v1"
    );
    assert_eq!(
        completion["outcome"]["dynamic_social_mutation"]["relationship_kind"],
        "tactics_complete_task"
    );
    assert_eq!(
        completion["outcome"]["region_story_unlock_runtime_contract_version"],
        "trillionnium_world_region_story_unlock_runtime_v1"
    );
    assert_eq!(
        completion["outcome"]["region_story_unlock_mutation"]["mutation_event"],
        "tactics_complete_task"
    );
    assert_eq!(
        completion["outcome"]["region_story_unlock_runtime"]["last_mutation_event"],
        "tactics_complete_task"
    );
    assert!(
        completion["outcome"]["region_story_unlock_runtime"]["unlocked_story_arc_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arc| arc == "jade_route_patrol")
    );
    assert!(
        completion["outcome"]["region_story_unlock_runtime"]["unlocked_story_arc_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arc| arc == "night_watch_dispute")
    );
    assert_eq!(
        completion["outcome"]["completion_contract_version"],
        "trillionnium_task_completion_v1"
    );
    assert_eq!(
        completion["outcome"]["reward_gate_contract_version"],
        "trillionnium_reward_gate_v1"
    );
    assert_eq!(
        completion["outcome"]["ledger_reward_requires_settlement"],
        true
    );
    assert_eq!(completion["outcome"]["review_hold_gate_enforced"], true);
    assert_eq!(completion["outcome"]["anti_cheese_gate_enforced"], true);
    assert_eq!(completion["outcome"]["payout_status"], "eligible");
    assert_eq!(
        completion["trillionnium_task_completion"]["ledger_status"],
        "skipped_missing_account"
    );
    assert_eq!(
        completion["trillionnium_task_completion"]["payout_status"],
        "eligible"
    );
    assert_eq!(
        completion["trillionnium_task_completion"]["anti_cheat_flags"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let (repeat_status, repeat_completion) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "complete_task",
            "unit_id": "lord",
            "target_tile": "G8",
            "task_archetype_id": "courier_letter",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "Trillionnium task report: deliverable is the courier receipt, evidence source is the OSM plaza marker, risk controls are checked, next action is queued, and self-review is complete."
        }),
    )
    .await;
    assert_eq!(repeat_status, StatusCode::OK);
    assert_eq!(repeat_completion["outcome"]["accepted"], false);
    assert_eq!(
        repeat_completion["outcome"]["result"],
        "task_completion_requires_offer"
    );

    let (wrong_objective_status, wrong_objective_completion) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "complete_task",
            "unit_id": "lord",
            "target_tile": "G8",
            "task_archetype_id": "courier_letter",
            "osm_game_overlay_id": "trillionnium-world-node:wrong-objective",
            "body": "Trillionnium task report: deliverable and evidence were attempted against a mismatched OSM objective."
        }),
    )
    .await;
    assert_eq!(wrong_objective_status, StatusCode::OK);
    assert_eq!(wrong_objective_completion["outcome"]["accepted"], false);
    assert_eq!(
        wrong_objective_completion["outcome"]["rejection_reason"],
        "task_completion_requires_osm_generated_candidate"
    );

    let (review_offer_status, review_offer) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "offer_task",
            "unit_id": "lord",
            "target_tile": "G8",
            "npc_id": "npc-street-compass-sifu",
            "task_archetype_id": "courier_letter",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "ask Compass Sifu Luo for a second courier task that will exercise review hold"
        }),
    )
    .await;
    assert_eq!(review_offer_status, StatusCode::OK);
    assert_eq!(review_offer["outcome"]["accepted"], true);
    assert_eq!(
        review_offer["trillionnium_task_contract"]["status"],
        "trillionnium_task_offered"
    );

    let (review_status, review_completion) = send_json_request(
        &app,
        "POST",
        "/v1/world/tactics/command",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!world:local.dev",
            "command": "complete_task",
            "unit_id": "lord",
            "target_tile": "G8",
            "task_archetype_id": "courier_letter",
            "osm_game_overlay_id": "trillionnium-world-node:mirror-city-square",
            "body": "done"
        }),
    )
    .await;
    assert_eq!(review_status, StatusCode::OK);
    assert_eq!(review_completion["outcome"]["accepted"], true);
    assert_eq!(review_completion["outcome"]["payout_status"], "review_hold");
    assert_eq!(
        review_completion["trillionnium_task_completion"]["ledger_status"],
        "held_review"
    );
    let review_flags = review_completion["trillionnium_task_completion"]["anti_cheat_flags"]
        .as_array()
        .unwrap();
    assert!(review_flags
        .iter()
        .any(|flag| flag == "task_report_too_short"));
    assert_eq!(
        review_completion["trillionnium_task_contract"]["status"],
        "review_hold"
    );

    let world_local_task_html = get_world_web_shell(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::extract::Query(HashMap::new()),
    )
    .await
    .0;
    assert!(world_local_task_html.contains("world-local-active-task-card"));
    assert!(world_local_task_html.contains("trillionnium_world_local_task_lifecycle_v1"));
    assert!(world_local_task_html.contains("data-lifecycle-step=\"review_hold\""));
    assert!(world_local_task_html.contains("world-local-task-completion-feedback"));
    assert!(world_local_task_html.contains("data-completion-present=\"true\""));
    assert!(
        world_local_task_html.contains("data-source-of-truth=\"rust_world_contract_completions\"")
    );

    let guard = state.inner.league_state.lock().await;
    let character = world_trillionnium_character_projection_json(&guard.world, "@alice:local.dev");
    assert!(character["skill_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skill_id| skill_id == "basic_unarmed"));
    assert_eq!(character["title"], "提交Trillionnium战报");
    assert_eq!(
        character["resource_pressure_runtime"]["contract_version"],
        "trillionnium_world_resource_pressure_runtime_v1"
    );
    assert_eq!(character["resource_pressure_runtime"]["mutation_count"], 3);
    assert_eq!(
        character["resource_pressure_runtime"]["last_mutation_event"],
        "tactics_complete_task"
    );
    assert_eq!(
        character["region_story_unlock_runtime"]["contract_version"],
        "trillionnium_world_region_story_unlock_runtime_v1"
    );
    assert_eq!(
        character["region_story_unlock_runtime"]["source_of_truth"],
        "rust_trillionnium_region_story_unlock_runtime_state"
    );
    assert_eq!(
        character["region_story_unlock_runtime"]["last_mutation_event"],
        "tactics_complete_task"
    );
    assert!(
        character["region_story_unlock_runtime"]["unlocked_story_arc_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arc| arc == "jade_route_patrol")
    );
    assert!(guard
        .world
        .world_contract_completions
        .iter()
        .any(|entry| entry
            .ledger_status
            .as_deref()
            .is_some_and(|status| status == "skipped_missing_account")));
    let player = guard
        .players_by_matrix_user
        .get("@alice:local.dev")
        .unwrap();
    assert!(player.earned_credits >= 5.0);
    assert!(player.xp >= 12);
    assert!(guard
        .world
        .world_economy_events
        .iter()
        .any(
            |event| event.event_kind.as_str() == "tactics_victory_reward"
                && event.matrix_user_id == "@alice:local.dev"
        ));
    let app = client_app_json(&guard, "@alice:local.dev");
    let route_tasks = app["map_hub"]["route_task_graph"]["tasks"]
        .as_array()
        .expect("route task graph tasks");
    let tactics_task = route_tasks
        .iter()
        .find(|task| task["latest_bucket"] == "tactics_objective")
        .expect("tactics objective should bind into route task graph");
    assert_eq!(
        tactics_task["tactics_route_task_binding_contract_version"],
        "trillionnium_tactics_route_task_binding_v1"
    );
    assert_eq!(tactics_task["tactics_victory_state"], "victory");
    assert_eq!(tactics_task["tactics_reward_status"], "settled");
    assert_eq!(
        tactics_task["tactics_anti_cheese_contract_version"],
        "trillionnium_tactics_repeat_farming_anti_cheese_v1"
    );
    assert_eq!(
        tactics_task["tactics_route_task_binding"]["repeat_farming"]["blocked_attempt_count"],
        1
    );
    assert!(tactics_task["tactics_reward_history"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["stage"] == "reward_settlement" && entry["status"] == "settled"));
    assert_eq!(
        tactics_task["next_opportunity_kind"],
        "tactics_next_route_after_reward"
    );
    let tactics_task_id = tactics_task["task_id"].as_str().unwrap();
    let avatar_task_route = app["map_hub"]["viewport"]["avatar_task_routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|route| route["task_id"] == tactics_task_id)
        .expect("tactics route should surface in avatar task routes");
    assert_eq!(avatar_task_route["tactics_reward_status"], "settled");
    assert_eq!(
        avatar_task_route["tactics_reward_history_contract_version"],
        "trillionnium_tactics_reward_history_v1"
    );
    let avatar_route_runner = app["map_hub"]["viewport"]["avatar_route_runners"]
        .as_array()
        .unwrap()
        .iter()
        .find(|runner| runner["task_id"] == tactics_task_id)
        .expect("tactics route should surface in avatar route runners");
    assert_eq!(avatar_route_runner["tactics_reward_status"], "settled");
    assert_eq!(
        avatar_route_runner["tactics_reward_settled_unlocked_route"],
        true
    );
    assert!(avatar_route_runner["checkpoint_history"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["stage"] == "reward_settlement" && entry["status"] == "settled"));
    let nodes: Vec<WorldMapNode> = guard.world.world_map_nodes.values().cloned().collect();
    let current_node = guard.world.world_map_nodes.get(default_world_node_id());
    let geodata = openstreetmap_geodata_v1_json(&nodes, current_node);
    let tactics = world_tactics_board_projection_json(
        &guard.world,
        "@alice:local.dev",
        current_node,
        &geodata,
    );
    assert_eq!(
        tactics["game_session"]["contract_version"],
        "trillionnium_tactics_game_session_v1"
    );
    assert_eq!(tactics["game_session"]["persistence_status"], "persisted");
    assert_eq!(tactics["game_session"]["victory_state"], "victory");
    assert_eq!(tactics["game_session"]["reward_status"], "settled");
    assert!(
        tactics["game_session"]["current_tick"]
            .as_i64()
            .unwrap_or_default()
            >= 8
    );
    assert!(tactics["simulation_ticks"].as_array().unwrap().len() >= 8);
    assert!(tactics["simulation_ticks"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |tick| tick["contract_version"] == "trillionnium_tactics_simulation_tick_v1"
                && tick["simulation_effect"] == "rejected_no_state_advance"
        ));
    let street_compass = tactics["npcs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|npc| npc["npc_id"] == "npc-street-compass-sifu")
        .unwrap();
    assert_eq!(
        street_compass["relationship_contract_version"],
        "trillionnium_npc_relationship_v1"
    );
    assert_eq!(
        street_compass["relationship_state"]["source_of_truth"],
        "rust_world_relationships_persistent_state"
    );
    assert!(
        street_compass["relationship"].as_i64().unwrap_or_default()
            > street_compass["relationship_seed"]
                .as_i64()
                .unwrap_or_default()
    );
    assert!(
        street_compass["relationship_state"]["event_count"]
            .as_i64()
            .unwrap_or_default()
            >= 1
    );
}

#[tokio::test]
async fn world_map_move_endpoint_exposes_transition_semantics_contract() {
    let state = AppState::new(test_config());
    {
        let mut league = state.inner.league_state.lock().await;
        let current_id = default_world_node_id().to_string();
        let current = league
            .world
            .world_map_nodes
            .get(&current_id)
            .cloned()
            .expect("default world node");
        let mut insert_node =
            |direction: &str, node_id: &str, location_id: &str, zone_id: &str, status: &str| {
                league
                    .world
                    .world_map_nodes
                    .get_mut(&current_id)
                    .expect("current node exits")
                    .exits
                    .insert(direction.to_string(), node_id.to_string());
                league.world.world_map_nodes.insert(
                    node_id.to_string(),
                    WorldMapNode {
                        node_id: node_id.to_string(),
                        location_id: location_id.to_string(),
                        zone_id: zone_id.to_string(),
                        name: format!("{node_id} / 测试房间"),
                        node_kind: "transition_test_room".to_string(),
                        description: "Transition semantics fixture.".to_string(),
                        x: 7,
                        y: 7,
                        exits: HashMap::new(),
                        interaction_tags: Vec::new(),
                        freedom_hooks: Vec::new(),
                        status: status.to_string(),
                    },
                );
            };
        insert_node(
            "north-east",
            "world-transition-side-room",
            "world-transition-side-room",
            &current.zone_id,
            "open",
        );
        insert_node(
            "north-west",
            "world-transition-locked-room",
            &current.location_id,
            &current.zone_id,
            "locked",
        );
        insert_node(
            "south-west",
            "world-transition-interaction-room",
            &current.location_id,
            &current.zone_id,
            "interaction_required",
        );
    }
    let app = build_router(state.clone());

    let (blocked_status, blocked) = send_json_request(
        &app,
        "POST",
        "/v1/world/map/move",
        &[],
        json!({
            "matrix_user_id": "@transition:local.dev",
            "room_id": "!world:local.dev",
            "target": "south-east"
        }),
    )
    .await;
    assert_eq!(blocked_status, StatusCode::CONFLICT);
    assert_eq!(
        blocked["movement_transition"]["contract_version"],
        "trillionnium_world_transition_semantics_v1"
    );
    assert_eq!(
        blocked["movement_transition"]["source_of_truth"],
        "rust_world_map_transition_rules"
    );
    assert_eq!(
        blocked["movement_transition"]["web_role"],
        "intent_only_visualization_input"
    );
    assert_eq!(blocked["movement_transition"]["accepted"], false);
    assert_eq!(blocked["movement_transition"]["result"], "blocked_terrain");
    assert_eq!(
        blocked["movement_transition"]["transition_status"],
        "blocked"
    );
    assert_eq!(
        blocked["movement_transition"]["transition_kind"],
        "blocked_terrain"
    );
    assert_eq!(
        blocked["movement_transition"]["blocked_reason"],
        "no_exit_for_direction"
    );

    let (locked_status, locked) = send_json_request(
        &app,
        "POST",
        "/v1/world/map/move",
        &[],
        json!({
            "matrix_user_id": "@transition:local.dev",
            "room_id": "!world:local.dev",
            "target": "north-west"
        }),
    )
    .await;
    assert_eq!(locked_status, StatusCode::CONFLICT);
    assert_eq!(locked["movement_transition"]["accepted"], false);
    assert_eq!(locked["movement_transition"]["result"], "locked_route");
    assert_eq!(locked["movement_transition"]["transition_status"], "locked");
    assert_eq!(
        locked["movement_transition"]["transition_kind"],
        "locked_route"
    );
    assert_eq!(
        locked["movement_transition"]["to_node_id"],
        "world-transition-locked-room"
    );

    let (interaction_status, interaction) = send_json_request(
        &app,
        "POST",
        "/v1/world/map/move",
        &[],
        json!({
            "matrix_user_id": "@transition:local.dev",
            "room_id": "!world:local.dev",
            "target": "south-west"
        }),
    )
    .await;
    assert_eq!(interaction_status, StatusCode::CONFLICT);
    assert_eq!(
        interaction["movement_transition"]["result"],
        "interaction_required"
    );
    assert_eq!(
        interaction["movement_transition"]["transition_kind"],
        "interaction_required"
    );
    assert_eq!(
        interaction["movement_transition"]["requires_interaction"],
        true
    );

    let (room_status, room) = send_json_request(
        &app,
        "POST",
        "/v1/world/map/move",
        &[],
        json!({
            "matrix_user_id": "@transition:local.dev",
            "room_id": "!world:local.dev",
            "target": "north-east"
        }),
    )
    .await;
    assert_eq!(room_status, StatusCode::OK);
    assert_eq!(room["movement_transition"]["accepted"], true);
    assert_eq!(room["movement_transition"]["result"], "open_exit");
    assert_eq!(room["movement_transition"]["transition_status"], "accepted");
    assert_eq!(
        room["movement_transition"]["transition_kind"],
        "room_transition"
    );
    assert_eq!(room["movement_transition"]["changes_location"], true);
    assert_eq!(room["movement_transition"]["changes_zone"], false);
    assert_eq!(room["position"]["node_id"], "world-transition-side-room");
    assert_eq!(
        room["world_objective_travel_contract_version"],
        "trillionnium_world_objective_travel_v1"
    );
    assert_eq!(
        room["world_objective_travel"]["contract_version"],
        "trillionnium_world_objective_travel_v1"
    );
    assert_eq!(
        room["world_objective_travel"]["source_of_truth"],
        "rust_world_graph_objective_travel"
    );
    assert_eq!(
        room["world_objective_travel"]["current_node_id"],
        "world-transition-side-room"
    );
    assert_eq!(
        room["world_objective_travel"]["movement_source_of_truth"],
        "rust_world_map_move"
    );
    assert_eq!(
        room["rust_owned_ui_contract_version"],
        "trillionnium_world_rust_owned_ui_shell_v1"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["contract_version"],
        "trillionnium_world_rust_owned_ui_shell_v1"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["source_of_truth"],
        "rust_world_ui_renderer"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["render_owner"],
        "rust_world_ui_renderer"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["web_role"],
        "input_only_event_bridge"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["ui_ownership"]["keypad_viewport"],
        "rust_rendered"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["ui_ownership"]["route_task_graph"],
        "rust_rendered"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["route_ui"]["contract_version"],
        "trillionnium_world_rust_route_ui_fragments_v1"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["route_ui"]["default"]["render_owner"],
        "rust_world_ui_renderer"
    );
    assert_eq!(
        room["rust_owned_ui_fragments"]["route_ui"]["default"]["web_role"],
        "input_only_focus_bridge"
    );
    assert!(room["rust_owned_ui_fragments"]["keypad_grid_html"]
        .as_str()
        .unwrap_or_default()
        .contains("data-render-owner=\"rust_world_ui_renderer\""));
    assert!(room["rust_owned_ui_fragments"]["keypad_buttons_html"]
        .as_str()
        .unwrap_or_default()
        .contains("data-render-owner=\"rust_world_ui_renderer\""));
    assert!(room["rust_owned_ui_fragments"]["keypad_buttons_html"]
        .as_str()
        .unwrap_or_default()
        .contains("data-source-of-truth=\"rust_world_map_move\""));
    assert_eq!(
        room["resource_pressure_runtime_contract_version"],
        "trillionnium_world_resource_pressure_runtime_v1"
    );
    assert_eq!(
        room["resource_pressure_mutation"]["mutation_event"],
        "world_map_move"
    );
    assert_eq!(
        room["resource_pressure_mutation"]["mutation"]["time_delta_minutes"],
        12
    );
    assert_eq!(
        room["resource_pressure_runtime"]["source_of_truth"],
        "rust_trillionnium_resource_pressure_runtime_state"
    );
    assert_eq!(room["resource_pressure_runtime"]["mutation_count"], 1);
    assert_eq!(room["resource_pressure_runtime"]["stamina"]["current"], 96);
    assert_eq!(
        room["resource_pressure_runtime"]["evidence_integrity"]["fragments"],
        1
    );
    assert_eq!(
        room["food_water_age_survival_runtime_contract_version"],
        "trillionnium_world_food_water_age_survival_v1"
    );
    assert_eq!(room["survival_mutation"]["event_kind"], "world_map_move");
    assert_eq!(room["survival_mutation"]["food_delta"], -2);
    assert_eq!(room["survival_mutation"]["water_delta"], -4);
    assert_eq!(
        room["survival_runtime"]["source_of_truth"],
        "rust_trillionnium_food_water_age_survival_state"
    );
    assert_eq!(room["survival_runtime"]["food"]["current"], 74);
    assert_eq!(room["survival_runtime"]["water"]["current"], 78);
    assert_eq!(
        room["region_story_unlock_runtime_contract_version"],
        "trillionnium_world_region_story_unlock_runtime_v1"
    );
    assert_eq!(
        room["region_story_unlock_mutation"]["mutation_event"],
        "world_map_move"
    );
    assert_eq!(
        room["region_story_unlock_runtime"]["source_of_truth"],
        "rust_trillionnium_region_story_unlock_runtime_state"
    );
    assert_eq!(room["region_story_unlock_runtime"]["mutation_count"], 1);
    assert!(room["region_story_unlock_runtime"]["visited_node_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node == "world-transition-side-room"));
}

#[tokio::test]
async fn world_web_shell_marks_keypad_and_exit_transition_semantics() {
    let state = AppState::new(test_config());
    {
        let mut league = state.inner.league_state.lock().await;
        let current_id = default_world_node_id().to_string();
        let current = league
            .world
            .world_map_nodes
            .get(&current_id)
            .cloned()
            .expect("default world node");
        for (direction, node_id, location_id, zone_id, status) in [
            (
                "north-east",
                "world-shell-side-room",
                "world-shell-side-room",
                current.zone_id.as_str(),
                "open",
            ),
            (
                "north-west",
                "world-shell-locked-room",
                current.location_id.as_str(),
                current.zone_id.as_str(),
                "locked",
            ),
            (
                "south-west",
                "world-shell-interaction-room",
                current.location_id.as_str(),
                current.zone_id.as_str(),
                "interaction_required",
            ),
        ] {
            league
                .world
                .world_map_nodes
                .get_mut(&current_id)
                .expect("current node exits")
                .exits
                .insert(direction.to_string(), node_id.to_string());
            league.world.world_map_nodes.insert(
                node_id.to_string(),
                WorldMapNode {
                    node_id: node_id.to_string(),
                    location_id: location_id.to_string(),
                    zone_id: zone_id.to_string(),
                    name: format!("{node_id} / 壳测试房间"),
                    node_kind: "transition_shell_room".to_string(),
                    description: "Transition shell fixture.".to_string(),
                    x: 8,
                    y: 8,
                    exits: HashMap::new(),
                    interaction_tags: Vec::new(),
                    freedom_hooks: Vec::new(),
                    status: status.to_string(),
                },
            );
        }
    }

    let world_html = get_world_web_shell(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::extract::Query(HashMap::new()),
    )
    .await
    .0;
    assert!(world_html.contains("trillionnium_world_transition_semantics_v1"));
    assert!(
        world_html.contains("data-transition-source-of-truth=\"rust_world_map_transition_rules\"")
    );
    assert!(world_html.contains("data-transition-kind=\"blocked_terrain\""));
    assert!(world_html.contains("data-blocked-reason=\"no_exit_for_direction\""));
    assert!(world_html.contains("data-transition-status=\"locked\""));
    assert!(world_html.contains("data-transition-kind=\"locked_route\""));
    assert!(world_html.contains("data-transition-kind=\"interaction_required\""));
    assert!(world_html.contains("data-requires-interaction=\"true\""));
    assert!(world_html.contains("data-transition-kind=\"room_transition\""));
    assert!(world_html.contains("data-changes-location=\"true\""));
    assert!(world_html.contains(
        "\"transition_contract_version\":\"trillionnium_world_transition_semantics_v1\""
    ));
    assert!(world_html.contains("transitionForNext"));
    assert!(world_html.contains("window.trillionniumKeyboardMap"));
}

#[test]
fn world_home_json_exposes_shared_renderer_adapter_for_matrix_cards() {
    let league = default_league_state();
    let home = world_home_json(&league);
    let engine = &home["real_world_map_engine"];

    assert_eq!(engine["engine_id"], "leaflet_openstreetmap_v1");
    assert_eq!(engine["tile_provider"], "OpenStreetMap");
    assert_eq!(
        engine["geodata_provider_contract"]["contract_version"],
        "openstreetmap_geodata_v1"
    );
    assert_eq!(
        home["openstreetmap_geodata"]["provider_contract"],
        "OpenStreetMapDataProvider"
    );
    assert_eq!(
        home["openstreetmap_geodata"]["gameplay_owner"],
        "trillionnium_rust_world_state"
    );
    assert_eq!(
        home["openstreetmap_geodata"]["legal"]["odbl_database_obligations"],
        true
    );
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
    assert_eq!(
        engine["planned_upgrade_engine"]["readiness_contract_version"],
        "trillionnium_world_future_engine_readiness_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["future_engine_readiness"]["contract_version"],
        "trillionnium_world_future_engine_readiness_v1"
    );
    assert_eq!(
        engine["renderer_adapter"]["future_engine_readiness"]["shadow_renderer_contract"]["status"],
        "shadow_only_not_user_facing"
    );
    assert_eq!(
        home["world_map_subsystem_contract"]["contract_version"],
        "trillionnium_world_map_subsystem_v1"
    );
    assert_eq!(
        home["world_map_subsystem_contract"]["module_boundary_contract"]["contract_version"],
        "trillionnium_world_map_module_boundary_v1"
    );
    assert!(
        home["world_map_subsystem_contract"]["module_boundary_contract"]["source_modules"]
            .as_array()
            .is_some_and(|modules| modules
                .iter()
                .any(|module| module["module"] == "world_map_projection"))
    );
    assert_eq!(
        home["route_runner_handoff"]["contract_version"],
        "trillionnium_route_runner_handoff_v1"
    );
    assert_eq!(
        home["route_runner_handoff"]["supports_route_runner_next_route_actions"],
        true
    );
    assert_eq!(
        home["route_runner_handoff"]["supports_route_runner_lifecycle"],
        true
    );
    assert_eq!(
        home["route_runner_handoff"]["lifecycle_contract_version"],
        "trillionnium_route_runner_lifecycle_v1"
    );
    assert_eq!(
        home["route_runner_handoff"]["supports_route_mastery_progression"],
        true
    );
    assert_eq!(
        home["route_runner_handoff"]["route_mastery_contract_version"],
        "trillionnium_route_mastery_v1"
    );
    assert!(home["route_runner_handoff"]["route_mastery_runner_count"]
        .as_u64()
        .is_some());
    assert!(home["route_runner_handoff"]["first_route_mastery_xp"]
        .as_u64()
        .is_some());
    assert!(home["route_runner_handoff"]["handoff_prompt"]
        .as_str()
        .unwrap_or_default()
        .contains("deliverable → evidence → risk controls → next action → self-review"));
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
    assert_eq!(
        map["route_runner_handoff"]["contract_version"],
        "trillionnium_route_runner_handoff_v1"
    );
    assert!(map["route_runner_handoff"]["first_next_route_action_body"]
        .as_str()
        .unwrap_or_default()
        .contains("risk controls, next action, and self-review"));
    assert_eq!(app["route_contract"], expected);
    assert_eq!(app["map"]["route_contract"], expected);
    assert_eq!(app["feed"]["route_contract"], expected);
    assert_eq!(app["map_hub"]["route_contract"], expected);
    assert_eq!(feed["route_contract"], expected);
    assert_eq!(
        feed["route_runner_handoff"]["contract_version"],
        "trillionnium_route_runner_handoff_v1"
    );
    assert_eq!(
        feed["route_runner_handoff"]["lifecycle_contract_version"],
        "trillionnium_route_runner_lifecycle_v1"
    );
    assert_eq!(
        feed["route_runner_handoff"]["route_mastery_contract_version"],
        "trillionnium_route_mastery_v1"
    );
    assert_eq!(
        app["feed"]["route_runner_handoff"]["supports_route_mastery_progression"],
        true
    );
    assert_eq!(
        app["feed"]["route_runner_handoff"]["supports_route_runner_next_route_actions"],
        true
    );
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
    assert!(app["map_hub"]["player_avatar_count"].as_u64().unwrap_or(0) >= 1);
    assert!(
        app["map_hub"]["avatar_task_route_count"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );
    assert!(
        app["map_hub"]["avatar_route_runner_count"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );
    assert_eq!(
        app["map_hub"]["route_runner_handoff"]["contract_version"],
        "trillionnium_route_runner_handoff_v1"
    );
    assert!(
        app["map_hub"]["route_runner_handoff"]["next_route_action_count"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );
    assert!(app["map_hub"]["route_runner_handoff"]["summary"]
        .as_str()
        .unwrap_or_default()
        .contains("next-route actions"));
    assert!(
        app["map_hub"]["route_runner_handoff"]["first_next_route_action_body"]
            .as_str()
            .unwrap_or_default()
            .contains("risk controls, next action, and self-review")
    );
    assert_eq!(
        app["map_hub"]["route_runner_handoff"]["supports_route_runner_next_route_actions"],
        true
    );
    assert_eq!(
        app["map_hub"]["route_runner_handoff"]["supports_route_runner_lifecycle"],
        true
    );
    assert_eq!(
        app["map_hub"]["route_runner_handoff"]["lifecycle_contract_version"],
        "trillionnium_route_runner_lifecycle_v1"
    );
    assert_eq!(
        app["map_hub"]["route_runner_handoff"]["supports_route_mastery_progression"],
        true
    );
    assert_eq!(
        app["map_hub"]["route_runner_handoff"]["route_mastery_contract_version"],
        "trillionnium_route_mastery_v1"
    );
    assert!(
        app["map_hub"]["route_runner_handoff"]["first_route_mastery_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("Route mastery")
    );
    assert_eq!(app["map_hub"]["player_density_mode"], "dense");
    assert_eq!(app["modules"][0]["name"], "Trillionnium World Map");
    assert!(app["modules"][0]["summary"]
        .as_str()
        .unwrap_or_default()
        .contains("角色跑图"));
    assert!(app["modules"][0]["summary"]
        .as_str()
        .unwrap_or_default()
        .contains("任务路线"));
    assert!(app["modules"][0]["summary"]
        .as_str()
        .unwrap_or_default()
        .contains("动态角色"));
    assert_eq!(
        app["feed"]["active_region_id"],
        app["map_hub"]["active_region_id"]
    );
    assert_eq!(
        app["feed"]["route_task_graph"]["task_count"],
        app["map_hub"]["route_task_graph"]["task_count"]
    );
    assert_eq!(
        app["feed"]["route_runner_handoff"]["contract_version"],
        "trillionnium_route_runner_handoff_v1"
    );
    assert_eq!(
        app["feed"]["route_runner_handoff"]["next_route_action_count"],
        app["map_hub"]["route_runner_handoff"]["next_route_action_count"]
    );
    assert_eq!(
        app["mobile_shell_contract"]["contract_version"],
        "trillionnium_mobile_shell_ux_v1"
    );
    assert_eq!(
        app["mobile_shell_contract"]["primary_cta"]["contract_version"],
        "trillionnium_mobile_single_primary_cta_v1"
    );
    assert_eq!(
        app["mobile_shell_contract"]["primary_cta"]["bottom_sheet_id"],
        "app-mobile-action-sheet"
    );
    assert_eq!(
        app["mobile_shell_contract"]["primary_cta"]["primary_cta_id"],
        "app-mobile-primary-cta"
    );
    assert_eq!(
        app["mobile_shell_contract"]["primary_cta"]["target_id"],
        "app-map-action-rail"
    );
    assert_eq!(
        app["mobile_shell_contract"]["primary_cta"]["single_primary_cta"],
        true
    );
    assert_eq!(
        app["mobile_shell_contract"]["copy_layering"]["contract_version"],
        "trillionnium_mobile_copy_layering_v1"
    );
    assert_eq!(
        app["mobile_shell_contract"]["copy_layering"]["summary_id"],
        "app-map-copy-summary"
    );
    assert_eq!(
        app["mobile_shell_contract"]["copy_layering"]["details_id"],
        "app-map-copy-layer-details"
    );
    assert_eq!(
        app["mobile_shell_contract"]["copy_layering"]["default_state"],
        "collapsed"
    );
    assert_eq!(
        app["mobile_shell_contract"]["map_readability_lod"]["contract_version"],
        "trillionnium_world_map_readability_lod_v1"
    );
    assert_eq!(
        app["mobile_shell_contract"]["map_readability_lod"]["visible_contract_id"],
        "app-map-readability-lod"
    );
    assert_eq!(
        app["mobile_shell_contract"]["map_readability_lod"]["max_primary_cta_count"],
        1
    );
    assert!(
        app["mobile_shell_contract"]["map_readability_lod"]["max_summary_chars"]
            .as_u64()
            .unwrap_or(999)
            <= 150
    );
    assert!(
        app["mobile_shell_contract"]["map_readability_lod"]["max_visible_markers"]
            .as_u64()
            .unwrap_or(999)
            <= 18
    );
    assert_eq!(
        app["mobile_shell_contract"]["map_readability_lod"]["semantic_layer_contract_version"],
        "trillionnium_world_map_game_layer_semantics_v1"
    );
    assert!(
        app["mobile_shell_contract"]["map_readability_lod"]["semantic_roles"]
            .as_array()
            .is_some_and(|roles| roles.len() >= 4)
    );
    assert_eq!(
        app["mobile_shell_contract"]["first_screen_decision"]["contract_version"],
        "trillionnium_world_map_first_screen_decision_v1"
    );
    assert_eq!(
        app["mobile_shell_contract"]["runtime_performance_budget"]["contract_version"],
        "trillionnium_world_map_runtime_performance_budget_v1"
    );
    assert_eq!(
        app["world_map_subsystem_contract"]["contract_version"],
        "trillionnium_world_map_subsystem_v1"
    );
    assert_eq!(
        app["map_hub"]["viewport"]["runtime_performance_budget"]["contract_version"],
        "trillionnium_world_map_runtime_performance_budget_v1"
    );
    assert_eq!(
        app["map_hub"]["viewport"]["transport_delta_contract"]["contract_version"],
        "trillionnium_world_map_transport_delta_v1"
    );
    assert!(
        app["map_hub"]["viewport"]["runtime_performance_budget"]["degrade_strategy"]
            ["delta_viewport_updates_required"]
            .as_bool()
            .unwrap_or(false)
    );
    assert_eq!(
        app["economy_retention_ops"]["route_runner_funnel_telemetry"]["contract_version"],
        "trillionnium_route_runner_funnel_telemetry_v1"
    );
    assert_eq!(
        app["route_runner_funnel_telemetry"]["contract_version"],
        "trillionnium_route_runner_funnel_telemetry_v1"
    );
    assert!(
        app["route_runner_funnel_telemetry"]["event_counts"]["route_started"]
            .as_i64()
            .unwrap_or(0)
            > 0
    );
    assert!(
        app["route_runner_funnel_telemetry"]["event_counts"]["evidence_submitted"]
            .as_i64()
            .is_some()
    );
    assert!(
        app["route_runner_funnel_telemetry"]["event_counts"]["reward_claimed"]
            .as_i64()
            .is_some()
    );
    assert!(
        app["route_runner_funnel_telemetry"]["event_counts"]["daily_return_resume"]
            .as_i64()
            .unwrap_or(0)
            > 0
    );
    assert_eq!(
        app["route_runner_funnel_telemetry"]["time_to_reward"]["target_seconds"],
        1800
    );
    assert_eq!(
        app["route_runner_funnel_telemetry"]["cohort_quality"]["contract_version"],
        "trillionnium_route_runner_funnel_cohort_quality_v1"
    );
    assert_eq!(
        app["route_runner_funnel_telemetry"]["funnel_integrity"]["contract_version"],
        "trillionnium_route_runner_funnel_integrity_v1"
    );
    assert!(app["route_runner_funnel_telemetry"]["funnel_integrity"]
        ["cohort_denominator_consistent"]
        .as_bool()
        .unwrap_or(false));
    assert!(app["route_runner_funnel_telemetry"]["funnel_integrity"]
        ["reward_to_next_route_blockers"]["blocked_reason_candidates"]
        .as_array()
        .is_some_and(|reasons| reasons.len() >= 3));
    assert!(app["route_runner_funnel_telemetry"]["cohort_quality"]
        ["reward_to_next_route_conversion_percent"]
        .as_i64()
        .is_some());
    assert!(
        app["route_runner_funnel_telemetry"]["cohort_quality"]["d1_resume_rate_percent"]
            .as_i64()
            .is_some()
    );
    assert!(
        app["route_runner_funnel_telemetry"]["cohort_quality"]["abandon_reason_breakdown"]
            .as_object()
            .is_some()
    );
    assert_eq!(
        app["route_archetypes"]["contract_version"],
        "trillionnium_world_route_archetypes_v1"
    );
    assert!(app["route_archetypes"]["archetypes"]
        .as_array()
        .is_some_and(|archetypes| archetypes.len() >= 5));
    assert_eq!(
        app["commercial_operating_dashboard"]["contract_version"],
        "trillionnium_world_commercial_operating_dashboard_v1"
    );
    assert_eq!(
        app["commercial_operating_dashboard"]["route_recommendation_policy"]["contract_version"],
        "trillionnium_world_route_recommendation_policy_v1"
    );
    assert!(
        app["commercial_operating_dashboard"]["route_start_to_paid_task_conversion_percent"]
            .as_i64()
            .is_some()
    );
    assert!(app["route_runner_funnel_telemetry"]["readiness_checks"]
        .as_array()
        .is_some_and(|checks| checks
            .iter()
            .any(|check| check == "time_to_reward_target_visible")));
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
        "mobile_bottom_sheet_single_primary_cta_visible",
        "mobile_copy_layering_visible",
        "map_readability_lod_visible",
        "map_game_layer_semantics_visible",
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
    assert_eq!(app["onboarding"]["quick_path_label"], "Quick Path");
    assert_eq!(app["onboarding"]["quick_path_label_zh"], "快速路径");
    assert_eq!(
        app["onboarding"]["quick_path_summary"],
        "Choose map focus → run one bounty → submit/review reward"
    );
    assert_eq!(
        app["onboarding"]["quick_path_summary_zh"],
        "选择地图焦点 → 跑一个悬赏 → 提交/查看奖励"
    );
    let quick_path_steps = app["onboarding"]["quick_path_steps"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(quick_path_steps.len(), 3);
    assert_eq!(quick_path_steps[1]["label"], "2 · Run one bounty");
    assert!(quick_path_steps[1]["description"]
        .as_str()
        .unwrap_or_default()
        .contains("rated commission"));
    assert_eq!(
        app["onboarding"]["command_disclosure_label"],
        "Full Commands"
    );
    assert_eq!(app["onboarding"]["command_disclosure_label_zh"], "完整命令");
    assert!(app["onboarding"]["command_disclosure"]
        .as_str()
        .unwrap_or_default()
        .contains("Use these when you are ready to submit real work"));
    assert!(app["onboarding"]["command_disclosure_zh"]
        .as_str()
        .unwrap_or_default()
        .contains("准备真实提交时再展开"));
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
    assert!(app_html.contains("truncated_runtime_bootstrap_with_lazy_delta_hydration"));
    assert!(app_html.contains("data-cache-contract=\"trillionnium_world_map_payload_cache_v1\""));
    assert!(app_html.contains("buildViewportDeltaUrl"));
    assert!(app_html.contains("postMapRumSample"));
    assert!(app_html.contains("/world/web/map-rum"));
    assert!(!app_html.contains("leafletMap"));
    assert!(app_html.contains("renderRouteLine"));
    assert!(app_html.contains("renderTileFrame"));
    assert!(app_html.contains("renderEventPulse"));
    assert!(app_html.contains("renderPlayerAvatar"));
    assert!(app_html.contains("renderMovingAvatar"));
    assert!(app_html.contains("Trillionnium World Map"));
    assert!(app_html.contains("OpenStreetMap upgraded into a playable world"));
    assert!(app_html.contains("app-openstreetmap-attribution"));
    assert!(app_html.contains("openstreetmap_attribution_presence_v1"));
    assert!(app_html.contains("© OpenStreetMap contributors"));
    assert!(app_html.contains("data-attribution-visible=\"true\""));
    assert!(app_html.contains("data-derived-database-tracking-required=\"true\""));
    assert!(app_html.contains("Player avatars / 跑图角色"));
    assert!(app_html.contains("Task routes / 任务路线"));
    assert!(app_html.contains("Avatar Task Routes"));
    assert!(app_html.contains("Avatar Movement"));
    assert!(app_html.contains("data-overlay-target=\"avatars\""));
    assert!(app_html.contains("data-overlay-target=\"taskRoutes\""));
    assert!(app_html.contains("data-overlay-target=\"routeRunners\""));
    assert!(app_html.contains("app-avatar-task-routes-live"));
    assert!(app_html.contains("app-avatar-route-runners-live"));
    assert!(app_html.contains("app-route-runner-handoff-summary"));
    assert!(app_html.contains("app-feed-route-runner-handoff"));
    assert!(app_html.contains("trillionnium_route_runner_handoff_v1"));
    assert!(app_html.contains("Route runner handoff:"));
    assert!(app_html.contains("data-next-route-status="));
    assert!(app_html.contains("renderRouteRunnerHandoffSummary"));
    assert!(app_html.contains("data-runner-count="));
    assert!(app_html.contains("avatar_task_routes"));
    assert!(app_html.contains("avatar_route_runners"));
    assert!(app_html.contains("runner_trace_points"));
    assert!(app_html.contains("remaining_distance_meters"));
    assert!(app_html.contains("eta_label"));
    assert!(app_html.contains("reward_checkpoint"));
    assert!(app_html.contains("reward_claim_action_body"));
    assert!(app_html.contains("next_route_action_body"));
    assert!(app_html.contains("next_route_sequence_summary"));
    assert!(app_html.contains("completion_command"));
    assert!(app_html.contains("completion_action_body"));
    assert!(app_html.contains("checkpoint_history"));
    assert!(app_html.contains("checkpoint_history_summary"));
    assert!(app_html.contains("reward_history_summary"));
    assert!(app_html.contains("trillionnium_route_mastery_v1"));
    assert!(app_html.contains("route_mastery_xp"));
    assert!(app_html.contains("routeRunnerMasteryChipsHtml"));
    assert!(app_html.contains("data-route-mastery-tier"));
    assert!(app_html.contains("dataset.routeMasteryContract"));
    assert!(app_html.contains("routeRunnerHistoryChipsHtml"));
    assert!(app_html.contains("buildRouteRunnerRewardClaimAction"));
    assert!(app_html.contains("routeRunnerRewardClaimButtonHtml"));
    assert!(app_html.contains("trillionnium-reward-claim-action"));
    assert!(app_html.contains("Prepare reward claim"));
    assert!(app_html.contains("buildRouteRunnerNextRouteAction"));
    assert!(app_html.contains("routeRunnerNextRouteButtonHtml"));
    assert!(app_html.contains("trillionnium-next-route-action"));
    assert!(app_html.contains("Open next route"));
    assert!(app_html.contains("Preview next route"));
    assert!(app_html.contains("Checkpoint history"));
    assert!(app_html.contains("Reward history"));
    assert!(app_html.contains("agent_party"));
    assert!(app_html.contains("agent_party_summary"));
    assert!(app_html.contains("agentPartyChipsHtml"));
    assert!(app_html.contains("buildAgentPartyAction"));
    assert!(app_html.contains("agentPartyActionButtonsHtml"));
    assert!(app_html.contains("trillionnium-agent-party-action"));
    assert!(app_html.contains("Agent party"));
    assert!(app_html.contains("Agent handoff"));
    assert!(app_html.contains("Oracle Scout"));
    assert!(app_html.contains("Scout route"));
    assert!(app_html.contains("Build deliverable"));
    assert!(app_html.contains("Audit risk"));
    assert!(app_html.contains("Close reward"));
    assert!(app_html.contains("buildRouteRunnerCompletionAction"));
    assert!(app_html.contains("routeRunnerCompletionButtonHtml"));
    assert!(app_html.contains("Complete checkpoint"));
    assert!(app_html.contains("trillionnium-app-route-flow-action"));
    assert!(app_html.contains("filterAvatarTaskRoutes"));
    assert!(app_html.contains("filterAvatarRouteRunners"));
    assert!(app_html.contains("trillionnium-avatar-task-route-path"));
    assert!(app_html.contains("trillionnium-avatar-route-runner-dot"));
    assert!(app_html.contains("trillionnium-avatar-route-runner-progress"));
    assert!(app_html.contains("trillionnium-avatar-route-runner-remaining"));
    assert!(app_html.contains("trillionnium-avatar-route-reward-checkpoint"));
    assert!(app_html.contains("trillionnium-route-dash"));
    assert!(app_html.contains("trillionnium-runner-bob"));
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
    assert!(app_html.contains("trillionnium_mobile_single_primary_cta_v1"));
    assert!(app_html.contains("app-mobile-action-sheet"));
    assert!(app_html.contains("app-mobile-primary-cta"));
    assert!(app_html.contains("data-primary-cta-count=\"1\""));
    assert!(app_html.contains("data-primary-cta-target=\"app-map-action-rail\""));
    assert!(app_html.contains("Continue Route"));
    assert!(app_html.contains("mobile_bottom_sheet_single_primary_cta_visible"));
    assert!(app_html.contains("trillionnium_mobile_copy_layering_v1"));
    assert!(app_html.contains("app-map-copy-summary"));
    assert!(app_html.contains("app-map-copy-layer-details"));
    assert!(app_html.contains("data-default-state=\"collapsed\""));
    assert!(app_html.contains("Pick a nearby route"));
    assert!(app_html.contains("Why this map matters"));
    assert!(app_html.contains("mobile_copy_layering_visible"));
    assert!(app_html.contains("trillionnium_world_map_readability_lod_v1"));
    assert!(app_html.contains("app-map-readability-lod"));
    assert!(app_html.contains("data-first-screen-mode=\"route_first_street_detail\""));
    assert!(app_html.contains("data-primary-cta-budget=\"1\""));
    assert!(app_html.contains("data-visible-marker-budget=\"18\""));
    assert!(app_html.contains("One route first"));
    assert!(app_html.contains("trillionnium_world_map_game_layer_semantics_v1"));
    assert!(app_html.contains("map_readability_lod_visible"));
    assert!(app_html.contains("map_game_layer_semantics_visible"));
    assert!(app_html.contains("app-route-runner-funnel-telemetry"));
    assert!(app_html.contains("trillionnium_route_runner_funnel_telemetry_v1"));
    assert!(app_html.contains("data-time-to-reward-target-seconds=\"1800\""));
    assert!(app_html.contains("data-reward-to-next-route-percent="));
    assert!(app_html.contains("app-route-archetype-catalog"));
    assert!(app_html.contains("app-commercial-operating-dashboard"));
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
    assert!(app_html.contains("app-first-playable-quick-path"));
    assert!(app_html.contains("Quick Path"));
    assert!(app_html.contains("Choose map focus → run one bounty → submit/review reward"));
    assert!(app_html.contains("2 · Run one bounty"));
    assert!(app_html.contains("Start the first world action and capture it as a rated commission."));
    assert!(app_html.contains("app-first-playable-full-commands"));
    assert!(app_html.contains("Full Commands"));
    assert!(app_html.contains("Use these when you are ready to submit real work"));
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
    assert!(app_html.contains("return routeFlowActionButtonHtml(opportunityAction, className) + routeFlowActionButtonHtml(suggestedAction, className);"));
    assert!(app_html.contains("\"contract_version\":1"));
    assert!(app_html.contains("app-tactics-player-hud"));
    assert!(app_html.contains("trillionnium_tactics_player_visible_surface_v1"));
    assert!(app_html.contains("app-tactics-objective-card"));
    assert!(app_html.contains("app-tactics-current-session-card"));
    assert!(app_html.contains("app-tactics-intent-draft-card"));
    assert!(app_html.contains("app-tactics-reward-history-handoff"));
    assert!(app_html.contains("app-tactics-repeat-farming-copy"));
    assert!(app_html.contains("data-source-of-truth=\"rust_world_tactics_sessions\""));
    assert!(app_html.contains("data-source-of-truth=\"rust_tactics_command_model\""));
    assert!(app_html.contains("data-web-role=\"visualization_input_only\""));
    assert!(app_html.contains("data-web-role=\"intent_only_visualization_input\""));
    assert!(
        app_html.contains("data-contract-version=\"trillionnium_tactics_command_intent_draft_v1\"")
    );
    assert!(app_html.contains(
        "data-board-cell-interaction-contract=\"trillionnium_tactics_board_cell_interaction_v1\""
    ));
    assert!(app_html
        .contains("data-unit-selection-contract=\"trillionnium_tactics_unit_selection_v1\""));
    assert!(
        app_html.contains("data-accessibility-contract=\"trillionnium_tactics_accessibility_v1\"")
    );
    assert!(app_html.contains("data-low-motion-support=\"prefers_reduced_motion\""));
    assert!(app_html.contains("data-draft-owner=\"browser_tactics_intent_builder\""));
    assert!(app_html.contains("data-command-handler-owner=\"rust_world_tactics_command_handler\""));
    assert!(app_html
        .contains("data-reward-history-contract=\"trillionnium_tactics_reward_history_v1\""));
    assert!(app_html.contains(
        "data-anti-cheese-contract=\"trillionnium_tactics_repeat_farming_anti_cheese_v1\""
    ));
    assert!(app_html.contains("Repeat-farming guard"));

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
    assert!(world_html.contains("world-mobile-primary-cta"));
    assert!(world_html.contains("data-primary-cta-count=\"1\""));
    assert!(world_html.contains("Pick route"));
    assert!(world_html.contains("Submit proof"));
    assert!(world_html.contains("Claim reward"));
    assert!(world_html.contains(r#"id="trillionnium-world-game-first-shell""#));
    assert!(world_html.contains("trillionnium_world_game_first_playable_shell_v1"));
    assert!(world_html.contains("trillionnium_world_rust_owned_ui_shell_v1"));
    assert!(world_html
        .contains(r#"data-rust-owned-ui-contract="trillionnium_world_rust_owned_ui_shell_v1""#));
    assert!(world_html.contains(r#"data-ui-render-owner="rust_world_ui_renderer""#));
    assert!(world_html.contains(r#"data-browser-ui-owner="input_only_event_bridge""#));
    assert!(world_html.contains(r#"data-source-of-truth="rust_world_state_projection""#));
    assert!(world_html.contains(r#"data-web-role="input_only_visualization""#));
    assert!(world_html.contains(r#"data-heavy-panels-policy="secondary_collapsed_deferred""#));
    assert!(world_html.contains(
        r#"data-forbidden-intermediate="no_full_hero_tan_replica_then_replace_workflow""#
    ));
    assert!(world_html.contains(
        r#"data-resource-source-of-truth="rust_trillionnium_resource_pressure_runtime_state""#
    ));
    assert!(world_html.contains(
        r#"data-combat-source-of-truth="rust_trillionnium_combat_numerics_runtime_state""#
    ));
    assert!(world_html.contains("world-game-first-action-list"));
    assert!(world_html.contains(r#"data-action-kind="talk_npc""#));
    assert!(world_html.contains(r#"data-action-kind="train_skill""#));
    assert!(world_html.contains(r#"data-action-kind="combat""#));
    assert!(world_html.contains(
        r#"data-authored-quest-chain-contract="trillionnium_world_authored_quest_chain_v1""#
    ));
    assert!(world_html.contains(r#"data-deferred-payload="true""#));
    let game_first_index = world_html
        .find(r#"id="trillionnium-world-game-first-shell""#)
        .expect("game-first shell missing");
    let map_panel_index = world_html
        .find(r#"id="world-map-shell-panel""#)
        .expect("map shell panel missing");
    assert!(game_first_index < map_panel_index);
    assert!(!world_html.contains("Platinum Hero Tale LCD"));
    assert!(!world_html.contains("白金英雄坛说小绿屏"));
    assert!(world_html.contains("world-keypad-adventure-shell"));
    assert!(world_html.contains("trillionnium_text_adventure_keypad_movement_v1"));
    assert!(world_html.contains("data-interface-style=\"yingxiongtanshuo_keyboard_tile_map\""));
    assert!(world_html.contains("data-reference-project=\"albert10jp/yxts-gold-asm\""));
    assert!(world_html.contains("data-lcd-screen=\"160x80\""));
    assert!(world_html.contains("data-lcd-viewport=\"5x3\""));
    assert!(world_html.contains("data-lcd-palette=\"green_monochrome\""));
    assert!(world_html.contains("data-lcd-cols=\"5\""));
    assert!(world_html.contains("data-lcd-rows=\"3\""));
    assert!(world_html.contains("data-keypad-controls=\"7,8,9,4,5,6,1,2,3\""));
    assert!(world_html.contains("data-source-of-truth=\"rust_world_map_move\""));
    assert!(world_html.contains("world-keypad-map-grid"));
    assert!(world_html.contains("world-keypad-numpad"));
    assert!(world_html.contains("world-keypad-move-form"));
    assert!(world_html.contains("world-play-first-action-prompt"));
    assert!(world_html.contains("trillionnium_world_play_first_exploration_loop_v1"));
    assert!(world_html.contains("world-current-location-card"));
    assert!(world_html.contains("world-current-exits"));
    assert!(world_html.contains("world-local-actions"));
    assert!(world_html.contains("world-local-npc-talk"));
    assert!(world_html.contains("world-local-skill-practice"));
    assert!(world_html.contains("world-local-task-loop"));
    assert!(world_html.contains("trillionnium_world_local_task_lifecycle_v1"));
    assert!(world_html.contains("trillionnium_world_skill_practice_loop_v1"));
    assert!(world_html.contains("data-command=\"talk_npc\""));
    assert!(world_html.contains("data-practice-command=\"train_skill\""));
    assert!(world_html.contains("data-pickup-command=\"offer_task\""));
    assert!(world_html.contains("data-completion-command=\"complete_task\""));
    assert!(world_html.contains("data-source-of-truth=\"rust_world_contracts_and_completions\""));
    assert!(world_html.contains("world-local-skill-practice-feedback"));
    assert!(world_html.contains("data-source-of-truth=\"rust_mentor_training_validator\""));
    assert!(
        world_html.contains("data-source-of-truth=\"rust_world_map_nodes_and_tactics_commands\"")
    );
    assert!(world_html.contains("window.trillionniumKeyboardMap"));
    assert!(world_html.contains("rust_owned_ui_contract_version"));
    assert!(world_html.contains("server_rendered_then_rust_fragment_swap"));
    assert!(world_html.contains("applyRustOwnedUiFragments"));
    assert!(world_html.contains("Numpad 8/2/4/6"));
    assert!(world_html.contains("world-route-archetype-catalog"));
    assert!(world_html.contains("trillionnium_world_route_archetypes_v1"));
    assert!(world_html.contains("bounty_delivery"));
    assert!(world_html.contains("trillionnium-active-route-line"));
    assert!(world_html.contains("trillionnium-map-pin"));
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
    assert!(world_html.contains("truncated_runtime_bootstrap_with_lazy_delta_hydration"));
    assert!(world_html.contains("data-cache-contract=\"trillionnium_world_map_payload_cache_v1\""));
    assert!(world_html.contains("world-secondary-collapsed"));
    assert!(world_html.contains("data-mobile-ia=\"collapsed_secondary_panel\""));
    assert!(world_html.contains("trillionnium_secondary_dashboard_panels_v1"));
    assert!(world_html.contains("data-secondary-dashboard-role=\"secondary_detail_panel\""));
    assert!(world_html.contains("data-secondary-dashboard-role=\"secondary_counter_drawer\""));
    assert!(world_html.contains("data-secondary-dashboard-role=\"supporting_engine_diagnostics\""));
    assert!(world_html.contains("data-main-experience=\"false\""));
    assert!(world_html.contains("data-default-state=\"collapsed_on_mobile\""));
    assert!(world_html.contains("data-primary-loop-anchor=\"trillionnium-tactics-game-shell\""));
    assert!(
        world_html
            .matches(
                "data-secondary-dashboard-contract=\"trillionnium_secondary_dashboard_panels_v1\""
            )
            .count()
            >= 10
    );
    assert!(world_html.contains("buildViewportDeltaUrl"));
    assert!(world_html.contains("postMapRumSample"));
    assert!(world_html.contains("/world/web/map-rum"));
    assert!(!world_html.contains("leafletMap"));
    assert!(world_html.contains("renderRouteLine"));
    assert!(world_html.contains("renderTileFrame"));
    assert!(world_html.contains("renderEventPulse"));
    assert!(world_html.contains("renderPlayerAvatar"));
    assert!(world_html.contains("renderMovingAvatar"));
    assert!(world_html.contains("trillionnium_open_source_tactics_world_shell_v1"));
    assert!(
        world_html.contains("data-tactics-board-contract=\"trillionnium_world_tactics_board_v1\"")
    );
    assert!(
        world_html.contains("data-trillionnium-character-contract=\"trillionnium_character_v1\"")
    );
    assert!(world_html.contains("trillionnium_world_tactics_unit_v1"));
    assert!(world_html.contains("trillionnium_world_tactics_command_v1"));
    assert!(world_html.contains("trillionnium_skill_v1"));
    assert!(world_html.contains("trillionnium_training_command_v1"));
    assert!(world_html.contains("trillionnium_sect_v1"));
    assert!(world_html.contains("trillionnium_npc_v1"));
    assert!(world_html.contains("trillionnium_sect_osm_binding_v1"));
    assert!(world_html.contains("trillionnium_npc_spawn_anchor_v1"));
    assert!(world_html.contains("trillionnium_npc_command_descriptor_v1"));
    assert!(world_html.contains("trillionnium_mentor_training_task_v1"));
    assert!(world_html.contains("trillionnium_task_archetype_v1"));
    assert!(world_html.contains("trillionnium_task_completion_v1"));
    assert!(world_html.contains("trillionnium_reward_gate_v1"));
    assert!(world_html.contains("trillionnium-task-candidates"));
    assert!(world_html.contains("trillionnium-full-content-alignment"));
    assert!(world_html.contains("trillionnium_hero_tan_full_content_alignment_v1"));
    assert!(world_html.contains("data-full-content-alignment-contract="));
    assert!(world_html.contains("data-thresholds-green=\"true\""));
    assert!(world_html.contains(
        "data-clean-room-content-scale-contract=\"trillionnium_clean_room_content_scale_v1\""
    ));
    assert!(world_html.contains("data-clean-room-scale-status=\"clean_room_scale_scaffold_green\""));
    assert!(world_html.contains(
        "data-forbidden-intermediate=\"no_full_hero_tan_replica_then_replace_workflow\""
    ));
    assert!(world_html.contains("rust_trillionnium_full_content_volume_alignment_gate"));
    assert!(world_html.contains("trillionnium_native_no_copied_hero_tan_text_assets_or_tables"));
    assert!(world_html.contains("data-content-domain=\"items_and_equipment\""));
    assert!(world_html.contains("data-domain-status=\"rust_runtime_backed\""));
    assert!(world_html.contains("id=\"trillionnium-equipment\""));
    assert!(world_html.contains(
        "data-item-equipment-runtime-contract=\"trillionnium_world_item_equipment_runtime_v1\""
    ));
    assert!(world_html.contains("rust_trillionnium_item_equipment_runtime_state"));
    assert!(world_html.contains("class=\"trillionnium-equipment-form\""));
    assert!(world_html.contains("name=\"command\" value=\"equip_item\""));
    assert!(world_html.contains("name=\"target_slot\""));
    assert!(world_html.contains("data-content-domain=\"survival_time_resource_pressure\""));
    assert!(world_html.contains("data-content-domain=\"food_water_age_survival\""));
    assert!(world_html.contains("data-content-domain=\"npc_social_relationships\""));
    assert!(world_html.contains("data-content-domain=\"combat_numerics\""));
    assert!(world_html.contains(
        "data-combat-numerics-runtime-contract=\"trillionnium_world_combat_numerics_runtime_v1\""
    ));
    assert!(world_html.contains("trillionnium-combat-numerics-panel"));
    assert!(world_html.contains("rust_trillionnium_combat_numerics_runtime_state"));
    assert!(world_html.contains("world_state.world_trillionnium_characters.combat_numerics_state"));
    assert!(world_html
        .contains("data-runtime-status=\"rust_owned_hp_energy_guard_focus_hitcrit_live\""));
    assert!(world_html.contains("data-resource-pressure-runtime-contract=\"trillionnium_world_resource_pressure_runtime_v1\""));
    assert!(world_html.contains("trillionnium-resource-pressure-panel"));
    assert!(world_html.contains("rust_trillionnium_resource_pressure_runtime_state"));
    assert!(
        world_html.contains("world_state.world_trillionnium_characters.resource_pressure_state")
    );
    assert!(world_html.contains(
        "data-runtime-status=\"rust_owned_time_stamina_injury_evidence_food_water_age_live\""
    ));
    assert!(world_html.contains(
        "data-survival-runtime-contract=\"trillionnium_world_food_water_age_survival_v1\""
    ));
    assert!(world_html.contains("id=\"trillionnium-food-water-age-survival\""));
    assert!(world_html.contains("rust_trillionnium_food_water_age_survival_state"));
    assert!(world_html.contains(
        "world_state.world_trillionnium_characters.resource_pressure_state.food_water_age"
    ));
    assert!(world_html.contains("data-food-status=\"fed\""));
    assert!(world_html.contains("data-water-status=\"hydrated\""));
    assert!(world_html.contains("data-age-stage=\"young_adult\""));
    assert!(world_html.contains(
        "data-dynamic-social-simulation-contract=\"trillionnium_world_dynamic_social_simulation_v1\""
    ));
    assert!(world_html.contains("id=\"trillionnium-dynamic-social-simulation\""));
    assert!(world_html.contains("rust_world_relationships_dynamic_social_state"));
    assert!(world_html.contains("world_state.world_relationships"));
    assert!(world_html.contains("data-society-phase=\"watchful_city_society\""));
    assert!(world_html.contains("data-web-role=\"visualization_input_only\""));
    assert!(world_html.contains(
        "data-authored-quest-chain-contract=\"trillionnium_world_authored_quest_chain_v1\""
    ));
    assert!(world_html.contains("id=\"trillionnium-authored-quest-chains\""));
    assert!(world_html.contains("rust_trillionnium_authored_quest_chain_catalog"));
    assert!(world_html.contains("world_state.world_map_nodes.exits"));
    assert!(world_html.contains(
        "world_state.world_trillionnium_characters.resource_pressure_state.food_water_age"
    ));
    assert!(world_html.contains("data-content-domain=\"authored_quest_chains\""));
    assert!(world_html.contains("data-content-domain=\"story_arcs\""));
    assert!(world_html.contains("data-region-story-unlock-runtime-contract=\"trillionnium_world_region_story_unlock_runtime_v1\""));
    assert!(world_html.contains("trillionnium-region-story-unlock-panel"));
    assert!(world_html.contains("rust_trillionnium_region_story_unlock_runtime_state"));
    assert!(
        world_html.contains("world_state.world_trillionnium_characters.region_story_unlock_state")
    );
    assert!(world_html
        .contains("data-runtime-status=\"rust_owned_region_graph_story_arc_unlocks_live\""));
    assert!(world_html.contains("data-ledger-reward-requires-settlement=\"true\""));
    assert!(world_html.contains("data-review-hold-gate-enforced=\"true\""));
    assert!(world_html.contains("data-anti-cheese-gate-enforced=\"true\""));
    assert!(world_html.contains("rust_trillionnium_task_completion_handler"));
    assert!(world_html.contains("trillionnium_battle_log_style_v1"));
    assert!(world_html.contains("trillionnium_combat_log_v1"));
    assert!(world_html.contains("trillionnium_npc_relationship_v1"));
    assert!(world_html.contains("trillionnium_osm_objective_v1"));
    assert!(world_html.contains("trillionnium_tactics_combat_resolution_v1"));
    assert!(world_html.contains("trillionnium_tactics_game_session_v1"));
    assert!(world_html.contains("trillionnium_tactics_simulation_tick_v1"));
    assert!(world_html.contains("trillionnium_tactics_reward_settlement_v1"));
    assert!(world_html.contains(
        "data-tactics-reward-settlement-contract=\"trillionnium_tactics_reward_settlement_v1\""
    ));
    assert!(world_html.contains("trillionnium_map_overlay_identity_v1"));
    assert!(world_html.contains("trillionnium_world_objective_travel_v1"));
    assert!(world_html.contains("trillionnium_world_skill_practice_loop_v1"));
    assert!(world_html.contains(
        "data-world-objective-travel-contract=\"trillionnium_world_objective_travel_v1\""
    ));
    assert!(world_html.contains("id=\"world-objective-travel\""));
    assert!(world_html.contains("data-source-of-truth=\"rust_world_graph_objective_travel\""));
    assert!(world_html.contains("data-objective-travel-role="));
    assert!(world_html.contains("world-objective-party-member"));
    assert!(world_html.contains("trillionnium-tactics-session-state"));
    assert!(world_html.contains("data-objective-progress=\"0\""));
    assert!(world_html.contains("data-victory-state=\"active\""));
    assert!(world_html.contains("data-reward-status=\"not_eligible\""));
    assert!(world_html.contains("world-tactics-player-hud"));
    assert!(world_html.contains("trillionnium_tactics_player_visible_surface_v1"));
    assert!(world_html.contains("world-tactics-objective-card"));
    assert!(world_html.contains("world-tactics-current-session-card"));
    assert!(world_html.contains("world-tactics-command-draft-panel"));
    assert!(world_html.contains("world-tactics-command-draft-form"));
    assert!(world_html.contains("world-tactics-reward-history-handoff"));
    assert!(world_html.contains("world-tactics-repeat-farming-copy"));
    assert!(world_html.contains("data-source-of-truth=\"rust_world_tactics_sessions\""));
    assert!(world_html.contains("data-source-of-truth=\"rust_tactics_command_model\""));
    assert!(world_html.contains("data-source-of-truth=\"rust_world_tactics_command_handler\""));
    assert!(world_html.contains("data-web-role=\"visualization_input_only\""));
    assert!(world_html.contains("data-web-role=\"intent_only_visualization_input\""));
    assert!(world_html.contains("trillionnium_tactics_board_cell_interaction_v1"));
    assert!(world_html.contains("trillionnium_tactics_unit_selection_v1"));
    assert!(world_html.contains("trillionnium_tactics_command_intent_draft_v1"));
    assert!(world_html.contains("trillionnium_tactics_accessibility_v1"));
    assert!(world_html.contains("data-keyboard-traversal=\"roving_grid_focus\""));
    assert!(world_html.contains("data-low-motion-support=\"prefers_reduced_motion\""));
    assert!(world_html.contains("world-tactics-keyboard-help"));
    assert!(world_html.contains("aria-live=\"polite\""));
    assert!(world_html.contains("role=\"gridcell\""));
    assert!(world_html.contains("data-roving-tabindex=\"tactics_board\""));
    assert!(world_html.contains("aria-rowindex=\"1\""));
    assert!(world_html.contains("aria-colindex=\"1\""));
    assert!(world_html.contains("focusAdjacentTile"));
    assert!(world_html.contains("data-draft-target-tile="));
    assert!(world_html.contains("data-draft-unit-id="));
    assert!(world_html.contains("data-draft-command="));
    assert!(world_html.contains("name=\"unit_id\""));
    assert!(world_html.contains("name=\"target_tile\""));
    assert!(world_html.contains("initializeTacticsIntentDraft"));
    assert!(world_html.contains("window.trillionniumTacticsIntentDraft"));
    assert!(world_html
        .contains("data-reward-history-contract=\"trillionnium_tactics_reward_history_v1\""));
    assert!(world_html.contains(
        "data-anti-cheese-contract=\"trillionnium_tactics_repeat_farming_anti_cheese_v1\""
    ));
    assert!(world_html.contains("Repeat-farming guard"));
    assert!(world_html
        .contains("data-map-overlay-identity-contract=\"trillionnium_map_overlay_identity_v1\""));
    assert!(world_html.contains("data-objective-contract=\"trillionnium_osm_objective_v1\""));
    assert!(
        world_html.contains("data-npc-relationship-contract=\"trillionnium_npc_relationship_v1\"")
    );
    assert!(world_html.contains("rust_trillionnium_osm_objective_generator"));
    assert!(world_html.contains("trillionnium_native_combat_task_templates_v1"));
    assert!(world_html.contains("native_templates_only_no_verbatim_source_reference_strings"));
    assert!(world_html.contains("镜城风从巷口压低"));
    assert!(world_html.contains("/world/web/tactics-command"));
    assert!(world_html.contains("/v1/world/tactics/command"));
    assert!(world_html.contains("rust_mentor_training_validator"));
    assert!(world_html.contains("rust_mentor_training_command_model"));
    assert!(world_html.contains("rust_trillionnium_sect_model"));
    assert!(world_html.contains("rust_trillionnium_npc_model"));
    assert!(world_html.contains("rust_trillionnium_npc_interaction_validator"));
    assert!(world_html.contains("rust_trillionnium_task_offer_validator"));
    assert!(world_html.contains("导师修炼"));
    assert!(world_html.contains("npc-street-compass-sifu"));
    assert!(world_html.contains("name=\"npc_id\""));
    assert!(world_html.contains("name=\"task_archetype_id\""));
    assert!(world_html.contains("talk_npc"));
    assert!(world_html.contains("offer_task"));
    assert!(world_html.contains("complete_task"));
    assert!(world_html.contains("courier_letter"));
    assert!(world_html.contains("data-source-of-truth=\"rust_trillionnium_game_state\""));
    assert!(world_html.contains("data-interface-style=\"turn_based_strategy_rpg\""));
    assert!(world_html.contains("data-open-source-base=\"tranchikhang/MedievalWar\""));
    assert!(world_html.contains("data-base-license=\"MIT\""));
    assert!(world_html.contains("data-base-engine=\"Phaser 3\""));
    assert!(world_html.contains("data-base-patterns=\"map,cursor,control,turn_system,pathfinding,context_menu,objectives,ai\""));
    assert!(world_html.contains("data-map-engine-role=\"openclawstreetmap_underlay\""));
    assert!(world_html.contains("OpenClawStreetMap"));
    assert!(world_html.contains("三国魔改界面"));
    assert!(world_html.contains("战棋指令菜单"));
    assert!(world_html.contains("data-source-of-truth=\"rust_tactics_board_projection\""));
    assert!(world_html.contains("data-source-of-truth=\"rust_trillionnium_character\""));
    assert!(world_html.contains("data-source-of-truth=\"rust_tactics_command_model\""));
    assert!(world_html.contains("data-validation-owner=\"rust_tactics_combat_handler\""));
    assert!(world_html.contains("data-required-skill-id=\"basic_inner_power\""));
    assert!(world_html.contains("发起攻击"));
    assert!(world_html.contains("结束回合"));
    assert!(
        world_html.contains("data-completion-owner=\"rust_command_handler_ledger_progression\"")
    );
    assert!(world_html.contains("rust_trillionnium_combat_log_generator"));
    assert!(world_html.contains("真实街格只提供锚点"));
    assert!(world_html.contains("data-openclawstreetmap-role=\"supporting_engine_diagnostics\""));
    assert!(world_html.contains("支撑层，不是主界面"));
    assert!(world_html.contains("world-openstreetmap-geodata"));
    assert!(world_html.contains("openstreetmap_geodata_v1"));
    assert!(world_html.contains("OpenStreetMapDataProvider"));
    assert!(world_html.contains("fixture_openstreetmap_data_provider_v1"));
    assert!(world_html.contains("stable_fixture_table"));
    assert!(world_html.contains("data-source-of-truth=\"rust_openstreetmap_data_provider\""));
    assert!(world_html.contains("data-web-role=\"visualization_input_only\""));
    assert!(world_html.contains("osm_id"));
    assert!(world_html.contains("osm_type"));
    assert!(world_html.contains("game_overlay_id"));
    assert!(world_html.contains("odbl_database_obligations"));
    assert!(world_html.contains("no_live_overpass"));
    assert!(world_html.contains("world-openstreetmap-provider-readiness"));
    assert!(world_html.contains("openstreetmap_provider_readiness_v1"));
    assert!(world_html.contains("fixture_ready_live_fail_closed"));
    assert!(world_html.contains("data-fixture-mode-green=\"true\""));
    assert!(world_html.contains("data-live-modes-fail-closed=\"true\""));
    assert!(world_html.contains("data-live-network-ingestion-enabled=\"false\""));
    assert!(world_html.contains("data-production-ingestion-enabled=\"false\""));
    assert!(world_html.contains("overpass_bbox_cache"));
    assert!(world_html.contains("geofabrik_extract_import"));
    assert!(world_html.contains("vendor_tile_cache"));
    assert!(world_html.contains("world-openstreetmap-geodata-freshness"));
    assert!(world_html.contains("openstreetmap_geodata_freshness_v1"));
    assert!(world_html.contains("fixture_static_fresh_live_stale_blocked"));
    assert!(world_html.contains("data-fixture-static-snapshot=\"true\""));
    assert!(world_html.contains("data-wall-clock-freshness-applies=\"false\""));
    assert!(world_html.contains("data-live-data-freshness-applies=\"false\""));
    assert!(world_html.contains("data-staleness-gate-green=\"true\""));
    assert!(world_html.contains("data-stale-live-ingestion-blocked=\"true\""));
    assert!(world_html.contains("data-fixture-snapshot-age-seconds=\"0\""));
    assert!(world_html.contains("world-openstreetmap-attribution"));
    assert!(world_html.contains("openstreetmap_attribution_presence_v1"));
    assert!(world_html.contains("© OpenStreetMap contributors"));
    assert!(world_html.contains("ODbL-1.0"));
    assert!(world_html.contains("data-attribution-visible=\"true\""));
    assert!(world_html.contains("data-derived-database-tracking-required=\"true\""));
    assert!(world_html.contains("Trillionnium World Map"));
    assert!(world_html.contains("OpenStreetMap upgraded into a playable world"));
    assert!(world_html.contains("Player avatars / 跑图角色"));
    assert!(world_html.contains("Task routes / 任务路线"));
    assert!(world_html.contains("Avatar Task Routes"));
    assert!(world_html.contains("Avatar Movement"));
    assert!(world_html.contains("data-overlay-target=\"avatars\""));
    assert!(world_html.contains("data-overlay-target=\"taskRoutes\""));
    assert!(world_html.contains("data-overlay-target=\"routeRunners\""));
    assert!(world_html.contains("world-avatar-task-routes-live"));
    assert!(world_html.contains("world-avatar-route-runners-live"));
    assert!(world_html.contains("trillionnium_world_rust_live_task_ui_fragments_v1"));
    assert!(world_html.contains("rust_owned_live_task_ui_fragments"));
    assert!(world_html
        .contains("server_rendered_live_event_and_task_route_cards_selected_by_focus_bridge"));
    assert!(world_html.contains(
        "data-rust-live-task-ui-contract=\"trillionnium_world_rust_live_task_ui_fragments_v1\""
    ));
    assert!(world_html.contains("const renderRustLiveTaskCards ="));
    assert!(world_html.contains(
        "renderRustLiveTaskCards(liveEventTarget, taskRouteTarget, lastViewport, focus)"
    ));
    assert!(world_html.contains("live_event_cards\":\"rust_rendered"));
    assert!(world_html.contains("avatar_task_route_cards\":\"rust_rendered"));
    assert!(world_html.contains("trillionnium_world_rust_map_popup_ui_fragments_v1"));
    assert!(world_html.contains("rust_owned_map_popup_ui_fragments"));
    assert!(world_html.contains(
        "data-rust-map-popup-ui-contract=\"trillionnium_world_rust_map_popup_ui_fragments_v1\""
    ));
    assert!(world_html.contains("server_rendered_map_popups_bound_by_browser_adapter"));
    assert!(world_html.contains("const rustPoiMarkerPopupHtml ="));
    assert!(world_html.contains("const rustRouteRunnerPopupHtml ="));
    assert!(world_html.contains("const rustPlayerAvatarPopupHtml ="));
    assert!(world_html.contains("poi_marker_popups\":\"rust_rendered"));
    assert!(world_html.contains("avatar_route_runner_popups\":\"rust_rendered"));
    assert!(world_html.contains("player_avatar_popups\":\"rust_rendered"));
    assert!(world_html.contains("trillionnium_world_rust_map_support_ui_fragments_v1"));
    assert!(world_html.contains("rust_owned_map_support_ui_fragments"));
    assert!(world_html.contains(
        "data-rust-map-support-ui-contract=\"trillionnium_world_rust_map_support_ui_fragments_v1\""
    ));
    assert!(world_html.contains("server_rendered_map_support_cards_selected_by_viewport_bridge"));
    assert!(world_html.contains("const renderRustMapSupportCards ="));
    assert!(world_html.contains("marker_cluster_cards_html"));
    assert!(world_html.contains("support_cards\":\"rust_rendered"));
    assert!(world_html.contains("trillionnium_world_rust_map_focus_ui_fragments_v1"));
    assert!(world_html.contains("rust_owned_map_focus_ui_fragments"));
    assert!(world_html.contains(
        "data-rust-map-focus-ui-contract=\"trillionnium_world_rust_map_focus_ui_fragments_v1\""
    ));
    assert!(world_html.contains("server_rendered_map_focus_action_rail_selected_by_focus_bridge"));
    assert!(world_html.contains("const applyRustMapFocusUiFragment ="));
    assert!(world_html.contains("const rustMapFocusUiFragmentForFocus ="));
    assert!(world_html.contains("focus_action_rail\":\"rust_rendered"));
    assert!(world_html.contains("trillionnium_world_rust_route_runner_ui_fragments_v1"));
    assert!(world_html.contains("rust_owned_route_runner_ui_fragments"));
    assert!(world_html.contains("server_rendered_runner_cards_selected_by_focus_bridge"));
    assert!(world_html.contains("data-rust-route-runner-ui-contract=\"trillionnium_world_rust_route_runner_ui_fragments_v1\""));
    assert!(world_html.contains("const renderRustRouteRunnerCards ="));
    assert!(
        world_html.contains("renderRustRouteRunnerCards(routeRunnerTarget, lastViewport, focus)")
    );
    assert!(world_html.contains("route_runner_cards\":\"rust_rendered"));
    assert!(world_html.contains("reward_claim_action_buttons\":\"rust_rendered"));
    assert!(world_html.contains("next_route_action_buttons\":\"rust_rendered"));
    assert!(world_html.contains("world-route-runner-handoff-summary"));
    assert!(world_html.contains("Route runner handoff:"));
    assert!(world_html.contains("data-next-route-status="));
    assert!(world_html.contains("data-reward-claim-count="));
    assert!(world_html.contains("renderRouteRunnerHandoffSummary"));
    assert!(world_html.contains("avatar_task_routes"));
    assert!(world_html.contains("avatar_route_runners"));
    assert!(world_html.contains("runner_trace_points"));
    assert!(world_html.contains("remaining_distance_meters"));
    assert!(world_html.contains("eta_label"));
    assert!(world_html.contains("reward_checkpoint"));
    assert!(world_html.contains("reward_claim_action_body"));
    assert!(world_html.contains("next_route_action_body"));
    assert!(world_html.contains("next_route_sequence_summary"));
    assert!(world_html.contains("completion_command"));
    assert!(world_html.contains("completion_action_body"));
    assert!(world_html.contains("checkpoint_history"));
    assert!(world_html.contains("checkpoint_history_summary"));
    assert!(world_html.contains("reward_history_summary"));
    assert!(world_html.contains("trillionnium_route_mastery_v1"));
    assert!(world_html.contains("route_mastery_xp"));
    assert!(world_html.contains("routeRunnerMasteryChipsHtml"));
    assert!(world_html.contains("data-route-mastery-tier"));
    assert!(world_html.contains("dataset.routeMasteryContract"));
    assert!(world_html.contains("routeRunnerHistoryChipsHtml"));
    assert!(world_html.contains("buildRouteRunnerRewardClaimAction"));
    assert!(world_html.contains("routeRunnerRewardClaimButtonHtml"));
    assert!(world_html.contains("trillionnium-reward-claim-action"));
    assert!(world_html.contains("Prepare reward claim"));
    assert!(world_html.contains("buildRouteRunnerNextRouteAction"));
    assert!(world_html.contains("routeRunnerNextRouteButtonHtml"));
    assert!(world_html.contains("trillionnium-next-route-action"));
    assert!(world_html.contains("Open next route"));
    assert!(world_html.contains("Preview next route"));
    assert!(world_html.contains("Checkpoint history"));
    assert!(world_html.contains("Reward history"));
    assert!(world_html.contains("agent_party"));
    assert!(world_html.contains("agent_party_summary"));
    assert!(world_html.contains("agentPartyChipsHtml"));
    assert!(world_html.contains("buildAgentPartyAction"));
    assert!(world_html.contains("agentPartyActionButtonsHtml"));
    assert!(world_html.contains("trillionnium-agent-party-action"));
    assert!(world_html.contains("Agent party"));
    assert!(world_html.contains("Agent handoff"));
    assert!(world_html.contains("Oracle Scout"));
    assert!(world_html.contains("Scout route"));
    assert!(world_html.contains("Build deliverable"));
    assert!(world_html.contains("Audit risk"));
    assert!(world_html.contains("Close reward"));
    assert!(world_html.contains("buildRouteRunnerCompletionAction"));
    assert!(world_html.contains("routeRunnerCompletionButtonHtml"));
    assert!(world_html.contains("Complete checkpoint"));
    assert!(world_html.contains("trillionnium-route-flow-action"));
    assert!(world_html.contains("filterAvatarTaskRoutes"));
    assert!(world_html.contains("filterAvatarRouteRunners"));
    assert!(world_html.contains("trillionnium-avatar-task-route-path"));
    assert!(world_html.contains("trillionnium-avatar-route-runner-dot"));
    assert!(world_html.contains("trillionnium-avatar-route-runner-progress"));
    assert!(world_html.contains("trillionnium-avatar-route-runner-remaining"));
    assert!(world_html.contains("trillionnium-avatar-route-reward-checkpoint"));
    assert!(world_html.contains("trillionnium-route-dash"));
    assert!(world_html.contains("trillionnium-runner-bob"));
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
    assert!(world_html.contains("trillionnium_world_rust_route_ui_fragments_v1"));
    assert!(world_html.contains("rust_owned_route_ui_fragments"));
    assert!(world_html.contains("server_rendered_fragments_selected_by_focus_bridge"));
    assert!(world_html
        .contains("data-rust-route-ui-contract=\"trillionnium_world_rust_route_ui_fragments_v1\""));
    assert!(world_html.contains("data-browser-ui-owner=\"input_only_focus_bridge\""));
    assert!(world_html.contains("const applyRustRouteUiFragments ="));
    assert!(world_html.contains("const rustRouteUiFragmentForFocus ="));
    assert!(world_html.contains("route_task_graph\":\"rust_rendered"));
    assert!(world_html.contains("route_flow_action_rail\":\"rust_rendered"));
    assert!(world_html.contains("route_status_copy\":\"rust_rendered"));
    assert!(world_html.contains("routeFlowActions.innerHTML = fragments.route_flow_actions_html"));
    assert!(!world_html.contains("const actionButtons = routeTaskGraphActionButtonsHtml(task);"));
    assert!(!world_html.contains("routeFlowActions.innerHTML = actions.join(' ');"));
    assert!(world_html
        .contains("status: task.next_opportunity_hint || task.next_opportunity_command || ''"));
    assert!(world_html.contains("((latestWorkItem && latestWorkItem.dataset.workOrderId) || (workItem && workItem.dataset.workOrderId) || '')"));
    assert!(world_html.contains("\"contract_version\":1"));
    assert!(world_html.contains("customer deliverable"));
    assert!(world_html.contains("evidence package, risk controls, next action"));
    assert!(world_html.contains("world-work-cancel-body"));
    assert!(world_html.contains("refund risk controls, next action, and self-review"));
    assert!(world_html.contains("customer deliverable"));
    assert!(world_html.contains("refund risk controls"));
}

#[tokio::test]
async fn world_map_runtime_endpoints_expose_rum_delta_cache_and_mobile_ia_gates() {
    let app = build_router(AppState::new(test_config()));

    let (app_status, app_headers, app_body) =
        send_text_request_with_headers(&app, "GET", "/app", &[]).await;
    assert_eq!(app_status, StatusCode::OK);
    assert_eq!(
        app_headers
            .get("x-trillionnium-cache-contract")
            .and_then(|value| value.to_str().ok()),
        Some("trillionnium_world_map_payload_cache_v1")
    );
    assert!(app_headers
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .contains("stale-while-revalidate"));
    assert!(app_headers
        .get("vary")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .contains("Accept-Encoding"));
    assert!(app_body.contains("truncated_runtime_bootstrap_with_lazy_delta_hydration"));
    assert!(app_body.contains("buildViewportDeltaUrl"));
    assert!(app_body.contains("postMapRumSample"));
    assert!(app_body.contains("AbortController"));
    assert!(app_body.contains("changed_group_deferred_render"));
    assert!(app_body.contains("focus_to_action_rail"));
    assert!(app_body.contains("trillionnium_world_map_rum_slo_v1"));
    assert!(app_body.contains("trillionnium_world_map_real_user_rum_matrix_v1"));
    assert!(
        app_body.contains("cold_cache_interactive,warm_delta_or_304,weak_network_cached_snapshot")
    );
    assert!(app_body.contains("trillionnium_world_map_density_scalability_v1"));
    assert!(app_body.contains("trillionnium_world_map_weak_network_resilience_v1"));
    assert!(app_body.contains("trillionnium_world_map_offline_action_queue_v1"));
    assert!(app_body.contains("trillionnium_world_map_location_privacy_v1"));
    assert!(app_body.contains("trillionnium_world_map_gameplay_accessibility_i18n_v1"));
    assert!(app_body.contains("viewportWeakNetworkCacheKey"));
    assert!(app_body.contains("buildMapLibreShadowParityProbe"));

    let (world_status, world_headers, world_body) =
        send_text_request_with_headers(&app, "GET", "/world", &[]).await;
    assert_eq!(world_status, StatusCode::OK);
    assert_eq!(
        world_headers
            .get("x-trillionnium-resource-contract")
            .and_then(|value| value.to_str().ok()),
        Some("trillionnium_world_map_world_shell_payload_v1")
    );
    assert!(world_body.contains("world-secondary-collapsed"));
    assert!(world_body.contains("data-mobile-ia=\"collapsed_secondary_panel\""));
    assert!(world_body.contains("world-map-rum-slo"));
    assert!(world_body.contains("world-map-weak-network"));
    assert!(world_body.contains("world-map-location-privacy"));
    assert!(world_body
        .contains("data-parity-contract=\"trillionnium_world_map_maplibre_shadow_parity_v1\""));
    assert!(world_body.contains("data-rollback-drill-required=\"true\""));

    let (delta_status, delta_headers, delta_body) = send_text_request_with_headers(
        &app,
        "GET",
        "/world/web/map-delta?lat=31.230400&lng=121.473700&zoom=15&radius_km=4.5&limit=6&cursor=stale-cursor",
        &[],
    )
    .await;
    assert_eq!(delta_status, StatusCode::OK);
    assert!(delta_headers.get("etag").is_some());
    assert!(delta_headers
        .get("server-timing")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .contains("trillionnium-world-map-delta"));
    assert!(delta_headers
        .get("x-trillionnium-world-map-server-ms")
        .is_some());
    assert!(delta_headers
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .contains("max-age=3"));
    let delta: Value = serde_json::from_str(&delta_body).expect("decode delta response");
    assert_eq!(
        delta["contract_version"],
        "trillionnium_world_map_transport_delta_v1"
    );
    assert_eq!(delta["changed"], true);
    assert_eq!(delta["delta_mode"], "entity_group_versioned_delta_v1");
    assert_eq!(
        delta["entity_delta_cache"]["mode"],
        "entity_group_versioned_delta_v1"
    );
    assert!(delta["entity_delta"]["changed_group_count"]
        .as_u64()
        .is_some_and(|count| count > 0));
    assert!(delta["delta"]["avatar_route_runners"].as_array().is_some());
    assert!(delta["delta"]["visible_markers"].as_array().is_some());
    assert!(delta["delta"]["marker_clusters"].as_array().is_some());
    assert!(delta["counts"]["marker_cluster_count"].as_u64().is_some());
    assert_eq!(
        delta["renderer_shadow_parity"]["shadow_engine_id"],
        "maplibre_gl_v1"
    );
    assert_eq!(
        delta["renderer_shadow_parity"]["maplibre_shadow_parity"]["contract_version"],
        "trillionnium_world_map_maplibre_shadow_parity_v1"
    );
    let cursor = delta["next_cursor"].as_str().expect("next cursor");
    let encoded_cursor = cursor
        .replace('%', "%25")
        .replace(';', "%3B")
        .replace('=', "%3D")
        .replace(':', "%3A");
    let (noop_status, noop_headers, noop_body) = send_text_request_with_headers(
        &app,
        "GET",
        &format!("/world/web/map-delta?lat=31.230400&lng=121.473700&zoom=15&radius_km=4.5&limit=6&cursor={encoded_cursor}"),
        &[],
    )
    .await;
    assert_eq!(noop_status, StatusCode::OK);
    let noop_delta: Value = serde_json::from_str(&noop_body).expect("decode noop delta response");
    assert_eq!(noop_delta["changed"], false);
    assert_eq!(noop_delta["snapshot_fallback_required"], true);
    assert_eq!(noop_delta["snapshot_fallback_is_failure"], false);
    assert_eq!(noop_delta["entity_delta"]["changed_group_count"], 0);
    let noop_etag = noop_headers
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .expect("noop etag")
        .to_string();
    let (not_modified_status, not_modified_headers, not_modified_body) =
        send_text_request_with_headers(
            &app,
            "GET",
            &format!("/world/web/map-delta?lat=31.230400&lng=121.473700&zoom=15&radius_km=4.5&limit=6&cursor={encoded_cursor}"),
            &[("if-none-match", noop_etag.as_str())],
        )
        .await;
    assert_eq!(not_modified_status, StatusCode::NOT_MODIFIED);
    assert!(not_modified_body.is_empty());
    assert_eq!(
        not_modified_headers
            .get("x-trillionnium-cache-contract")
            .and_then(|value| value.to_str().ok()),
        Some("trillionnium_world_map_payload_cache_v1")
    );
    let (browser_not_modified_status, _, browser_not_modified_body) = send_text_request_with_headers(
        &app,
        "GET",
        &format!("/world/web/map-delta?lat=31.230400&lng=121.473700&zoom=15&radius_km=4.5&limit=6&cursor={encoded_cursor}"),
        &[("x-trillionnium-map-if-none-match", noop_etag.as_str())],
    )
    .await;
    assert_eq!(browser_not_modified_status, StatusCode::NOT_MODIFIED);
    assert!(browser_not_modified_body.is_empty());

    let (rum_status, rum_body) = send_json_request(
        &app,
        "POST",
        "/world/web/map-rum",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "surface_id": "world-map-shell-panel",
            "session_id": "rum-test-session",
            "sample_kind": "first_map_interactive",
            "user_agent_class": "mobile",
            "viewport_cursor": cursor,
            "first_map_interactive_ms": 1234,
            "viewport_refresh_ms": 88,
            "focus_to_action_rail_ms": 144,
            "main_thread_long_task_ms": 12,
            "tile_error_count": 0
        }),
    )
    .await;
    assert_eq!(rum_status, StatusCode::OK);
    assert_eq!(rum_body["ok"], true);
    assert_eq!(rum_body["session_id"], "rum-test-session");
    assert_eq!(rum_body["metrics"]["sample_count"], 1);
    assert_eq!(rum_body["metrics"]["first_map_interactive_max_ms"], 1234);
    assert_eq!(rum_body["metrics"]["first_map_interactive_p95_ms"], 1234);
    assert_eq!(
        rum_body["metrics"]["slo_gate"]["contract_version"],
        "trillionnium_world_map_rum_slo_v1"
    );
    assert_eq!(rum_body["metrics"]["slo_gate"]["green"], true);

    for surface in ["app-map-shell-panel", "world-map-shell-panel"] {
        for device in ["mobile", "desktop"] {
            for sample_kind in [
                "first_map_interactive_runtime_ready",
                "delta_not_modified_304_fast_path",
                "weak_network_cached_snapshot",
            ] {
                if surface == "world-map-shell-panel"
                    && device == "mobile"
                    && sample_kind == "first_map_interactive_runtime_ready"
                {
                    continue;
                }
                let mut payload = json!({
                    "matrix_user_id": "@alice:local.dev",
                    "surface_id": surface,
                    "session_id": "rum-test-session-matrix",
                    "sample_kind": sample_kind,
                    "user_agent_class": device,
                    "viewport_cursor": cursor,
                    "tile_error_count": 0
                });
                if sample_kind.starts_with("first_map_interactive") {
                    payload["first_map_interactive_ms"] = json!(140);
                } else {
                    payload["viewport_refresh_ms"] = json!(24);
                }
                let (status, body) =
                    send_json_request(&app, "POST", "/world/web/map-rum", &[], payload).await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body["ok"], true);
            }
        }
    }

    let (metrics_status, metrics_body) = send_metrics_request(&app).await;
    assert_eq!(metrics_status, StatusCode::OK);
    assert!(metrics_body.contains("cex_consumer_entry_trillionnium_world_map_rum_samples_total 12"));
    assert!(
        metrics_body.contains("cex_consumer_entry_trillionnium_world_map_delta_requests_total 4")
    );
    assert!(metrics_body
        .contains("cex_consumer_entry_trillionnium_world_map_delta_noop_responses_total 3"));
    assert!(metrics_body.contains("cex_consumer_entry_trillionnium_world_map_rum_slo_gate_green 1"));
    assert!(metrics_body
        .contains("cex_consumer_entry_trillionnium_world_map_rum_slo_raw_split_green 1"));
    assert!(
        metrics_body.contains("cex_consumer_entry_trillionnium_world_map_rum_slo_sample_count 12")
    );
    assert!(metrics_body.contains("cex_consumer_entry_trillionnium_world_map_rum_slo_warming 1"));
    assert!(metrics_body
        .contains("cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_gate_green 1"));
    assert!(metrics_body
        .contains("cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_raw_green 1"));
    assert!(metrics_body
        .contains("cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_coverage_count 12"));
    assert!(metrics_body.contains(
        "cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_missing_bucket_count 0"
    ));
    assert!(
        metrics_body.contains("cex_consumer_entry_trillionnium_world_map_delta_cache_gate_green 1")
    );
    assert!(metrics_body
        .contains("cex_consumer_entry_trillionnium_world_map_runtime_safety_gate_green 1"));
    assert!(metrics_body
        .contains("cex_consumer_entry_trillionnium_world_map_location_privacy_gate_green 1"));
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
    assert_eq!(feed.get("source_count").and_then(Value::as_u64), Some(7));
    assert!(feed
        .get("sources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .any(|source| source == "route_runner_handoff"));
    assert_eq!(
        feed.pointer("/route_runner_handoff/contract_version")
            .and_then(Value::as_str),
        Some("trillionnium_route_runner_handoff_v1")
    );
    assert!(
        feed.pointer("/route_runner_handoff/next_route_action_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 1
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
fn latest_contract_prefers_completable_contract_over_newer_terminal_contract() {
    let matrix_user_id = "@world-latest-contract-actor:local.dev";
    let mut league = default_league_state();
    league.world.world_contracts.push(WorldContract {
        contract_id: "world-contract-open-latest-guard".to_string(),
        event_id: "world-event-open-latest-guard".to_string(),
        actor_matrix_user_id: matrix_user_id.to_string(),
        location_id: "starter-studio".to_string(),
        task_id: "task-open-latest-guard".to_string(),
        title: "Open latest guard contract".to_string(),
        body: "Open contract should remain the default completion target.".to_string(),
        status: "open".to_string(),
        cex_status: Some("Running".to_string()),
        value_score: 44,
        created_at_epoch: 1_777_902_001,
    });
    league.world.world_contracts.push(WorldContract {
        contract_id: "world-contract-completed-latest-guard".to_string(),
        event_id: "world-event-completed-latest-guard".to_string(),
        actor_matrix_user_id: matrix_user_id.to_string(),
        location_id: "starter-studio".to_string(),
        task_id: "task-completed-latest-guard".to_string(),
        title: "Completed latest guard contract".to_string(),
        body: "Newer terminal contract must not shadow the completable one.".to_string(),
        status: "completed_settled".to_string(),
        cex_status: Some("completed".to_string()),
        value_score: 88,
        created_at_epoch: 1_777_902_002,
    });

    let indexes = build_world_indexes(&league.world);
    let latest_contract = indexes
        .latest_contract_index_for_actor(matrix_user_id)
        .and_then(|index| league.world.world_contracts.get(index))
        .expect("latest contract");
    assert_eq!(
        latest_contract.contract_id, "world-contract-open-latest-guard",
        "web/default latest completion should not be shadowed by a terminal contract"
    );
}

#[tokio::test]
async fn world_web_work_action_defaults_use_actionable_work_order_ids() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    {
        let mut league = state.inner.league_state.lock().await;
        let mut push_work = |work_order_id: &str,
                             buyer_matrix_user_id: &str,
                             seller_matrix_user_id: &str,
                             status: &str,
                             created_at_epoch: i64| {
            league.world.world_work_orders.push(WorldWorkOrder {
                work_order_id: work_order_id.to_string(),
                purchase_id: format!("purchase-{work_order_id}"),
                listing_id: format!("listing-{work_order_id}"),
                buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
                seller_matrix_user_id: seller_matrix_user_id.to_string(),
                company_id: format!("company-{work_order_id}"),
                status: status.to_string(),
                brief: format!("{status} work order for web action default routing"),
                value_score: 80,
                created_at_epoch,
            });
        };
        push_work(
            "work-deliverable-web-default",
            "@buyer-deliverable-web-default:local.dev",
            "@alice:local.dev",
            "open",
            1_777_903_001,
        );
        push_work(
            "work-acceptable-web-default",
            "@alice:local.dev",
            "@seller-acceptable-web-default:local.dev",
            "delivered",
            1_777_903_002,
        );
        push_work(
            "work-rejectable-web-default",
            "@alice:local.dev",
            "@seller-rejectable-web-default:local.dev",
            "rejected_chargeback_failed",
            1_777_903_003,
        );
        push_work(
            "work-reopenable-web-default",
            "@alice:local.dev",
            "@seller-reopenable-web-default:local.dev",
            "rejected_refunded",
            1_777_903_004,
        );
        push_work(
            "work-cancellable-web-default",
            "@alice:local.dev",
            "@seller-cancellable-web-default:local.dev",
            "cancelled_refund_failed",
            1_777_903_005,
        );
        push_work(
            "work-terminal-web-default",
            "@alice:local.dev",
            "@seller-terminal-web-default:local.dev",
            "completed",
            1_777_903_006,
        );
    }

    let world_html = get_world_web_shell(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::extract::Query(HashMap::new()),
    )
    .await
    .0;

    assert!(world_html.contains(
        "id=\"world-work-deliver-id\" name=\"work_order_id\" value=\"work-deliverable-web-default\""
    ));
    assert!(world_html.contains(
        "id=\"world-work-accept-id\" name=\"work_order_id\" value=\"work-acceptable-web-default\""
    ));
    assert!(world_html.contains(
        "id=\"world-work-reject-id\" name=\"work_order_id\" value=\"work-rejectable-web-default\""
    ));
    assert!(world_html.contains(
        "id=\"world-work-reopen-id\" name=\"work_order_id\" value=\"work-reopenable-web-default\""
    ));
    assert!(world_html.contains(
        "id=\"world-work-cancel-id\" name=\"work_order_id\" value=\"work-cancellable-web-default\""
    ));
    for form_id in [
        "world-work-deliver-id",
        "world-work-accept-id",
        "world-work-reject-id",
        "world-work-reopen-id",
        "world-work-cancel-id",
    ] {
        assert!(
            !world_html.contains(&format!(
                "id=\"{form_id}\" name=\"work_order_id\" value=\"work-terminal-web-default\""
            )),
            "{form_id} should not default to a terminal work order"
        );
    }
}

#[tokio::test]
async fn world_buy_latest_prefers_other_players_listed_bounty_before_self_listing() {
    let state = test_state(test_config(), IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_listings.push(WorldListing {
            listing_id: "listing-other-buyable-default".to_string(),
            shop_id: "shop-other-buyable-default".to_string(),
            company_id: "company-other-buyable-default".to_string(),
            owner_matrix_user_id: "@seller-buyable-default:local.dev".to_string(),
            asset_id: "asset-other-buyable-default".to_string(),
            title: "Other player bounty should be accepted first".to_string(),
            listing_kind: "service_offer".to_string(),
            status: "listed".to_string(),
            price_credits: 60,
            quality_score: 70,
            created_at_epoch: 1_777_903_010,
        });
        league.world.world_listings.push(WorldListing {
            listing_id: "listing-self-newer-default".to_string(),
            shop_id: "shop-self-newer-default".to_string(),
            company_id: "company-self-newer-default".to_string(),
            owner_matrix_user_id: "@alice:local.dev".to_string(),
            asset_id: "asset-self-newer-default".to_string(),
            title: "Newer self bounty should not hijack accept latest".to_string(),
            listing_kind: "service_offer".to_string(),
            status: "listed".to_string(),
            price_credits: 80,
            quality_score: 85,
            created_at_epoch: 1_777_903_020,
        });
    }

    let world_html = get_world_web_shell(
        axum::extract::State(state.clone()),
        HeaderMap::new(),
        axum::extract::Query(HashMap::new()),
    )
    .await
    .0;
    assert!(world_html.contains(
        "id=\"world-buy-listing-id\" name=\"listing_id\" value=\"listing-other-buyable-default\""
    ));
    assert!(!world_html.contains(
        "id=\"world-buy-listing-id\" name=\"listing_id\" value=\"listing-self-newer-default\""
    ));

    let (status, buy) = send_json_request(
        &app,
        "POST",
        "/v1/world/listings/latest/buy",
        &[],
        json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!buyable-default:local.dev",
            "body": "Accept latest should pick another player's bounty when available: deliverable, evidence, rating standard, risk controls, next action, and self-review."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "buy latest response: {buy}");
    assert_eq!(
        buy["listing"]["listing_id"],
        "listing-other-buyable-default"
    );
    assert_eq!(
        buy["work_order"]["seller_matrix_user_id"],
        "@seller-buyable-default:local.dev"
    );
    assert_eq!(
        buy["work_order"]["buyer_matrix_user_id"],
        "@alice:local.dev"
    );
}

#[test]
fn world_route_recovery_opportunities_prioritize_settlement_retry_over_unavailable_next_steps() {
    let mut league = default_league_state();
    league.world.world_purchases.push(WorldPurchase {
        purchase_id: "purchase-rejection-recovery-route".to_string(),
        listing_id: "listing-rejection-recovery-route".to_string(),
        shop_id: "shop-rejection-recovery-route".to_string(),
        company_id: "company-rejection-recovery-route".to_string(),
        buyer_matrix_user_id: "@route-recovery-buyer:local.dev".to_string(),
        seller_matrix_user_id: "@route-recovery-seller:local.dev".to_string(),
        price_credits: 70,
        status: "rejected_chargeback_failed".to_string(),
        ledger_status: Some("seller_chargeback_failed".to_string()),
        ledger_account_id: Some("seller-recovery-account".to_string()),
        ledger_entry_id: Some("seller-original-settlement".to_string()),
        ledger_balance_after: Some(63.0),
        ledger_error: Some("insufficient reserved balance for consume".to_string()),
        buyer_ledger_status: Some("reserved".to_string()),
        buyer_ledger_account_id: Some("buyer-recovery-account".to_string()),
        buyer_ledger_entry_id: Some("buyer-original-reserve".to_string()),
        buyer_ledger_balance_after: Some(250.0),
        buyer_ledger_error: None,
        buyer_consume_status: Some("refunded".to_string()),
        buyer_consume_entry_id: Some("buyer-refund-entry".to_string()),
        buyer_consume_balance_after: Some(250.0),
        buyer_consume_error: None,
        created_at_epoch: 1_777_948_001,
    });
    league.world.world_work_orders.push(WorldWorkOrder {
        work_order_id: "work-rejection-recovery-route".to_string(),
        purchase_id: "purchase-rejection-recovery-route".to_string(),
        listing_id: "listing-rejection-recovery-route".to_string(),
        buyer_matrix_user_id: "@route-recovery-buyer:local.dev".to_string(),
        seller_matrix_user_id: "@route-recovery-seller:local.dev".to_string(),
        company_id: "company-rejection-recovery-route".to_string(),
        status: "rejected_chargeback_failed".to_string(),
        brief: "Rejected work waiting for seller chargeback recovery".to_string(),
        value_score: 70,
        created_at_epoch: 1_777_948_002,
    });
    league.world.world_work_rejections.push(WorldWorkRejection {
        rejection_id: "rejection-recovery-route".to_string(),
        work_order_id: "work-rejection-recovery-route".to_string(),
        matrix_user_id: "@route-recovery-buyer:local.dev".to_string(),
        body: "Rejected because evidence was missing; seller chargeback failed and needs recovery."
            .to_string(),
        status: "rejected_chargeback_failed".to_string(),
        refund_status: "refunded".to_string(),
        created_at_epoch: 1_777_948_003,
    });

    let artifacts = build_world_route_artifacts(&league.world);
    let rejection_task = artifacts.task_graph["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["task_id"] == "work-rejection-recovery-route")
        .expect("rejection recovery task should be visible");
    assert_eq!(
        rejection_task["next_opportunity_kind"],
        "rejection_chargeback_recovery"
    );
    assert_eq!(
        rejection_task["next_opportunity_panel_id"],
        "world-commerce-panel"
    );
    assert_eq!(
        rejection_task["next_opportunity_input_id"],
        "world-work-reject-id"
    );
    assert_eq!(
        rejection_task["next_opportunity_textarea_id"],
        "world-work-reject-body"
    );
    assert!(rejection_task["next_opportunity_command"]
        .as_str()
        .unwrap_or("")
        .contains("/work reject latest"));
    let rejection_body = rejection_task["next_opportunity_body"]
        .as_str()
        .unwrap_or("");
    assert!(rejection_body.contains("不二次退款"));
    assert!(rejection_body.contains("卖家扣回"));
    assert_hidden_test_ready_prompt("rejection_recovery_route_target", rejection_body);

    let mut cancellation_league = default_league_state();
    cancellation_league
        .world
        .world_purchases
        .push(WorldPurchase {
            purchase_id: "purchase-cancel-recovery-route".to_string(),
            listing_id: "listing-cancel-recovery-route".to_string(),
            shop_id: "shop-cancel-recovery-route".to_string(),
            company_id: "company-cancel-recovery-route".to_string(),
            buyer_matrix_user_id: "@route-cancel-buyer:local.dev".to_string(),
            seller_matrix_user_id: "@route-cancel-seller:local.dev".to_string(),
            price_credits: 65,
            status: "cancelled_chargeback_failed".to_string(),
            ledger_status: Some("seller_chargeback_failed".to_string()),
            ledger_account_id: Some("seller-cancel-account".to_string()),
            ledger_entry_id: Some("seller-original-settlement".to_string()),
            ledger_balance_after: Some(58.0),
            ledger_error: Some("insufficient reserved balance for consume".to_string()),
            buyer_ledger_status: Some("reserved".to_string()),
            buyer_ledger_account_id: Some("buyer-cancel-account".to_string()),
            buyer_ledger_entry_id: Some("buyer-original-reserve".to_string()),
            buyer_ledger_balance_after: Some(250.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("refunded".to_string()),
            buyer_consume_entry_id: Some("buyer-cancel-refund".to_string()),
            buyer_consume_balance_after: Some(250.0),
            buyer_consume_error: None,
            created_at_epoch: 1_777_948_101,
        });
    cancellation_league
        .world
        .world_work_orders
        .push(WorldWorkOrder {
            work_order_id: "work-cancel-recovery-route".to_string(),
            purchase_id: "purchase-cancel-recovery-route".to_string(),
            listing_id: "listing-cancel-recovery-route".to_string(),
            buyer_matrix_user_id: "@route-cancel-buyer:local.dev".to_string(),
            seller_matrix_user_id: "@route-cancel-seller:local.dev".to_string(),
            company_id: "company-cancel-recovery-route".to_string(),
            status: "cancelled_chargeback_failed".to_string(),
            brief: "Cancelled work waiting for seller chargeback recovery".to_string(),
            value_score: 65,
            created_at_epoch: 1_777_948_102,
        });
    cancellation_league
        .world
        .world_work_cancellations
        .push(WorldWorkCancellation {
            cancellation_id: "cancel-recovery-route".to_string(),
            work_order_id: "work-cancel-recovery-route".to_string(),
            matrix_user_id: "@route-cancel-buyer:local.dev".to_string(),
            body: "Cancelled after scope mismatch; seller chargeback failed and needs recovery."
                .to_string(),
            status: "cancelled_chargeback_failed".to_string(),
            refund_status: "refunded".to_string(),
            created_at_epoch: 1_777_948_103,
        });

    let cancellation_artifacts = build_world_route_artifacts(&cancellation_league.world);
    let cancellation_task = cancellation_artifacts.task_graph["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["task_id"] == "work-cancel-recovery-route")
        .expect("cancellation recovery task should be visible");
    assert_eq!(
        cancellation_task["next_opportunity_kind"],
        "cancellation_settlement_recovery"
    );
    assert_eq!(
        cancellation_task["next_opportunity_input_id"],
        "world-work-cancel-id"
    );
    assert_eq!(
        cancellation_task["next_opportunity_textarea_id"],
        "world-work-cancel-body"
    );
    assert!(cancellation_task["next_opportunity_command"]
        .as_str()
        .unwrap_or("")
        .contains("/work cancel latest"));
    let cancellation_body = cancellation_task["next_opportunity_body"]
        .as_str()
        .unwrap_or("");
    assert!(cancellation_body.contains("不二次退款"));
    assert!(cancellation_body.contains("卖家扣回"));
    assert_hidden_test_ready_prompt("cancellation_recovery_route_target", cancellation_body);
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
async fn world_self_dealing_purchase_keeps_solo_loop_but_blocks_progression_farming() {
    let (ledger_base_url, ledger_admin_token) = start_real_ledger_service_for_world_e2e().await;
    let http = Client::new();
    let account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 1_000.0).await;

    let matrix_user_id = "@world-self-deal:local.dev";
    let room_id = "!world-self-deal:local.dev";
    let company_id = "company-self-deal";
    let shop_id = "shop-self-deal";
    let listing_id = "listing-self-deal";
    let mut bindings = IdentityBindings::default();
    bindings.matrix_users.insert(
        matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-self-deal-org".to_string()),
            account_id: Some(account_id),
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
            owner_matrix_user_id: matrix_user_id.to_string(),
            asset_id: "asset-self-deal".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Self Deal Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 120,
            reputation_score: 12,
            level: 2,
            created_at_epoch: 1_777_903_020,
        });
        league.world.world_shops.push(WorldShop {
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            name: "Self Deal Guard Storefront".to_string(),
            shop_kind: "studio".to_string(),
            status: "operating".to_string(),
            listing_count: 1,
            gross_merchandise_score: 120,
            created_at_epoch: 1_777_903_020,
        });
        league.world.world_listings.push(WorldListing {
            listing_id: listing_id.to_string(),
            shop_id: shop_id.to_string(),
            company_id: company_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            asset_id: "asset-self-deal".to_string(),
            title: "Self deal guard offer".to_string(),
            listing_kind: "service_offer".to_string(),
            status: "listed".to_string(),
            price_credits: 80,
            quality_score: 80,
            created_at_epoch: 1_777_903_020,
        });
    }

    let (status, buy) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/listings/{listing_id}/buy"),
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "room_id": room_id,
            "body": "Solo rehearsal purchase: open a work loop with deliverable, evidence package, acceptance standard, risk controls, next action, and self-review without farming progression."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "self buy response: {buy}");
    assert_eq!(buy["buyer_ledger_status"], "reserved");
    assert_eq!(buy["ledger_status"], "settled");
    assert_eq!(buy["purchase"]["status"], "reserved");
    assert_eq!(buy["work_order"]["status"], "open");
    assert!(buy["economy_event"].is_null());
    assert!(buy["seller_standing"].is_null());
    assert!(buy["buyer_standing"].is_null());
    let work_order_id = buy["work_order"]["work_order_id"]
        .as_str()
        .expect("work order id")
        .to_string();

    let (status, delivery) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/deliver"),
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "room_id": room_id,
            "body": "Solo rehearsal delivery: final deliverable, evidence package, acceptance checklist, risk review, next action, and self-review."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "self delivery response: {delivery}");
    assert_eq!(delivery["work_order"]["status"], "delivered");
    assert!(delivery["economy_event"].is_null());
    assert!(delivery["standing"].is_null());

    let (status, acceptance) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/accept"),
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "room_id": room_id,
            "body": "Solo rehearsal acceptance: evidence reviewed, quality accepted, risk closed, next collaboration noted, and self-review complete."
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "self acceptance response: {acceptance}"
    );
    assert_eq!(acceptance["work_order"]["status"], "completed");
    assert_eq!(acceptance["buyer_consume_status"], "consumed");
    assert!(acceptance["economy_event"].is_null());
    assert!(acceptance["standing"].is_null());

    let league = state.inner.league_state.lock().await;
    let company = league
        .world
        .world_companies
        .iter()
        .find(|company| company.company_id == company_id)
        .expect("company should remain present");
    assert_eq!(company.revenue_score, 120);
    assert_eq!(company.reputation_score, 12);
    assert_eq!(company.level, 2);
    let shop = league
        .world
        .world_shops
        .iter()
        .find(|shop| shop.shop_id == shop_id)
        .expect("shop should remain present");
    assert_eq!(shop.gross_merchandise_score, 120);
    assert!(!league.players_by_matrix_user.contains_key(matrix_user_id));
    assert!(!league.world.world_economy_events.iter().any(|event| {
        event.subject_id == work_order_id
            && matches!(
                event.event_kind.as_str(),
                "listing_purchase" | "work_delivered" | "work_accepted"
            )
    }));
    assert!(league.world.world_economy_events.iter().any(|event| {
        event.subject_id == buy["purchase"]["purchase_id"].as_str().unwrap()
            && event.event_kind == "market_tax_sink"
            && event.credits_delta < 0
    }));
    assert!(!league
        .world
        .world_faction_standings
        .iter()
        .any(|standing| standing.matrix_user_id == matrix_user_id));
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
async fn world_reject_does_not_release_refund_progression_without_seller_chargeback() {
    let (ledger_base_url, ledger_admin_token) = start_real_ledger_service_for_world_e2e().await;
    let http = Client::new();
    let buyer_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 250.0).await;
    let seller_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 0.0).await;
    apply_real_ledger_action(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        "reserve",
        &buyer_account_id,
        70.0,
        "seed-reject-chargeback-buyer-reserve",
    )
    .await;

    let buyer_matrix_user_id = "@world-reject-chargeback-buyer:local.dev";
    let seller_matrix_user_id = "@world-reject-chargeback-seller:local.dev";
    let room_id = "!world-reject-chargeback:local.dev";
    let mut bindings = IdentityBindings::default();
    bindings.matrix_users.insert(
        buyer_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-reject-chargeback-org".to_string()),
            account_id: Some(buyer_account_id.clone()),
        },
    );
    bindings.matrix_users.insert(
        seller_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-reject-chargeback-org".to_string()),
            account_id: Some(seller_account_id.clone()),
        },
    );
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    config.ledger_base_url = ledger_base_url.clone();
    config.ledger_admin_token = Some(ledger_admin_token.clone());
    let state = test_state(config, bindings, HashMap::new());
    let app = build_router(state.clone());
    let purchase_id = "purchase-reject-chargeback-blocked";
    let work_order_id = "work-reject-chargeback-blocked";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: "listing-reject-chargeback-blocked".to_string(),
            shop_id: "shop-reject-chargeback-blocked".to_string(),
            company_id: "company-reject-chargeback-blocked".to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 70,
            status: "reserved".to_string(),
            ledger_status: Some("settled".to_string()),
            ledger_account_id: Some(seller_account_id.clone()),
            ledger_entry_id: Some("seller-original-settlement-entry".to_string()),
            ledger_balance_after: Some(63.0),
            ledger_error: None,
            buyer_ledger_status: Some("reserved".to_string()),
            buyer_ledger_account_id: Some(buyer_account_id.clone()),
            buyer_ledger_entry_id: Some("buyer-original-reserve-entry".to_string()),
            buyer_ledger_balance_after: Some(250.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("pending_acceptance".to_string()),
            buyer_consume_entry_id: None,
            buyer_consume_balance_after: None,
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_981,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: "listing-reject-chargeback-blocked".to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: "company-reject-chargeback-blocked".to_string(),
            status: "delivered".to_string(),
            brief: "Delivered work whose seller chargeback will fail".to_string(),
            value_score: 70,
            created_at_epoch: 1_777_897_981,
        });
    }

    let (status, rejection) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/reject"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Buyer rejects after evidence review, but seller chargeback cannot settle; keep world progression blocked until the clawback is recovered."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reject response: {rejection}");
    assert_eq!(rejection["buyer_refund_status"], "refunded");
    assert_eq!(
        rejection["seller_chargeback_status"],
        "seller_chargeback_reserve_failed"
    );
    assert_eq!(
        rejection["purchase"]["status"],
        "rejected_chargeback_failed"
    );
    assert_eq!(
        rejection["work_order"]["status"],
        "rejected_chargeback_failed"
    );
    assert_eq!(
        rejection["rejection"]["status"],
        "rejected_chargeback_failed"
    );
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

    apply_real_ledger_action(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        "grant",
        &seller_account_id,
        67.0,
        "seed-reject-chargeback-retry-seller-funds",
    )
    .await;
    let (status, retry) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/reject"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Retry the seller chargeback after recovery funds arrive; do not refund the buyer twice, just clear the clawback and release the rejected work state."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reject retry response: {retry}");
    assert_eq!(retry["buyer_refund_status"], "refunded");
    assert_eq!(
        retry["seller_chargeback_status"],
        "seller_chargeback_consumed"
    );
    assert_eq!(retry["purchase"]["status"], "rejected_refunded");
    assert_eq!(retry["work_order"]["status"], "rejected_refunded");
    assert_eq!(retry["rejection"]["status"], "rejected_refunded");
    assert!(retry["economy_event"].is_object());
    assert!(retry["standing"].is_object());

    let league = state.inner.league_state.lock().await;
    assert_eq!(
        league
            .world
            .world_work_rejections
            .iter()
            .filter(|rejection| rejection.work_order_id == work_order_id)
            .count(),
        1,
        "chargeback retry should update the existing rejection instead of creating a second rejection"
    );
    assert_eq!(
        league
            .world
            .world_economy_events
            .iter()
            .filter(|event| event.subject_id == work_order_id && event.event_kind == "work_rejected")
            .count(),
        1
    );
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
    assert_eq!(seller_account["reserved"].as_f64().unwrap(), 0.0);
    assert_eq!(seller_account["balance"].as_f64().unwrap(), 0.0);
}

#[tokio::test]
async fn world_cancel_does_not_release_refund_progression_without_seller_chargeback() {
    let (ledger_base_url, ledger_admin_token) = start_real_ledger_service_for_world_e2e().await;
    let http = Client::new();
    let buyer_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 250.0).await;
    let seller_account_id =
        create_real_ledger_account(&http, &ledger_base_url, &ledger_admin_token, 0.0).await;
    apply_real_ledger_action(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        "reserve",
        &buyer_account_id,
        65.0,
        "seed-cancel-chargeback-buyer-reserve",
    )
    .await;

    let buyer_matrix_user_id = "@world-cancel-chargeback-buyer:local.dev";
    let seller_matrix_user_id = "@world-cancel-chargeback-seller:local.dev";
    let room_id = "!world-cancel-chargeback:local.dev";
    let mut bindings = IdentityBindings::default();
    bindings.matrix_users.insert(
        buyer_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-cancel-chargeback-org".to_string()),
            account_id: Some(buyer_account_id.clone()),
        },
    );
    bindings.matrix_users.insert(
        seller_matrix_user_id.to_string(),
        IdentityBindingEntry {
            product_user_id: None,
            org_id: Some("world-cancel-chargeback-org".to_string()),
            account_id: Some(seller_account_id.clone()),
        },
    );
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    config.ledger_base_url = ledger_base_url.clone();
    config.ledger_admin_token = Some(ledger_admin_token.clone());
    let state = test_state(config, bindings, HashMap::new());
    let app = build_router(state.clone());
    let purchase_id = "purchase-cancel-chargeback-blocked";
    let work_order_id = "work-cancel-chargeback-blocked";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_purchases.push(WorldPurchase {
            purchase_id: purchase_id.to_string(),
            listing_id: "listing-cancel-chargeback-blocked".to_string(),
            shop_id: "shop-cancel-chargeback-blocked".to_string(),
            company_id: "company-cancel-chargeback-blocked".to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            price_credits: 65,
            status: "reserved".to_string(),
            ledger_status: Some("settled".to_string()),
            ledger_account_id: Some(seller_account_id.clone()),
            ledger_entry_id: Some("seller-original-settlement-entry".to_string()),
            ledger_balance_after: Some(58.0),
            ledger_error: None,
            buyer_ledger_status: Some("reserved".to_string()),
            buyer_ledger_account_id: Some(buyer_account_id.clone()),
            buyer_ledger_entry_id: Some("buyer-original-reserve-entry".to_string()),
            buyer_ledger_balance_after: Some(250.0),
            buyer_ledger_error: None,
            buyer_consume_status: Some("pending_acceptance".to_string()),
            buyer_consume_entry_id: None,
            buyer_consume_balance_after: None,
            buyer_consume_error: None,
            created_at_epoch: 1_777_897_982,
        });
        league.world.world_work_orders.push(WorldWorkOrder {
            work_order_id: work_order_id.to_string(),
            purchase_id: purchase_id.to_string(),
            listing_id: "listing-cancel-chargeback-blocked".to_string(),
            buyer_matrix_user_id: buyer_matrix_user_id.to_string(),
            seller_matrix_user_id: seller_matrix_user_id.to_string(),
            company_id: "company-cancel-chargeback-blocked".to_string(),
            status: "open".to_string(),
            brief: "Open work whose seller chargeback will fail on cancellation".to_string(),
            value_score: 65,
            created_at_epoch: 1_777_897_982,
        });
    }

    let (status, cancellation) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/cancel"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Buyer cancels before delivery, but seller chargeback cannot settle; keep world progression blocked until the clawback is recovered."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cancel response: {cancellation}");
    assert_eq!(cancellation["buyer_cancel_refund_status"], "refunded");
    assert_eq!(
        cancellation["seller_chargeback_status"],
        "seller_chargeback_reserve_failed"
    );
    assert_eq!(
        cancellation["purchase"]["status"],
        "cancelled_chargeback_failed"
    );
    assert_eq!(
        cancellation["work_order"]["status"],
        "cancelled_chargeback_failed"
    );
    assert_eq!(
        cancellation["cancellation"]["status"],
        "cancelled_chargeback_failed"
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

    apply_real_ledger_action(
        &http,
        &ledger_base_url,
        &ledger_admin_token,
        "grant",
        &seller_account_id,
        62.0,
        "seed-cancel-chargeback-retry-seller-funds",
    )
    .await;
    let (status, retry) = send_json_request(
        &app,
        "POST",
        &format!("/v1/world/work-orders/{work_order_id}/cancel"),
        &[],
        json!({
            "matrix_user_id": buyer_matrix_user_id,
            "room_id": room_id,
            "body": "Retry the seller chargeback after recovery funds arrive; do not refund the buyer twice, just clear the clawback and release the cancelled work state."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cancel retry response: {retry}");
    assert_eq!(retry["buyer_cancel_refund_status"], "refunded");
    assert_eq!(
        retry["seller_chargeback_status"],
        "seller_chargeback_consumed"
    );
    assert_eq!(retry["purchase"]["status"], "cancelled_refunded");
    assert_eq!(retry["work_order"]["status"], "cancelled_refunded");
    assert_eq!(retry["cancellation"]["status"], "cancelled_refunded");
    assert!(retry["economy_event"].is_object());
    assert!(retry["standing"].is_object());

    let league = state.inner.league_state.lock().await;
    assert_eq!(
        league
            .world
            .world_work_cancellations
            .iter()
            .filter(|cancellation| cancellation.work_order_id == work_order_id)
            .count(),
        1,
        "chargeback retry should update the existing cancellation instead of creating a second cancellation"
    );
    assert_eq!(
        league
            .world
            .world_economy_events
            .iter()
            .filter(
                |event| event.subject_id == work_order_id && event.event_kind == "work_cancelled"
            )
            .count(),
        1
    );
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
    assert_eq!(seller_account["reserved"].as_f64().unwrap(), 0.0);
    assert_eq!(seller_account["balance"].as_f64().unwrap(), 0.0);
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
async fn latest_company_listing_prefers_operating_company_over_newer_review_hold() {
    let mut config = test_config();
    config.runtime_profile = RuntimeProfile::Production;
    let state = test_state(config, IdentityBindings::default(), HashMap::new());
    let app = build_router(state.clone());
    let matrix_user_id = "@world-latest-company-review-hold:local.dev";
    let operating_company_id = "company-operating-latest-guard";
    let held_asset_id = "asset-latest-company-review-hold";
    {
        let mut league = state.inner.league_state.lock().await;
        league.world.world_companies.push(WorldCompany {
            company_id: operating_company_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            asset_id: "asset-operating-latest-guard".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Operating Latest Guard Studio".to_string(),
            company_kind: "studio".to_string(),
            status: "operating".to_string(),
            revenue_score: 160,
            reputation_score: 70,
            level: 2,
            created_at_epoch: 1_777_901_020,
        });
        league.world.world_shops.push(WorldShop {
            shop_id: "shop-operating-latest-guard".to_string(),
            company_id: operating_company_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            name: "Operating Latest Guard Storefront".to_string(),
            shop_kind: "studio".to_string(),
            status: "operating".to_string(),
            listing_count: 0,
            gross_merchandise_score: 0,
            created_at_epoch: 1_777_901_021,
        });
        league.world.world_assets.push(WorldAsset {
            asset_id: held_asset_id.to_string(),
            owner_matrix_user_id: matrix_user_id.to_string(),
            location_id: "starter-studio".to_string(),
            asset_kind: "studio".to_string(),
            name: "Held Latest Guard Asset".to_string(),
            status: "seeded".to_string(),
            value_score: 120,
            upgrade_level: 2,
            upgrade_points: 90,
            last_upgrade_kind: Some("manual_upgrade".to_string()),
            created_at_epoch: 1_777_901_022,
        });
    }

    let (held_status, held_company_response) = send_json_request(
        &app,
        "POST",
        "/v1/world/companies",
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "asset_id": held_asset_id,
            "body": "copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy copy"
        }),
    )
    .await;
    assert_eq!(held_status, StatusCode::OK);
    assert_eq!(held_company_response["payout_status"], "review_hold");
    let held_company_id = held_company_response["company"]["company_id"]
        .as_str()
        .expect("held company id")
        .to_string();

    let (listing_status, listing_response) = send_json_request(
        &app,
        "POST",
        "/v1/world/listings",
        &[],
        json!({
            "matrix_user_id": matrix_user_id,
            "company_id": "latest",
            "body": "Publish the legitimate operating studio offer with customer deliverable, evidence package, risk controls, self review, acceptance standard, and next action."
        }),
    )
    .await;
    assert_eq!(
        listing_status,
        StatusCode::OK,
        "latest should not be shadowed by a newer review-held company: {listing_response}"
    );
    assert_eq!(
        listing_response["listing"]["company_id"],
        operating_company_id
    );
    assert_eq!(listing_response["listing"]["status"], "listed");

    let league = state.inner.league_state.lock().await;
    let operating_shop = league
        .world
        .world_shops
        .iter()
        .find(|shop| shop.company_id == operating_company_id)
        .expect("operating shop");
    assert_eq!(operating_shop.listing_count, 1);
    assert_eq!(
        league
            .world
            .world_listings
            .iter()
            .filter(|listing| listing.company_id == held_company_id)
            .count(),
        1,
        "held company should still only have its review-held bootstrap listing"
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

async fn send_text_request_with_headers(
    app: &axum::Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, HeaderMap, String) {
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
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body bytes");
    let body = String::from_utf8(bytes.to_vec()).expect("decode response body as utf8");

    (status, headers, body)
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

async fn apply_real_ledger_action(
    http: &Client,
    ledger_base_url: &str,
    admin_token: &str,
    action: &str,
    account_id: &str,
    amount: f64,
    idempotency_key: &str,
) -> Value {
    let response = http
        .post(format!("{}/v1/ledger/{action}", ledger_base_url))
        .header("x-admin-token", admin_token)
        .json(&json!({
            "account_id": account_id,
            "amount": amount,
            "idempotency_key": idempotency_key,
            "reference_id": idempotency_key,
        }))
        .send()
        .await
        .expect("apply real ledger action");
    assert!(
        response.status().is_success(),
        "ledger action {action} failed with {}",
        response.status()
    );
    response
        .json::<Value>()
        .await
        .expect("decode ledger action response")
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
        body["trillionnium_world_closed_beta_prototype"]["route_runner_handoff_gate"]
            ["contract_version"],
        "trillionnium_playability_route_runner_handoff_gate_v1"
    );
    assert!(
        body["trillionnium_world_closed_beta_prototype"]["axes"]["product_loop"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "route_runner_handoff_gate_visible")
    );
    assert!(
        body["trillionnium_world_closed_beta_prototype"]["axes"]["world_depth"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "route_runner_handoff_world_loop_ready")
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
        body["trillionnium_world_real_user_beta"]["route_runner_handoff_gate"]["contract_version"],
        "trillionnium_playability_route_runner_handoff_gate_v1"
    );
    assert!(
        body["trillionnium_world_real_user_beta"]["axes"]["product_retention"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "route_runner_handoff_gate_visible")
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
        body["trillionnium_world_public_commercial_product"]["route_runner_handoff_gate"]
            ["contract_version"],
        "trillionnium_playability_route_runner_handoff_gate_v1"
    );
    assert!(
        body["trillionnium_world_public_commercial_product"]["axes"]["public_launch_surface"]
            ["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "route_runner_handoff_gate_visible")
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
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_handoff_gate"]
            ["contract_version"],
        "trillionnium_playability_route_runner_handoff_gate_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_handoff_gate"]
            ["feed_handoff_contract_version"],
        "trillionnium_route_runner_handoff_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_handoff_gate"]
            ["map_hub_handoff_contract_version"],
        "trillionnium_route_runner_handoff_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_handoff_gate"]
            ["route_mastery_contract_version"],
        "trillionnium_route_mastery_v1"
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["route_runner_handoff_gate"]
            ["route_mastery_runner_count"]
            .as_u64()
            .is_some()
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["route_runner_handoff_gate"]
            ["first_route_mastery_xp"]
            .as_u64()
            .is_some()
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["route_runner_handoff_gate"]
            ["next_route_action_count"]
            .as_u64()
            .is_some()
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["map_readability_lod_gate"]
            ["contract_version"],
        "trillionnium_world_map_readability_lod_gate_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["map_readability_lod_gate"]
            ["shell_contract_version"],
        "trillionnium_world_map_readability_lod_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["map_readability_lod_gate"]
            ["within_budget"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["contract_version"],
        "trillionnium_route_runner_funnel_telemetry_gate_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["telemetry_contract_version"],
        "trillionnium_route_runner_funnel_telemetry_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["cohort_quality_contract_version"],
        "trillionnium_route_runner_funnel_cohort_quality_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["funnel_integrity_contract_version"],
        "trillionnium_route_runner_funnel_integrity_v1"
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["route_started_count"]
            .as_i64()
            .is_some()
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["reward_claimed_count"]
            .as_i64()
            .is_some()
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["time_to_reward_target_seconds"],
        1800
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["reward_to_next_route_conversion_percent"]
            .as_i64()
            .is_some()
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["cohort_denominator_consistent"]
            .as_bool()
            .unwrap_or(false)
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["route_runner_funnel_telemetry_gate"]
            ["reward_to_next_route_blockers_visible"]
            .as_bool()
            .unwrap_or(false)
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["commercial_operating_dashboard_gate"]
            ["dashboard_contract_version"],
        "trillionnium_world_commercial_operating_dashboard_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["commercial_operating_dashboard_gate"]
            ["route_recommendation_policy_contract_version"],
        "trillionnium_world_route_recommendation_policy_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["commercial_operating_dashboard_gate"]
            ["route_recommendation_quality_contract_version"],
        "trillionnium_world_route_recommendation_quality_v1"
    );
    assert!(body["trillionnium_world_playability_scorecard"]
        ["commercial_operating_dashboard_gate"]["route_recommendation_quality_status"]
        .as_str()
        .is_some());
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["commercial_operating_dashboard_gate"]
            ["route_recommendation_quality_score_target_percent"],
        60
    );
    let recommendation_quality_score = body["trillionnium_world_playability_scorecard"]
        ["commercial_operating_dashboard_gate"]["route_recommendation_quality_score_percent"]
        .as_i64()
        .unwrap();
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["commercial_operating_dashboard_gate"]
            ["route_recommendation_quality_score_ready"],
        recommendation_quality_score >= 60
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["commercial_operating_dashboard_gate"]
            ["route_recommendation_denominator_consistent"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["commercial_operating_dashboard_gate"]
            ["route_recommendation_raw_counts_preserved"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["commercial_operating_dashboard_gate"]
            ["route_recommendation_risk_controls_visible"],
        true
    );
    assert!(body["trillionnium_world_playability_scorecard"]
        ["commercial_operating_dashboard_gate"]["reward_claim_to_next_commission_percent"]
        .as_i64()
        .is_some());
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["contract_version"],
        "trillionnium_world_future_engine_readiness_gate_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["active_engine_id"],
        "leaflet_openstreetmap_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["candidate_engine_id"],
        "maplibre_gl_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["shadow_renderer_contract_version"],
        "trillionnium_world_map_renderer_shadow_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["maplibre_shadow_parity_contract_version"],
        "trillionnium_world_map_maplibre_shadow_parity_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["maplibre_shadow_only"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["maplibre_canary_percent"],
        0
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["maplibre_max_canary_percent_without_new_signoff"],
        1
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["maplibre_canary_starts_at_zero"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["maplibre_rollback_drill_evidence_required"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["future_engine_readiness_gate"]
            ["maplibre_canary_rollback_drill_visible"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_provider_readiness_gate"]["contract_version"],
        "trillionnium_openstreetmap_provider_readiness_gate_v1"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_provider_readiness_gate"]["readiness_contract_version"],
        "openstreetmap_provider_readiness_v1"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_provider_readiness_gate"]["provider_mode"],
        "fixture"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_provider_readiness_gate"]["fixture_mode_green"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_provider_readiness_gate"]["live_modes_fail_closed"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_provider_readiness_gate"]["network_ingestion_disabled"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_provider_readiness_gate"]["production_ingestion_disabled"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_provider_readiness_gate"]
            ["overpass_bbox_cache_fail_closed"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["openstreetmap_provider_readiness_gate"]
            ["readiness_status"],
        "fixture_ready_live_fail_closed"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["contract_version"],
        "trillionnium_openstreetmap_geodata_freshness_gate_v1"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["freshness_contract_version"],
        "openstreetmap_geodata_freshness_v1"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["freshness_status"],
        "fixture_static_fresh_live_stale_blocked"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["fixture_static_snapshot"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["wall_clock_freshness_applies"],
        false
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["live_data_freshness_applies"],
        false
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["fixture_snapshot_age_seconds"],
        0
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["live_ingestion_disabled"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_geodata_freshness_gate"]["stale_live_ingestion_blocked"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["openstreetmap_geodata_freshness_gate"]
            ["freshness_green"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_attribution_presence_gate"]["contract_version"],
        "trillionnium_openstreetmap_attribution_presence_gate_v1"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_attribution_presence_gate"]
            ["attribution_presence_contract_version"],
        "openstreetmap_attribution_presence_v1"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_attribution_presence_gate"]["attribution"],
        "© OpenStreetMap contributors"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_attribution_presence_gate"]["database_license"],
        "ODbL-1.0"
    );
    assert_eq!(
        body["trillionnium_openstreetmap_attribution_presence_gate"]
            ["attribution_visible_required"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_attribution_presence_gate"]
            ["derived_database_tracking_required"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_attribution_presence_gate"]
            ["odbl_database_obligations_visible"],
        true
    );
    assert_eq!(
        body["trillionnium_openstreetmap_attribution_presence_gate"]["attribution_presence_green"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["openstreetmap_attribution_presence_gate"]
            ["attribution_presence_green"],
        true
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["world_map_runtime_safety_gate"]
            ["contract_version"],
        "trillionnium_world_map_runtime_safety_gate_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["world_map_runtime_safety_gate"]
            ["rum_slo_contract_version"],
        "trillionnium_world_map_rum_slo_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["world_map_runtime_safety_gate"]
            ["weak_network_contract_version"],
        "trillionnium_world_map_weak_network_resilience_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["world_map_runtime_safety_gate"]
            ["rum_sample_matrix_contract_version"],
        "trillionnium_world_map_real_user_rum_matrix_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["world_map_runtime_safety_gate"]
            ["offline_action_queue_contract_version"],
        "trillionnium_world_map_offline_action_queue_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["world_map_runtime_safety_gate"]
            ["location_privacy_contract_version"],
        "trillionnium_world_map_location_privacy_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["world_map_runtime_safety_gate"]
            ["density_scalability_contract_version"],
        "trillionnium_world_map_density_scalability_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["world_map_runtime_safety_gate"]
            ["gameplay_accessibility_contract_version"],
        "trillionnium_world_map_gameplay_accessibility_i18n_v1"
    );
    assert_eq!(
        body["trillionnium_world_map_rum_slo_gate"]["contract_version"],
        "trillionnium_world_map_rum_slo_v1"
    );
    assert_eq!(
        body["trillionnium_world_map_delta_cache_gate"]["entity_delta_cache_contract"],
        "entity_group_versioned_delta_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["map_readability_lod_gate"]
            ["runtime_performance_budget_contract_version"],
        "trillionnium_world_map_runtime_performance_budget_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["map_readability_lod_gate"]
            ["map_subsystem_contract_version"],
        "trillionnium_world_map_subsystem_v1"
    );
    assert_eq!(
        body["trillionnium_world_playability_scorecard"]["map_readability_lod_gate"]
            ["transport_delta_contract_version"],
        "trillionnium_world_map_transport_delta_v1"
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["surface_feedback"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "map_readability_lod_contract_green")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["observability_gates"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "future_engine_readiness_contract_visible")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["observability_gates"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "openstreetmap_provider_readiness_gate_green")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["observability_gates"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "openstreetmap_geodata_freshness_gate_green")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["observability_gates"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "openstreetmap_attribution_presence_gate_green")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["surface_feedback"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "openstreetmap_provider_readiness_section_visible")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["surface_feedback"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "openstreetmap_geodata_freshness_section_visible")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["surface_feedback"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "openstreetmap_attribution_presence_section_visible")
    );
    assert!(
        body["trillionnium_world_playability_scorecard"]["axes"]["observability_gates"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check_id"] == "world_map_rum_delta_weak_privacy_gates_visible")
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
    assert!(body
        .contains("cex_consumer_entry_trillionnium_route_runner_handoff_playability_gate_green"));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_route_runner_handoff_closed_beta_gate_green"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_handoff_real_user_beta_gate_green"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_handoff_public_commercial_gate_green"
    ));
    assert!(body.contains("cex_consumer_entry_trillionnium_route_runner_handoff_all_gates_green"));
    assert!(body.contains("cex_consumer_entry_trillionnium_route_runner_handoff_feed_source_count"));
    assert!(body.contains("cex_consumer_entry_trillionnium_route_runner_handoff_runner_count"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_handoff_reward_claim_action_count"
    ));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_route_runner_handoff_next_route_action_count"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_contract_visible"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_runner_count"
    ));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_route_runner_handoff_first_route_mastery_xp"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_tier_visible"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_next_goal_evidence_visible"
    ));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_map_readability_lod_gate_green"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_map_rum_slo_gate_green"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_map_delta_cache_gate_green"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_map_runtime_safety_gate_green"));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_world_map_weak_network_resilience_gate_green"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_map_location_privacy_gate_green"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_gate_green"));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_world_map_density_scalability_gate_green")
    );
    assert!(
        body.contains("cex_consumer_entry_trillionnium_world_map_offline_action_queue_gate_green")
    );
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_map_gameplay_accessibility_i18n_gate_green"
    ));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_world_map_maplibre_shadow_parity_gate_green"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_map_maplibre_shadow_only"));
    assert!(body.contains("cex_consumer_entry_trillionnium_world_map_maplibre_canary_percent"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_map_maplibre_max_canary_percent_without_new_signoff"
    ));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_world_map_maplibre_canary_starts_at_zero")
    );
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_map_maplibre_rollback_drill_evidence_required"
    ));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_world_map_maplibre_canary_rollback_ready")
    );
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_map_readability_lod_visible_marker_budget"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_map_readability_lod_avatar_runner_budget"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_funnel_telemetry_contract_visible"
    ));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_route_runner_funnel_route_started_count")
    );
    assert!(body
        .contains("cex_consumer_entry_trillionnium_route_runner_funnel_evidence_submitted_count"));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_route_runner_funnel_time_to_reward_seconds")
    );
    assert!(body
        .contains("cex_consumer_entry_trillionnium_route_runner_funnel_daily_return_resume_count"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_route_runner_funnel_reward_to_next_route_percent"
    ));
    assert!(body.contains("cex_consumer_entry_trillionnium_route_runner_funnel_d1_resume_percent"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_commercial_operating_dashboard_gate_green"
    ));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_world_route_recommendation_quality_gate_green"));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_target_percent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_ready"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_route_recommendation_reward_lift_visible"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_route_recommendation_abandon_risk_visible"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_route_recommendation_denominator_consistent"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_route_recommendation_raw_counts_preserved"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_route_recommendation_risk_controls_visible"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_world_commercial_reward_claim_to_next_commission_percent"
    ));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_world_future_engine_readiness_gate_green")
    );
    assert!(body
        .contains("cex_consumer_entry_trillionnium_world_future_engine_promotion_blocker_count"));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_openstreetmap_provider_readiness_gate_green"));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_openstreetmap_provider_fail_closed_mode_count"));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_openstreetmap_geodata_freshness_gate_green")
    );
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_openstreetmap_geodata_fixture_snapshot_age_seconds 0"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_openstreetmap_geodata_staleness_alarm_active 0"
    ));
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_gate_green 1"
    ));
    assert!(body.contains("cex_consumer_entry_trillionnium_openstreetmap_attribution_required 1"));
    assert!(body
        .contains("cex_consumer_entry_trillionnium_openstreetmap_attribution_visible_required 1"));
    assert!(
        body.contains("cex_consumer_entry_trillionnium_openstreetmap_odbl_obligations_visible 1")
    );
    assert!(body.contains(
        "cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_check_count 4"
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

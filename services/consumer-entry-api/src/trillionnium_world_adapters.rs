use super::*;
use term_exchange_protocol::{
    CEX_SETTLEMENT_BACKEND_ID, TERM_EXCHANGE_BACKEND_CONTRACT_VERSION,
    TERM_EXCHANGE_PROTOCOL_VERSION,
};

const CEX_TRNM_ECONOMY_ADAPTER_CONTRACT: &str = "cex_trnm_game_economy_adapter_v1";
const CEX_WORLD_AUTHORITY_ADAPTER_CONTRACT: &str = "cex_trillionnium_world_authority_adapter_v1";
const TRILLIONNIUM_WORLD_API_CONTRACT: &str = "trillionnium_world_api_v1";
const TRILLIONNIUM_WORLD_CUTOVER_CONTRACT: &str = "trillionnium_world_authority_cutover_v1";
const TRILLIONNIUM_WORLD_OWNER_REPOSITORY: &str = "TrillionniumFoundation/Trillionnium-World";

fn first_non_empty_world_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn world_profile_is_production_like() -> bool {
    first_non_empty_world_env(&[
        "CONSUMER_ENTRY_RUNTIME_PROFILE",
        "CEX_RUNTIME_PROFILE",
        "APP_ENV",
    ])
    .map(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "beta" | "staging" | "stage" | "production" | "prod"
        )
    })
    .unwrap_or(false)
}

fn remote_world_authority_readiness_json() -> Value {
    let production_like = world_profile_is_production_like();
    let mode = first_non_empty_world_env(&["CEX_WORLD_AUTHORITY_MODE"])
        .unwrap_or_else(|| "embedded".to_string())
        .to_ascii_lowercase();
    let base_url = first_non_empty_world_env(&["TRILLIONNIUM_WORLD_BASE_URL"]);
    let api_contract = first_non_empty_world_env(&["TRILLIONNIUM_WORLD_API_CONTRACT"]);
    let auth_token_configured = first_non_empty_world_env(&["TRILLIONNIUM_WORLD_AUTH_TOKEN"])
        .map(|value| value.len() >= 24)
        .unwrap_or(false);
    let remote_mode = mode == "remote";
    let contract_match = api_contract.as_deref() == Some(TRILLIONNIUM_WORLD_API_CONTRACT);
    let startup_configuration_ready = remote_mode
        && base_url.is_some()
        && contract_match
        && (!production_like || auth_token_configured);

    json!({
        "adapter_contract": CEX_WORLD_AUTHORITY_ADAPTER_CONTRACT,
        "cutover_contract": TRILLIONNIUM_WORLD_CUTOVER_CONTRACT,
        "api_contract": TRILLIONNIUM_WORLD_API_CONTRACT,
        "owner_repository": TRILLIONNIUM_WORLD_OWNER_REPOSITORY,
        "mode": mode,
        "production_like": production_like,
        "base_url_configured": base_url.is_some(),
        "api_contract_match": contract_match,
        "auth_token_configured": auth_token_configured,
        "startup_configuration_ready": startup_configuration_ready,
        "local_world_writer": {
            "production_status": "quarantined_by_consumer_entry_router_fence",
            "development_status": "compatibility_and_migration_source_only",
            "authoritative_after_cutover": false
        },
        "remote_adapter_binary": "world-authority-adapter",
        "source_candidate": {
            "world_pull_request": 60,
            "world_repository": TRILLIONNIUM_WORLD_OWNER_REPOSITORY,
            "production_authorization": "not_granted"
        },
        "cutover_status": if startup_configuration_ready {
            "remote_adapter_configured_waiting_for_exact_cross_repository_evidence"
        } else {
            "blocked_until_remote_adapter_configuration_and_world_source_evidence"
        },
        "required_remaining_evidence": [
            "world_exact_sha_source_ci_green",
            "cex_exact_sha_adapter_ci_green",
            "backfill_count_and_hash_reconciliation_exact",
            "success_timeout_replay_partial_outage_and_rollback_green",
            "no_dual_writer_proof",
            "embedded_cex_world_sources_removed_or_quarantined"
        ],
        "production_adapter_trait_ready": false,
        "production_authorization": "not_granted"
    })
}

pub(super) async fn get_trillionnium_world_adapter_readiness(
    State(state): State<AppState>,
) -> Json<Value> {
    let league = state.inner.league_state.lock().await;
    Json(cex_trillionnium_world_adapter_readiness_json_for_league(
        &league,
    ))
}

pub(super) fn cex_trillionnium_world_adapter_readiness_json_for_league(
    league: &LeagueState,
) -> Value {
    let world = &league.world;
    let receipt_count =
        world.world_term_exchange_receipts.len() + league.term_exchange_receipts.len();
    let record_total = world.world_events.len()
        + world.world_contracts.len()
        + world.world_purchases.len()
        + world.world_work_orders.len()
        + world.world_work_deliveries.len()
        + world.world_work_acceptances.len()
        + world.world_work_rejections.len()
        + world.world_work_reopens.len()
        + world.world_work_cancellations.len();
    let statuses = [
        "account_binding",
        "wallet_read_model",
        "ledger_intent",
        "receipt_verification",
        "idempotency",
        "reconciliation",
    ]
    .into_iter()
    .map(|role| {
        json!({
            "role": role,
            "status": "cex_trnm_game_economy_impl_connected",
            "production_adapter_trait_ready": true,
            "source_of_truth": "cex_term_exchange_backend_and_trnm_owned_protocol"
        })
    })
    .collect::<Vec<_>>();

    json!({
        "contract_version": CEX_TRNM_ECONOMY_ADAPTER_CONTRACT,
        "protocol_contract": TERM_EXCHANGE_PROTOCOL_VERSION,
        "backend_contract": TERM_EXCHANGE_BACKEND_CONTRACT_VERSION,
        "domain_contract": "trnm_game_economy_v1",
        "status": "cex_trnm_game_economy_adapter_ready",
        "status_scope": "cex_owned_economy_adapter_only",
        "world_authority_cutover_status": "not_yet_authorized",
        "backend_id": CEX_SETTLEMENT_BACKEND_ID,
        "source_of_truth": "cex_consumer_entry_term_exchange_backend",
        "world_authority": remote_world_authority_readiness_json(),
        "standalone_runtime_adapter_readiness": {
            "cutover_status": "economy_protocol_connected_world_state_remote_cutover_pending",
            "cex_dependency_status": "consumer_entry_api_depends_on_trnm_economy_protocol_and_remote_world_http_contract",
            "statuses": statuses,
        },
        "identity": {
            "adapter_contract": CEX_TRNM_ECONOMY_ADAPTER_CONTRACT,
            "source_of_truth": "cex_postgres_trnm_player_identities_plus_ingress_authenticated_account_binding",
            "recovery_api": "admin_protected_register_and_rotating_recovery_credential_ready",
            "real_user_recovery_drill": "release_gated"
        },
        "session": {
            "source_of_truth": "cex_signed_player_session_and_account_ownership_required",
            "shared_client_entry_token": false
        },
        "repository": {
            "source_of_truth": "cex_postgres_trnm_economic_intents_and_receipts",
            "status": "economic_records_authoritative_world_records_migration_source_only",
            "migration_floor": "0029_add_trnm_value_entitlements_and_player_sessions.sql"
        },
        "ledger": {
            "source_of_truth": "cex_postgres_ledger_and_escrow_backend",
            "fail_fast": true,
            "in_memory_fallback": false,
            "escrow_commit_before_seller_payment": true,
            "seller_payout_reserved_during_reversible_window": true,
            "receipt_count": receipt_count
        },
        "public_player_market": {
            "enabled": false,
            "status": "release_gated",
            "trusted_system_market_only": true,
            "blocked_until": [
                "real_user_identity_recovery_and_account_abuse_drill",
                "public_listing_custody_and_matching_review",
                "anti_cheat_abuse_controls",
                "dispute_and_customer_support_operations",
                "human_usability_and_legal_release_approval"
            ]
        },
        "current_game_counts": {
            "world_receipts": world.world_term_exchange_receipts.len(),
            "league_receipts": league.term_exchange_receipts.len(),
            "legacy_records_available_for_migration": record_total,
        },
        "route_records": {
            "total": record_total,
            "authority": "migration_source_only_after_remote_cutover"
        },
        "standalone_world_counts": {
            "nodes": world.world_map_nodes.len(),
            "receipts": receipt_count,
            "authority": "not_authoritative_after_remote_cutover"
        },
        "production_authorization": "not_granted"
    })
}

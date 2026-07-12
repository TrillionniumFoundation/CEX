use super::*;
use term_exchange_protocol::{
    CEX_SETTLEMENT_BACKEND_ID, TERM_EXCHANGE_BACKEND_CONTRACT_VERSION,
    TERM_EXCHANGE_PROTOCOL_VERSION,
};

const CEX_TRNM_ECONOMY_ADAPTER_CONTRACT: &str = "cex_trnm_game_economy_adapter_v1";

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
        "backend_id": CEX_SETTLEMENT_BACKEND_ID,
        "source_of_truth": "cex_consumer_entry_term_exchange_backend",
        "legacy_world_dependency_status": "absent_current_game_protocol_only",
        "standalone_runtime_adapter_readiness": {
            "cutover_status": "cex_depends_on_trnm_owned_economy_protocol_without_legacy_world_crates",
            "cex_dependency_status": "consumer_entry_api_depends_on_trnm_economy_protocol",
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
            "status": "atomic_intent_receipt_and_reconciliation_cursor_persistence_ready",
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
            "total": record_total
        },
        "standalone_world_counts": {
            "nodes": world.world_map_nodes.len(),
            "receipts": receipt_count
        }
    })
}

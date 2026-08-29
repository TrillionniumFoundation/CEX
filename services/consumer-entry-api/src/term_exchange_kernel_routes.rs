use super::*;
use term_exchange_protocol::{
    cex_settlement_backend_manifest, protocol_manifest_json, EconomicIntent, EconomicReceipt,
    WalletSnapshot, CEX_SETTLEMENT_BACKEND_ID, LEGACY_CEX_RUNTIME_PLUGIN_CONTRACT_VERSION,
    TERM_EXCHANGE_BACKEND_CONTRACT_VERSION, TERM_EXCHANGE_KERNEL_CONTRACT_VERSION,
    TERM_EXCHANGE_KERNEL_ID, TERM_EXCHANGE_PROTOCOL_PACKAGE_VERSION,
    TERM_EXCHANGE_PROTOCOL_VERSION,
};

const TRILLIONNIUM_TERM_EXCHANGE_HOST_CONTRACT_VERSION: &str = "trillionnium_term_exchange_host_v1";
const LEGACY_CEX_RUNTIME_PLUGIN_ENDPOINT: &str = "/v1/trillionnium/runtime/cex/manifest";
const TERM_EXCHANGE_KERNEL_MANIFEST_ENDPOINT: &str =
    "/v1/trillionnium/term-exchange/kernel/manifest";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TrnmEconomicIntentRequest {
    pub(super) intent: EconomicIntent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TrnmWalletRequest {
    pub(super) actor_id: String,
    pub(super) account_id: String,
    #[serde(default)]
    pub(super) reconciliation_cursor: u64,
}

pub(super) fn term_exchange_kernel_manifest_json(state: &AppState) -> Value {
    json!({
        "kind": "trillionnium_term_exchange_kernel_manifest",
        "contract_version": TERM_EXCHANGE_KERNEL_CONTRACT_VERSION,
        "kernel_id": TERM_EXCHANGE_KERNEL_ID,
        "kernel_name": "Trillionnium Term Exchange Kernel",
        "protocol": protocol_manifest_json(),
        "host_contract_version": TRILLIONNIUM_TERM_EXCHANGE_HOST_CONTRACT_VERSION,
        "backend_contract_version": TERM_EXCHANGE_BACKEND_CONTRACT_VERSION,
        "backend_adapter_contract_version": TERM_EXCHANGE_BACKEND_ADAPTER_CONTRACT_VERSION,
        "active_backend_id": CEX_SETTLEMENT_BACKEND_ID,
        "active_backend_kind": "cex",
        "settlement_backends": [
            cex_settlement_backend_manifest(true),
            {
                "backend_id": "dex-settlement-backend",
                "backend_name": "DEX Settlement Backend",
                "backend_kind": "dex",
                "contract_version": TERM_EXCHANGE_BACKEND_CONTRACT_VERSION,
                "active": false,
                "fail_closed": true,
                "capabilities": [
                    "reserve",
                    "settle",
                    "refund",
                    "reward_release",
                    "receipt_verification",
                    "on_chain_proof"
                ],
                "receipt_verification_required": true,
                "status": "planned_backend_same_protocol"
            }
        ],
        "integration_model": {
            "boundary": "Term/domain runtimes emit EconomicIntent; the Term Exchange Kernel validates terms and routes settlement to the active backend; CEX is the first backend, DEX is a future backend under the same protocol.",
            "invocation_mode": "domain_event_to_economic_intent_to_backend_receipt",
            "trnm_progression_gate": "campaign_state_advances_only_after_a_verified_economic_receipt_allows_progression_or_terminal_skip",
            "browser_role": "input_only_no_economic_authority",
            "matrix_role": "command_surface_no_economic_authority"
        },
        "ownership": {
            "term_exchange_kernel_owns": [
                "term_registry",
                "term_definition_schema",
                "economic_intent_schema",
                "economic_receipt_schema",
                "progression_gate_policy",
                "backend_capability_negotiation",
                "idempotency_scope_policy",
                "receipt_verification_policy"
            ],
            "trnm_game_owns": [
                "soft_credits",
                "bound_inventory",
                "regional_market_simulation",
                "caravan_simulation",
                "rts_ephemeral_resources",
                "campaign_state",
                "game_event_to_economic_intent_mapping"
            ],
            "cex_backend_owns": [
                "identity_resolution",
                "account_scope_resolution",
                "ledger_intent_validation",
                "wallet_read_model",
                "reserve",
                "escrow_like_hold",
                "seller_settlement",
                "buyer_consume",
                "refund",
                "seller_chargeback",
                "reward_release",
                "review_hold_release",
                "work_order_economy",
                "audit_receipts",
                "recovery_and_dead_letter_policy"
            ],
            "dex_backend_will_own": [
                "on_chain_reserve_or_lock",
                "smart_contract_settlement",
                "on_chain_refund",
                "proof_verification",
                "chain_finality_mapping"
            ],
            "shared_contracts": [
                "TermDefinition",
                "EconomicIntent",
                "EconomicReceipt",
                "SettlementBackendManifest",
                "ReceiptProgressionClass",
                "IdempotencyKey"
            ]
        },
        "term_capabilities": [
            {
                "term_family": "trnm_native_game_economy",
                "current_backend": "cex",
                "current_endpoints": [
                    "/v1/trillionnium/economy/intents",
                    "/v1/trillionnium/economy/wallet",
                    "/v1/trillionnium/economy/adapters/readiness"
                ]
            },
            {
                "term_family": "wallet_read_model",
                "current_backend": "cex",
                "current_endpoint": "/v1/matrix/users/:matrix_user_id/wallet"
            },
            {
                "term_family": "league_reward_settlement",
                "current_backend": "cex",
                "current_endpoints": [
                    "/v1/league/matches/:match_id/submit",
                    "/v1/league/reviews/:reward_id/approve",
                    "/v1/league/players/:matrix_user_id/rewards"
                ]
            },
            {
                "term_family": "world_contract_completion_settlement",
                "current_backend": "cex",
                "current_endpoint": "/v1/world/contracts/:contract_id/complete"
            },
            {
                "term_family": "world_commerce_lifecycle",
                "current_backend": "cex",
                "current_endpoints": [
                    "/v1/world/listings/:listing_id/buy",
                    "/v1/world/work-orders/:work_order_id/deliver",
                    "/v1/world/work-orders/:work_order_id/accept",
                    "/v1/world/work-orders/:work_order_id/reject",
                    "/v1/world/work-orders/:work_order_id/reopen",
                    "/v1/world/work-orders/:work_order_id/cancel"
                ]
            },
            {
                "term_family": "world_commerce_read_model",
                "current_backend": "cex",
                "current_endpoint": "/v1/world/commerce"
            }
        ],
        "receipt_policy": {
            "required_for_domain_progression": [
                "reserved",
                "settled",
                "consumed",
                "refunded",
                "seller_chargeback_consumed",
                "approved_release"
            ],
            "recoverable_hold_statuses": [
                "failed_network",
                "failed_identity",
                "failed_ledger",
                "seller_chargeback_reserve_failed",
                "seller_chargeback_failed",
                "rejected_refund_failed",
                "cancelled_refund_failed"
            ],
            "terminal_skip_statuses": [
                "skipped_zero_price",
                "skipped_zero_seller_net"
            ],
            "must_not_advance_domain_state_on": [
                "failed_bad_response",
                "failed_ledger",
                "failed_network",
                "failed_identity",
                "missing_account",
                "missing_ledger_token"
            ]
        },
        "runtime_requirements": {
            "protocol_crate": "trnm-economy-protocol",
            "protocol_version": TERM_EXCHANGE_PROTOCOL_VERSION,
            "protocol_package_version": TERM_EXCHANGE_PROTOCOL_PACKAGE_VERSION,
            "dependency_source": "CEX/vendor/trnm-economy-protocol pinned to exact TRNM-owned package version",
            "fail_closed": true,
            "idempotency_required": true,
            "receipt_required": true,
            "receipt_verification_required": true,
            "recoverable_retry_required": true,
            "domain_state_mutation_after_receipt_only": true,
            "backend_hot_swap_goal": "CEX and DEX backends implement the same Term Exchange Protocol; domain plugins keep EconomicIntent/EconomicReceipt stable."
        },
        "current_backend_adapter": {
            "adapter_contract_version": TERM_EXCHANGE_BACKEND_ADAPTER_CONTRACT_VERSION,
            "adapter_id": "cex-term-exchange-backend-adapter",
            "backend_id": CEX_SETTLEMENT_BACKEND_ID,
            "backend_kind": "cex",
            "trait": "TermExchangeBackend",
            "request_type": "TermExchangeLedgerActionRequest",
            "receipt_type": "EconomicReceipt",
            "legacy_status_projection": "LeagueLedgerSettlement",
            "migrated_call_paths": [
                "trnm_native_release_reward_reserve_settle_consume_refund_chargeback",
                "league_reward_settlement",
                "world_commerce_purchase_reserve_settle_consume_refund_chargeback",
                "world_contract_completion_settlement"
            ],
            "direct_ledger_http_policy": "world_and_league_economic_routes_call_the_backend_adapter_not_ledger_http_directly"
        },
        "state_persistence": {
            "receipt_state_type": "TermExchangeReceiptState",
            "league_receipt_index": "LeagueState.term_exchange_receipts",
            "world_receipt_index": "WorldState.world_term_exchange_receipts",
            "stored_fields": [
                "protocol_version",
                "receipt_id",
                "intent_id",
                "term_id",
                "backend_id",
                "backend_kind",
                "status",
                "progression_class",
                "settlement_reference",
                "ledger_entry_id",
                "reason",
                "amount_credits",
                "finalized_at_epoch"
            ],
            "legacy_status_compatibility": true,
            "state_json_persisted": true,
            "normalized_sql_shadow_status": "receipt_tables_shadowed",
            "normalized_sql_migration_floor": "0087_add_term_exchange_receipt_event_history.sql",
            "normalized_sql_receipt_tables": [
                "league_term_exchange_receipts",
                "world_term_exchange_receipts"
            ],
            "normalized_sql_receipt_event_tables": [
                "league_term_exchange_receipt_events_v1",
                "world_term_exchange_receipt_events_v1"
            ],
            "sql_shadow_preserves": [
                "status",
                "progression_class",
                "receipt_id",
                "intent_id",
                "backend_kind"
            ],
            "sql_direct_write_status": "typed_sqlx_receipt_upserts_active",
            "sql_direct_write_helper": "upsert_normalized_term_exchange_receipt_tables",
            "sql_direct_write_mode": "typed_sqlx_receipt_upserts_from_repository_snapshot",
            "sql_receipt_event_history_mode": "append_distinct_snapshots_with_sequence_and_hash_chain",
            "progression_source": "ReceiptProgressionClass_prefers_typed_receipts_with_legacy_status_fallback",
            "normalized_receipt_read_model_probe_status": "receipt_projection_objects_exposed_in_world_home_client_feed_and_client_app",
            "runtime_receipt_projection_status": "typed_receipts_drive_world_commerce_recovery_and_sql_read_model_surfaces"
        },
        "current_cex_backend_runtime": {
            "runtime_profile": state.config().runtime_profile.as_str(),
            "ledger_base_url_configured": !state.config().ledger_base_url.trim().is_empty(),
            "ledger_admin_token_configured": state.config().ledger_admin_token.is_some(),
            "identity_binding_required": state.config().require_identity_binding,
            "session_auth_required": state.config().require_session_auth,
            "league_normalized_dual_write_enabled": state.config().league_normalized_dual_write_enabled,
            "league_normalized_read_switch_enabled": state.config().league_normalized_read_switch_enabled,
            "league_normalized_final_cutover_enabled": state.config().league_normalized_final_cutover_enabled
        },
        "legacy_compatibility": {
            "legacy_endpoint": LEGACY_CEX_RUNTIME_PLUGIN_ENDPOINT,
            "legacy_contract_version": LEGACY_CEX_RUNTIME_PLUGIN_CONTRACT_VERSION,
            "legacy_plugin_id": "cex-econ-kernel",
            "status": "upgraded_to_term_exchange_kernel_manifest",
            "primary_endpoint": TERM_EXCHANGE_KERNEL_MANIFEST_ENDPOINT
        },
        "migration_status": {
            "status": "postgres_atomic_intent_receipt_escrow_and_reconciliation_persistence_active",
            "split_strategy": "protocol_first_then_backend_adapter_then_storage_boundary",
            "current_source_of_evidence": "TRNM revision 12 capped value events plus native Bevy input, PostgreSQL restart, seller payout hold and cross-instance idempotency tests",
            "public_player_market": "release_gated_trusted_system_market_only",
            "next_step": "real-user identity recovery drills, custody/listing review, anti-abuse, dispute operations and legal release approval before public listings"
        }
    })
}

pub(super) async fn get_term_exchange_kernel_manifest(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    (
        StatusCode::OK,
        Json(term_exchange_kernel_manifest_json(&state)),
    )
        .into_response()
}

pub(super) async fn post_trnm_economic_intent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<TrnmEconomicIntentRequest>,
) -> Response {
    let player_session = headers
        .get("x-trnm-player-session")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if player_session.is_none() {
        if let Err(response) = authorize_ingress(&headers, state.config()) {
            state.inner.metrics.inc_ingress_auth_failures();
            return response;
        }
    }
    if let Err(error) = payload.intent.validate() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": error}))).into_response();
    }
    let Some(token) = state.config().ledger_admin_token.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "ledger admin token is not configured"})),
        )
            .into_response();
    };
    let url = format!(
        "{}/v1/trnm/economy/intents",
        state.config().ledger_base_url.trim_end_matches('/')
    );
    let mut request = state
        .inner
        .ledger_http
        .post(url)
        .header("x-admin-token", token);
    request = if let Some(session) = player_session {
        request.header("x-trnm-player-session", session)
    } else {
        request.header("x-trnm-system-operation", "true")
    };
    let response = match request.json(&payload).send().await {
        Ok(response) => response,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": format!("persistent ledger unavailable: {error}")})),
            )
                .into_response()
        }
    };
    let status = response.status();
    let body = match read_bounded_ledger_body(response).await {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("persistent ledger response invalid: {error}")})),
            )
                .into_response()
        }
    };
    if !status.is_success() {
        let value = serde_json::from_str::<Value>(&body).unwrap_or(Value::Null);
        return (StatusCode::BAD_GATEWAY, Json(value)).into_response();
    }
    let receipt = match serde_json::from_str::<EconomicReceipt>(&body) {
        Ok(receipt) => receipt,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("invalid persistent ledger receipt: {error}")})),
            )
                .into_response()
        }
    };
    let league_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        record_league_term_exchange_receipt(
            &mut league,
            Some(TermExchangeReceiptState::from(&receipt)),
        );
        league.clone()
    };
    if let Err(response) = persist_league_state(&state, &league_snapshot).await {
        return response;
    }
    (StatusCode::OK, Json(receipt)).into_response()
}

pub(super) async fn post_trnm_wallet_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<TrnmWalletRequest>,
) -> Response {
    let player_session = headers
        .get("x-trnm-player-session")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if player_session.is_none() {
        if let Err(response) = authorize_ingress(&headers, state.config()) {
            state.inner.metrics.inc_ingress_auth_failures();
            return response;
        }
    }
    if payload.actor_id.trim().is_empty() || payload.account_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "actor_id and account_id are required"})),
        )
            .into_response();
    }
    let Some(token) = state.config().ledger_admin_token.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "ledger admin token is not configured"})),
        )
            .into_response();
    };
    let url = format!(
        "{}/v1/trnm/economy/wallet",
        state.config().ledger_base_url.trim_end_matches('/')
    );
    let mut request = state
        .inner
        .ledger_http
        .post(url)
        .header("x-admin-token", token);
    request = if let Some(session) = player_session {
        request.header("x-trnm-player-session", session)
    } else {
        request.header("x-trnm-system-operation", "true")
    };
    let response = match request.json(&payload).send().await {
        Ok(response) => response,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": format!("ledger wallet unavailable: {error}")})),
            )
                .into_response();
        }
    };
    let status = response.status();
    let body = match read_bounded_ledger_body(response).await {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("ledger wallet response invalid: {error}")})),
            )
                .into_response()
        }
    };
    let value = serde_json::from_str::<Value>(&body).unwrap_or(Value::Null);
    if !status.is_success() {
        return (StatusCode::BAD_GATEWAY, Json(value)).into_response();
    }
    match serde_json::from_value::<WalletSnapshot>(value) {
        Ok(snapshot) => (StatusCode::OK, Json(snapshot)).into_response(),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("invalid wallet snapshot: {error}")})),
        )
            .into_response(),
    }
}

pub(super) async fn post_trnm_receipt_projection_rebuild(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(token) = state.config().ledger_admin_token.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "ledger admin token is not configured"})),
        )
            .into_response();
    };
    let url = format!(
        "{}/v1/trnm/economy/receipts",
        state.config().ledger_base_url.trim_end_matches('/')
    );
    let receipts =
        match state
            .inner
            .ledger_http
            .get(url)
            .header("x-admin-token", token)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                let body = match read_bounded_ledger_body(response).await {
                    Ok(body) => body,
                    Err(error) => return (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({"error": format!("invalid receipt rebuild payload: {error}")})),
                    )
                        .into_response(),
                };
                match serde_json::from_str::<Vec<EconomicReceipt>>(&body) {
                    Ok(receipts) => receipts,
                    Err(error) => return (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({"error": format!("invalid receipt rebuild payload: {error}")})),
                    )
                        .into_response(),
                }
            }
            Ok(response) => return (
                StatusCode::BAD_GATEWAY,
                Json(
                    json!({"error": format!("ledger receipt rebuild HTTP {}", response.status())}),
                ),
            )
                .into_response(),
            Err(error) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"error": format!("ledger receipt rebuild unavailable: {error}")})),
                )
                    .into_response()
            }
        };
    let league_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        league.term_exchange_receipts.clear();
        for receipt in &receipts {
            record_league_term_exchange_receipt(
                &mut league,
                Some(TermExchangeReceiptState::from(receipt)),
            );
        }
        league.clone()
    };
    if let Err(response) = persist_league_state(&state, &league_snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "rebuilt": true,
            "authoritative_receipts": receipts.len(),
            "projected_receipts": league_snapshot.term_exchange_receipts.len(),
        })),
    )
        .into_response()
}

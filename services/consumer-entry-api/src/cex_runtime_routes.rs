use super::*;

const TRILLIONNIUM_CEX_RUNTIME_PLUGIN_CONTRACT_VERSION: &str = "trillionnium_cex_runtime_plugin_v1";
const TRILLIONNIUM_CEX_ECON_KERNEL_CONTRACT_VERSION: &str = "trillionnium_cex_econ_kernel_v1";
const TRILLIONNIUM_WORLD_RUNTIME_PLUGIN_HOST_CONTRACT_VERSION: &str =
    "trillionnium_world_runtime_plugin_host_v1";

pub(super) fn cex_runtime_plugin_manifest_json(state: &AppState) -> Value {
    json!({
        "kind": "trillionnium_cex_runtime_plugin_manifest",
        "contract_version": TRILLIONNIUM_CEX_RUNTIME_PLUGIN_CONTRACT_VERSION,
        "plugin_id": "cex-econ-kernel",
        "plugin_name": "CEX Econ Kernel Runtime",
        "runtime_role": "runtime_inserted_econ_system_kernel",
        "host_contract_version": TRILLIONNIUM_WORLD_RUNTIME_PLUGIN_HOST_CONTRACT_VERSION,
        "econ_contract_version": TRILLIONNIUM_CEX_ECON_KERNEL_CONTRACT_VERSION,
        "integration_model": {
            "boundary": "Trillionnium World owns gameplay/world progression; CEX owns economic validity and receipts.",
            "invocation_mode": "world_emits_economic_intent_cex_returns_receipt",
            "world_progression_gate": "world_state_advances_only_after_cex_receipt_or_explicit_recoverable_hold",
            "browser_role": "input_only_no_economic_authority",
            "matrix_role": "command_surface_no_economic_authority"
        },
        "ownership": {
            "trillionnium_world_owns": [
                "world_state",
                "map_topology",
                "player_position",
                "npc_and_agent_presence",
                "combat_and_skill_practice",
                "task_semantics",
                "route_projection",
                "rust_owned_world_ui_fragments"
            ],
            "cex_owns": [
                "identity_resolution",
                "account_scope_resolution",
                "ledger_intent_validation",
                "reserve",
                "escrow_like_hold",
                "seller_settlement",
                "buyer_consume",
                "refund",
                "seller_chargeback",
                "reward_release",
                "review_hold_release",
                "work_order_economy",
                "idempotency",
                "audit_receipts",
                "recovery_and_dead_letter_policy"
            ],
            "shared_contracts": [
                "economic_intent",
                "economic_receipt",
                "world_progression_gate",
                "route_recovery_hint",
                "operator_evidence_package"
            ]
        },
        "capabilities": [
            {
                "capability": "wallet_read_model",
                "authority": "cex",
                "current_endpoint": "/v1/matrix/users/:matrix_user_id/wallet"
            },
            {
                "capability": "league_reward_settlement",
                "authority": "cex",
                "current_endpoints": [
                    "/v1/league/matches/:match_id/submit",
                    "/v1/league/reviews/:reward_id/approve",
                    "/v1/league/players/:matrix_user_id/rewards"
                ]
            },
            {
                "capability": "world_contract_completion_settlement",
                "authority": "cex",
                "current_endpoint": "/v1/world/contracts/:contract_id/complete"
            },
            {
                "capability": "world_commerce_lifecycle",
                "authority": "cex",
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
                "capability": "world_commerce_read_model",
                "authority": "cex",
                "current_endpoint": "/v1/world/commerce"
            }
        ],
        "receipt_policy": {
            "required_for_world_progression": [
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
            "must_not_advance_world_state_on": [
                "failed_bad_response",
                "failed_ledger",
                "failed_network",
                "failed_identity",
                "missing_account",
                "missing_ledger_token"
            ]
        },
        "runtime_requirements": {
            "fail_closed": true,
            "idempotency_required": true,
            "ledger_receipt_required": true,
            "audit_receipt_required": true,
            "recoverable_retry_required": true,
            "world_state_mutation_after_receipt_only": true,
            "runtime_hot_swap_goal": "Trillionnium World can point at a local CEX runtime, remote CEX runtime, or future chain-backed settlement adapter without changing gameplay clients."
        },
        "current_cex_runtime": {
            "runtime_profile": state.config().runtime_profile.as_str(),
            "ledger_base_url_configured": !state.config().ledger_base_url.trim().is_empty(),
            "ledger_admin_token_configured": state.config().ledger_admin_token.is_some(),
            "identity_binding_required": state.config().require_identity_binding,
            "session_auth_required": state.config().require_session_auth,
            "league_normalized_dual_write_enabled": state.config().league_normalized_dual_write_enabled,
            "league_normalized_read_switch_enabled": state.config().league_normalized_read_switch_enabled,
            "league_normalized_final_cutover_enabled": state.config().league_normalized_final_cutover_enabled
        },
        "migration_status": {
            "status": "incubating_inside_consumer_entry_api",
            "split_strategy": "extract_contract_first_then_runtime_adapter_then_storage_boundary",
            "current_source_of_evidence": "CEX local-production run/* gates",
            "next_step": "move world economic calls behind a CexRuntime trait/adapter while preserving current endpoints and E2E evidence"
        }
    })
}

pub(super) async fn get_cex_runtime_plugin_manifest(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    (
        StatusCode::OK,
        Json(cex_runtime_plugin_manifest_json(&state)),
    )
        .into_response()
}

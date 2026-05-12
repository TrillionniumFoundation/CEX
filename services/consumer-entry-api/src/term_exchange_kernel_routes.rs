use super::*;
use term_exchange_protocol::{
    cex_settlement_backend_manifest, protocol_manifest_json, CEX_SETTLEMENT_BACKEND_ID,
    LEGACY_CEX_RUNTIME_PLUGIN_CONTRACT_VERSION, TERM_EXCHANGE_BACKEND_CONTRACT_VERSION,
    TERM_EXCHANGE_KERNEL_CONTRACT_VERSION, TERM_EXCHANGE_KERNEL_ID, TERM_EXCHANGE_PROTOCOL_VERSION,
};

const TRILLIONNIUM_TERM_EXCHANGE_HOST_CONTRACT_VERSION: &str = "trillionnium_term_exchange_host_v1";
const LEGACY_CEX_RUNTIME_PLUGIN_ENDPOINT: &str = "/v1/trillionnium/runtime/cex/manifest";
const TERM_EXCHANGE_KERNEL_MANIFEST_ENDPOINT: &str =
    "/v1/trillionnium/term-exchange/kernel/manifest";

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
            "world_progression_gate": "domain_state_advances_only_after_economic_receipt_allows_progression_or_terminal_skip",
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
            "trillionnium_world_term_owns": [
                "world_state",
                "map_topology",
                "player_position",
                "npc_and_agent_presence",
                "combat_and_skill_practice",
                "task_semantics",
                "route_projection",
                "rust_owned_world_ui_fragments",
                "world_event_to_economic_intent_mapping"
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
            "protocol_crate": "term-exchange-protocol",
            "protocol_version": TERM_EXCHANGE_PROTOCOL_VERSION,
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
                "finalized_at_epoch"
            ],
            "legacy_status_compatibility": true,
            "state_json_persisted": true,
            "normalized_sql_shadow_status": "receipt_tables_shadowed",
            "normalized_sql_migration_floor": "0026_add_term_exchange_receipt_tables.sql",
            "normalized_sql_receipt_tables": [
                "league_term_exchange_receipts",
                "world_term_exchange_receipts"
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
            "sql_direct_write_mode": "typed_sqlx_receipt_upserts_from_repository_snapshot"
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
            "status": "typed_receipt_state_direct_written_to_normalized_sql",
            "split_strategy": "protocol_first_then_backend_adapter_then_storage_boundary",
            "current_source_of_evidence": "CEX local-production run/* gates",
            "next_step": "migrate remaining progression checks from legacy string statuses to ReceiptProgressionClass and add normalized receipt read-model probes"
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

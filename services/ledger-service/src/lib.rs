pub mod account_control;
pub mod api;
pub mod ledger_effects;
pub mod repository;
pub mod state;

use axum::{
    routing::{get, post},
    Router,
};
use state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(api::health))
        .route("/metrics", get(api::metrics))
        .route("/v2/accounts", post(account_control::open_account_v2))
        .route("/v2/accounts/:id", get(account_control::get_account_exact))
        .route("/v2/account-opening-inventories", post(account_control::build_inventory))
        .route(
            "/v2/account-opening-inventories/:run_id/seal",
            post(account_control::seal_inventory),
        )
        .route("/v2/account-projections", post(account_control::capture_projection))
        .route("/v2/account-projections/status/:org_id", get(account_control::projection_status))
        .route("/v2/account-projections/policy", post(account_control::set_read_policy))
        .route("/v2/account-projections/repair", post(account_control::repair_projection))
        .route("/v2/ledger/effects", post(ledger_effects::apply_effect))
        .route("/v2/ledger/effects/:operation_id", get(ledger_effects::get_effect))
        .route("/v2/ledger/traces/:trace_id", get(ledger_effects::list_trace))
        .route("/v1/trnm/economy/readiness", get(api::trnm_economy_readiness))
        .route("/v1/trnm/economy/intents", post(api::post_trnm_economic_intent))
        .route("/v1/trnm/economy/wallet", post(api::post_trnm_wallet_snapshot))
        .route("/v1/trnm/identity/register", post(api::post_trnm_identity_register))
        .route("/v1/trnm/identity/recover", post(api::post_trnm_identity_recover))
        .route("/v1/trnm/identity/status", post(api::post_trnm_identity_status))
        .route("/v1/trnm/identity/session", post(api::post_trnm_player_session_issue))
        .route("/v1/trnm/identity/session/revoke", post(api::post_trnm_player_session_revoke))
        .route("/v1/trnm/identity/session/verify", post(api::post_trnm_player_session_verify))
        .route("/v1/trnm/product/register", post(api::post_trnm_product_register))
        .route("/v1/trnm/product/registration-invites", post(api::post_trnm_product_invite_issue))
        .route("/v1/trnm/product/login", post(api::post_trnm_product_login))
        .route("/v1/trnm/product/credentials/rotate", post(api::post_trnm_product_credential_rotate))
        .route("/v1/trnm/product/appeals", post(api::post_trnm_identity_appeal))
        .route("/v1/trnm/product/appeals/resolve", post(api::post_trnm_identity_appeal_resolve))
        .route("/v1/trnm/economy/entitlements", post(api::post_trnm_value_entitlement_issue))
        .route("/v1/trnm/economy/issuer-keys/status", post(api::post_trnm_entitlement_issuer_key_status))
        .route("/v1/trnm/economy/receipts", get(api::get_trnm_economic_receipts))
        .route("/v1/trnm/economy/maintenance", post(api::post_trnm_economy_maintenance))
        .route("/v1/accounts", post(account_control::legacy_create_account))
        .route("/v1/accounts/:id", get(account_control::get_account_exact))
        .route("/v1/ledger/reserve", post(account_control::gone_legacy_value_write))
        .route("/v1/ledger/consume", post(account_control::gone_legacy_value_write))
        .route("/v1/ledger/refund", post(account_control::gone_legacy_value_write))
        .route("/v1/ledger/grant", post(account_control::gone_legacy_value_write))
        .with_state(state)
}

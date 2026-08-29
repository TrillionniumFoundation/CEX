use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminAuthorizationFailure, AdminPrincipal,
};
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedInventorySealRequest {
    pub signer_key_id: String,
    pub signer_public_key_base64: String,
    pub signature_base64: String,
}

pub async fn seal_inventory(
    Path(run_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<VerifiedInventorySealRequest>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let pool = match state.operation_pool.as_ref() {
        Some(pool) => pool,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "inventory_persistence_unavailable",
                "inventory signature verification requires PostgreSQL",
            )
        }
    };

    let row = sqlx::query(
        "select org_id::text as org_id, status, inventory_digest\
           from public.cex_account_opening_inventory_runs_v1\
          where run_id=$1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await;
    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "inventory_not_found",
                "inventory run not found",
            )
        }
        Err(error) => return database_error_response("load inventory signature target", error),
    };
    let org_id: String = match row.try_get("org_id") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode inventory organization", error),
    };
    if let Err(response) = enforce_org_boundary(&admin, &org_id) {
        return response;
    }
    let status: String = match row.try_get("status") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode inventory status", error),
    };
    let inventory_digest: Option<String> = match row.try_get("inventory_digest") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode inventory digest", error),
    };
    let Some(inventory_digest) = inventory_digest else {
        return error_response(
            StatusCode::CONFLICT,
            "inventory_digest_missing",
            "inventory must be fully built before it can be sealed",
        );
    };
    if status != "draft" && status != "sealed" {
        return error_response(
            StatusCode::CONFLICT,
            "inventory_status_invalid",
            "inventory status is not sealable",
        );
    }

    let public_key_bytes = match STANDARD.decode(request.signer_public_key_base64.trim()) {
        Ok(bytes) if bytes.len() == 32 => bytes,
        _ => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_inventory_public_key",
                "signer_public_key_base64 must decode to exactly 32 bytes",
            )
        }
    };
    let signature_bytes = match STANDARD.decode(request.signature_base64.trim()) {
        Ok(bytes) if bytes.len() == 64 => bytes,
        _ => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_inventory_signature",
                "signature_base64 must decode to exactly 64 bytes",
            )
        }
    };
    let public_key_array: [u8; 32] = match public_key_bytes.as_slice().try_into() {
        Ok(bytes) => bytes,
        Err(_) => unreachable!("public key length was checked"),
    };
    let verifying_key = match VerifyingKey::from_bytes(&public_key_array) {
        Ok(key) => key,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_inventory_public_key",
                "signer public key is not a valid Ed25519 key",
            )
        }
    };
    let signature = match Signature::from_slice(&signature_bytes) {
        Ok(signature) => signature,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_inventory_signature",
                "signature is not a valid Ed25519 signature",
            )
        }
    };
    if verifying_key
        .verify(inventory_digest.as_bytes(), &signature)
        .is_err()
    {
        return error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "inventory_signature_verification_failed",
            "signature does not verify the exact UTF-8 inventory_digest value",
        );
    }

    let public_key_sha256 = format!(
        "sha256:{}",
        Sha256::digest(&public_key_bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let verification_evidence = json!({
        "verified": true,
        "algorithm": "ed25519",
        "signed_payload": "inventory_digest_utf8",
        "inventory_digest": inventory_digest,
        "verifier": "ledger-service",
        "verifier_version": env!("CARGO_PKG_VERSION"),
    });

    let result = sqlx::query_scalar::<_, String>(
        "select to_jsonb(public.cex_seal_account_opening_inventory_v1(\
            $1,$2,$3,$4,$5::jsonb\
        ))::text",
    )
    .bind(run_id)
    .bind(request.signer_key_id.trim())
    .bind(public_key_sha256)
    .bind(request.signature_base64.trim())
    .bind(verification_evidence.to_string())
    .fetch_one(pool)
    .await;

    match result {
        Ok(body) => match serde_json::from_str::<Value>(&body) {
            Ok(value) => (StatusCode::OK, Json(value)).into_response(),
            Err(error) => {
                eprintln!("ledger-service: decode sealed inventory response failed: {error}");
                error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "inventory_response_invalid",
                    "sealed inventory response could not be verified",
                )
            }
        },
        Err(error) => database_error_response("seal verified inventory", error),
    }
}

#[allow(clippy::result_large_err)]
fn authorize_ledger_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, Response> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        &["ledger:manage"],
        admin_principal_has_scope,
        "ledger admin token not configured",
        Some("configure a scoped ledger principal before sealing inventory"),
    )
    .cloned()
    .map_err(|error: AdminAuthorizationFailure| error.into_response().into_response())
}

#[allow(clippy::result_large_err)]
fn enforce_org_boundary(admin: &AdminPrincipal, org_id: &str) -> Result<(), Response> {
    if admin.org_ids.is_empty() || admin_principal_allows_org(admin, org_id) {
        return Ok(());
    }
    Err(error_response(
        StatusCode::FORBIDDEN,
        "ledger_org_forbidden",
        "authenticated principal is not authorized for this organization",
    ))
}

fn database_error_response(context: &str, error: sqlx::Error) -> Response {
    let mut status = StatusCode::SERVICE_UNAVAILABLE;
    let mut code = "inventory_seal_unavailable";
    if let Some(database_error) = error.as_database_error() {
        let message = database_error.message().to_ascii_lowercase();
        if database_error.code().as_deref() == Some("23505") || message.contains("collision") {
            status = StatusCode::CONFLICT;
            code = "inventory_seal_collision";
        } else if database_error.code().as_deref() == Some("P0002")
            || message.contains("not found")
        {
            status = StatusCode::NOT_FOUND;
            code = "inventory_not_found";
        } else if message.contains("requires") || message.contains("invalid") {
            status = StatusCode::BAD_REQUEST;
            code = "inventory_seal_invalid";
        }
    }
    eprintln!("ledger-service: {context} failed: {error}");
    error_response(status, code, "inventory seal could not be completed")
}

fn error_response(status: StatusCode, code: &'static str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": "inventory seal rejected",
            "code": code,
            "message": message,
            "schema_version": "cex.account.inventory.seal.v1",
        })),
    )
        .into_response()
}

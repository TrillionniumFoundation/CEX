use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminAuthorizationFailure, AdminPrincipal,
};
use shared_types::ledger_v2::{
    LedgerEffectRequestV1, LedgerOperationKind, LEDGER_EFFECT_SCHEMA_V1,
};
use uuid::Uuid;

use crate::{
    account_control::{self, OpenAccountV2Request},
    ledger_effects,
    state::{AccountRecord, AppState, LedgerEntryRecord},
};

const ACCOUNT_OPENING_SCHEMA_V1: &str = "cex.account.opening.v1";
const ACCOUNT_MONEY_SCHEMA_V2: &str = "cex.account.money.v2";
const TRACE_RESULT_LIMIT: usize = 200;

pub async fn open_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OpenAccountV2Request>,
) -> Response {
    if state.operation_pool.is_some() || !state.memory_fallback_enabled() {
        return account_control::open_account_v2(State(state), headers, Json(request)).await;
    }
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, &request.org_id.to_string()) {
        return response;
    }
    if request.account_id.is_nil() || request.org_id.is_nil() || request.trace_id.is_nil() {
        return account_error(
            StatusCode::BAD_REQUEST,
            "invalid_exact_account_request",
            "account_id, org_id and trace_id must be non-nil UUIDs",
        );
    }
    let account_type = match normalized_component(&request.account_type, 128) {
        Some(value) => value,
        None => {
            return account_error(
                StatusCode::BAD_REQUEST,
                "invalid_exact_account_request",
                "account_type must match [A-Za-z0-9._:-]{1,128}",
            )
        }
    };
    let currency_unit = match normalized_lower_component(&request.currency_unit, 32) {
        Some(value) => value,
        None => {
            return account_error(
                StatusCode::BAD_REQUEST,
                "invalid_exact_account_request",
                "currency_unit must match [a-z0-9._-]{1,32}",
            )
        }
    };
    if request.currency_scale > 6 {
        return account_error(
            StatusCode::BAD_REQUEST,
            "invalid_exact_account_request",
            "currency_scale must be between 0 and 6",
        );
    }
    let opening_minor = match parse_nonnegative_minor(&request.opening_minor) {
        Ok(value) => value,
        Err(message) => {
            return account_error(
                StatusCode::BAD_REQUEST,
                "invalid_exact_account_request",
                message,
            )
        }
    };
    let scope = match normalized_component(&request.idempotency_scope, 160) {
        Some(value) => value,
        None => {
            return account_error(
                StatusCode::BAD_REQUEST,
                "invalid_exact_account_request",
                "idempotency_scope is invalid",
            )
        }
    };
    let key = request.idempotency_key.trim().to_string();
    if key.is_empty() || key.chars().count() > 256 || key.chars().any(char::is_control) {
        return account_error(
            StatusCode::BAD_REQUEST,
            "invalid_exact_account_request",
            "idempotency_key must contain 1..256 non-control characters",
        );
    }
    let source_principal = admin.actor_id.trim().to_string();
    if source_principal.is_empty() || source_principal.chars().count() > 256 {
        return account_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_exact_account_principal",
            "authenticated principal cannot be represented",
        );
    }

    let operation_id = deterministic_uuid(&format!(
        "ledger-account-opening:{}:{}:{}:{}",
        request.org_id, request.account_id, scope, key
    ));
    let genesis_entry_id = (opening_minor > 0)
        .then(|| deterministic_uuid(&format!("ledger-genesis-entry:{operation_id}")));
    let canonical_request = json!({
        "account_id": request.account_id,
        "org_id": request.org_id,
        "trace_id": request.trace_id,
        "operation_id": operation_id,
        "idempotency_scope": scope,
        "idempotency_key": key,
        "account_type": account_type,
        "currency_unit": currency_unit,
        "currency_scale": request.currency_scale,
        "opening_minor": opening_minor.to_string(),
        "source_principal": source_principal,
        "schema_version": ACCOUNT_OPENING_SCHEMA_V1,
    });
    let request_fingerprint = sha256_json(&canonical_request);
    let opening_key = exact_key(
        request.org_id.to_string().as_str(),
        canonical_request["idempotency_scope"]
            .as_str()
            .expect("canonical scope"),
        canonical_request["idempotency_key"]
            .as_str()
            .expect("canonical key"),
    );

    let mut exact = state.exact_memory.write().await;
    let by_account = exact
        .account_openings_by_account
        .get(&request.account_id)
        .cloned();
    let by_key = exact
        .account_opening_account_by_key
        .get(&opening_key)
        .copied();
    if let (Some(existing), Some(key_account)) = (&by_account, by_key) {
        if key_account != request.account_id || existing.get("request") != Some(&canonical_request)
        {
            return account_error(
                StatusCode::CONFLICT,
                "ledger_account_control_collision",
                "account opening identity collides with different immutable content",
            );
        }
        return (StatusCode::OK, Json(opening_response(existing, true))).into_response();
    }
    if let Some(existing) = by_account {
        if existing.get("request") != Some(&canonical_request) {
            return account_error(
                StatusCode::CONFLICT,
                "ledger_account_control_collision",
                "account exists with a different exact opening contract",
            );
        }
        return (StatusCode::OK, Json(opening_response(&existing, true))).into_response();
    }
    if let Some(key_account) = by_key {
        let Some(existing) = exact.account_openings_by_account.get(&key_account) else {
            return account_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "exact_memory_invariant_broken",
                "opening key points to missing account evidence",
            );
        };
        if key_account != request.account_id || existing.get("request") != Some(&canonical_request)
        {
            return account_error(
                StatusCode::CONFLICT,
                "ledger_account_control_collision",
                "scoped opening key collides with different immutable content",
            );
        }
        return (StatusCode::OK, Json(opening_response(existing, true))).into_response();
    }
    if state
        .accounts
        .read()
        .await
        .contains_key(&request.account_id)
    {
        return account_error(
            StatusCode::CONFLICT,
            "ledger_account_control_collision",
            "account exists without matching exact opening evidence",
        );
    }

    let now = Utc::now().to_rfc3339();
    let record = json!({
        "request": canonical_request.clone(),
        "account_state": {
            "account_id": request.account_id,
            "org_id": request.org_id,
            "account_type": canonical_request["account_type"],
            "currency_unit": canonical_request["currency_unit"],
            "currency_scale": request.currency_scale,
            "balance_minor": opening_minor,
            "reserved_minor": 0,
            "status": "active",
            "created_at": now,
        },
        "opening": {
            "trace_id": request.trace_id,
            "operation_id": operation_id,
            "idempotency_scope": canonical_request["idempotency_scope"],
            "idempotency_key": canonical_request["idempotency_key"],
            "opening_minor": opening_minor.to_string(),
            "opening_kind": if opening_minor > 0 { "genesis" } else { "zero_open" },
            "genesis_entry_id": genesis_entry_id,
            "source_principal": canonical_request["source_principal"],
            "request_fingerprint": request_fingerprint,
            "created_at": now,
        },
    });

    exact
        .account_opening_account_by_key
        .insert(opening_key, request.account_id);
    exact
        .account_openings_by_account
        .insert(request.account_id, record.clone());

    let account = AccountRecord {
        account_id: request.account_id,
        org_id: request.org_id.to_string(),
        account_type: canonical_request["account_type"]
            .as_str()
            .expect("account type")
            .to_string(),
        currency_unit: canonical_request["currency_unit"]
            .as_str()
            .expect("currency")
            .to_string(),
        balance: minor_to_f64(opening_minor, request.currency_scale),
        reserved: 0.0,
    };
    state
        .accounts
        .write()
        .await
        .insert(request.account_id, account);
    if opening_minor > 0 {
        state.entries.write().await.push(LedgerEntryRecord {
            entry_id: genesis_entry_id.expect("positive opening has genesis"),
            account_id: request.account_id,
            action: "genesis".to_string(),
            amount: minor_to_f64(opening_minor, request.currency_scale),
            reference_id: Some(request.account_id.to_string()),
            idempotency_key: Some(
                canonical_request["idempotency_key"]
                    .as_str()
                    .expect("opening key")
                    .to_string(),
            ),
        });
    }

    (StatusCode::CREATED, Json(opening_response(&record, false))).into_response()
}

pub async fn get_account(
    Path(account_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if state.operation_pool.is_some() || !state.memory_fallback_enabled() {
        return account_control::get_account_exact(Path(account_id), State(state), headers).await;
    }
    if account_id.is_nil() {
        return account_error(
            StatusCode::BAD_REQUEST,
            "invalid_account_id",
            "account_id must be non-nil",
        );
    }
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:read", "ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let exact = state.exact_memory.read().await;
    let Some(record) = exact.account_openings_by_account.get(&account_id) else {
        return account_error(
            StatusCode::NOT_FOUND,
            "exact_account_not_found",
            "account not found",
        );
    };
    let org_id = record["account_state"]["org_id"]
        .as_str()
        .expect("exact-memory org UUID");
    if let Err(response) = enforce_org_boundary(&admin, org_id) {
        return response;
    }
    (StatusCode::OK, Json(money_response(record))).into_response()
}

pub async fn apply_effect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LedgerEffectRequestV1>,
) -> Response {
    if state.operation_pool.is_some() || !state.memory_fallback_enabled() {
        return ledger_effects::apply_effect(State(state), headers, Json(request)).await;
    }
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(error) = request.validate(state.require_explicit_ledger_trace) {
        return ledger_error(StatusCode::BAD_REQUEST, error.code(), &error.to_string());
    }
    let money = match request.money() {
        Ok(money) => money,
        Err(error) => {
            return ledger_error(StatusCode::BAD_REQUEST, error.code(), &error.to_string())
        }
    };
    let scope = request.idempotency_scope.trim().to_string();
    let key = request.idempotency_key.trim().to_string();
    let operation_id = request
        .operation_id
        .unwrap_or_else(|| deterministic_uuid(&format!("cex:ledger-operation:{scope}:{key}")));
    let trace_id = request.trace_id.unwrap_or(operation_id);
    let provenance_mode = if request.trace_id.is_some() {
        "explicit"
    } else {
        "operation_scoped_compatibility"
    };
    let source_principal = admin.actor_id.trim().to_string();
    let canonical_request = json!({
        "account_id": request.account_id,
        "trace_id": trace_id,
        "operation_id": operation_id,
        "operation_kind": request.operation_kind.as_str(),
        "currency_unit": money.currency.clone(),
        "currency_scale": money.scale,
        "amount_minor": money.minor_units.to_string(),
        "reference_type": request.reference_type.as_deref().map(str::trim),
        "reference_id": request.reference_id,
        "idempotency_scope": scope,
        "idempotency_key": key,
        "source_service": "ledger-service",
        "source_principal": source_principal,
        "schema_version": LEDGER_EFFECT_SCHEMA_V1,
        "provenance_mode": provenance_mode,
    });
    let scoped_key = exact_key(
        "ledger-effect",
        canonical_request["idempotency_scope"]
            .as_str()
            .expect("scope"),
        canonical_request["idempotency_key"].as_str().expect("key"),
    );
    let request_fingerprint = sha256_json(&canonical_request);

    let mut exact = state.exact_memory.write().await;
    let by_operation = exact.effects_by_operation.get(&operation_id).cloned();
    let by_key = exact.effect_operation_by_key.get(&scoped_key).copied();
    if let (Some(existing), Some(key_operation)) = (&by_operation, by_key) {
        if key_operation != operation_id || existing.get("request") != Some(&canonical_request) {
            return ledger_error(
                StatusCode::CONFLICT,
                "ledger_operation_collision",
                "operation ID and scoped key identify different immutable effects",
            );
        }
        return effect_replay_response(existing);
    }
    if let Some(existing) = by_operation {
        if existing.get("request") != Some(&canonical_request) {
            return ledger_error(
                StatusCode::CONFLICT,
                "ledger_operation_collision",
                "operation ID collides with different immutable content",
            );
        }
        return effect_replay_response(&existing);
    }
    if let Some(key_operation) = by_key {
        let Some(existing) = exact.effects_by_operation.get(&key_operation) else {
            return ledger_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "exact_memory_invariant_broken",
                "scoped key points to missing effect evidence",
            );
        };
        if key_operation != operation_id || existing.get("request") != Some(&canonical_request) {
            return ledger_error(
                StatusCode::CONFLICT,
                "ledger_operation_collision",
                "scoped key collides with different immutable content",
            );
        }
        return effect_replay_response(existing);
    }

    let Some(opening) = exact
        .account_openings_by_account
        .get_mut(&request.account_id)
    else {
        return ledger_error(
            StatusCode::NOT_FOUND,
            "ledger_account_not_found",
            "account not found",
        );
    };
    let account = opening
        .get_mut("account_state")
        .and_then(Value::as_object_mut)
        .expect("exact-memory account state");
    let org_id = account
        .get("org_id")
        .and_then(Value::as_str)
        .expect("exact-memory org")
        .to_string();
    if let Err(response) = enforce_org_boundary(&admin, &org_id) {
        return response;
    }
    let currency_unit = account
        .get("currency_unit")
        .and_then(Value::as_str)
        .expect("exact-memory currency");
    let currency_scale = account
        .get("currency_scale")
        .and_then(Value::as_u64)
        .expect("exact-memory scale") as u8;
    if currency_unit
        != canonical_request["currency_unit"]
            .as_str()
            .expect("request currency")
        || currency_scale != money.scale
    {
        return ledger_error(
            StatusCode::BAD_REQUEST,
            "ledger_currency_mismatch",
            "request currency/scale does not match the account contract",
        );
    }
    let balance_minor = account
        .get("balance_minor")
        .and_then(Value::as_i64)
        .expect("exact-memory balance");
    let reserved_minor = account
        .get("reserved_minor")
        .and_then(Value::as_i64)
        .expect("exact-memory reserved");
    let amount_minor = money.minor_units;
    let (next_balance, next_reserved, direction) = match request.operation_kind {
        LedgerOperationKind::Reserve => {
            let available = balance_minor
                .checked_sub(reserved_minor)
                .unwrap_or(i64::MIN);
            if available < amount_minor {
                return ledger_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ledger_insufficient_funds",
                    "insufficient available minor units",
                );
            }
            let Some(next_reserved) = reserved_minor.checked_add(amount_minor) else {
                return ledger_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ledger_amount_overflow",
                    "reserved balance overflow",
                );
            };
            (balance_minor, next_reserved, "debit")
        }
        LedgerOperationKind::Consume => {
            if reserved_minor < amount_minor {
                return ledger_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ledger_insufficient_funds",
                    "insufficient reserved minor units",
                );
            }
            let Some(next_balance) = balance_minor.checked_sub(amount_minor) else {
                return ledger_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ledger_amount_overflow",
                    "balance underflow",
                );
            };
            (next_balance, reserved_minor - amount_minor, "debit")
        }
        LedgerOperationKind::Refund => {
            if reserved_minor < amount_minor {
                return ledger_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ledger_insufficient_funds",
                    "insufficient reserved minor units",
                );
            }
            (balance_minor, reserved_minor - amount_minor, "credit")
        }
        LedgerOperationKind::Grant => {
            let Some(next_balance) = balance_minor.checked_add(amount_minor) else {
                return ledger_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ledger_amount_overflow",
                    "balance overflow",
                );
            };
            (next_balance, reserved_minor, "credit")
        }
    };
    account.insert("balance_minor".to_string(), json!(next_balance));
    account.insert("reserved_minor".to_string(), json!(next_reserved));

    let entry_id = deterministic_uuid(&format!("ledger-entry:{operation_id}"));
    let created_at = Utc::now().to_rfc3339();
    let response = json!({
        "replayed": false,
        "persistent": false,
        "local_dev_fallback": true,
        "account": {
            "account_id": request.account_id,
            "org_id": org_id,
            "account_type": account["account_type"],
            "currency_unit": account["currency_unit"],
            "currency_scale": currency_scale,
            "balance_minor": next_balance,
            "reserved_minor": next_reserved,
        },
        "effect": {
            "entry_id": entry_id,
            "account_id": request.account_id,
            "trace_id": trace_id,
            "operation_id": operation_id,
            "operation_kind": request.operation_kind.as_str(),
            "idempotency_scope": canonical_request["idempotency_scope"],
            "idempotency_key": canonical_request["idempotency_key"],
            "direction": direction,
            "amount_minor": amount_minor,
            "currency_scale": currency_scale,
            "reference_type": canonical_request["reference_type"],
            "reference_id": request.reference_id,
            "source_service": "ledger-service",
            "source_principal": canonical_request["source_principal"],
            "schema_version": LEDGER_EFFECT_SCHEMA_V1,
            "provenance_mode": provenance_mode,
            "request_fingerprint": request_fingerprint,
            "created_at": created_at,
        }
    });
    let stored = json!({
        "request": canonical_request,
        "response": response.clone(),
        "org_id": org_id,
        "trace_id": trace_id,
    });
    exact
        .effect_operation_by_key
        .insert(scoped_key, operation_id);
    exact.effects_by_operation.insert(operation_id, stored);

    if let Some(account_mirror) = state.accounts.write().await.get_mut(&request.account_id) {
        account_mirror.balance = minor_to_f64(next_balance, currency_scale);
        account_mirror.reserved = minor_to_f64(next_reserved, currency_scale);
    }
    state.entries.write().await.push(LedgerEntryRecord {
        entry_id,
        account_id: request.account_id,
        action: request.operation_kind.as_str().to_string(),
        amount: minor_to_f64(amount_minor, currency_scale),
        reference_id: request.reference_id.map(|value| value.to_string()),
        idempotency_key: Some(request.idempotency_key.trim().to_string()),
    });

    (StatusCode::CREATED, Json(response)).into_response()
}

pub async fn get_effect(
    Path(operation_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if state.operation_pool.is_some() || !state.memory_fallback_enabled() {
        return ledger_effects::get_effect(Path(operation_id), State(state), headers).await;
    }
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:read", "ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let exact = state.exact_memory.read().await;
    let Some(stored) = exact.effects_by_operation.get(&operation_id) else {
        return ledger_error(
            StatusCode::NOT_FOUND,
            "ledger_operation_not_found",
            "ledger operation not found",
        );
    };
    let org_id = stored["org_id"].as_str().expect("stored effect org");
    if let Err(response) = enforce_org_boundary(&admin, org_id) {
        return response;
    }
    let mut response = stored["response"].clone();
    response["replayed"] = json!(false);
    (StatusCode::OK, Json(response)).into_response()
}

pub async fn list_trace(
    Path(trace_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if state.operation_pool.is_some() || !state.memory_fallback_enabled() {
        return ledger_effects::list_trace(Path(trace_id), State(state), headers).await;
    }
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:read", "ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let exact = state.exact_memory.read().await;
    let mut effects = Vec::new();
    for stored in exact.effects_by_operation.values() {
        if stored["trace_id"].as_str() != Some(&trace_id.to_string()) {
            continue;
        }
        let org_id = stored["org_id"].as_str().expect("stored effect org");
        if let Err(response) = enforce_org_boundary(&admin, org_id) {
            return response;
        }
        effects.push(stored["response"].clone());
    }
    effects.sort_by(|left, right| {
        left["effect"]["entry_id"]
            .as_str()
            .cmp(&right["effect"]["entry_id"].as_str())
    });
    effects.truncate(TRACE_RESULT_LIMIT);
    (
        StatusCode::OK,
        Json(json!({
            "trace_id": trace_id,
            "limit": TRACE_RESULT_LIMIT,
            "effects": effects,
        })),
    )
        .into_response()
}

fn opening_response(record: &Value, replayed: bool) -> Value {
    let account = &record["account_state"];
    json!({
        "replayed": replayed,
        "persistent": false,
        "local_dev_fallback": true,
        "schema_version": ACCOUNT_OPENING_SCHEMA_V1,
        "account": {
            "account_id": account["account_id"],
            "org_id": account["org_id"],
            "account_type": account["account_type"],
            "currency_unit": account["currency_unit"],
            "currency_scale": account["currency_scale"],
            "balance_minor": account["balance_minor"].as_i64().expect("balance").to_string(),
            "reserved_minor": account["reserved_minor"].as_i64().expect("reserved").to_string(),
            "status": account["status"],
            "created_at": account["created_at"],
        },
        "opening": record["opening"],
    })
}

fn money_response(record: &Value) -> Value {
    let account = &record["account_state"];
    json!({
        "schema_version": ACCOUNT_MONEY_SCHEMA_V2,
        "mode": "shadow",
        "eligible": false,
        "stop_reason": "service_local_exact_memory",
        "persistent": false,
        "local_dev_fallback": true,
        "account_id": account["account_id"],
        "org_id": account["org_id"],
        "currency_unit": account["currency_unit"],
        "currency_scale": account["currency_scale"],
        "balance_minor": account["balance_minor"].as_i64().expect("balance").to_string(),
        "reserved_minor": account["reserved_minor"].as_i64().expect("reserved").to_string(),
        "shadow_projection": Value::Null,
    })
}

fn effect_replay_response(stored: &Value) -> Response {
    let mut response = stored["response"].clone();
    response["replayed"] = json!(true);
    (StatusCode::OK, Json(response)).into_response()
}

fn deterministic_uuid(namespace: &str) -> Uuid {
    let digest = Sha256::digest(namespace.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn sha256_json(value: &Value) -> String {
    format!("sha256:{:x}", Sha256::digest(value.to_string().as_bytes()))
}

fn exact_key(namespace: &str, scope: &str, key: &str) -> String {
    format!("{namespace}\u{0}{scope}\u{0}{key}")
}

fn normalized_component(raw: &str, max: usize) -> Option<String> {
    let value = raw.trim();
    (!value.is_empty()
        && value.len() <= max
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        }))
    .then(|| value.to_string())
}

fn normalized_lower_component(raw: &str, max: usize) -> Option<String> {
    let value = raw.trim();
    (!value.is_empty()
        && value.len() <= max
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        }))
    .then(|| value.to_string())
}

fn parse_nonnegative_minor(raw: &str) -> Result<i64, &'static str> {
    let value = raw.trim();
    if value.is_empty()
        || value.starts_with('+')
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("opening_minor must be a canonical non-negative integer string");
    }
    value
        .parse::<i64>()
        .map_err(|_| "opening_minor is outside the signed 64-bit range")
}

fn minor_to_f64(value: i64, scale: u8) -> f64 {
    value as f64 / 10_i64.pow(u32::from(scale)) as f64
}

#[allow(clippy::result_large_err)]
fn authorize_ledger_admin(
    state: &AppState,
    headers: &HeaderMap,
    required_scopes: &[&str],
) -> Result<AdminPrincipal, Response> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        required_scopes,
        admin_principal_has_scope,
        "ledger admin token not configured",
        Some("configure a scoped ledger principal before using exact-memory APIs"),
    )
    .cloned()
    .map_err(|error: AdminAuthorizationFailure| error.into_response().into_response())
}

#[allow(clippy::result_large_err)]
fn enforce_org_boundary(admin: &AdminPrincipal, org_id: &str) -> Result<(), Response> {
    if admin.org_ids.is_empty() || admin_principal_allows_org(admin, org_id) {
        return Ok(());
    }
    Err(account_error(
        StatusCode::FORBIDDEN,
        "ledger_org_forbidden",
        "authenticated principal is not authorized for this organization",
    ))
}

fn account_error(status: StatusCode, code: &'static str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": "exact account operation rejected",
            "code": code,
            "message": message,
            "schema_version": ACCOUNT_MONEY_SCHEMA_V2,
        })),
    )
        .into_response()
}

fn ledger_error(status: StatusCode, code: &'static str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": "ledger operation rejected",
            "code": code,
            "message": message,
            "schema_version": LEDGER_EFFECT_SCHEMA_V1,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_ids_and_exact_keys_are_stable() {
        assert_eq!(deterministic_uuid("a"), deterministic_uuid("a"));
        assert_ne!(deterministic_uuid("a"), deterministic_uuid("b"));
        assert_ne!(exact_key("a", "b", "c"), exact_key("ab", "", "c"));
    }

    #[test]
    fn exact_minor_text_is_canonical() {
        assert_eq!(parse_nonnegative_minor("0").unwrap(), 0);
        assert_eq!(parse_nonnegative_minor("42").unwrap(), 42);
        assert!(parse_nonnegative_minor("01").is_err());
        assert!(parse_nonnegative_minor("-1").is_err());
    }
}

use super::*;

pub(super) async fn create_chat_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateChatTaskRequest>,
) -> Response {
    state.inner.metrics.inc_task_create_requests();

    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let resolved_identity = match resolve_chat_identity(&state, &payload).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let request_fingerprint = build_chat_request_fingerprint(&payload);
    let authorized_session = match authorize_user_session(
        &state,
        &headers,
        &resolved_identity.scope,
        request_fingerprint.as_str(),
    ) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let replay_key = build_chat_replay_key(&payload);
    if let Some(response) = replay_cached_response(&state, replay_key.as_deref()).await {
        return response;
    }

    if let Err(response) = enforce_rate_limit(
        &state,
        build_chat_rate_limit_key(&payload),
        state.config().rate_limit_max_requests,
        "consumer_entry_chat_task_rate_limited",
        RateLimitBucketKind::SourceScope,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_chat_user_rate_limit_key(&payload),
        state.config().rate_limit_user_max_requests,
        "consumer_entry_chat_user_rate_limited",
        RateLimitBucketKind::User,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_chat_room_rate_limit_key(&payload),
        state.config().rate_limit_room_max_requests,
        "consumer_entry_chat_room_rate_limited",
        RateLimitBucketKind::Room,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_chat_session_rate_limit_key(&payload),
        state.config().rate_limit_session_max_requests,
        "consumer_entry_chat_session_rate_limited",
        RateLimitBucketKind::Session,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_chat_org_rate_limit_key(&payload),
        state.config().rate_limit_org_max_requests,
        "consumer_entry_chat_org_rate_limited",
        RateLimitBucketKind::Org,
    )
    .await
    {
        return response;
    }

    let prompt = match validate_text_payload(&payload.text, state.config().max_text_chars) {
        Ok(prompt) => prompt,
        Err(response) => return response,
    };

    let resolved_account_id = resolved_identity.scope.account_id.clone();
    let source = json!({
        "kind": "chat_task",
        "identity_scope": resolved_identity.scope,
        "identity_resolution": resolved_identity.resolution,
        "user_id": payload.user_id,
        "room_id": payload.room_id,
        "session_id": payload.session_id,
        "org_id": payload.org_id,
        "text": prompt,
        "idempotency_key": payload.idempotency_key,
        "metadata": payload.metadata,
        "session_auth": authorized_session,
    });

    let response = match forward_to_cex_task(
        state.clone(),
        payload.capability_id,
        resolved_account_id,
        source,
        prompt,
    )
    .await
    {
        Ok(response) => response,
        Err(response) => return response,
    };

    remember_replay_response(&state, replay_key.as_deref(), &response).await;
    (StatusCode::ACCEPTED, Json(response)).into_response()
}

pub(super) async fn create_matrix_message_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<MatrixMessageRequest>,
) -> Response {
    state.inner.metrics.inc_task_create_requests();

    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let resolved_identity = match resolve_matrix_identity(&state, &payload).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let request_fingerprint = build_matrix_request_fingerprint(&payload);
    let authorized_session = match authorize_user_session(
        &state,
        &headers,
        &resolved_identity.scope,
        request_fingerprint.as_str(),
    ) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let replay_key = build_matrix_replay_key(&payload);
    if let Some(response) = replay_cached_response(&state, replay_key.as_deref()).await {
        return response;
    }

    if let Err(response) = enforce_rate_limit(
        &state,
        build_matrix_rate_limit_key(&payload),
        state.config().rate_limit_max_requests,
        "consumer_entry_matrix_message_rate_limited",
        RateLimitBucketKind::SourceScope,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        Some(build_matrix_user_rate_limit_key(&payload)),
        state.config().rate_limit_user_max_requests,
        "consumer_entry_matrix_user_rate_limited",
        RateLimitBucketKind::User,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        Some(build_matrix_room_rate_limit_key(&payload)),
        state.config().rate_limit_room_max_requests,
        "consumer_entry_matrix_room_rate_limited",
        RateLimitBucketKind::Room,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_matrix_session_rate_limit_key(&payload),
        state.config().rate_limit_session_max_requests,
        "consumer_entry_matrix_session_rate_limited",
        RateLimitBucketKind::Session,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_matrix_org_rate_limit_key(&payload),
        state.config().rate_limit_org_max_requests,
        "consumer_entry_matrix_org_rate_limited",
        RateLimitBucketKind::Org,
    )
    .await
    {
        return response;
    }

    let prompt = match validate_text_payload(&payload.message, state.config().max_text_chars) {
        Ok(prompt) => prompt,
        Err(response) => return response,
    };

    let resolved_account_id = resolved_identity.scope.account_id.clone();
    let source = json!({
        "kind": "matrix_message",
        "identity_scope": resolved_identity.scope,
        "identity_resolution": resolved_identity.resolution,
        "matrix_user_id": payload.matrix_user_id,
        "room_id": payload.room_id,
        "session_id": payload.session_id,
        "org_id": payload.org_id,
        "event_id": payload.event_id,
        "idempotency_key": payload.idempotency_key,
        "metadata": payload.metadata,
        "session_auth": authorized_session,
    });

    let response = match forward_to_cex_task(
        state.clone(),
        payload.capability_id,
        resolved_account_id,
        source,
        prompt,
    )
    .await
    {
        Ok(response) => response,
        Err(response) => return response,
    };

    remember_replay_response(&state, replay_key.as_deref(), &response).await;
    (StatusCode::ACCEPTED, Json(response)).into_response()
}

pub(super) async fn get_league_home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_home",
            "league": "trillionnium_league",
            "name": "Trillionnium League",
            "season": "preseason-zero",
            "status": "preseason",
            "tagline": "AI Agent esports for real work, skill, and earnings.",
            "player_count": league.players_by_matrix_user.len(),
            "match_count": league.matches.len(),
            "commands": ["/arena", "/quest", "/join <match-id>", "/battle <match-id> <action>", "/rank", "/loadout", "/wallet"]
        })),
    )
        .into_response()
}

pub(super) async fn get_league_world(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_world",
            "league": "trillionnium_league",
            "zones": [
                {"zone_id": "prompt-forge", "name": "Prompt Forge", "status": "open", "theme": "drafting and agent tuning"},
                {"zone_id": "research-wilds", "name": "Research Wilds", "status": "preview", "theme": "sourcing and evidence"},
                {"zone_id": "code-citadel", "name": "Code Citadel", "status": "preview", "theme": "tests and execution"},
                {"zone_id": "audit-sanctum", "name": "Audit Sanctum", "status": "open", "theme": "review and anti-hallucination"},
                {"zone_id": "market-bazaar", "name": "Market Bazaar", "status": "preview", "theme": "bounties and client tasks"}
            ],
            "match_count": league.matches.len(),
            "guild_count": league.guilds.len(),
        })),
    )
        .into_response()
}

pub(super) async fn forward_to_cex_task(
    state: AppState,
    capability_id: Option<String>,
    account_id: Option<String>,
    source: Value,
    prompt: String,
) -> Result<ConsumerTaskResponse, Response> {
    let capability_id = capability_id
        .or_else(|| state.config().default_capability_id.clone())
        .filter(|v| !v.trim().is_empty());

    let account_id = account_id
        .or_else(|| state.config().default_account_id.clone())
        .filter(|v| !v.trim().is_empty());

    let mut body = json!({
        "prompt": prompt,
    });

    if let Some(capability_id) = capability_id {
        body["capability_id"] = Value::String(capability_id);
    }
    if let Some(account_id) = account_id {
        body["account_id"] = Value::String(account_id);
    }

    let url = format!(
        "{}/v1/invocations",
        state.config().cex_gateway_base_url.trim_end_matches('/')
    );

    let response = match state
        .inner
        .http
        .post(url)
        .header("x-api-key", state.config().cex_gateway_api_key.clone())
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorBody {
                    error: format!("failed to reach cex gateway: {err}"),
                }),
            )
                .into_response())
        }
    };

    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorBody {
                    error: format!("cex gateway returned non-json response: {err}"),
                }),
            )
                .into_response())
        }
    };

    if !status.is_success() {
        return Err((status, Json(value)).into_response());
    }

    let task_id = value
        .get("invocation_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let invocation_status = value
        .get("status")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let consumer_status = invocation_status
        .as_deref()
        .map(project_consumer_status)
        .unwrap_or("received")
        .to_string();

    Ok(ConsumerTaskResponse {
        task_id,
        consumer_status,
        invocation_status,
        execution: value.get("execution").cloned(),
        trace: value.get("trace").cloned(),
        request: value.get("request").cloned(),
        source,
        raw: value,
    })
}

pub(super) async fn get_chat_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    state.inner.metrics.inc_task_lookup_requests();

    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let url = format!(
        "{}/v1/invocations/{}",
        state.config().cex_gateway_base_url.trim_end_matches('/'),
        id
    );

    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string)
        .unwrap_or_else(|| state.config().cex_gateway_api_key.clone());

    let response = match state
        .inner
        .http
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorBody {
                    error: format!("failed to reach cex gateway: {err}"),
                }),
            )
                .into_response()
        }
    };

    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorBody {
                    error: format!("cex gateway returned non-json response: {err}"),
                }),
            )
                .into_response()
        }
    };

    if !status.is_success() {
        return (status, Json(value)).into_response();
    }

    let invocation_status = value
        .get("status")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    (
        StatusCode::OK,
        Json(ConsumerTaskResponse {
            task_id: id,
            consumer_status: invocation_status
                .as_deref()
                .map(project_consumer_status)
                .unwrap_or("received")
                .to_string(),
            invocation_status,
            execution: value.get("execution").cloned(),
            trace: value.get("trace").cloned(),
            request: value.get("request").cloned(),
            source: json!({ "kind": "task_lookup" }),
            raw: value,
        }),
    )
        .into_response()
}

/// Exact money projection returned by the ledger account read endpoint.
///
/// The ledger's current contract is expressed in integer minor units. Keep
/// all arithmetic in that representation and only turn it into a decimal
/// string at the response/display boundary. Older ledger deployments used
/// decimal balance/reserved fields; those are accepted by the explicit
/// compatibility parser below, but are never used for arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WalletMoneyProjection {
    currency_scale: u8,
    balance_minor: i64,
    reserved_minor: i64,
    available_minor: i64,
}

fn parse_currency_scale(value: Option<&Value>, required: bool) -> Result<u8, String> {
    let Some(value) = value else {
        return if required {
            Err("currency_scale is required for exact money responses".to_string())
        } else {
            // The pre-minor-unit endpoint used the ledger's historical
            // six-decimal credit precision.
            Ok(6)
        };
    };

    let raw = match value {
        Value::Number(number) => number.to_string(),
        Value::String(raw) => raw.trim().to_string(),
        _ => return Err("currency_scale must be an integer".to_string()),
    };
    let parsed = raw
        .parse::<u16>()
        .map_err(|_| "currency_scale must be an integer".to_string())?;
    u8::try_from(parsed)
        .ok()
        .filter(|scale| *scale <= 6)
        .ok_or_else(|| "currency_scale must be between 0 and 6".to_string())
}

fn value_text(value: &Value, field: &str) -> Result<String, String> {
    match value {
        Value::String(raw) => Ok(raw.trim().to_string()),
        Value::Number(number) => Ok(number.to_string()),
        _ => Err(format!("{field} must be a decimal number or string")),
    }
}

fn parse_minor_integer(value: &Value, field: &str) -> Result<i64, String> {
    let raw = value_text(value, field)?;
    if raw.is_empty() || raw.starts_with('-') || raw.starts_with('+') {
        return Err(format!("{field} must be a non-negative integer"));
    }
    if !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{field} must be a non-negative integer"));
    }
    let parsed = raw
        .parse::<u128>()
        .map_err(|_| format!("{field} is outside the supported integer range"))?;
    i64::try_from(parsed).map_err(|_| format!("{field} is outside the supported integer range"))
}

fn checked_pow10(power: u32) -> Option<i128> {
    // Bound untrusted scientific-notation exponents before entering the loop.
    // A larger factor cannot fit the i64 minor-unit contract anyway.
    if power > 38 {
        return None;
    }
    let mut result = 1_i128;
    for _ in 0..power {
        result = result.checked_mul(10)?;
    }
    Some(result)
}

/// Parse a legacy major-unit decimal into minor units without doing any
/// floating-point arithmetic. Scientific notation is accepted because a JSON
/// number may be rendered that way by a proxy; values that cannot be
/// represented exactly at currency_scale are rejected.
fn parse_legacy_major_to_minor(
    value: &Value,
    currency_scale: u8,
    field: &str,
) -> Result<i64, String> {
    let raw = value_text(value, field)?;
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('-') || raw.starts_with('+') {
        return Err(format!("{field} must be a non-negative decimal"));
    }

    let (mantissa, exponent) = match raw.find(['e', 'E']) {
        Some(index) => {
            let exponent = raw[index + 1..]
                .parse::<i64>()
                .map_err(|_| format!("{field} has an invalid exponent"))?;
            (&raw[..index], exponent)
        }
        None => (raw, 0),
    };
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (mantissa, ""),
    };
    if (integer.is_empty() && fraction.is_empty())
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("{field} has an invalid decimal"));
    }

    let digits = format!("{integer}{fraction}");
    let coefficient = digits
        .parse::<i128>()
        .map_err(|_| format!("{field} is outside the supported integer range"))?;
    let shift = exponent
        .checked_sub(i64::try_from(fraction.len()).map_err(|_| format!("{field} is too long"))?)
        .and_then(|value| value.checked_add(i64::from(currency_scale)))
        .ok_or_else(|| format!("{field} exponent is outside the supported range"))?;

    let scaled = if shift >= 0 {
        let power = u32::try_from(shift).map_err(|_| format!("{field} is too large"))?;
        coefficient
            .checked_mul(checked_pow10(power).ok_or_else(|| format!("{field} is too large"))?)
            .ok_or_else(|| format!("{field} is outside the supported integer range"))?
    } else {
        let magnitude = shift
            .checked_neg()
            .ok_or_else(|| format!("{field} exponent is outside the supported range"))?;
        let power = u32::try_from(magnitude).map_err(|_| format!("{field} is too small"))?;
        let divisor = checked_pow10(power).ok_or_else(|| format!("{field} is too small"))?;
        if coefficient % divisor != 0 {
            return Err(format!("{field} has more precision than currency_scale"));
        }
        coefficient / divisor
    };

    i64::try_from(scaled).map_err(|_| format!("{field} is outside the supported integer range"))
}

fn project_wallet_money(account: &Value) -> Result<WalletMoneyProjection, String> {
    let (balance_minor, reserved_minor, currency_scale) =
        match (account.get("balance_minor"), account.get("reserved_minor")) {
            (Some(balance), Some(reserved)) => (
                parse_minor_integer(balance, "balance_minor")?,
                parse_minor_integer(reserved, "reserved_minor")?,
                parse_currency_scale(account.get("currency_scale"), true)?,
            ),
            (None, None) => {
                let currency_scale = parse_currency_scale(account.get("currency_scale"), false)?;
                (
                    parse_legacy_major_to_minor(
                        account
                            .get("balance")
                            .ok_or_else(|| "balance is missing".to_string())?,
                        currency_scale,
                        "balance",
                    )?,
                    parse_legacy_major_to_minor(
                        account
                            .get("reserved")
                            .ok_or_else(|| "reserved is missing".to_string())?,
                        currency_scale,
                        "reserved",
                    )?,
                    currency_scale,
                )
            }
            _ => {
                return Err("balance_minor and reserved_minor must be provided together".to_string())
            }
        };

    let available_minor = balance_minor
        .checked_sub(reserved_minor)
        .ok_or_else(|| "reserved_minor exceeds balance_minor".to_string())?;
    if available_minor < 0 {
        return Err("reserved_minor exceeds balance_minor".to_string());
    }

    Ok(WalletMoneyProjection {
        currency_scale,
        balance_minor,
        reserved_minor,
        available_minor,
    })
}

fn format_minor_units(minor: i64, currency_scale: u8) -> String {
    let negative = minor < 0;
    let magnitude = if negative {
        (-(minor as i128)) as u128
    } else {
        minor as u128
    };
    let digits = magnitude.to_string();
    if currency_scale == 0 {
        return if negative {
            format!("-{digits}")
        } else {
            digits
        };
    }

    let scale = usize::from(currency_scale);
    let padded = if digits.len() <= scale {
        format!("{}{}", "0".repeat(scale + 1 - digits.len()), digits)
    } else {
        digits
    };
    let split = padded.len() - scale;
    let value = format!("{}.{}", &padded[..split], &padded[split..]);
    if negative {
        format!("-{value}")
    } else {
        value
    }
}

fn format_minor_summary(minor: i64, currency_scale: u8) -> String {
    let value = format_minor_units(minor, currency_scale);
    let Some((whole, fraction)) = value.split_once('.') else {
        return format!("{value}.00");
    };
    let mut fraction = fraction.trim_end_matches('0').to_string();
    while fraction.len() < 2 {
        fraction.push('0');
    }
    format!("{whole}.{fraction}")
}

pub(super) async fn get_matrix_wallet(
    Path(matrix_user_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let room_id = query.get("room_id").cloned().unwrap_or_default();
    let payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.clone(),
        room_id: room_id.clone(),
        session_id: query.get("session_id").cloned(),
        org_id: query.get("org_id").cloned(),
        message: "wallet lookup".to_string(),
        capability_id: None,
        account_id: query.get("account_id").cloned(),
        event_id: None,
        idempotency_key: None,
        metadata: None,
    };

    let resolved_identity = match resolve_matrix_identity(&state, &payload).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };

    let Some(account_id) = resolved_identity.scope.account_id.clone() else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "wallet account not resolved",
                "identity_scope": resolved_identity.scope,
                "identity_resolution": resolved_identity.resolution,
            })),
        )
            .into_response();
    };

    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "consumer-entry ledger admin token not configured",
                "message": "set CONSUMER_ENTRY_LEDGER_ADMIN_TOKEN or LEDGER_ADMIN_TOKENS_JSON to enable wallet projection",
            })),
        )
            .into_response();
    };

    let url = format!(
        "{}/v1/accounts/{}",
        state.config().ledger_base_url.trim_end_matches('/'),
        account_id
    );
    let response = match state
        .inner
        .ledger_http
        .get(url)
        .header("x-admin-token", ledger_admin_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to reach ledger-service: {err}") })),
            )
                .into_response()
        }
    };

    let status = response.status();
    let body =
        match read_bounded_ledger_body(response).await {
            Ok(body) => body,
            Err(err) => return (
                StatusCode::BAD_GATEWAY,
                Json(
                    json!({ "error": format!("ledger-service returned invalid response: {err}") }),
                ),
            )
                .into_response(),
        };
    let account =
        match serde_json::from_str::<Value>(&body) {
            Ok(value) => value,
            Err(err) => return (
                StatusCode::BAD_GATEWAY,
                Json(
                    json!({ "error": format!("ledger-service returned non-json response: {err}") }),
                ),
            )
                .into_response(),
        };

    if !status.is_success() {
        return (status, Json(account)).into_response();
    }

    let money = match project_wallet_money(&account) {
        Ok(money) => money,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": "ledger account money response invalid",
                    "message": error,
                    "account_id": account_id,
                })),
            )
                .into_response();
        }
    };
    let currency_unit = account
        .get("currency_unit")
        .and_then(Value::as_str)
        .unwrap_or("credit");
    let balance_display = format_minor_units(money.balance_minor, money.currency_scale);
    let reserved_display = format_minor_units(money.reserved_minor, money.currency_scale);
    let available_display = format_minor_units(money.available_minor, money.currency_scale);

    (
        StatusCode::OK,
        Json(json!({
            "kind": "wallet_projection",
            "matrix_user_id": matrix_user_id,
            "room_id": room_id,
            "identity_scope": resolved_identity.scope,
            "identity_resolution": resolved_identity.resolution,
            "account": {
                "account_id": account_id,
                "account_type": account.get("account_type").cloned().unwrap_or(Value::Null),
                "currency_unit": currency_unit,
                "currency_scale": money.currency_scale,
                "balance_minor": money.balance_minor.to_string(),
                "reserved_minor": money.reserved_minor.to_string(),
                "available_minor": money.available_minor.to_string(),
                // Legacy keys remain available as exact decimal strings for
                // clients that have not migrated to *_minor yet.
                "balance": balance_display,
                "reserved": reserved_display,
                "available": available_display,
                "raw": account,
            },
            "package": {
                "code": "local-production-basic",
                "name": "Local Production Credits",
                "status": "active",
                "billing_model": "credit_wallet",
                "features": ["chat_tasks", "matrix_entry", "provider_dispatch"]
            },
            "display": {
                "title": "CEX 钱包 / 套餐",
                "summary": format!(
                    "可用 {} {}，已预留 {}，总额 {}",
                    format_minor_summary(money.available_minor, money.currency_scale),
                    currency_unit,
                    format_minor_summary(money.reserved_minor, money.currency_scale),
                    format_minor_summary(money.balance_minor, money.currency_scale),
                ),
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod wallet_money_tests {
    use super::*;

    #[test]
    fn exact_minor_response_is_projected_without_float_fields() {
        let account = json!({
            "account_id": "00000000-0000-0000-0000-000000000001",
            "currency_unit": "credit",
            "currency_scale": 6,
            "balance_minor": "123456789",
            "reserved_minor": "456789"
        });
        let projection = project_wallet_money(&account).expect("exact response should decode");
        assert_eq!(
            projection,
            WalletMoneyProjection {
                currency_scale: 6,
                balance_minor: 123_456_789,
                reserved_minor: 456_789,
                available_minor: 123_000_000,
            }
        );
        assert_eq!(
            format_minor_units(projection.available_minor, 6),
            "123.000000"
        );
        assert_eq!(
            format_minor_summary(projection.available_minor, 6),
            "123.00"
        );
    }

    #[test]
    fn legacy_decimal_response_is_converted_at_compatibility_boundary() {
        let account = json!({
            "balance": 100.25,
            "reserved": "0.125000",
            "currency_scale": 6
        });
        let projection = project_wallet_money(&account).expect("legacy response should decode");
        assert_eq!(projection.balance_minor, 100_250_000);
        assert_eq!(projection.reserved_minor, 125_000);
        assert_eq!(projection.available_minor, 100_125_000);
    }

    #[test]
    fn fractional_legacy_precision_is_rejected() {
        let account = json!({
            "balance": "1.0000001",
            "reserved": "0",
            "currency_scale": 6
        });
        let error = project_wallet_money(&account).expect_err("precision loss must fail closed");
        assert!(error.contains("more precision"));
    }

    #[test]
    fn malformed_extreme_exponent_and_scale_fail_closed() {
        let exponent = json!({
            "balance": "1e-9223372036854775808",
            "reserved": "0"
        });
        assert!(project_wallet_money(&exponent).is_err());

        let huge_positive_exponent = json!({
            "balance": "1e1000000000",
            "reserved": "0"
        });
        assert!(project_wallet_money(&huge_positive_exponent).is_err());

        let scale = json!({
            "currency_scale": 7,
            "balance_minor": "1",
            "reserved_minor": "0"
        });
        assert!(project_wallet_money(&scale).is_err());
    }
}

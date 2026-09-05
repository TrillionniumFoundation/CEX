use super::*;

#[path = "durable_store.rs"]
mod durable_store;

const MAX_DURABLE_INGRESS_STATE_BYTES: u64 = 32 * 1024 * 1024;

pub(super) fn authorize_ingress(
    headers: &HeaderMap,
    config: &ConsumerEntryConfig,
) -> Result<(), Response> {
    let Some(expected) = config.ingress_token.as_deref() else {
        return Ok(());
    };

    let provided = headers
        .get("x-entry-token")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());

    match provided {
        Some(token) if token == expected => Ok(()),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "missing or invalid entry token" })),
        )
            .into_response()),
    }
}

pub(super) fn normalize_identity_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(super) fn normalize_request_fingerprint_value(value: Option<&str>) -> String {
    normalize_identity_value(value).unwrap_or_default()
}

pub(super) fn normalize_request_fingerprint_text(value: &str) -> String {
    value.trim().to_string()
}

pub(super) fn build_request_fingerprint(parts: &[String]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0x1f]);
    }
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

pub(super) fn build_chat_request_fingerprint(payload: &CreateChatTaskRequest) -> String {
    build_request_fingerprint(&[
        "chat_task".to_string(),
        normalize_request_fingerprint_value(payload.user_id.as_deref()),
        normalize_request_fingerprint_value(payload.room_id.as_deref()),
        normalize_request_fingerprint_value(payload.session_id.as_deref()),
        normalize_request_fingerprint_value(payload.org_id.as_deref()),
        normalize_request_fingerprint_value(payload.account_id.as_deref()),
        normalize_request_fingerprint_value(payload.capability_id.as_deref()),
        normalize_request_fingerprint_value(payload.idempotency_key.as_deref()),
        normalize_request_fingerprint_text(&payload.text),
    ])
}

pub(super) fn build_matrix_request_fingerprint(payload: &MatrixMessageRequest) -> String {
    build_request_fingerprint(&[
        "matrix_message".to_string(),
        normalize_request_fingerprint_value(Some(payload.matrix_user_id.as_str())),
        normalize_request_fingerprint_value(Some(payload.room_id.as_str())),
        normalize_request_fingerprint_value(payload.session_id.as_deref()),
        normalize_request_fingerprint_value(payload.org_id.as_deref()),
        normalize_request_fingerprint_value(payload.account_id.as_deref()),
        normalize_request_fingerprint_value(payload.capability_id.as_deref()),
        normalize_request_fingerprint_value(payload.event_id.as_deref()),
        normalize_request_fingerprint_value(payload.idempotency_key.as_deref()),
        normalize_request_fingerprint_text(&payload.message),
    ])
}

pub(super) fn build_world_action_request_fingerprint(payload: &WorldActionRequest) -> String {
    build_request_fingerprint(&[
        "matrix_message".to_string(),
        normalize_request_fingerprint_value(Some(payload.matrix_user_id.as_str())),
        normalize_request_fingerprint_value(payload.room_id.as_deref()),
        normalize_request_fingerprint_value(None),
        normalize_request_fingerprint_value(None),
        normalize_request_fingerprint_value(payload.account_id.as_deref()),
        normalize_request_fingerprint_value(payload.capability_id.as_deref()),
        normalize_request_fingerprint_value(payload.event_id.as_deref()),
        normalize_request_fingerprint_value(None),
        normalize_request_fingerprint_text(payload.message.as_deref().unwrap_or("")),
    ])
}

pub(super) fn verify_session_auth_claim_field(
    field_name: &str,
    claimed: Option<&str>,
    actual: Option<&str>,
) -> Result<(), Response> {
    let claimed = normalize_identity_value(claimed);
    if claimed.is_none() {
        return Ok(());
    }

    let actual = normalize_identity_value(actual);
    if claimed == actual {
        return Ok(());
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "signed session claim mismatch",
            "field": field_name,
            "claimed": claimed,
            "actual": actual,
        })),
    )
        .into_response())
}

pub(super) fn sign_user_session_assertion(
    assertion_b64: &str,
    secret: &str,
) -> Result<String, Response> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "invalid session auth secret configuration"
            })),
        )
            .into_response()
    })?;
    mac.update(assertion_b64.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

pub(super) fn authorize_user_session(
    state: &AppState,
    headers: &HeaderMap,
    scope: &IdentityScope,
    expected_request_fingerprint: &str,
) -> Result<Option<AuthorizedUserSession>, Response> {
    let assertion = headers
        .get(USER_SESSION_ASSERTION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let signature = headers
        .get(USER_SESSION_SIGNATURE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let provided = assertion.is_some() || signature.is_some();

    if !state.config().require_session_auth && !provided {
        return Ok(None);
    }

    let assertion = assertion.ok_or_else(|| {
        state.inner.metrics.inc_session_auth_failures();
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "missing signed user session assertion",
                "assertion_header": USER_SESSION_ASSERTION_HEADER,
                "signature_header": USER_SESSION_SIGNATURE_HEADER,
            })),
        )
            .into_response()
    })?;
    let signature = signature.ok_or_else(|| {
        state.inner.metrics.inc_session_auth_failures();
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "missing signed user session signature",
                "assertion_header": USER_SESSION_ASSERTION_HEADER,
                "signature_header": USER_SESSION_SIGNATURE_HEADER,
            })),
        )
            .into_response()
    })?;

    let assertion_bytes = match URL_SAFE_NO_PAD.decode(assertion) {
        Ok(value) => value,
        Err(_) => {
            state.inner.metrics.inc_session_auth_failures();
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "session assertion must be base64url encoded JSON"
                })),
            )
                .into_response());
        }
    };
    let claims = match serde_json::from_slice::<UserSessionAuthClaims>(&assertion_bytes) {
        Ok(value) => value,
        Err(err) => {
            state.inner.metrics.inc_session_auth_failures();
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("invalid session assertion payload: {err}")
                })),
            )
                .into_response());
        }
    };

    let issuer = normalize_identity_value(Some(claims.issuer.as_str())).ok_or_else(|| {
        state.inner.metrics.inc_session_auth_failures();
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "signed session assertion is missing issuer"
            })),
        )
            .into_response()
    })?;

    let key_id = normalize_identity_value(claims.key_id.as_deref());
    let session_auth_registry_state = session_auth_issuer_registry_runtime_state(state);

    let secret = if let Some(registry_entry) = session_auth_registry_state.registry.get(&issuer) {
        let key_id = key_id.clone().ok_or_else(|| {
            state.inner.metrics.inc_session_auth_failures();
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "signed session assertion is missing key_id for issuer registry verification",
                    "issuer": issuer,
                })),
            )
                .into_response()
        })?;

        registry_entry
            .keys
            .get(&key_id)
            .map(|value| value.as_str())
            .ok_or_else(|| {
                state.inner.metrics.inc_session_auth_failures();
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "error": "unknown signed session key_id for issuer registry verification",
                        "issuer": issuer,
                        "key_id": key_id,
                    })),
                )
                    .into_response()
            })?
    } else if let Some(issuer_keys) = state.config().session_auth_issuer_keys.get(&issuer) {
        let key_id = key_id.clone().ok_or_else(|| {
            state.inner.metrics.inc_session_auth_failures();
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "signed session assertion is missing key_id for issuer-managed keys",
                    "issuer": issuer,
                })),
            )
                .into_response()
        })?;

        issuer_keys
            .get(&key_id)
            .map(|value| value.as_str())
            .ok_or_else(|| {
                state.inner.metrics.inc_session_auth_failures();
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "error": "unknown signed session key_id for issuer-managed keys",
                        "issuer": issuer,
                        "key_id": key_id,
                    })),
                )
                    .into_response()
            })?
    } else {
        state
            .config()
            .session_auth_issuer_secrets
            .get(&issuer)
            .map(|value| value.as_str())
            .or_else(|| {
                state
                    .config()
                    .session_auth_secret
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
            })
            .ok_or_else(|| {
                state.inner.metrics.inc_session_auth_failures();
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": "session auth is required but no matching issuer secret is configured",
                        "issuer": issuer,
                    })),
                )
                    .into_response()
            })?
    };

    let expected_signature = match sign_user_session_assertion(assertion, secret) {
        Ok(value) => value,
        Err(response) => {
            state.inner.metrics.inc_session_auth_failures();
            return Err(response);
        }
    };
    if expected_signature != signature {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "invalid signed user session signature",
                "assertion_header": USER_SESSION_ASSERTION_HEADER,
                "signature_header": USER_SESSION_SIGNATURE_HEADER,
            })),
        )
            .into_response());
    }

    if !state.config().session_auth_allowed_issuers.is_empty()
        && !state
            .config()
            .session_auth_allowed_issuers
            .iter()
            .any(|allowed| allowed == &issuer)
    {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "signed session issuer is not allowed",
                "issuer": issuer,
                "allowed_issuers": state.config().session_auth_allowed_issuers,
            })),
        )
            .into_response());
    }

    if let Some(expected_audience) = state.config().session_auth_expected_audience.as_deref() {
        let actual_audience = normalize_identity_value(claims.audience.as_deref());
        if actual_audience.as_deref() != Some(expected_audience) {
            state.inner.metrics.inc_session_auth_failures();
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "signed session audience mismatch",
                    "claimed": actual_audience,
                    "expected": expected_audience,
                })),
            )
                .into_response());
        }
    }

    let claimed_request_fingerprint =
        normalize_identity_value(claims.request_fingerprint.as_deref()).ok_or_else(|| {
            state.inner.metrics.inc_session_auth_failures();
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "signed session assertion is missing request_fingerprint"
                })),
            )
                .into_response()
        })?;
    if claimed_request_fingerprint != expected_request_fingerprint {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "signed session request_fingerprint mismatch",
                "claimed": claimed_request_fingerprint,
                "expected": expected_request_fingerprint,
            })),
        )
            .into_response());
    }

    let now_epoch = Utc::now().timestamp();
    let max_skew = state.config().session_auth_max_clock_skew_secs as i64;
    if claims.issued_at_epoch > now_epoch + max_skew {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "signed session assertion is not valid yet",
                "issued_at_epoch": claims.issued_at_epoch,
                "now_epoch": now_epoch,
            })),
        )
            .into_response());
    }
    if claims.expires_at_epoch < now_epoch - max_skew {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "signed session assertion has expired",
                "expires_at_epoch": claims.expires_at_epoch,
                "now_epoch": now_epoch,
            })),
        )
            .into_response());
    }
    if claims.expires_at_epoch < claims.issued_at_epoch {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "signed session assertion has invalid lifetime",
                "issued_at_epoch": claims.issued_at_epoch,
                "expires_at_epoch": claims.expires_at_epoch,
            })),
        )
            .into_response());
    }
    if claims
        .expires_at_epoch
        .saturating_sub(claims.issued_at_epoch)
        > state.config().session_auth_max_ttl_secs as i64
    {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "signed session assertion ttl exceeds configured maximum",
                "issued_at_epoch": claims.issued_at_epoch,
                "expires_at_epoch": claims.expires_at_epoch,
                "max_ttl_secs": state.config().session_auth_max_ttl_secs,
            })),
        )
            .into_response());
    }

    if normalize_identity_value(Some(claims.source_kind.as_str()))
        != normalize_identity_value(Some(scope.source_kind))
    {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "signed session source_kind mismatch",
                "claimed": claims.source_kind,
                "actual": scope.source_kind,
            })),
        )
            .into_response());
    }

    if let Err(response) = verify_session_auth_claim_field(
        "subject",
        Some(claims.subject.as_str()),
        scope.user_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }
    if let Err(response) = verify_session_auth_claim_field(
        "room_id",
        claims.room_id.as_deref(),
        scope.room_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }
    if let Err(response) = verify_session_auth_claim_field(
        "session_id",
        claims.session_id.as_deref(),
        scope.session_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }
    if let Err(response) =
        verify_session_auth_claim_field("org_id", claims.org_id.as_deref(), scope.org_id.as_deref())
    {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }
    if let Err(response) = verify_session_auth_claim_field(
        "account_id",
        claims.account_id.as_deref(),
        scope.account_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }

    state.inner.metrics.inc_session_auth_successes();
    Ok(Some(AuthorizedUserSession {
        claims,
        assertion_header: USER_SESSION_ASSERTION_HEADER,
        signature_header: USER_SESSION_SIGNATURE_HEADER,
    }))
}

pub(super) async fn resolve_chat_identity(
    state: &AppState,
    payload: &CreateChatTaskRequest,
) -> Result<ResolvedIdentity, Response> {
    let requested = build_chat_identity_scope(payload);
    let store = state.inner.identity_binding_store.read().await;
    let binding = store
        .bindings
        .chat_users
        .get(
            payload
                .user_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default(),
        )
        .cloned();
    let metadata = store.metadata.clone();
    let product_users = store.product_users.clone();
    drop(store);

    resolve_identity_scope(
        state,
        requested,
        payload.user_id.as_deref(),
        binding.as_ref(),
        &metadata,
        &product_users,
    )
}

pub(super) async fn resolve_matrix_identity(
    state: &AppState,
    payload: &MatrixMessageRequest,
) -> Result<ResolvedIdentity, Response> {
    let requested = build_matrix_identity_scope(payload);
    let store = state.inner.identity_binding_store.read().await;
    let binding = store
        .bindings
        .matrix_users
        .get(payload.matrix_user_id.trim())
        .cloned();
    let metadata = store.metadata.clone();
    let product_users = store.product_users.clone();
    drop(store);

    resolve_identity_scope(
        state,
        requested,
        Some(payload.matrix_user_id.as_str()),
        binding.as_ref(),
        &metadata,
        &product_users,
    )
}

pub(super) fn resolve_identity_scope(
    state: &AppState,
    mut requested: IdentityScope,
    binding_subject: Option<&str>,
    binding: Option<&IdentityBindingEntry>,
    metadata: &IdentityBindingMetadata,
    product_users: &HashMap<String, ProductUserIdentity>,
) -> Result<ResolvedIdentity, Response> {
    let binding_subject = binding_subject
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);
    let base_resolution = IdentityResolution {
        matched: false,
        required: state.config().require_identity_binding,
        binding_subject: binding_subject.clone(),
        binding_source_format: metadata.format.clone(),
        binding_version: metadata.version,
        binding_revision: metadata.revision.clone(),
        product_user_id: None,
        binding_source_kind: "none".to_string(),
    };

    if let Some(binding) = binding {
        let product_user_id = binding
            .product_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let product_user = match product_user_id.as_deref() {
            Some(product_user_id) => match product_users.get(product_user_id) {
                Some(product_user) => Some(product_user),
                None => {
                    state.inner.metrics.inc_identity_binding_failures();
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "error": "identity binding references unknown product_user_id",
                            "subject": binding_subject,
                            "product_user_id": product_user_id,
                            "binding_source_format": metadata.format.clone(),
                            "binding_version": metadata.version,
                            "binding_revision": metadata.revision.clone(),
                        })),
                    )
                        .into_response());
                }
            },
            None => None,
        };

        let bound_org_id = match merge_bound_identity_source(
            binding.org_id.clone(),
            product_user.and_then(|value| value.org_id.clone()),
            "org_id",
            product_user_id.as_deref(),
        ) {
            Ok(value) => value,
            Err(response) => {
                state.inner.metrics.inc_identity_binding_failures();
                return Err(response);
            }
        };
        let bound_account_id = match merge_bound_identity_source(
            binding.account_id.clone(),
            product_user.and_then(|value| value.account_id.clone()),
            "account_id",
            product_user_id.as_deref(),
        ) {
            Ok(value) => value,
            Err(response) => {
                state.inner.metrics.inc_identity_binding_failures();
                return Err(response);
            }
        };

        requested.org_id = match merge_identity_field(requested.org_id, bound_org_id, "org_id") {
            Ok(value) => value,
            Err(response) => {
                state.inner.metrics.inc_identity_binding_failures();
                return Err(response);
            }
        };
        requested.account_id =
            match merge_identity_field(requested.account_id, bound_account_id, "account_id") {
                Ok(value) => value,
                Err(response) => {
                    state.inner.metrics.inc_identity_binding_failures();
                    return Err(response);
                }
            };
        state.inner.metrics.inc_identity_binding_matches();
        return Ok(ResolvedIdentity {
            scope: requested,
            resolution: IdentityResolution {
                matched: true,
                product_user_id: product_user_id.clone(),
                binding_source_kind: if product_user_id.is_some() {
                    "product_user_registry".to_string()
                } else {
                    "inline_binding".to_string()
                },
                ..base_resolution
            },
        });
    }

    if state.config().require_identity_binding {
        state.inner.metrics.inc_identity_binding_failures();
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "identity binding required",
                "subject": binding_subject,
                "source_kind": requested.source_kind,
                "binding_source_format": metadata.format.clone(),
                "binding_version": metadata.version,
                "binding_revision": metadata.revision.clone(),
            })),
        )
            .into_response());
    }

    Ok(ResolvedIdentity {
        scope: requested,
        resolution: base_resolution,
    })
}

pub(super) fn merge_bound_identity_source(
    inline: Option<String>,
    registry: Option<String>,
    field_name: &str,
    product_user_id: Option<&str>,
) -> Result<Option<String>, Response> {
    match (
        inline
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        registry
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    ) {
        (Some(inline), Some(registry)) if inline != registry => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "identity source-of-truth conflict",
                "field": field_name,
                "product_user_id": product_user_id,
                "inline_value": inline,
                "registry_value": registry,
            })),
        )
            .into_response()),
        (Some(inline), Some(_)) => Ok(Some(inline)),
        (None, Some(registry)) => Ok(Some(registry)),
        (Some(inline), None) => Ok(Some(inline)),
        (None, None) => Ok(None),
    }
}

pub(super) fn merge_identity_field(
    requested: Option<String>,
    bound: Option<String>,
    field_name: &str,
) -> Result<Option<String>, Response> {
    match (
        requested
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        bound
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    ) {
        (Some(requested), Some(bound)) if requested != bound => Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "identity binding mismatch",
                "field": field_name,
                "requested": requested,
                "bound": bound,
            })),
        )
            .into_response()),
        (Some(requested), _) => Ok(Some(requested)),
        (None, Some(bound)) => Ok(Some(bound)),
        (None, None) => Ok(None),
    }
}

fn durable_store_unavailable(code: &'static str, error: &std::io::Error) -> Response {
    tracing::error!(error = %error, durable_store_error = code, "consumer durable state unavailable");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": code,
            "consumer_status": "durable_state_unavailable"
        })),
    )
        .into_response()
}

pub(super) async fn replay_cached_response(
    state: &AppState,
    replay_key: Option<&str>,
) -> Option<Response> {
    let replay_key = replay_key.map(str::trim).filter(|v| !v.is_empty())?;

    let now_epoch = Utc::now().timestamp();
    let ttl_secs = state.config().replay_window_secs;
    let max_size = state.config().replay_cache_size;
    let mut cache = state.inner.replay_cache.lock().await;

    if let Some(path) = state.config().replay_store_path.as_deref() {
        match durable_store::read_json::<ReplayCache>(
            StdPath::new(path),
            MAX_DURABLE_INGRESS_STATE_BYTES,
        ) {
            Ok(Some(persisted)) => *cache = persisted,
            Ok(None) => *cache = ReplayCache::default(),
            Err(error) => {
                return Some(durable_store_unavailable(
                    "replay_store_read_failed",
                    &error,
                ))
            }
        }
    }

    prune_replay_cache(&mut cache, now_epoch, ttl_secs, max_size);
    let existing = cache.seen.get(replay_key).cloned()?;

    state.inner.metrics.inc_replay_hits();
    if let Some(response) = existing.response {
        return Some((StatusCode::ACCEPTED, Json(response)).into_response());
    }

    Some(
        (
            StatusCode::OK,
            Json(json!({
                "accepted": false,
                "action": "duplicate_request",
                "replay_key": replay_key,
                "consumer_status": "duplicate_ignored"
            })),
        )
            .into_response(),
    )
}

pub(super) async fn remember_replay_response(
    state: &AppState,
    replay_key: Option<&str>,
    response: &ConsumerTaskResponse,
) -> Result<(), Response> {
    let Some(replay_key) = replay_key.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(());
    };
    let response = serde_json::to_value(response).map_err(|error| {
        tracing::error!(error = %error, "failed to encode replay response");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "replay_response_encode_failed" })),
        )
            .into_response()
    })?;

    let now_epoch = Utc::now().timestamp();
    let ttl_secs = state.config().replay_window_secs;
    let max_size = state.config().replay_cache_size;
    let mut cache = state.inner.replay_cache.lock().await;

    if let Some(path) = state.config().replay_store_path.as_deref() {
        let result = durable_store::update_json(
            StdPath::new(path),
            MAX_DURABLE_INGRESS_STATE_BYTES,
            ReplayCache::default(),
            |persisted: &mut ReplayCache| {
                prune_replay_cache(persisted, now_epoch, ttl_secs, max_size);
                persisted.seen.insert(
                    replay_key.to_string(),
                    ReplayEntry {
                        seen_at_epoch: now_epoch,
                        response: Some(response),
                    },
                );
                persisted.order.push_back(replay_key.to_string());
                prune_replay_cache(persisted, now_epoch, ttl_secs, max_size);
            },
        );
        return match result {
            Ok((persisted, ())) => {
                *cache = persisted;
                Ok(())
            }
            Err(error) => Err(durable_store_unavailable(
                "replay_store_write_failed",
                &error,
            )),
        };
    }

    prune_replay_cache(&mut cache, now_epoch, ttl_secs, max_size);
    cache.seen.insert(
        replay_key.to_string(),
        ReplayEntry {
            seen_at_epoch: now_epoch,
            response: Some(response),
        },
    );
    cache.order.push_back(replay_key.to_string());
    prune_replay_cache(&mut cache, now_epoch, ttl_secs, max_size);
    // Keep the legacy helper referenced for compatibility; with no configured path it is a no-op.
    super::persist_replay_cache(&cache, state.config());
    Ok(())
}

pub(super) fn prune_replay_cache(
    cache: &mut ReplayCache,
    now_epoch: i64,
    ttl_secs: u64,
    max_size: usize,
) {
    while let Some(front) = cache.order.front().cloned() {
        let should_drop = cache
            .seen
            .get(&front)
            .map(|entry| now_epoch.saturating_sub(entry.seen_at_epoch) >= ttl_secs as i64)
            .unwrap_or(true)
            || cache.seen.len() > max_size;

        if !should_drop {
            break;
        }

        cache.order.pop_front();
        cache.seen.remove(&front);
    }
}

pub(super) fn build_chat_replay_key(payload: &CreateChatTaskRequest) -> Option<String> {
    let key = payload.idempotency_key.as_deref()?.trim();
    if key.is_empty() {
        return None;
    }

    Some(format!(
        "chat:{}:{}:{}",
        payload.user_id.as_deref().unwrap_or("anonymous"),
        payload.room_id.as_deref().unwrap_or("global"),
        key
    ))
}

pub(super) fn build_matrix_replay_key(payload: &MatrixMessageRequest) -> Option<String> {
    if let Some(event_id) = payload
        .event_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Some(format!("matrix-event:{}", event_id));
    }

    let key = payload.idempotency_key.as_deref()?.trim();
    if key.is_empty() {
        return None;
    }

    Some(format!(
        "matrix:{}:{}:{}",
        payload.matrix_user_id, payload.room_id, key
    ))
}

pub(super) async fn enforce_optional_rate_limit(
    state: &AppState,
    key: Option<String>,
    max_requests: usize,
    error_code: &str,
    bucket_kind: RateLimitBucketKind,
) -> Result<(), Response> {
    let Some(key) = key else {
        return Ok(());
    };
    if max_requests == 0 {
        return Ok(());
    }

    enforce_rate_limit(state, key, max_requests, error_code, bucket_kind).await
}

fn rate_limit_rejection(
    state: &AppState,
    max_requests: usize,
    error_code: &str,
    bucket_kind: RateLimitBucketKind,
) -> Response {
    state.inner.metrics.inc_rate_limited_requests();
    match bucket_kind {
        RateLimitBucketKind::SourceScope => {
            state.inner.metrics.inc_rate_limited_source_scope_requests()
        }
        RateLimitBucketKind::User => state.inner.metrics.inc_rate_limited_user_requests(),
        RateLimitBucketKind::Room => state.inner.metrics.inc_rate_limited_room_requests(),
        RateLimitBucketKind::Session => state.inner.metrics.inc_rate_limited_session_requests(),
        RateLimitBucketKind::Org => state.inner.metrics.inc_rate_limited_org_requests(),
    }
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({
            "error": error_code,
            "rate_limit_window_secs": state.config().rate_limit_window_secs,
            "rate_limit_max_requests": max_requests,
            "rate_limit_bucket": rate_limit_bucket_name(bucket_kind),
        })),
    )
        .into_response()
}

pub(super) async fn enforce_rate_limit(
    state: &AppState,
    key: String,
    max_requests: usize,
    error_code: &str,
    bucket_kind: RateLimitBucketKind,
) -> Result<(), Response> {
    let now_epoch = Utc::now().timestamp();
    let max_entries = max_rate_limit_entries(state.config());
    let mut rate_limits = state.inner.rate_limits.lock().await;

    if let Some(path) = state.config().rate_limit_store_path.as_deref() {
        let result = durable_store::update_json(
            StdPath::new(path),
            MAX_DURABLE_INGRESS_STATE_BYTES,
            RateLimitCache::default(),
            |persisted: &mut RateLimitCache| {
                let entries = persisted.seen.entry(key).or_default();
                prune_rate_limit_entries(
                    entries,
                    now_epoch,
                    state.config().rate_limit_window_secs,
                    max_entries,
                );
                if entries.len() >= max_requests {
                    false
                } else {
                    entries.push_back(now_epoch);
                    prune_rate_limit_entries(
                        entries,
                        now_epoch,
                        state.config().rate_limit_window_secs,
                        max_entries,
                    );
                    true
                }
            },
        );
        return match result {
            Ok((persisted, true)) => {
                *rate_limits = persisted;
                Ok(())
            }
            Ok((persisted, false)) => {
                *rate_limits = persisted;
                Err(rate_limit_rejection(
                    state,
                    max_requests,
                    error_code,
                    bucket_kind,
                ))
            }
            Err(error) => Err(durable_store_unavailable(
                "rate_limit_store_write_failed",
                &error,
            )),
        };
    }

    let entries = rate_limits.seen.entry(key).or_default();
    let modified = prune_rate_limit_entries(
        entries,
        now_epoch,
        state.config().rate_limit_window_secs,
        max_entries,
    );
    if entries.len() >= max_requests {
        if modified {
            super::persist_rate_limit_cache(&rate_limits, state.config());
        }
        return Err(rate_limit_rejection(
            state,
            max_requests,
            error_code,
            bucket_kind,
        ));
    }

    entries.push_back(now_epoch);
    prune_rate_limit_entries(
        entries,
        now_epoch,
        state.config().rate_limit_window_secs,
        max_entries,
    );
    // Keep the legacy helper referenced for compatibility; with no configured path it is a no-op.
    super::persist_rate_limit_cache(&rate_limits, state.config());
    Ok(())
}

pub(super) fn rate_limit_bucket_name(bucket_kind: RateLimitBucketKind) -> &'static str {
    match bucket_kind {
        RateLimitBucketKind::SourceScope => "source_scope",
        RateLimitBucketKind::User => "user",
        RateLimitBucketKind::Room => "room",
        RateLimitBucketKind::Session => "session",
        RateLimitBucketKind::Org => "org",
    }
}

pub(super) fn build_chat_identity_scope(payload: &CreateChatTaskRequest) -> IdentityScope {
    IdentityScope {
        source_kind: "chat_task",
        user_id: payload.user_id.clone(),
        room_id: payload.room_id.clone(),
        session_id: payload.session_id.clone(),
        org_id: payload.org_id.clone(),
        account_id: payload.account_id.clone(),
    }
}

pub(super) fn build_matrix_identity_scope(payload: &MatrixMessageRequest) -> IdentityScope {
    IdentityScope {
        source_kind: "matrix_message",
        user_id: Some(payload.matrix_user_id.clone()),
        room_id: Some(payload.room_id.clone()),
        session_id: payload.session_id.clone(),
        org_id: payload.org_id.clone(),
        account_id: payload.account_id.clone(),
    }
}

pub(super) fn build_chat_rate_limit_key(payload: &CreateChatTaskRequest) -> String {
    format!(
        "chat:{}:{}",
        payload.user_id.as_deref().unwrap_or("anonymous"),
        payload.room_id.as_deref().unwrap_or("global")
    )
}

pub(super) fn build_chat_user_rate_limit_key(payload: &CreateChatTaskRequest) -> Option<String> {
    payload
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|user_id| format!("chat-user:{}", user_id))
}

pub(super) fn build_chat_room_rate_limit_key(payload: &CreateChatTaskRequest) -> Option<String> {
    payload
        .room_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|room_id| format!("chat-room:{}", room_id))
}

pub(super) fn build_chat_session_rate_limit_key(payload: &CreateChatTaskRequest) -> Option<String> {
    payload
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|session_id| format!("chat-session:{}", session_id))
}

pub(super) fn build_chat_org_rate_limit_key(payload: &CreateChatTaskRequest) -> Option<String> {
    payload
        .org_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|org_id| format!("chat-org:{}", org_id))
}

pub(super) fn build_matrix_rate_limit_key(payload: &MatrixMessageRequest) -> String {
    format!("matrix:{}:{}", payload.matrix_user_id, payload.room_id)
}

pub(super) fn build_matrix_user_rate_limit_key(payload: &MatrixMessageRequest) -> String {
    format!("matrix-user:{}", payload.matrix_user_id)
}

pub(super) fn build_matrix_room_rate_limit_key(payload: &MatrixMessageRequest) -> String {
    format!("matrix-room:{}", payload.room_id)
}

pub(super) fn build_matrix_session_rate_limit_key(
    payload: &MatrixMessageRequest,
) -> Option<String> {
    payload
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|session_id| format!("matrix-session:{}", session_id))
}

pub(super) fn build_matrix_org_rate_limit_key(payload: &MatrixMessageRequest) -> Option<String> {
    payload
        .org_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|org_id| format!("matrix-org:{}", org_id))
}

pub(super) fn validate_text_payload(raw: &str, max_text_chars: usize) -> Result<String, Response> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "text payload must not be empty" })),
        )
            .into_response());
    }

    let length = trimmed.chars().count();
    if length > max_text_chars {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "text payload too large",
                "max_text_chars": max_text_chars,
                "received_text_chars": length,
            })),
        )
            .into_response());
    }

    Ok(trimmed.to_string())
}

pub(super) fn project_consumer_status(status: &str) -> &'static str {
    match status {
        "Created" => "received",
        "Queued" => "queued",
        "AwaitingApproval" => "waiting_for_confirmation",
        "Approved" | "Dispatching" | "Running" => "processing",
        "Succeeded" => "done",
        "Failed" => "failed",
        "Refunded" => "refunded",
        _ => "received",
    }
}

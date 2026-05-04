use super::*;

const TRILLIONNIUM_WORLD_ACTION_ENGINE_CONTRACT_VERSION: &str =
    "trillionnium_world_action_engine_v1";

pub(super) fn world_action_kind(body: &str) -> (&'static str, &'static str, i64) {
    let lower = body.to_ascii_lowercase();
    if lower.contains("contract")
        || lower.contains("bounty")
        || lower.contains("commission")
        || body.contains("委托")
        || body.contains("悬赏")
        || body.contains("任务")
    {
        (
            "contract",
            "把现实需求登记成 World Contract，并生成可执行的 CEX 委托任务。",
            20,
        )
    } else if lower.contains("build")
        || lower.contains("craft")
        || lower.contains("fabricate")
        || body.contains("建")
        || body.contains("造")
        || body.contains("工坊")
    {
        (
            "craft",
            "在 Craft District 完成一次建造/创造行动，生成可迭代资产。",
            14,
        )
    } else if lower.contains("market")
        || lower.contains("listing")
        || lower.contains("buy")
        || lower.contains("client")
        || body.contains("客户")
        || body.contains("接单")
        || body.contains("市场")
    {
        (
            "market",
            "进入 Market Bazaar，把现实机会映射为世界委托。",
            16,
        )
    } else if lower.contains("company")
        || lower.contains("shop")
        || lower.contains("studio")
        || body.contains("公司")
        || body.contains("店")
        || body.contains("工作室")
    {
        (
            "venture",
            "创建了一个现实映射经营体，获得资产雏形和声望入口。",
            18,
        )
    } else if lower.contains("hire")
        || lower.contains("agent")
        || body.contains("招募")
        || body.contains("雇佣")
    {
        (
            "recruit",
            "与 Agent 居民建立合作关系，队伍能力获得提升。",
            12,
        )
    } else {
        (
            "explore",
            "完成一次开放世界探索，产生新的线索和关系变化。",
            10,
        )
    }
}

fn world_action_quality_signal(body: &str) -> (i64, Vec<&'static str>) {
    let lower = body.to_ascii_lowercase();
    let mut score = 0;
    let mut missing = Vec::new();
    let signals = [
        (
            "deliverable",
            lower.contains("deliverable") || body.contains("成果") || body.contains("交付"),
        ),
        (
            "evidence",
            lower.contains("evidence")
                || lower.contains("source")
                || lower.contains("proof")
                || body.contains("证据")
                || body.contains("依据"),
        ),
        (
            "risk_control",
            lower.contains("risk") || body.contains("风险") || body.contains("风控"),
        ),
        (
            "next_action",
            lower.contains("next") || body.contains("下一步") || body.contains("计划"),
        ),
        (
            "self_review",
            lower.contains("review") || body.contains("自检") || body.contains("复盘"),
        ),
    ];
    for (signal, present) in signals {
        if present {
            score += 1;
        } else {
            missing.push(signal);
        }
    }
    if body.chars().count() >= 80 {
        score += 1;
    }
    (score, missing)
}

fn world_action_playability_outcome(
    league: &LeagueState,
    matrix_user_id: &str,
    kind: &str,
    base_result: &str,
    body: &str,
    base_impact: i64,
    now: i64,
) -> (Value, String, i64) {
    let cooldown_seconds = 300;
    let normalized_body = body.trim().to_ascii_lowercase();
    let recent_events: Vec<&WorldEvent> = league
        .world
        .world_events
        .iter()
        .filter(|event| {
            event.actor_matrix_user_id == matrix_user_id
                && event.event_kind == kind
                && now.saturating_sub(event.created_at_epoch) <= cooldown_seconds
        })
        .collect();
    let duplicate_count = recent_events
        .iter()
        .filter(|event| event.body.trim().to_ascii_lowercase() == normalized_body)
        .count() as i64;
    let remaining_cooldown_seconds = recent_events
        .iter()
        .filter(|event| event.body.trim().to_ascii_lowercase() == normalized_body)
        .map(|event| cooldown_seconds - now.saturating_sub(event.created_at_epoch))
        .max()
        .unwrap_or(0)
        .max(0);
    let (quality_score, missing_signals) = world_action_quality_signal(body);
    let mut risk_flags: Vec<String> = missing_signals
        .iter()
        .map(|signal| format!("missing_{signal}"))
        .collect();
    if duplicate_count > 0 {
        risk_flags.push("duplicate_action_signature".to_string());
    }
    if !recent_events.is_empty() && duplicate_count == 0 {
        risk_flags.push("repeat_kind_cooldown_pressure".to_string());
    }
    if body.chars().count() < 32 {
        risk_flags.push("too_short_for_full_reward".to_string());
    }
    risk_flags.sort();
    risk_flags.dedup();
    let quality_bonus = (quality_score - 3).max(0) * 2;
    let duplicate_penalty = duplicate_count * base_impact.max(4) / 2;
    let cooldown_penalty = if !recent_events.is_empty() && duplicate_count == 0 {
        2
    } else {
        0
    };
    let missing_penalty = missing_signals.len() as i64;
    let computed_impact =
        (base_impact + quality_bonus - duplicate_penalty - cooldown_penalty - missing_penalty)
            .clamp(1, base_impact + 8);
    let final_impact = if duplicate_count > 0 {
        0
    } else {
        computed_impact
    };
    let success_tier = if duplicate_count > 0 {
        "cooldown_review_hold"
    } else if missing_signals.is_empty() && quality_score >= 6 {
        "critical_success"
    } else if missing_signals.len() <= 2 {
        "solid_success"
    } else {
        "partial_success"
    };
    let status = if duplicate_count > 0 {
        "review_hold"
    } else if missing_signals.len() >= 3 {
        "needs_recovery_choice"
    } else {
        "resolved"
    };
    let payout_status = if duplicate_count > 0 {
        "review_hold"
    } else {
        "settled"
    };
    let next_choice = match kind {
        "contract" => "/world/web/contract-complete or /work deliver latest",
        "market" => "/world/web/listing-buy then /work deliver latest",
        "craft" | "venture" => "/world/web/company then /world/web/listing",
        "recruit" => "/league/web/action team or /league/web/action raid",
        _ => "/map then choose a nearby action node",
    };
    let recovery_hint = if duplicate_count > 0 {
        "Wait for cooldown or change the evidence/body before farming the same action."
    } else if missing_signals.is_empty() {
        "Push the next route while the reward window is warm."
    } else {
        "Add the missing deliverable/evidence/risk/next-action signals, then retry or route to review."
    };
    let result_text = format!(
        "{} Outcome={success_tier}; impact {base_impact}->{final_impact}; next={next_choice}; recovery={recovery_hint}",
        base_result
    );
    let outcome = json!({
        "contract_version": TRILLIONNIUM_WORLD_ACTION_ENGINE_CONTRACT_VERSION,
        "status": status,
        "success_tier": success_tier,
        "action_kind": kind,
        "base_impact": base_impact,
        "final_impact": final_impact,
        "computed_impact_before_gate": computed_impact,
        "quality_score": quality_score,
        "payout_status": payout_status,
        "anti_cheese_gate_enforced": duplicate_count > 0,
        "reward_delta_xp": final_impact,
        "reward_delta_reputation": if payout_status == "review_hold" { 0 } else { (final_impact / 4).max(1) },
        "reward_delta_rating": if payout_status == "review_hold" { 0 } else { (final_impact / 3).max(1) },
        "energy_cost": (base_impact / 4).max(1),
        "difficulty": match kind {
            "contract" | "market" => "medium",
            "venture" => "hard",
            "craft" | "recruit" => "medium_light",
            _ => "light",
        },
        "cooldown_seconds": cooldown_seconds,
        "remaining_cooldown_seconds": remaining_cooldown_seconds,
        "duplicate_count": duplicate_count,
        "risk_flags": risk_flags,
        "missing_signals": missing_signals,
        "next_choice": next_choice,
        "recovery_hint": recovery_hint,
        "telemetry_step": format!("world_action_{kind}_{success_tier}"),
    });
    (outcome, result_text, final_impact)
}

fn world_default_location_for_kind(kind: &str) -> &'static str {
    match kind {
        "venture" | "market" | "contract" => "zbj-market-gate",
        "craft" => "starter-studio",
        "recruit" => "mirror-city-square",
        _ => "mirror-city-square",
    }
}

pub(super) fn world_faction_for_location(location_id: &str) -> &'static str {
    match location_id {
        "starter-studio" => "faction-craft-union",
        "zbj-market-gate" => "faction-market-guild",
        "league-coliseum" => "faction-league-order",
        _ => "faction-city-clerks",
    }
}

fn world_faction_rank(reputation_score: i64) -> &'static str {
    if reputation_score >= 500 {
        "legend"
    } else if reputation_score >= 220 {
        "trusted_partner"
    } else if reputation_score >= 80 {
        "known_operator"
    } else if reputation_score >= 20 {
        "new_contact"
    } else {
        "stranger"
    }
}

pub(super) fn upsert_world_faction_standing(
    league: &mut LeagueState,
    matrix_user_id: &str,
    faction_id: &str,
    delta: i64,
    now: i64,
) -> WorldFactionStanding {
    let standing_id = league_hash_id(
        "world-standing",
        &format!("{}:{}", matrix_user_id, faction_id),
    );
    let indexes = build_world_indexes(&league.world);
    let standing = if let Some(index) = indexes.faction_standing_index(matrix_user_id, faction_id) {
        let standing = &mut league.world.world_faction_standings[index];
        standing.reputation_score = (standing.reputation_score + delta).max(0);
        standing.rank = world_faction_rank(standing.reputation_score).to_string();
        standing.updated_at_epoch = now;
        standing.clone()
    } else {
        let reputation_score = delta.max(0);
        let standing = WorldFactionStanding {
            standing_id,
            matrix_user_id: matrix_user_id.to_string(),
            faction_id: faction_id.to_string(),
            reputation_score,
            rank: world_faction_rank(reputation_score).to_string(),
            updated_at_epoch: now,
        };
        league.world.world_faction_standings.push(standing.clone());
        standing
    };
    if let Some(faction) = league.world.world_factions.get_mut(faction_id) {
        faction.reputation_score = (faction.reputation_score + delta).max(0);
    }
    standing
}

pub(super) async fn get_world_map(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(world_map_json(&league, &matrix_user_id)),
    )
        .into_response()
}

pub(super) async fn get_world_map_viewport(
    Path(matrix_user_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let lat = query.get("lat").and_then(|value| value.parse::<f64>().ok());
    let lng = query.get("lng").and_then(|value| value.parse::<f64>().ok());
    let zoom = query
        .get("zoom")
        .and_then(|value| value.parse::<i64>().ok());
    let radius_km = query
        .get("radius_km")
        .and_then(|value| value.parse::<f64>().ok());
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok());
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(world_map_viewport_json(
            &league.world,
            &matrix_user_id,
            lat,
            lng,
            zoom,
            radius_km,
            limit,
        )),
    )
        .into_response()
}

pub(super) async fn get_world_web_map_viewport(
    Query(query): Query<HashMap<String, String>>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let allow_missing_cookie = matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
        || !state.config().league_web_session_required;
    let web_session =
        match authorize_league_web_session_readonly(&state, &headers, allow_missing_cookie) {
            Ok(value) => value,
            Err(response) => return response,
        };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                query
                    .get("matrix_user_id")
                    .map(String::as_str)
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let lat = query.get("lat").and_then(|value| value.parse::<f64>().ok());
    let lng = query.get("lng").and_then(|value| value.parse::<f64>().ok());
    let zoom = query
        .get("zoom")
        .and_then(|value| value.parse::<i64>().ok());
    let radius_km = query
        .get("radius_km")
        .and_then(|value| value.parse::<f64>().ok());
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok());
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(world_map_viewport_json(
            &league.world,
            &matrix_user_id,
            lat,
            lng,
            zoom,
            radius_km,
            limit,
        )),
    )
        .into_response()
}

pub(super) async fn move_world_map_inner(
    state: AppState,
    payload: WorldMapMoveRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let target = match validate_text_payload(&payload.target, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let _room_id = payload.room_id.as_deref();
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let current_node_id = league
            .world
            .world_player_positions
            .get(&matrix_user_id)
            .map(|position| position.node_id.clone())
            .filter(|node_id| league.world.world_map_nodes.contains_key(node_id))
            .unwrap_or_else(|| default_world_node_id().to_string());
        let Some(current_node) = league.world.world_map_nodes.get(&current_node_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world map current node missing", "node_id": current_node_id })),
            )
                .into_response();
        };
        let target_trimmed = target.trim();
        let target_node_id = current_node
            .exits
            .get(target_trimmed)
            .cloned()
            .unwrap_or_else(|| target_trimmed.to_string());
        let Some(target_node) = league.world.world_map_nodes.get(&target_node_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "world map target node not found or not reachable by that direction",
                    "target": target_trimmed,
                    "current_node_id": current_node_id,
                    "exits": current_node.exits,
                })),
            )
                .into_response();
        };
        let is_direct_exit = current_node
            .exits
            .values()
            .any(|node_id| node_id == &target_node_id);
        if target_node_id != current_node_id && !is_direct_exit {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world map target is not adjacent",
                    "target_node_id": target_node_id,
                    "current_node_id": current_node_id,
                    "exits": current_node.exits,
                })),
            )
                .into_response();
        }
        let position = WorldPlayerPosition {
            matrix_user_id: matrix_user_id.clone(),
            node_id: target_node.node_id.clone(),
            location_id: target_node.location_id.clone(),
            updated_at_epoch: now,
        };
        league
            .world
            .world_player_positions
            .insert(matrix_user_id.clone(), position.clone());
        let economy_event = WorldEconomyEvent {
            economy_event_id: league_hash_id(
                "world-econ",
                &format!("{}:{}:{}", matrix_user_id, target_node.node_id, now),
            ),
            matrix_user_id: matrix_user_id.clone(),
            event_kind: "map_move".to_string(),
            subject_id: target_node.node_id.clone(),
            credits_delta: 0,
            reputation_delta: 0,
            created_at_epoch: now,
        };
        league
            .world
            .world_economy_events
            .push(economy_event.clone());
        (
            league.clone(),
            current_node,
            target_node,
            position,
            economy_event,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_map_move").await
    {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_map_move",
            "world": "trillionnium_world",
            "from_node": snapshot.1,
            "to_node": snapshot.2,
            "position": snapshot.3,
            "economy_event": snapshot.4,
        })),
    )
        .into_response()
}

pub(super) async fn move_world_map(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldMapMoveRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    move_world_map_inner(state, payload).await
}

pub(super) async fn get_client_app_home(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(client_app_json(&league, &matrix_user_id)),
    )
        .into_response()
}

pub(super) async fn get_client_feed_home(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(client_feed_json(&league, &matrix_user_id)),
    )
        .into_response()
}

pub(super) async fn get_client_web_feed_home(
    Query(query): Query<HashMap<String, String>>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let allow_missing_cookie = matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
        || !state.config().league_web_session_required;
    let web_session =
        match authorize_league_web_session_readonly(&state, &headers, allow_missing_cookie) {
            Ok(value) => value,
            Err(response) => return response,
        };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                query
                    .get("matrix_user_id")
                    .map(String::as_str)
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(client_feed_json(&league, &matrix_user_id)),
    )
        .into_response()
}

pub(super) async fn post_world_web_map_move(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebMapMoveRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let target = payload
        .target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("east")
        .to_string();
    let response = move_world_map_inner(
        state,
        WorldMapMoveRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            target,
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?map=moved").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (StatusCode::OK, Json(world_home_json(&league))).into_response()
}

async fn create_world_contract_task(
    state: &AppState,
    headers: &HeaderMap,
    payload: &WorldActionRequest,
    matrix_user_id: &str,
    body: &str,
) -> Result<ConsumerTaskResponse, Response> {
    let room_id = payload
        .room_id
        .clone()
        .unwrap_or_else(|| "!world-contract:local.dev".to_string());
    let contract_prompt = format!(
        "Trillionnium World Contract: {body}\n\n请把这条现实镜像委托转成可执行交付计划，包含目标、证据、风险、下一步和验收标准。"
    );
    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.to_string(),
        room_id,
        session_id: None,
        org_id: None,
        message: contract_prompt,
        capability_id: payload.capability_id.clone(),
        account_id: payload.account_id.clone(),
        event_id: payload.event_id.clone(),
        idempotency_key: None,
        metadata: Some(json!({
            "world": "trillionnium_world",
            "module": "world_contract",
            "location_id": payload.location_id,
            "source_body": body,
        })),
    };
    let resolved_identity = resolve_matrix_identity(state, &matrix_payload).await?;
    let request_fingerprint = build_world_action_request_fingerprint(payload);
    let authorized_session = authorize_user_session(
        state,
        headers,
        &resolved_identity.scope,
        request_fingerprint.as_str(),
    )?;
    let prompt = validate_text_payload(&matrix_payload.message, state.config().max_text_chars)?;
    let resolved_account_id = resolved_identity.scope.account_id.clone();
    let source = json!({
        "kind": "trillionnium_world_contract",
        "world": "trillionnium_world",
        "identity_scope": resolved_identity.scope,
        "identity_resolution": resolved_identity.resolution,
        "matrix_user_id": matrix_user_id,
        "room_id": matrix_payload.room_id,
        "event_id": matrix_payload.event_id,
        "metadata": matrix_payload.metadata,
        "session_auth": authorized_session,
    });
    forward_to_cex_task(
        state.clone(),
        matrix_payload.capability_id,
        resolved_account_id,
        source,
        prompt,
    )
    .await
}

async fn record_world_action(
    state: &AppState,
    payload: WorldActionRequest,
) -> Result<
    (
        LeagueState,
        WorldEvent,
        Option<WorldContract>,
        Value,
        Value,
        WorldEconomyEvent,
    ),
    Response,
> {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response())
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return Err(response),
    };
    let (kind, result, base_impact) = world_action_kind(&body);
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        let location_id = payload
            .location_id
            .as_deref()
            .filter(|location_id| league.world.world_locations.contains_key(*location_id))
            .unwrap_or_else(|| world_default_location_for_kind(kind))
            .to_string();
        let now = Utc::now().timestamp();
        let (playability_outcome, resolved_result, impact) = world_action_playability_outcome(
            &league,
            &matrix_user_id,
            kind,
            result,
            &body,
            base_impact,
            now,
        );
        let event = WorldEvent {
            event_id: league_hash_id(
                "world-event",
                &format!("{}:{}:{}", matrix_user_id, now, body),
            ),
            actor_matrix_user_id: matrix_user_id.clone(),
            room_id: payload.room_id.clone(),
            location_id: location_id.clone(),
            event_kind: kind.to_string(),
            body: body.clone(),
            result: resolved_result,
            impact_score: impact,
            cex_task_id: payload.cex_task_id.clone(),
            cex_status: payload.cex_status.clone(),
            created_at_epoch: now,
        };
        let mut created_contract = None;
        if let Some(task_id) = payload.cex_task_id.clone() {
            let contract = WorldContract {
                contract_id: league_hash_id("world-contract", &event.event_id),
                event_id: event.event_id.clone(),
                actor_matrix_user_id: matrix_user_id.clone(),
                location_id: location_id.clone(),
                task_id,
                title: "World Contract".to_string(),
                body: body.clone(),
                status: "task_created".to_string(),
                cex_status: payload.cex_status.clone(),
                value_score: impact,
                created_at_epoch: now,
            };
            league.world.world_contracts.push(contract.clone());
            created_contract = Some(contract);
        }
        if matches!(kind, "venture" | "craft") {
            league.world.world_assets.push(WorldAsset {
                asset_id: league_hash_id(
                    "world-asset",
                    &format!("{}:{}:{}", matrix_user_id, kind, now),
                ),
                owner_matrix_user_id: matrix_user_id.clone(),
                location_id: location_id.clone(),
                asset_kind: kind.to_string(),
                name: if kind == "venture" {
                    "Reality Venture Seed".to_string()
                } else {
                    "Craft Build Seed".to_string()
                },
                status: "active".to_string(),
                value_score: impact,
                upgrade_level: 1,
                upgrade_points: impact,
                last_upgrade_kind: Some(kind.to_string()),
                created_at_epoch: now,
            });
        }
        league.world.world_relationships.push(WorldRelationship {
            relationship_id: league_hash_id(
                "world-rel",
                &format!("{}:{}:{}", matrix_user_id, location_id, now),
            ),
            from_id: matrix_user_id.clone(),
            to_id: location_id.clone(),
            relation_kind: kind.to_string(),
            strength: impact,
            updated_at_epoch: now,
        });
        let reward_delta_reputation = playability_outcome
            .get("reward_delta_reputation")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| (impact / 4).max(1));
        let reward_delta_rating = playability_outcome
            .get("reward_delta_rating")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| (impact / 3).max(1));
        player.xp += impact;
        player.reputation += reward_delta_reputation;
        player.rating += reward_delta_rating;
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player);
        let telemetry_event = WorldEconomyEvent {
            economy_event_id: league_hash_id(
                "world-playability-telemetry",
                &format!("{}:{}:{}", matrix_user_id, event.event_id, now),
            ),
            matrix_user_id: matrix_user_id.clone(),
            event_kind: "playability_telemetry".to_string(),
            subject_id: playability_outcome
                .get("telemetry_step")
                .and_then(Value::as_str)
                .unwrap_or("world_action")
                .to_string(),
            credits_delta: 0,
            reputation_delta: reward_delta_reputation,
            created_at_epoch: now,
        };
        league.world.world_events.push(event.clone());
        league
            .world
            .world_economy_events
            .push(telemetry_event.clone());
        let home = world_home_json(&league);
        (
            league.clone(),
            event,
            created_contract,
            home,
            playability_outcome,
            telemetry_event,
        )
    };
    Ok(snapshot)
}

pub(super) async fn post_world_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut payload): Json<WorldActionRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let (kind, _result, _impact) = world_action_kind(&body);
    let task = if kind == "contract" && payload.cex_task_id.is_none() {
        match create_world_contract_task(&state, &headers, &payload, &matrix_user_id, &body).await {
            Ok(task) => {
                payload.cex_task_id = Some(task.task_id.clone());
                payload.cex_status = Some(task.consumer_status.clone());
                Some(task)
            }
            Err(response) => return response,
        }
    } else {
        None
    };
    let snapshot = match record_world_action(&state, payload).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_action").await
    {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_action",
            "world": "trillionnium_world",
            "event": snapshot.1,
            "contract": snapshot.2,
            "task": task,
            "home": snapshot.3,
            "playability_outcome": snapshot.4,
            "playability_telemetry": snapshot.5,
        })),
    )
        .into_response()
}

pub(super) async fn post_world_web_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebActionRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Launch an AI Design Studio for global customers: define the customer deliverable, evidence package, risk controls, next action, self-review, and League quest handoff.")
        .to_string();
    let request = WorldActionRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        location_id: payload
            .location_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        body,
        message: None,
        event_id: None,
        capability_id: None,
        account_id: None,
        cex_task_id: None,
        cex_status: None,
    };
    let snapshot = match record_world_action(&state, request).await {
        Ok(value) => value,
        Err(_response) => {
            return Redirect::to("/world?played=0&recovery=action-input#world-action-console")
                .into_response()
        }
    };
    if let Err(_response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_action").await
    {
        return Redirect::to("/world?played=0&recovery=persistence#world-action-console")
            .into_response();
    }
    Redirect::to("/world?played=1#world-action-console").into_response()
}

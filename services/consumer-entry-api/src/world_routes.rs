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
    let started_at = std::time::Instant::now();
    let league = state.inner.league_state.lock().await;
    let viewport = world_map_viewport_json(
        &league.world,
        &matrix_user_id,
        lat,
        lng,
        zoom,
        radius_km,
        limit,
    );
    let mut response = json_resource_response(
        viewport.clone(),
        "private, max-age=5, stale-while-revalidate=25",
        viewport
            .get("etag")
            .and_then(Value::as_str)
            .map(str::to_string),
    );
    add_world_map_server_timing_headers(&mut response, started_at, "viewport");
    response
}

pub(super) async fn get_world_map_delta(
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
    world_map_delta_response(state, &matrix_user_id, &query, &headers).await
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
    let started_at = std::time::Instant::now();
    let league = state.inner.league_state.lock().await;
    let viewport = world_map_viewport_json(
        &league.world,
        &matrix_user_id,
        lat,
        lng,
        zoom,
        radius_km,
        limit,
    );
    let mut response = json_resource_response(
        viewport.clone(),
        "private, max-age=5, stale-while-revalidate=25",
        viewport
            .get("etag")
            .and_then(Value::as_str)
            .map(str::to_string),
    );
    add_world_map_server_timing_headers(&mut response, started_at, "web_viewport");
    response
}

pub(super) async fn get_world_web_map_delta(
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
    world_map_delta_response(state, &matrix_user_id, &query, &headers).await
}

async fn world_map_delta_response(
    state: AppState,
    matrix_user_id: &str,
    query: &HashMap<String, String>,
    headers: &HeaderMap,
) -> Response {
    state.inner.metrics.inc_world_map_delta_requests();
    let started_at = std::time::Instant::now();
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
    let cursor = query.get("cursor").cloned();
    let league = state.inner.league_state.lock().await;
    let delta = world_map_delta_json(
        &league.world,
        matrix_user_id,
        lat,
        lng,
        zoom,
        radius_km,
        limit,
        cursor,
    );
    if !delta
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        state.inner.metrics.inc_world_map_delta_noop_responses();
    }
    if delta
        .get("snapshot_fallback_required")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        state.inner.metrics.inc_world_map_delta_snapshot_fallbacks();
    }
    let etag = delta
        .get("etag")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let (Some(request_etag), Some(response_etag)) = (
        headers
            .get(header::IF_NONE_MATCH)
            .or_else(|| headers.get("x-trillionnium-map-if-none-match"))
            .and_then(|value| value.to_str().ok()),
        etag.as_deref(),
    ) {
        if request_etag == response_etag {
            let mut response = StatusCode::NOT_MODIFIED.into_response();
            apply_resource_headers(
                &mut response,
                "private, max-age=3, stale-while-revalidate=15",
                etag,
            );
            add_world_map_server_timing_headers(&mut response, started_at, "delta_304");
            return response;
        }
    }
    let mut response = json_resource_response(
        delta.clone(),
        "private, max-age=3, stale-while-revalidate=15",
        etag,
    );
    add_world_map_server_timing_headers(&mut response, started_at, "delta");
    response
}

fn add_world_map_server_timing_headers(
    response: &mut Response,
    started_at: std::time::Instant,
    phase: &'static str,
) {
    let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
    if let Ok(value) = HeaderValue::from_str(&format!("{elapsed_ms:.0}")) {
        response.headers_mut().insert(
            HeaderName::from_static("x-trillionnium-world-map-server-ms"),
            value,
        );
    }
    if let Ok(value) = HeaderValue::from_str(&format!(
        "trillionnium-world-map-{phase};dur={elapsed_ms:.1}"
    )) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("server-timing"), value);
    }
}

pub(super) async fn post_world_map_rum(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut payload): Json<WorldMapRumRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    payload.matrix_user_id = normalize_league_matrix_user(
        payload
            .matrix_user_id
            .as_deref()
            .unwrap_or(matrix_user_id.as_str()),
    );
    if payload.matrix_user_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    }
    world_map_rum_response(&state, &payload)
}

pub(super) async fn post_world_web_map_rum(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut payload): Json<WorldMapRumRequest>,
) -> Response {
    let allow_missing_cookie = matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
        || !state.config().league_web_session_required;
    let web_session =
        match authorize_league_web_session_readonly(&state, &headers, allow_missing_cookie) {
            Ok(value) => value,
            Err(response) => return response,
        };
    payload.matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        });
    world_map_rum_response(&state, &payload)
}

fn world_map_rum_response(state: &AppState, payload: &WorldMapRumRequest) -> Response {
    state.inner.metrics.record_world_map_rum(payload);
    Json(json!({
        "ok": true,
        "contract_version": TRILLIONNIUM_WORLD_MAP_RUNTIME_PERFORMANCE_BUDGET_CONTRACT_VERSION,
        "source": "world_map_real_user_measurement",
        "matrix_user_id": &payload.matrix_user_id,
        "surface_id": &payload.surface_id,
        "session_id": &payload.session_id,
        "sample_kind": &payload.sample_kind,
        "user_agent_class": &payload.user_agent_class,
        "viewport_cursor": &payload.viewport_cursor,
        "metrics": state.inner.metrics.world_map_rum_snapshot(),
    }))
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
        let movement_transition =
            world_map_transition_decision(&league.world, &current_node, target_trimmed);
        if !movement_transition.accepted {
            let status = movement_transition.http_status();
            let error = movement_transition.error_message();
            return (
                status,
                Json(json!({
                    "error": error,
                    "target": target_trimmed,
                    "current_node_id": current_node_id,
                    "exits": current_node.exits,
                    "movement_transition": movement_transition,
                })),
            )
                .into_response();
        }
        let Some(target_node_id) = movement_transition.to_node_id.clone() else {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world map movement transition did not resolve a target node",
                    "target": target_trimmed,
                    "current_node_id": current_node_id,
                    "movement_transition": movement_transition,
                })),
            )
                .into_response();
        };
        let Some(target_node) = league.world.world_map_nodes.get(&target_node_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "world map target node not found after transition resolution",
                    "target": target_trimmed,
                    "current_node_id": current_node_id,
                    "movement_transition": movement_transition,
                })),
            )
                .into_response();
        };
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
        let resource_pressure_mutation = apply_world_resource_pressure_mutation(
            &mut league.world,
            &matrix_user_id,
            "world_map_move",
            "world_map_move",
            Some(movement_transition.result.as_str()),
            now,
        );
        let region_story_unlock_mutation = apply_world_region_story_unlock_mutation(
            &mut league.world,
            &matrix_user_id,
            "world_map_move",
            "world_map_move",
            Some(movement_transition.result.as_str()),
            Some(target_node.node_id.as_str()),
            Some(target_node.zone_id.as_str()),
            now,
        );
        (
            league.clone(),
            current_node,
            target_node,
            position,
            economy_event,
            movement_transition,
            resource_pressure_mutation,
            region_story_unlock_mutation,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_map_move").await
    {
        return response;
    }
    let moved_nodes = snapshot
        .0
        .world
        .world_map_nodes
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let moved_geodata = openstreetmap_geodata_v1_json(&moved_nodes, Some(&snapshot.2));
    let moved_tactics_board = world_tactics_board_projection_json(
        &snapshot.0.world,
        &matrix_user_id,
        Some(&snapshot.2),
        &moved_geodata,
    );
    let world_objective_travel = moved_tactics_board
        .get("world_objective_travel")
        .cloned()
        .unwrap_or(Value::Null);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_map_move",
            "world": "trillionnium_world",
            "from_node": snapshot.1,
            "to_node": snapshot.2,
            "position": snapshot.3,
            "economy_event": snapshot.4,
            "movement_transition": snapshot.5,
            "world_objective_travel_contract_version": TRILLIONNIUM_WORLD_OBJECTIVE_TRAVEL_CONTRACT_VERSION,
            "world_objective_travel": world_objective_travel,
            "resource_pressure_runtime_contract_version": TRILLIONNIUM_WORLD_RESOURCE_PRESSURE_RUNTIME_CONTRACT_VERSION,
            "resource_pressure_mutation": snapshot.6,
            "resource_pressure_runtime": snapshot.6.get("resource_pressure_runtime").cloned().unwrap_or(Value::Null),
            "region_story_unlock_runtime_contract_version": TRILLIONNIUM_WORLD_REGION_STORY_UNLOCK_RUNTIME_CONTRACT_VERSION,
            "region_story_unlock_mutation": snapshot.7,
            "region_story_unlock_runtime": snapshot.7.get("region_story_unlock_runtime").cloned().unwrap_or(Value::Null),
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
    let loopback_unsigned_play = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(|host| {
            let host = host
                .split_once(':')
                .map(|(name, _)| name)
                .unwrap_or(host)
                .trim()
                .trim_matches(['[', ']']);
            matches!(host, "127.0.0.1" | "localhost" | "::1")
        })
        .unwrap_or(false);
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if (matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                || loopback_unsigned_play)
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
    let wants_json = payload
        .response
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("json"));
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
        if wants_json {
            response
        } else {
            Redirect::to("/world?map=moved").into_response()
        }
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
        let released = playability_outcome
            .get("payout_status")
            .and_then(Value::as_str)
            != Some("review_hold");
        let released_cex_task_id = if released {
            payload.cex_task_id.clone()
        } else {
            None
        };
        let released_cex_status = if released {
            payload.cex_status.clone()
        } else {
            None
        };
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
            cex_task_id: released_cex_task_id.clone(),
            cex_status: released_cex_status.clone(),
            created_at_epoch: now,
        };
        let mut created_contract = None;
        if let Some(task_id) = released_cex_task_id.clone() {
            let contract = WorldContract {
                contract_id: league_hash_id("world-contract", &event.event_id),
                event_id: event.event_id.clone(),
                actor_matrix_user_id: matrix_user_id.clone(),
                location_id: location_id.clone(),
                task_id,
                title: "World Contract".to_string(),
                body: body.clone(),
                status: "task_created".to_string(),
                cex_status: released_cex_status.clone(),
                value_score: impact,
                created_at_epoch: now,
            };
            league.world.world_contracts.push(contract.clone());
            created_contract = Some(contract);
        }
        if released && matches!(kind, "venture" | "craft") {
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
        if released {
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
        }
        let reward_delta_reputation = playability_outcome
            .get("reward_delta_reputation")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| (impact / 4).max(1));
        let reward_delta_rating = playability_outcome
            .get("reward_delta_rating")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| (impact / 3).max(1));
        if released {
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.xp += impact;
            player.reputation += reward_delta_reputation;
            player.rating += reward_delta_rating;
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }
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
        if released {
            league
                .world
                .world_economy_events
                .push(telemetry_event.clone());
        }
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
    let held_by_review = if kind == "contract" && payload.cex_task_id.is_none() {
        let league = state.inner.league_state.lock().await;
        let (base_kind, base_result, base_impact) = world_action_kind(&body);
        let (playability_outcome, _, _) = world_action_playability_outcome(
            &league,
            &matrix_user_id,
            base_kind,
            base_result,
            &body,
            base_impact,
            Utc::now().timestamp(),
        );
        playability_outcome
            .get("payout_status")
            .and_then(Value::as_str)
            == Some("review_hold")
    } else {
        false
    };
    let task = if kind == "contract" && payload.cex_task_id.is_none() && !held_by_review {
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

fn world_trillionnium_task_id(task_archetype_id: &str) -> String {
    format!("trillionnium-task:{task_archetype_id}")
}

fn world_tactics_contract_completion_released(completion: &WorldContractCompletion) -> bool {
    matches!(
        completion.ledger_status.as_deref(),
        Some("settled") | Some("duplicate")
    )
}

fn latest_open_trillionnium_task_contract_index(
    world: &WorldState,
    matrix_user_id: &str,
    task_archetype_id: &str,
) -> Option<usize> {
    let task_id = world_trillionnium_task_id(task_archetype_id);
    world
        .world_contracts
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, contract)| {
            let released_completion_exists =
                world.world_contract_completions.iter().any(|completion| {
                    completion.contract_id == contract.contract_id
                        && world_tactics_contract_completion_released(completion)
                });
            if contract.actor_matrix_user_id == matrix_user_id
                && contract.task_id == task_id
                && !released_completion_exists
                && !contract.status.starts_with("completed_")
                && contract.status != "review_hold"
            {
                Some(index)
            } else {
                None
            }
        })
}

fn judge_trillionnium_task_completion(
    world: &WorldState,
    matrix_user_id: &str,
    contract_id: &str,
    task_archetype_id: &str,
    body: &str,
    now: i64,
) -> LeagueJudgement {
    let (quality_score, missing_signals) = world_action_quality_signal(body);
    let normalized_body = body.trim().to_ascii_lowercase();
    let duplicate_count = world
        .world_contract_completions
        .iter()
        .filter(|completion| {
            completion.contract_id == contract_id
                && completion.matrix_user_id == matrix_user_id
                && completion.body.trim().to_ascii_lowercase() == normalized_body
                && now.saturating_sub(completion.created_at_epoch) <= 300
        })
        .count();
    let mut anti_cheat_flags = missing_signals
        .iter()
        .map(|signal| format!("missing_{signal}"))
        .collect::<Vec<_>>();
    if duplicate_count > 0 {
        anti_cheat_flags.push("duplicate_trillionnium_task_completion".to_string());
    }
    if body.chars().count() < 64 {
        anti_cheat_flags.push("task_report_too_short".to_string());
    }
    anti_cheat_flags.sort();
    anti_cheat_flags.dedup();
    let score =
        (58.0 + quality_score as f64 * 7.0 - duplicate_count as f64 * 30.0).clamp(0.0, 96.0);
    let grade = if score >= 90.0 {
        "S"
    } else if score >= 82.0 {
        "A"
    } else if score >= 70.0 {
        "B"
    } else if score >= 55.0 {
        "C"
    } else {
        "D"
    }
    .to_string();
    let payout_status = if anti_cheat_flags.is_empty() {
        "eligible"
    } else {
        "review_hold"
    }
    .to_string();
    let reward_amount = (score / 10.0).max(1.0);
    LeagueJudgement {
        score,
        grade,
        reward_amount,
        judge_status: "deterministic_trillionnium_task_completion_judge_v1".to_string(),
        payout_status,
        anti_cheat_flags,
        score_events: vec![LeagueScoreEvent {
            dimension: "trillionnium_task_completion_quality".to_string(),
            score,
            weight: 1.0,
            judge_kind: "rust_deterministic_quality_gate".to_string(),
            evidence: json!({
                "task_archetype_id": task_archetype_id,
                "quality_score": quality_score,
                "missing_signals": missing_signals,
                "duplicate_count": duplicate_count,
                "ledger_reward_requires_settlement": true,
                "review_hold_gate_enforced": true,
                "anti_cheese_gate_enforced": true,
            }),
        }],
    }
}

async fn record_world_tactics_command(
    state: &AppState,
    payload: WorldTacticsCommandRequest,
) -> Result<
    (
        LeagueState,
        WorldEvent,
        Value,
        Value,
        Option<WorldContract>,
        Option<WorldContractCompletion>,
        Value,
        Value,
        Value,
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
    let command = validate_text_payload(&payload.command, state.config().max_text_chars)?;
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| validate_text_payload(value, state.config().max_text_chars))
        .transpose()?;
    let (mut snapshot, pending_settlement) = {
        let mut league = state.inner.league_state.lock().await;
        let now = Utc::now().timestamp();
        let mut outcome = apply_world_tactics_command(
            &mut league.world,
            &matrix_user_id,
            &command,
            payload.unit_id.as_deref(),
            payload.target_tile.as_deref(),
            payload.skill_id.as_deref(),
            payload.item_id.as_deref(),
            payload.target_slot.as_deref(),
            payload.npc_id.as_deref(),
            payload.task_archetype_id.as_deref(),
            payload.osm_game_overlay_id.as_deref(),
            now,
        );
        let mut accepted = outcome
            .get("accepted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let location_id = payload
            .osm_game_overlay_id
            .as_deref()
            .and_then(|overlay| overlay.strip_prefix("trillionnium-world-node:"))
            .filter(|node_id| league.world.world_locations.contains_key(*node_id))
            .unwrap_or_else(|| world_default_location_for_kind("tactics"))
            .to_string();
        let event_body = body.unwrap_or_else(|| {
            format!(
                "tactics command={} unit={} tile={} skill={}",
                command,
                payload.unit_id.as_deref().unwrap_or("lord"),
                payload.target_tile.as_deref().unwrap_or("none"),
                payload
                    .skill_id
                    .as_deref()
                    .or(payload.item_id.as_deref())
                    .or(payload.target_slot.as_deref())
                    .or(payload.npc_id.as_deref())
                    .or(payload.task_archetype_id.as_deref())
                    .unwrap_or("none")
            )
        });
        let event_id = league_hash_id(
            "world-tactics-event",
            &format!("{}:{}:{}:{}", matrix_user_id, command, now, event_body),
        );
        let mut created_contract = None;
        let mut created_completion = None;
        let mut completion_contract_for_settlement = None;

        if accepted && command == "offer_task" {
            let task_archetype_id = outcome
                .get("task_archetype_id")
                .and_then(Value::as_str)
                .or(payload.task_archetype_id.as_deref())
                .unwrap_or("courier_letter");
            let contract = WorldContract {
                contract_id: league_hash_id(
                    "world-trillionnium-task-contract",
                    &format!("{}:{}:{}", matrix_user_id, task_archetype_id, event_id),
                ),
                event_id: event_id.clone(),
                actor_matrix_user_id: matrix_user_id.clone(),
                location_id: location_id.clone(),
                task_id: world_trillionnium_task_id(task_archetype_id),
                title: format!("Trillionnium Task · {task_archetype_id}"),
                body: format!(
                    "Trillionnium task offer: archetype={task_archetype_id}; npc={}; objective={}; source=rust_trillionnium_task_offer_validator.",
                    payload.npc_id.as_deref().unwrap_or("unknown_npc"),
                    payload
                        .osm_game_overlay_id
                        .as_deref()
                        .unwrap_or("rust_generated_objective")
                ),
                status: "trillionnium_task_offered".to_string(),
                cex_status: Some("trillionnium_task_pending_completion".to_string()),
                value_score: 8,
                created_at_epoch: now,
            };
            outcome["trillionnium_task_contract_id"] = json!(contract.contract_id.clone());
            outcome["trillionnium_task_contract_status"] = json!(contract.status.clone());
            outcome["completion_command"] = json!("complete_task");
            outcome["ledger_reward_requires_settlement"] = json!(true);
            league.world.world_contracts.push(contract.clone());
            created_contract = Some(contract);
        }

        if accepted && command == "complete_task" {
            let task_archetype_id = outcome
                .get("task_archetype_id")
                .and_then(Value::as_str)
                .or(payload.task_archetype_id.as_deref())
                .unwrap_or("courier_letter")
                .to_string();
            match latest_open_trillionnium_task_contract_index(
                &league.world,
                &matrix_user_id,
                &task_archetype_id,
            ) {
                Some(contract_index) => {
                    let contract = league.world.world_contracts[contract_index].clone();
                    let judgement = judge_trillionnium_task_completion(
                        &league.world,
                        &matrix_user_id,
                        &contract.contract_id,
                        &task_archetype_id,
                        &event_body,
                        now,
                    );
                    let completion = WorldContractCompletion {
                        completion_id: league_hash_id(
                            "world-trillionnium-task-completion",
                            &format!("{}:{}:{}", contract.contract_id, now, event_body),
                        ),
                        contract_id: contract.contract_id.clone(),
                        matrix_user_id: matrix_user_id.clone(),
                        body: event_body.clone(),
                        score: judgement.score,
                        grade: judgement.grade.clone(),
                        reward_amount: judgement.reward_amount,
                        judge_status: judgement.judge_status.clone(),
                        payout_status: judgement.payout_status.clone(),
                        anti_cheat_flags: judgement.anti_cheat_flags.clone(),
                        score_events: judgement.score_events.clone(),
                        ledger_status: Some("pending".to_string()),
                        ledger_account_id: None,
                        ledger_entry_id: None,
                        ledger_balance_after: None,
                        ledger_error: None,
                        created_at_epoch: now,
                    };
                    let pending_release = completion.payout_status == "eligible";
                    let stored_contract = &mut league.world.world_contracts[contract_index];
                    stored_contract.status = if pending_release {
                        "trillionnium_task_completion_pending_settlement".to_string()
                    } else {
                        "review_hold".to_string()
                    };
                    stored_contract.cex_status = Some(if pending_release {
                        "settlement_pending".to_string()
                    } else {
                        "review_hold".to_string()
                    });
                    outcome["trillionnium_task_contract_id"] = json!(contract.contract_id.clone());
                    outcome["trillionnium_task_completion_id"] =
                        json!(completion.completion_id.clone());
                    outcome["completion_contract_version"] =
                        json!(TRILLIONNIUM_TASK_COMPLETION_CONTRACT_VERSION);
                    outcome["reward_gate_contract_version"] =
                        json!(TRILLIONNIUM_REWARD_GATE_CONTRACT_VERSION);
                    outcome["payout_status"] = json!(completion.payout_status.clone());
                    outcome["anti_cheat_flags"] = json!(completion.anti_cheat_flags.clone());
                    outcome["ledger_status"] = json!("pending");
                    outcome["ledger_reward_requires_settlement"] = json!(true);
                    outcome["review_hold_gate_enforced"] = json!(true);
                    outcome["anti_cheese_gate_enforced"] = json!(true);
                    league
                        .world
                        .world_contract_completions
                        .push(completion.clone());
                    completion_contract_for_settlement = Some((contract, completion.clone()));
                    created_completion = Some(completion);
                }
                None => {
                    accepted = false;
                    outcome = json!({
                        "contract_version": TRILLIONNIUM_TACTICS_COMMAND_OUTCOME_CONTRACT_VERSION,
                        "accepted": false,
                        "command": command,
                        "unit_id": payload.unit_id.as_deref().unwrap_or("lord"),
                        "target_tile": payload.target_tile.as_deref(),
                        "task_archetype_id": task_archetype_id,
                        "result": "task_completion_requires_offer",
                        "rejection_reason": "complete_task_requires_open_trillionnium_task_contract",
                        "source_of_truth": "rust_trillionnium_task_completion_handler",
                        "web_role": "intent_only_visualization_input",
                    });
                }
            }
        }

        let result = outcome
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or(if accepted {
                "tactics_command_accepted"
            } else {
                "tactics_command_rejected"
            })
            .to_string();
        let event = WorldEvent {
            event_id,
            actor_matrix_user_id: matrix_user_id.clone(),
            room_id: payload.room_id.clone(),
            location_id: location_id.clone(),
            event_kind: format!("tactics_{command}"),
            body: event_body,
            result,
            impact_score: if accepted { 8 } else { 0 },
            cex_task_id: None,
            cex_status: None,
            created_at_epoch: now,
        };
        league.world.world_events.push(event.clone());
        if accepted {
            let relationship_target = outcome
                .get("npc_id")
                .and_then(Value::as_str)
                .or_else(|| outcome.get("mentor_npc_id").and_then(Value::as_str))
                .map(ToString::to_string)
                .or_else(|| payload.npc_id.clone())
                .or_else(|| payload.skill_id.clone())
                .or_else(|| payload.task_archetype_id.clone())
                .or_else(|| payload.unit_id.clone())
                .unwrap_or_else(|| "lord".to_string());
            let relationship_kind = if relationship_target.starts_with("npc-") {
                format!("trillionnium_npc_{command}")
            } else {
                format!("tactics_{command}")
            };
            let relationship_strength = match command.as_str() {
                "talk_npc" => 3,
                "train_skill" => 4,
                "offer_task" => 5,
                "complete_task" => 2,
                "attack" => 2,
                _ => 8,
            };
            league.world.world_relationships.push(WorldRelationship {
                relationship_id: league_hash_id(
                    "world-tactics-rel",
                    &format!(
                        "{}:{}:{}:{}",
                        matrix_user_id, relationship_target, command, now
                    ),
                ),
                from_id: matrix_user_id.clone(),
                to_id: relationship_target,
                relation_kind: relationship_kind,
                strength: relationship_strength,
                updated_at_epoch: now,
            });
        }
        if accepted && matches!(command.as_str(), "attack" | "complete_task") {
            let pressure_event_kind = match command.as_str() {
                "attack" => "tactics_attack",
                "complete_task" => "tactics_complete_task",
                _ => "tactics_command",
            };
            let pressure_result = if command == "attack" {
                outcome
                    .get("combat_resolution")
                    .and_then(|resolution| resolution.get("result"))
                    .and_then(Value::as_str)
                    .or_else(|| outcome.get("result").and_then(Value::as_str))
            } else {
                outcome.get("result").and_then(Value::as_str)
            };
            let resource_pressure_mutation = apply_world_resource_pressure_mutation(
                &mut league.world,
                &matrix_user_id,
                pressure_event_kind,
                &command,
                pressure_result,
                now,
            );
            outcome["resource_pressure_runtime_contract_version"] =
                json!(TRILLIONNIUM_WORLD_RESOURCE_PRESSURE_RUNTIME_CONTRACT_VERSION);
            outcome["resource_pressure_mutation"] = resource_pressure_mutation.clone();
            outcome["resource_pressure_runtime"] = resource_pressure_mutation
                .get("resource_pressure_runtime")
                .cloned()
                .unwrap_or(Value::Null);
            let unlock_event_kind = match command.as_str() {
                "attack" => "tactics_attack",
                "complete_task" => "tactics_complete_task",
                _ => "tactics_command",
            };
            let unlock_result = outcome.get("result").and_then(Value::as_str);
            let unlock_node_id = league
                .world
                .world_player_positions
                .get(&matrix_user_id)
                .map(|position| position.node_id.clone())
                .filter(|node_id| league.world.world_map_nodes.contains_key(node_id))
                .unwrap_or_else(|| default_world_node_id().to_string());
            let unlock_zone_id = league
                .world
                .world_map_nodes
                .get(&unlock_node_id)
                .map(|node| node.zone_id.clone());
            let region_story_unlock_mutation = apply_world_region_story_unlock_mutation(
                &mut league.world,
                &matrix_user_id,
                unlock_event_kind,
                &command,
                unlock_result,
                Some(unlock_node_id.as_str()),
                unlock_zone_id.as_deref(),
                now,
            );
            outcome["region_story_unlock_runtime_contract_version"] =
                json!(TRILLIONNIUM_WORLD_REGION_STORY_UNLOCK_RUNTIME_CONTRACT_VERSION);
            outcome["region_story_unlock_mutation"] = region_story_unlock_mutation.clone();
            outcome["region_story_unlock_runtime"] = region_story_unlock_mutation
                .get("region_story_unlock_runtime")
                .cloned()
                .unwrap_or(Value::Null);
            if command == "attack" {
                let combat_resolution = outcome
                    .get("combat_resolution")
                    .cloned()
                    .unwrap_or(Value::Null);
                let combat_numerics_mutation = apply_world_combat_numerics_mutation(
                    &mut league.world,
                    &matrix_user_id,
                    "tactics_attack",
                    &command,
                    &combat_resolution,
                    now,
                );
                outcome["combat_numerics_runtime_contract_version"] =
                    json!(TRILLIONNIUM_WORLD_COMBAT_NUMERICS_RUNTIME_CONTRACT_VERSION);
                outcome["combat_numerics_mutation"] = combat_numerics_mutation.clone();
                outcome["combat_numerics_runtime"] = combat_numerics_mutation
                    .get("combat_numerics_runtime")
                    .cloned()
                    .unwrap_or(Value::Null);
            }
        }
        let (mut tactics_session, simulation_tick) = record_world_tactics_simulation_tick(
            &mut league.world,
            &matrix_user_id,
            payload.room_id.as_deref(),
            &command,
            payload.unit_id.as_deref(),
            payload.target_tile.as_deref(),
            payload.osm_game_overlay_id.as_deref(),
            &outcome,
            now,
        );
        let mut tactics_reward_settlement = json!({
            "contract_version": TRILLIONNIUM_TACTICS_REWARD_SETTLEMENT_CONTRACT_VERSION,
            "status": "not_eligible",
            "source_of_truth": "rust_tactics_reward_settlement",
            "reward_requires_victory": true,
            "web_role": "intent_only_visualization_input",
        });
        if tactics_session.get("victory_state").and_then(Value::as_str) == Some("victory")
            && tactics_session.get("reward_status").and_then(Value::as_str)
                == Some("pending_settlement")
        {
            let session_id = tactics_session
                .get("session_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let tick_id = simulation_tick
                .get("tick_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let reward_event_id = league_hash_id(
                "world-tactics-victory-reward",
                &format!("{}:{}:{}", matrix_user_id, session_id, tick_id),
            );
            let reward_credits = 5;
            let reward_xp = 12;
            let already_settled = league
                .world
                .world_economy_events
                .iter()
                .any(|event| event.economy_event_id == reward_event_id);
            if !already_settled {
                let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
                player.earned_credits += reward_credits as f64;
                player.xp += reward_xp;
                player.reputation += 1;
                player.rating += 1;
                league
                    .players_by_matrix_user
                    .insert(matrix_user_id.clone(), player);
                league.world.world_economy_events.push(WorldEconomyEvent {
                    economy_event_id: reward_event_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    event_kind: "tactics_victory_reward".to_string(),
                    subject_id: session_id.clone(),
                    credits_delta: reward_credits,
                    reputation_delta: 1,
                    created_at_epoch: now,
                });
            }
            if let Some(updated_session) = mark_world_tactics_reward_settled(
                &mut league.world,
                &session_id,
                &reward_event_id,
                reward_credits,
                reward_xp,
                now,
            ) {
                tactics_session = updated_session;
            }
            tactics_reward_settlement = json!({
                "contract_version": TRILLIONNIUM_TACTICS_REWARD_SETTLEMENT_CONTRACT_VERSION,
                "status": if already_settled { "duplicate_settled" } else { "settled" },
                "reward_event_id": reward_event_id,
                "session_id": session_id,
                "tick_id": tick_id,
                "credits_delta": reward_credits,
                "xp_delta": reward_xp,
                "reputation_delta": 1,
                "source_of_truth": "rust_tactics_reward_settlement",
                "settlement_owner": "world_state_and_league_player_projection",
                "ledger_required": false,
                "duplicate_safe": true,
                "web_role": "intent_only_visualization_input",
            });
        }
        outcome["tactics_game_session_contract_version"] =
            json!(TRILLIONNIUM_TACTICS_GAME_SESSION_CONTRACT_VERSION);
        outcome["tactics_simulation_tick_contract_version"] =
            json!(TRILLIONNIUM_TACTICS_SIMULATION_TICK_CONTRACT_VERSION);
        outcome["tactics_reward_settlement_contract_version"] =
            json!(TRILLIONNIUM_TACTICS_REWARD_SETTLEMENT_CONTRACT_VERSION);
        outcome["tactics_session_id"] = tactics_session["session_id"].clone();
        outcome["tactics_tick_id"] = simulation_tick["tick_id"].clone();
        outcome["tactics_reward_settlement"] = tactics_reward_settlement.clone();
        let home = world_home_json(&league);
        (
            (
                league.clone(),
                event,
                outcome,
                home,
                created_contract,
                created_completion,
                tactics_session,
                simulation_tick,
                tactics_reward_settlement,
            ),
            completion_contract_for_settlement,
        )
    };
    if let Some((contract, mut completion)) = pending_settlement {
        let settlement_payload = WorldContractCompleteRequest {
            matrix_user_id: matrix_user_id.clone(),
            room_id: payload.room_id.clone(),
            body: completion.body.clone(),
        };
        let settlement = settle_world_contract_completion_with_ledger(
            state,
            &settlement_payload,
            &matrix_user_id,
            &contract,
            &completion,
        )
        .await;
        completion.ledger_status = Some(settlement.status.clone());
        completion.ledger_account_id = settlement.account_id;
        completion.ledger_entry_id = settlement.entry_id;
        completion.ledger_balance_after = settlement.balance_after;
        completion.ledger_error = settlement.error;
        let mut final_snapshot = {
            let mut league = state.inner.league_state.lock().await;
            let indexes = build_world_indexes(&league.world);
            let settlement_completed = matches!(
                completion.ledger_status.as_deref(),
                Some("settled") | Some("duplicate")
            );
            let mut updated_contract = snapshot.4.clone();
            indexes.replace_contract_completion_by_id(&mut league.world, &completion);
            if settlement_completed {
                let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
                player.earned_credits += completion.reward_amount;
                player.xp += completion.score.round() as i64;
                player.reputation += (completion.score / 12.0).round() as i64;
                player.rating += ((completion.score - 50.0) / 4.0).round() as i64;
                league
                    .players_by_matrix_user
                    .insert(matrix_user_id.clone(), player);
                league.world.world_economy_events.push(WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-trillionnium-task-reward",
                        &completion.completion_id,
                    ),
                    matrix_user_id: matrix_user_id.clone(),
                    event_kind: "trillionnium_task_reward".to_string(),
                    subject_id: contract.contract_id.clone(),
                    credits_delta: completion.reward_amount.round() as i64,
                    reputation_delta: (completion.score / 12.0).round() as i64,
                    created_at_epoch: completion.created_at_epoch,
                });
            }
            if let Some(contract_index) = indexes.contract_index(&contract.contract_id) {
                let mut stored_contract = league.world.world_contracts[contract_index].clone();
                stored_contract.status = match completion.ledger_status.as_deref() {
                    Some("settled") | Some("duplicate") => "completed_settled".to_string(),
                    Some("held_review") => "review_hold".to_string(),
                    Some(status) => format!("completed_{status}"),
                    None => stored_contract.status.clone(),
                };
                stored_contract.cex_status = Some(match completion.ledger_status.as_deref() {
                    Some("settled") | Some("duplicate") => "completed".to_string(),
                    Some("held_review") => "review_hold".to_string(),
                    Some("skipped_zero_reward") => "completed_no_reward".to_string(),
                    Some(_) => "settlement_blocked".to_string(),
                    None => stored_contract
                        .cex_status
                        .clone()
                        .unwrap_or_else(|| "settlement_pending".to_string()),
                });
                if settlement_completed {
                    stored_contract.value_score += (completion.score / 10.0).round() as i64;
                }
                indexes.replace_contract_by_id(&mut league.world, &stored_contract);
                updated_contract = Some(stored_contract);
            }
            let home = world_home_json(&league);
            (
                league.clone(),
                snapshot.1.clone(),
                snapshot.2.clone(),
                home,
                updated_contract,
                Some(completion.clone()),
                snapshot.6.clone(),
                snapshot.7.clone(),
                snapshot.8.clone(),
            )
        };
        final_snapshot.2["ledger_status"] = json!(settlement.status);
        final_snapshot.2["ledger_entry_id"] = json!(completion.ledger_entry_id.clone());
        final_snapshot.2["ledger_error"] = json!(completion.ledger_error.clone());
        final_snapshot.2["trillionnium_task_completion"] = json!(completion);
        snapshot = final_snapshot;
    }
    Ok(snapshot)
}

pub(super) async fn post_world_tactics_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldTacticsCommandRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let snapshot = match record_world_tactics_command(&state, payload).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_tactics_command").await
    {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_tactics_command",
            "event": snapshot.1,
            "outcome": snapshot.2,
            "home": snapshot.3,
            "trillionnium_task_contract": snapshot.4,
            "trillionnium_task_completion": snapshot.5,
            "tactics_session": snapshot.6,
            "simulation_tick": snapshot.7,
            "tactics_reward_settlement": snapshot.8,
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

pub(super) async fn post_world_web_tactics_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebTacticsCommandRequest>,
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
    let command = payload
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("select_unit")
        .to_string();
    let command_for_redirect = command.clone();
    let request = WorldTacticsCommandRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        command,
        unit_id: payload.unit_id,
        target_tile: payload.target_tile,
        skill_id: payload.skill_id,
        item_id: payload.item_id,
        target_slot: payload.target_slot,
        npc_id: payload.npc_id,
        task_archetype_id: payload.task_archetype_id,
        osm_game_overlay_id: payload.osm_game_overlay_id,
        body: payload.body,
    };
    let snapshot = match record_world_tactics_command(&state, request).await {
        Ok(value) => value,
        Err(_response) => {
            return Redirect::to(
                "/world?tactics=0&recovery=command-input#trillionnium-tactics-game-shell",
            )
            .into_response()
        }
    };
    if let Err(_response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_tactics_command").await
    {
        return Redirect::to(
            "/world?tactics=0&recovery=persistence#trillionnium-tactics-game-shell",
        )
        .into_response();
    }
    let accepted = snapshot
        .2
        .get("accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if accepted {
        let target = match command_for_redirect.as_str() {
            "attack" => "/world?tactics=1&combat=resolved#world-local-combat-encounter",
            "equip_item" => "/world?tactics=1&item=equipped#trillionnium-equipment",
            "talk_npc" => "/world?tactics=1&npc=talked#world-play-first-action-prompt",
            "train_skill" => "/world?tactics=1&skill=trained#world-local-skill-practice",
            "offer_task" => "/world?tactics=1&task=offered#world-play-first-action-prompt",
            "complete_task" => "/world?tactics=1&task=completed#world-play-first-action-prompt",
            _ => "/world?tactics=1#trillionnium-tactics-game-shell",
        };
        Redirect::to(target).into_response()
    } else {
        let target = match command_for_redirect.as_str() {
            "attack" => "/world?tactics=0&combat=rejected#world-local-combat-encounter",
            "equip_item" => "/world?tactics=0&item=rejected#trillionnium-equipment",
            "talk_npc" | "train_skill" | "offer_task" | "complete_task" => {
                "/world?tactics=0&task=rejected#world-play-first-action-prompt"
            }
            _ => "/world?tactics=0#trillionnium-tactics-game-shell",
        };
        Redirect::to(target).into_response()
    }
}

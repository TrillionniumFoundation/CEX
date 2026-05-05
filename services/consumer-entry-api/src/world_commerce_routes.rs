use super::*;

const TRILLIONNIUM_MARKET_SIMULATOR_CONTRACT_VERSION: &str = "trillionnium_market_simulator_v1";

fn world_market_tax_credits_for_price(price_credits: i64) -> i64 {
    (price_credits.max(1) / 20).max(1)
}

fn world_seller_net_credits_for_price(price_credits: i64) -> i64 {
    price_credits
        .max(1)
        .saturating_sub(world_market_tax_credits_for_price(price_credits))
        .max(0)
}

fn world_purchase_seller_settlement_active(purchase: &WorldPurchase) -> bool {
    matches!(
        purchase.ledger_status.as_deref(),
        Some("settled") | Some("duplicate") | Some("reopened_settled")
    )
}

fn world_purchase_rejection_settlement_released(purchase: &WorldPurchase) -> bool {
    purchase.status == "rejected_refunded"
        && matches!(purchase.buyer_consume_status.as_deref(), Some("refunded"))
        && matches!(
            purchase.ledger_status.as_deref(),
            Some("seller_chargeback_consumed")
        )
}

fn world_contract_completion_released(completion: &WorldContractCompletion) -> bool {
    matches!(
        completion.ledger_status.as_deref(),
        Some("settled") | Some("duplicate") | Some("skipped_zero_reward")
    )
}

fn world_contract_completion_final(contract: &WorldContract) -> bool {
    matches!(contract.status.as_str(), "completed_settled")
        || matches!(
            contract.cex_status.as_deref(),
            Some("completed") | Some("completed_no_reward")
        )
}

fn world_market_simulation_json(world: &WorldState, listing: &WorldListing, now: i64) -> Value {
    let base_price = listing.price_credits.max(1);
    let recent_window_seconds = 86_400;
    let recent_company_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            purchase.company_id == listing.company_id
                && now.saturating_sub(purchase.created_at_epoch) <= recent_window_seconds
        })
        .count() as i64;
    let active_company_listing_count = world
        .world_listings
        .iter()
        .filter(|candidate| {
            candidate.company_id == listing.company_id && candidate.status == "listed"
        })
        .count() as i64;
    let demand_premium = ((recent_company_purchase_count * base_price) / 20).clamp(0, 50);
    let scarcity_premium = ((3 - active_company_listing_count).max(0) * 2).clamp(0, 12);
    let quality_premium = (listing.quality_score / 25).clamp(0, 20);
    let dynamic_price_credits =
        (base_price + demand_premium + scarcity_premium + quality_premium).max(1);
    let market_tax_credits = world_market_tax_credits_for_price(dynamic_price_credits);
    let seller_net_credits = world_seller_net_credits_for_price(dynamic_price_credits);
    let demand_index = (100 + recent_company_purchase_count * 8 + quality_premium).clamp(50, 200);
    let scarcity_index =
        (100 + scarcity_premium * 5 - active_company_listing_count * 2).clamp(40, 180);
    json!({
        "contract_version": TRILLIONNIUM_MARKET_SIMULATOR_CONTRACT_VERSION,
        "status": "priced",
        "listing_id": listing.listing_id,
        "company_id": listing.company_id,
        "base_price_credits": base_price,
        "dynamic_price_credits": dynamic_price_credits,
        "demand_premium_credits": demand_premium,
        "scarcity_premium_credits": scarcity_premium,
        "quality_premium_credits": quality_premium,
        "market_tax_credits": market_tax_credits,
        "seller_net_credits": seller_net_credits,
        "demand_index": demand_index,
        "scarcity_index": scarcity_index,
        "recent_company_purchase_count": recent_company_purchase_count,
        "active_company_listing_count": active_company_listing_count,
        "sinks": ["market_tax", "review_hold_delay", "refund_risk"],
        "strategy_hint": "High demand raises price; scarce quality supply earns more but pays a visible market tax.",
    })
}

pub(super) async fn get_world_assets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&league.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_assets",
            "world": "trillionnium_world",
            "assets": league.world.world_assets,
            "upgrades": league.world.world_asset_upgrades,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn upgrade_world_asset_inner(
    state: AppState,
    asset_id: String,
    payload: WorldAssetUpgradeRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let _room_id = payload.room_id.clone();
    let resolved_asset_id = {
        let league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(asset_index) = indexes.resolve_asset_index(&asset_id, &matrix_user_id) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "no world asset found for player", "asset_id": asset_id })),
            )
                .into_response();
        };
        league
            .world
            .world_assets
            .get(asset_index)
            .map(|asset| asset.asset_id.clone())
            .unwrap_or_else(|| asset_id.clone())
    };
    let judgement =
        judge_league_submission_with_pipeline(&state, &body, "world_asset_upgrade").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(asset_index) = indexes.asset_index(&resolved_asset_id) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world asset not found", "asset_id": resolved_asset_id })),
            )
                .into_response();
        };
        if league.world.world_assets[asset_index].owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world asset belongs to another player", "asset_id": resolved_asset_id })),
            )
                .into_response();
        }
        let level_before = league.world.world_assets[asset_index].upgrade_level.max(1);
        let points_before = league.world.world_assets[asset_index].upgrade_points.max(0);
        let value_delta = if judgement.payout_status == "eligible" {
            (judgement.score / 4.0).round().max(1.0) as i64
        } else {
            0
        };
        let points_after = points_before + value_delta;
        let level_after = level_before.max(1) + (points_after / 80) - (points_before / 80);
        let upgrade_status = if judgement.payout_status == "eligible" {
            "applied".to_string()
        } else {
            "review_hold".to_string()
        };
        if value_delta > 0 {
            let asset = &mut league.world.world_assets[asset_index];
            asset.value_score += value_delta;
            asset.upgrade_points = points_after;
            asset.upgrade_level = level_after.max(level_before);
            asset.last_upgrade_kind = Some("manual_upgrade".to_string());
            asset.status = "upgraded".to_string();
        }
        let upgrade = WorldAssetUpgrade {
            upgrade_id: league_hash_id(
                "world-asset-upgrade",
                &format!("{}:{}:{}", resolved_asset_id, now, body),
            ),
            asset_id: resolved_asset_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
            upgrade_kind: "manual_upgrade".to_string(),
            score: judgement.score,
            grade: judgement.grade.clone(),
            judge_status: judgement.judge_status.clone(),
            status: upgrade_status,
            value_delta,
            level_before,
            level_after: level_after.max(level_before),
            created_at_epoch: now,
        };
        if judgement.payout_status == "eligible" {
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.xp += judgement.score.round() as i64;
            player.reputation += (judgement.score / 10.0).round() as i64;
            player.rating += ((judgement.score - 50.0) / 4.0).round() as i64;
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }
        league.world.world_asset_upgrades.push(upgrade.clone());
        (
            league.clone(),
            league.world.world_assets[asset_index].clone(),
            upgrade,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_asset_upgrade").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_asset_upgrade",
            "world": "trillionnium_world",
            "asset": snapshot.1,
            "upgrade": snapshot.2,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn upgrade_world_asset(
    Path(asset_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldAssetUpgradeRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    upgrade_world_asset_inner(state, asset_id, payload).await
}

pub(super) async fn post_world_web_asset_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebAssetUpgradeRequest>,
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
    let asset_id = payload
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Upgrade this world item for a customer deliverable: strengthen capability, evidence package, risk controls, next action loop, self-review, and side-quest handoff.")
        .to_string();
    let request = WorldAssetUpgradeRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        body,
    };
    let response = upgrade_world_asset_inner(state, asset_id, request).await;
    if response.status().is_success() {
        Redirect::to("/world?asset=upgraded").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_companies(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&league.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_companies",
            "world": "trillionnium_world",
            "companies": league.world.world_companies,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn create_world_company_inner(
    state: AppState,
    payload: WorldCompanyRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let requested_asset_id = payload
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_company").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(asset) = indexes
            .resolve_asset_index(&requested_asset_id, &matrix_user_id)
            .and_then(|asset_index| league.world.world_assets.get(asset_index))
            .cloned()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world asset not found", "asset_id": requested_asset_id })),
            )
                .into_response();
        };
        if asset.owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world asset belongs to another player", "asset_id": asset.asset_id })),
            )
                .into_response();
        }
        let released = judgement.payout_status == "eligible";
        let revenue_score = if released {
            ((asset.value_score as f64) * 0.6 + judgement.score).round() as i64
        } else {
            0
        };
        let reputation_score = if released {
            ((asset.upgrade_level.max(1) * 10) as f64 + judgement.score / 2.0).round() as i64
        } else {
            0
        };
        let level = 1 + (revenue_score / 100).max(0);
        let company_kind = if body.contains("店") || body.to_ascii_lowercase().contains("shop") {
            "shop"
        } else if body.contains("工坊") || body.to_ascii_lowercase().contains("studio") {
            "studio"
        } else {
            "company"
        };
        let company = WorldCompany {
            company_id: league_hash_id(
                "world-company",
                &format!("{}:{}:{}", matrix_user_id, asset.asset_id, now),
            ),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: asset.asset_id.clone(),
            location_id: asset.location_id.clone(),
            name: if company_kind == "shop" {
                "Mirror Market Shop".to_string()
            } else if company_kind == "studio" {
                "Trillionnium Craft Studio".to_string()
            } else {
                "Reality Venture Company".to_string()
            },
            company_kind: company_kind.to_string(),
            status: if released {
                "operating".to_string()
            } else {
                "review_hold".to_string()
            },
            revenue_score,
            reputation_score,
            level,
            created_at_epoch: now,
        };
        let shop = WorldShop {
            shop_id: league_hash_id(
                "world-shop",
                &format!("{}:{}:{}", matrix_user_id, company.company_id, now),
            ),
            company_id: company.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            location_id: company.location_id.clone(),
            name: format!("{} Storefront", company.name),
            shop_kind: company_kind.to_string(),
            status: company.status.clone(),
            listing_count: if released { 1 } else { 0 },
            gross_merchandise_score: revenue_score.max(0),
            created_at_epoch: now,
        };
        let listing = WorldListing {
            listing_id: league_hash_id(
                "world-listing",
                &format!("{}:{}:{}", matrix_user_id, shop.shop_id, now),
            ),
            shop_id: shop.shop_id.clone(),
            company_id: company.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: asset.asset_id.clone(),
            title: body.chars().take(42).collect::<String>(),
            listing_kind: "service_offer".to_string(),
            status: company.status.clone(),
            price_credits: if released {
                (revenue_score / 2).max(10)
            } else {
                0
            },
            quality_score: if released {
                judgement.score.round() as i64
            } else {
                0
            },
            created_at_epoch: now,
        };
        let economy_event = if released {
            Some(WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!("{}:{}:{}", matrix_user_id, listing.listing_id, now),
                ),
                matrix_user_id: matrix_user_id.clone(),
                event_kind: "company_launch".to_string(),
                subject_id: company.company_id.clone(),
                credits_delta: listing.price_credits,
                reputation_delta: reputation_score,
                created_at_epoch: now,
            })
        } else {
            None
        };
        if released {
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.xp += judgement.score.round() as i64;
            player.reputation += (judgement.score / 6.0).round() as i64;
            player.rating += ((judgement.score - 50.0) / 4.0).round() as i64;
            league.world.world_relationships.push(WorldRelationship {
                relationship_id: league_hash_id(
                    "world-rel",
                    &format!("{}:{}:{}", matrix_user_id, company.company_id, now),
                ),
                from_id: matrix_user_id.clone(),
                to_id: company.company_id.clone(),
                relation_kind: "owner".to_string(),
                strength: reputation_score,
                updated_at_epoch: now,
            });
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }
        league.world.world_companies.push(company.clone());
        league.world.world_shops.push(shop.clone());
        league.world.world_listings.push(listing.clone());
        if let Some(economy_event) = economy_event.clone() {
            league.world.world_economy_events.push(economy_event);
        }
        (
            league.clone(),
            company,
            shop,
            listing,
            economy_event,
            judgement,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_company").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_company_created",
            "world": "trillionnium_world",
            "company": snapshot.1,
            "shop": snapshot.2,
            "listing": snapshot.3,
            "economy_event": snapshot.4,
            "judge_status": snapshot.5.judge_status,
            "payout_status": snapshot.5.payout_status,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn create_world_company(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldCompanyRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    create_world_company_inner(state, payload).await
}

pub(super) async fn post_world_web_company(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebCompanyRequest>,
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
        .unwrap_or("Launch a global-facing studio hub with this item: define customer deliverables, evidence package, risk controls, next action loop, self-review, and the first bounty route.")
        .to_string();
    let request = WorldCompanyRequest {
        matrix_user_id,
        asset_id: payload
            .asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        body,
    };
    let response = create_world_company_inner(state, request).await;
    if response.status().is_success() {
        Redirect::to("/world?company=created").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_shops(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&league.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_shops",
            "world": "trillionnium_world",
            "companies": league.world.world_companies,
            "shops": league.world.world_shops,
            "listings": league.world.world_listings,
            "economy_events": league.world.world_economy_events,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn create_world_listing_inner(
    state: AppState,
    payload: WorldListingRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let requested_company_id = payload
        .company_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_listing").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(company_index) =
            indexes.resolve_company_index(&requested_company_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world company not found", "company_id": requested_company_id })),
            )
                .into_response();
        };
        let company_seed = league.world.world_companies[company_index].clone();
        if company_seed.owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world company belongs to another player", "company_id": company_seed.company_id })),
            )
                .into_response();
        }
        if company_seed.status != "operating" {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world company is not operating",
                    "company_id": company_seed.company_id,
                    "status": company_seed.status,
                })),
            )
                .into_response();
        }
        let shop_index = match indexes
            .shop_index_by_company_id
            .get(&company_seed.company_id)
            .copied()
        {
            Some(index) => index,
            None => {
                league.world.world_shops.push(WorldShop {
                    shop_id: league_hash_id(
                        "world-shop",
                        &format!("{}:{}:{}", matrix_user_id, company_seed.company_id, now),
                    ),
                    company_id: company_seed.company_id.clone(),
                    owner_matrix_user_id: matrix_user_id.clone(),
                    location_id: company_seed.location_id.clone(),
                    name: format!("{} Storefront", company_seed.name),
                    shop_kind: company_seed.company_kind.clone(),
                    status: company_seed.status.clone(),
                    listing_count: 0,
                    gross_merchandise_score: 0,
                    created_at_epoch: now,
                });
                league.world.world_shops.len() - 1
            }
        };
        let released = judgement.payout_status == "eligible";
        let quality_score = if released {
            judgement.score.round() as i64
        } else {
            0
        };
        let price_credits = if released {
            ((((company_seed.revenue_score.max(10) as f64) * 0.35) + judgement.score).round()
                as i64)
                .max(10)
        } else {
            0
        };
        let listing = WorldListing {
            listing_id: league_hash_id(
                "world-listing",
                &format!(
                    "{}:{}:{}",
                    matrix_user_id, league.world.world_shops[shop_index].shop_id, now
                ),
            ),
            shop_id: league.world.world_shops[shop_index].shop_id.clone(),
            company_id: company_seed.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: company_seed.asset_id.clone(),
            title: body.chars().take(48).collect::<String>(),
            listing_kind: if body.contains("订阅")
                || body.to_ascii_lowercase().contains("subscription")
            {
                "subscription_offer".to_string()
            } else {
                "service_offer".to_string()
            },
            status: if released {
                "listed".to_string()
            } else {
                "review_hold".to_string()
            },
            price_credits,
            quality_score,
            created_at_epoch: now,
        };
        let economy_event = if released {
            Some(WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!("{}:{}:{}", matrix_user_id, listing.listing_id, now),
                ),
                matrix_user_id: matrix_user_id.clone(),
                event_kind: "listing_published".to_string(),
                subject_id: listing.listing_id.clone(),
                credits_delta: listing.price_credits,
                reputation_delta: (judgement.score / 5.0).round() as i64,
                created_at_epoch: now,
            })
        } else {
            None
        };
        if released {
            let reputation_delta = economy_event
                .as_ref()
                .map(|event| event.reputation_delta)
                .unwrap_or_default();
            league.world.world_shops[shop_index].listing_count += 1;
            league.world.world_shops[shop_index].gross_merchandise_score += listing.price_credits;
            league.world.world_companies[company_index].revenue_score += listing.price_credits;
            league.world.world_companies[company_index].reputation_score += reputation_delta;
            league.world.world_companies[company_index].level =
                1 + (league.world.world_companies[company_index].revenue_score / 100).max(0);
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.xp += quality_score;
            player.reputation += reputation_delta;
            player.rating += ((judgement.score - 50.0) / 5.0).round() as i64;
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }
        league.world.world_listings.push(listing.clone());
        if let Some(economy_event) = economy_event.clone() {
            league.world.world_economy_events.push(economy_event);
        }
        let company = league.world.world_companies[company_index].clone();
        let shop = league.world.world_shops[shop_index].clone();
        (
            league.clone(),
            company,
            shop,
            listing,
            economy_event,
            judgement,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_listing").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_listing_created",
            "world": "trillionnium_world",
            "company": snapshot.1,
            "shop": snapshot.2,
            "listing": snapshot.3,
            "economy_event": snapshot.4,
            "judge_status": snapshot.5.judge_status,
            "payout_status": snapshot.5.payout_status,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn create_world_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldListingRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    create_world_listing_inner(state, payload).await
}

pub(super) async fn post_world_web_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebListingRequest>,
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
        .unwrap_or("Publish a Trillionnium World service listing with deliverable, price logic, evidence package, customer promise, risk controls, self-review, and next action.")
        .to_string();
    let request = WorldListingRequest {
        matrix_user_id,
        company_id: payload
            .company_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        body,
    };
    let response = create_world_listing_inner(state, request).await;
    if response.status().is_success() {
        Redirect::to("/world?listing=created").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_commerce(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&league.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_commerce",
            "world": "trillionnium_world",
            "purchases": league.world.world_purchases,
            "work_orders": league.world.world_work_orders,
            "work_deliveries": league.world.world_work_deliveries,
            "work_acceptances": league.world.world_work_acceptances,
            "work_rejections": league.world.world_work_rejections,
            "work_reopens": league.world.world_work_reopens,
            "work_cancellations": league.world.world_work_cancellations,
            "economy_events": league.world.world_economy_events,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn get_world_factions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let world_indexes = build_world_indexes(&league.world);
    let factions: Vec<WorldFaction> = world_indexes
        .sorted_faction_ids
        .iter()
        .filter_map(|faction_id| league.world.world_factions.get(faction_id).cloned())
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_factions",
            "world": "trillionnium_world",
            "index_layer": "WorldIndexes::sorted_faction_ids_v1",
            "factions": factions,
            "standings": league.world.world_faction_standings,
        })),
    )
        .into_response()
}

pub(super) async fn settle_world_purchase_ledger_action(
    state: &AppState,
    room_id: Option<&str>,
    matrix_user_id: &str,
    purchase: &WorldPurchase,
    action: &str,
    success_status: &str,
    idempotency_key: String,
    reference_id: String,
    message: &str,
    failure_context: &str,
    amount_override_credits: Option<i64>,
) -> LeagueLedgerSettlement {
    let ledger_amount_credits = amount_override_credits.unwrap_or_else(|| {
        if action == "grant" {
            world_seller_net_credits_for_price(purchase.price_credits)
        } else {
            purchase.price_credits
        }
    });
    if ledger_amount_credits <= 0 {
        return LeagueLedgerSettlement {
            status: "skipped_zero_price".to_string(),
            ..Default::default()
        };
    }
    let Some(room_id) = room_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_room".to_string(),
            ..Default::default()
        };
    };
    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.to_string(),
        room_id: room_id.to_string(),
        session_id: None,
        org_id: None,
        message: message.to_string(),
        capability_id: None,
        account_id: None,
        event_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let resolved_identity = match resolve_matrix_identity(state, &matrix_payload).await {
        Ok(identity) => identity,
        Err(_) => {
            return LeagueLedgerSettlement {
                status: "failed_identity".to_string(),
                error: Some(failure_context.to_string()),
                ..Default::default()
            }
        }
    };
    let Some(account_id) = resolved_identity.scope.account_id.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_account".to_string(),
            error: Some("matrix identity did not resolve a ledger account_id".to_string()),
            ..Default::default()
        };
    };
    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_ledger_token".to_string(),
            account_id: Some(account_id),
            error: Some("consumer-entry ledger admin token is not configured".to_string()),
            ..Default::default()
        };
    };
    let url = format!(
        "{}/v1/ledger/{action}",
        state.config().ledger_base_url.trim_end_matches('/')
    );
    let body = json!({
        "account_id": account_id,
        "amount": ledger_amount_credits as f64,
        "gross_amount": purchase.price_credits as f64,
        "market_tax_amount": if action == "grant" { (purchase.price_credits - ledger_amount_credits) as f64 } else { 0.0 },
        "idempotency_key": idempotency_key,
        "reference_id": reference_id,
    });
    let response = match state
        .inner
        .http
        .post(url)
        .header("x-admin-token", ledger_admin_token)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_network".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("failed to reach ledger-service: {err}")),
                ..Default::default()
            }
        }
    };
    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_bad_response".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("ledger-service returned non-json response: {err}")),
                ..Default::default()
            }
        }
    };
    if !status.is_success() {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("ledger action failed");
        return LeagueLedgerSettlement {
            status: if status.as_u16() == 409 {
                "duplicate".to_string()
            } else {
                "failed_ledger".to_string()
            },
            account_id: body
                .get("account_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            error: Some(format!("{}: {error}", status.as_u16())),
            ..Default::default()
        };
    }
    LeagueLedgerSettlement {
        status: success_status.to_string(),
        account_id: value
            .get("account")
            .and_then(|account| account.get("account_id"))
            .and_then(Value::as_str)
            .or_else(|| body.get("account_id").and_then(Value::as_str))
            .map(ToString::to_string),
        entry_id: value
            .get("entry")
            .and_then(|entry| entry.get("entry_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        balance_after: value
            .get("account")
            .and_then(|account| account.get("balance"))
            .and_then(Value::as_f64),
        error: None,
    }
}

pub(super) async fn reserve_world_purchase_with_ledger(
    state: &AppState,
    payload: &WorldListingBuyRequest,
    purchase: &WorldPurchase,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        payload.room_id.as_deref(),
        &purchase.buyer_matrix_user_id,
        purchase,
        "reserve",
        "reserved",
        format!("world_purchase_reserve:{}", purchase.purchase_id),
        purchase.purchase_id.clone(),
        "world listing purchase reserve",
        "matrix identity could not be resolved for world purchase reserve",
        None,
    )
    .await
}

pub(super) async fn settle_world_purchase_with_ledger(
    state: &AppState,
    payload: &WorldListingBuyRequest,
    purchase: &WorldPurchase,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        payload.room_id.as_deref(),
        &purchase.seller_matrix_user_id,
        purchase,
        "grant",
        "settled",
        format!("world_purchase:{}", purchase.purchase_id),
        purchase.listing_id.clone(),
        "world listing purchase settlement",
        "matrix identity could not be resolved for world purchase settlement",
        None,
    )
    .await
}

pub(super) async fn consume_world_purchase_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.buyer_matrix_user_id,
        purchase,
        "consume",
        "consumed",
        format!("world_purchase_consume:{}", purchase.purchase_id),
        purchase.purchase_id.clone(),
        "world listing purchase consume",
        "matrix identity could not be resolved for world purchase consume",
        None,
    )
    .await
}

pub(super) async fn refund_world_purchase_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
    refund_scope: &str,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.buyer_matrix_user_id,
        purchase,
        "refund",
        "refunded",
        format!(
            "world_purchase_refund:{}:{}",
            purchase.purchase_id, refund_scope
        ),
        purchase.purchase_id.clone(),
        "world listing purchase refund",
        "matrix identity could not be resolved for world purchase refund",
        None,
    )
    .await
}

pub(super) async fn chargeback_world_purchase_seller_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
    chargeback_scope: &str,
) -> LeagueLedgerSettlement {
    let retrying_failed_chargeback = matches!(
        purchase.ledger_status.as_deref(),
        Some("seller_chargeback_failed")
    );
    if !world_purchase_seller_settlement_active(purchase) && !retrying_failed_chargeback {
        return LeagueLedgerSettlement {
            status: "skipped_seller_not_settled".to_string(),
            account_id: purchase.ledger_account_id.clone(),
            error: Some("seller settlement is not active for chargeback".to_string()),
            ..Default::default()
        };
    }
    let seller_net_credits = world_seller_net_credits_for_price(purchase.price_credits);
    if seller_net_credits <= 0 {
        return LeagueLedgerSettlement {
            status: "skipped_zero_seller_net".to_string(),
            account_id: purchase.ledger_account_id.clone(),
            ..Default::default()
        };
    }
    let reserve = settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.seller_matrix_user_id,
        purchase,
        "reserve",
        "seller_chargeback_reserved",
        format!(
            "world_purchase_seller_chargeback_reserve:{}:{}",
            purchase.purchase_id, chargeback_scope
        ),
        purchase.purchase_id.clone(),
        "world listing seller chargeback reserve",
        "matrix identity could not be resolved for world seller chargeback reserve",
        Some(seller_net_credits),
    )
    .await;
    if !matches!(
        reserve.status.as_str(),
        "seller_chargeback_reserved" | "duplicate"
    ) {
        return LeagueLedgerSettlement {
            status: "seller_chargeback_reserve_failed".to_string(),
            account_id: reserve.account_id,
            entry_id: reserve.entry_id,
            balance_after: reserve.balance_after,
            error: reserve.error.or(Some(format!(
                "seller chargeback reserve did not complete: {}",
                reserve.status
            ))),
        };
    }
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.seller_matrix_user_id,
        purchase,
        "consume",
        "seller_chargeback_consumed",
        format!(
            "world_purchase_seller_chargeback_consume:{}:{}",
            purchase.purchase_id, chargeback_scope
        ),
        purchase.purchase_id.clone(),
        "world listing seller chargeback consume",
        "matrix identity could not be resolved for world seller chargeback consume",
        Some(seller_net_credits),
    )
    .await
}

pub(super) async fn reserve_reopened_world_purchase_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
    reopen: &WorldWorkReopen,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.buyer_matrix_user_id,
        purchase,
        "reserve",
        "reserved",
        format!(
            "world_purchase_reopen_reserve:{}:{}",
            purchase.purchase_id, reopen.reopen_id
        ),
        purchase.purchase_id.clone(),
        "world listing purchase reopen reserve",
        "matrix identity could not be resolved for world purchase reopen reserve",
        None,
    )
    .await
}

pub(super) async fn settle_reopened_world_purchase_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
    reopen: &WorldWorkReopen,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.seller_matrix_user_id,
        purchase,
        "grant",
        "reopened_settled",
        format!(
            "world_purchase_reopen_settlement:{}:{}",
            purchase.purchase_id, reopen.reopen_id
        ),
        purchase.listing_id.clone(),
        "world listing purchase reopen settlement",
        "matrix identity could not be resolved for world purchase reopen settlement",
        None,
    )
    .await
}

pub(super) async fn buy_world_listing_inner(
    state: AppState,
    listing_id: String,
    payload: WorldListingBuyRequest,
) -> Response {
    let buyer_matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let brief = match validate_text_payload(
        payload
            .body
            .as_deref()
            .unwrap_or("Accept this quest card and open an adventure commission: confirm customer deliverables, evidence package, rating standards, risk controls, next action, and self-review."),
        state.config().max_text_chars,
    ) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(listing_index) = indexes.resolve_buyable_listing_index(
            &league.world,
            &listing_id,
            &buyer_matrix_user_id,
        ) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world listing not found", "listing_id": listing_id })),
            )
                .into_response();
        };
        let listing = league.world.world_listings[listing_index].clone();
        if listing.status != "listed" {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world listing is not open for purchase",
                    "listing_id": listing.listing_id,
                    "status": listing.status,
                })),
            )
                .into_response();
        }
        let company_index = indexes.company_index(&listing.company_id);
        let shop_index = indexes.shop_index(&listing.shop_id);
        let location_id = indexes
            .company_location_id(&listing.company_id)
            .or_else(|| indexes.shop_location_id(&listing.shop_id))
            .unwrap_or("zbj-market-gate");
        let faction_id = world_faction_for_location(location_id);
        let market_simulation = world_market_simulation_json(&league.world, &listing, now);
        let price_credits = market_simulation
            .get("dynamic_price_credits")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| listing.price_credits.max(1));
        let reputation_delta = (listing.quality_score / 5).max(1);
        let purchase_nonce = league.world.world_purchases.len();
        let work_order_nonce = league.world.world_work_orders.len();
        let purchase = WorldPurchase {
            purchase_id: league_hash_id(
                "world-purchase",
                &format!(
                    "{}:{}:{}:{}:{}",
                    buyer_matrix_user_id, listing.listing_id, price_credits, now, purchase_nonce
                ),
            ),
            listing_id: listing.listing_id.clone(),
            shop_id: listing.shop_id.clone(),
            company_id: listing.company_id.clone(),
            buyer_matrix_user_id: buyer_matrix_user_id.clone(),
            seller_matrix_user_id: listing.owner_matrix_user_id.clone(),
            price_credits,
            status: "pending_payment".to_string(),
            ledger_status: Some("pending".to_string()),
            ledger_account_id: None,
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: None,
            buyer_ledger_status: Some("pending".to_string()),
            buyer_ledger_account_id: None,
            buyer_ledger_entry_id: None,
            buyer_ledger_balance_after: None,
            buyer_ledger_error: None,
            buyer_consume_status: Some("pending_acceptance".to_string()),
            buyer_consume_entry_id: None,
            buyer_consume_balance_after: None,
            buyer_consume_error: None,
            created_at_epoch: now,
        };
        let work_order = WorldWorkOrder {
            work_order_id: league_hash_id(
                "world-work",
                &format!(
                    "{}:{}:{}:{}",
                    purchase.purchase_id, listing.listing_id, now, work_order_nonce
                ),
            ),
            purchase_id: purchase.purchase_id.clone(),
            listing_id: listing.listing_id.clone(),
            buyer_matrix_user_id: buyer_matrix_user_id.clone(),
            seller_matrix_user_id: listing.owner_matrix_user_id.clone(),
            company_id: listing.company_id.clone(),
            status: "open".to_string(),
            brief: brief.clone(),
            value_score: price_credits + listing.quality_score.max(0),
            created_at_epoch: now,
        };
        league.world.world_purchases.push(purchase.clone());
        league.world.world_work_orders.push(work_order.clone());
        let company = company_index.map(|index| league.world.world_companies[index].clone());
        let shop = shop_index.map(|index| league.world.world_shops[index].clone());
        (
            league.clone(),
            purchase,
            work_order,
            listing,
            company,
            shop,
            market_simulation,
            reputation_delta,
            faction_id.to_string(),
        )
    };
    let buyer_reserve = reserve_world_purchase_with_ledger(&state, &payload, &snapshot.1).await;
    let local_dev_ledger_bypass =
        matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
            && buyer_reserve.status.starts_with("skipped");
    let buyer_reserved = buyer_reserve.status == "reserved"
        || buyer_reserve.status == "duplicate"
        || local_dev_ledger_bypass;
    let settlement = if buyer_reserved {
        settle_world_purchase_with_ledger(&state, &payload, &snapshot.1).await
    } else {
        LeagueLedgerSettlement {
            status: "skipped_buyer_reserve".to_string(),
            error: Some(
                "seller settlement skipped because buyer reserve did not complete".to_string(),
            ),
            ..Default::default()
        }
    };
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let mut purchase = snapshot.1.clone();
        let mut work_order = snapshot.2.clone();
        let mut company = snapshot.4.clone();
        let mut shop = snapshot.5.clone();
        let mut economy_event = None;
        let mut seller_standing = None;
        let mut buyer_standing = None;
        let released = settlement.status == "settled" || settlement.status == "duplicate";
        purchase.buyer_ledger_status = Some(buyer_reserve.status.clone());
        purchase.buyer_ledger_account_id = buyer_reserve.account_id.clone();
        purchase.buyer_ledger_entry_id = buyer_reserve.entry_id.clone();
        purchase.buyer_ledger_balance_after = buyer_reserve.balance_after;
        purchase.buyer_ledger_error = buyer_reserve.error.clone();
        purchase.status = if buyer_reserved {
            if released {
                "reserved".to_string()
            } else if settlement.status.starts_with("skipped") {
                "seller_settlement_pending".to_string()
            } else {
                "seller_settlement_failed".to_string()
            }
        } else if buyer_reserve.status.starts_with("skipped") {
            "payment_hold".to_string()
        } else {
            "buyer_reserve_failed".to_string()
        };
        purchase.ledger_status = Some(settlement.status.clone());
        purchase.ledger_account_id = settlement.account_id.clone();
        purchase.ledger_entry_id = settlement.entry_id.clone();
        purchase.ledger_balance_after = settlement.balance_after;
        purchase.ledger_error = settlement.error.clone();
        work_order.status = if released || local_dev_ledger_bypass {
            "open".to_string()
        } else if buyer_reserved {
            purchase.status.clone()
        } else {
            "payment_hold".to_string()
        };
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        let self_dealing_purchase = purchase.buyer_matrix_user_id == purchase.seller_matrix_user_id;
        if released && !self_dealing_purchase {
            if let Some(index) = indexes.shop_index(&purchase.shop_id) {
                league.world.world_shops[index].gross_merchandise_score += purchase.price_credits;
            }
            if let Some(index) = indexes.company_index(&purchase.company_id) {
                league.world.world_companies[index].revenue_score += purchase.price_credits;
                league.world.world_companies[index].reputation_score += snapshot.7;
                league.world.world_companies[index].level =
                    1 + (league.world.world_companies[index].revenue_score / 100).max(0);
            }
            let mut buyer = ensure_league_player(&mut league, &purchase.buyer_matrix_user_id, None);
            buyer.xp += (snapshot.3.quality_score / 10).max(1);
            buyer.reputation += 1;
            buyer.rating += 1;
            league
                .players_by_matrix_user
                .insert(purchase.buyer_matrix_user_id.clone(), buyer);
            let mut seller =
                ensure_league_player(&mut league, &purchase.seller_matrix_user_id, None);
            seller.xp += (snapshot.3.quality_score / 2).max(1);
            seller.reputation += snapshot.7;
            seller.rating += (snapshot.3.quality_score / 10).max(1);
            seller.earned_credits +=
                world_seller_net_credits_for_price(purchase.price_credits) as f64;
            league
                .players_by_matrix_user
                .insert(purchase.seller_matrix_user_id.clone(), seller);
            let purchase_event = WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!(
                        "{}:{}:{}",
                        purchase.buyer_matrix_user_id,
                        purchase.purchase_id,
                        purchase.created_at_epoch
                    ),
                ),
                matrix_user_id: purchase.seller_matrix_user_id.clone(),
                event_kind: "listing_purchase".to_string(),
                subject_id: purchase.purchase_id.clone(),
                credits_delta: world_seller_net_credits_for_price(purchase.price_credits),
                reputation_delta: snapshot.7,
                created_at_epoch: purchase.created_at_epoch,
            };
            let market_tax_event = WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-market-tax",
                    &format!(
                        "{}:{}:{}",
                        purchase.buyer_matrix_user_id,
                        purchase.purchase_id,
                        purchase.created_at_epoch
                    ),
                ),
                matrix_user_id: purchase.buyer_matrix_user_id.clone(),
                event_kind: "market_tax_sink".to_string(),
                subject_id: purchase.purchase_id.clone(),
                credits_delta: -snapshot
                    .6
                    .get("market_tax_credits")
                    .and_then(Value::as_i64)
                    .unwrap_or(1),
                reputation_delta: 0,
                created_at_epoch: purchase.created_at_epoch,
            };
            league.world.world_relationships.push(WorldRelationship {
                relationship_id: league_hash_id(
                    "world-rel",
                    &format!(
                        "{}:{}:{}",
                        purchase.buyer_matrix_user_id,
                        purchase.company_id,
                        purchase.created_at_epoch
                    ),
                ),
                from_id: purchase.buyer_matrix_user_id.clone(),
                to_id: purchase.company_id.clone(),
                relation_kind: "customer".to_string(),
                strength: snapshot.7,
                updated_at_epoch: purchase.created_at_epoch,
            });
            seller_standing = Some(upsert_world_faction_standing(
                &mut league,
                &purchase.seller_matrix_user_id,
                &snapshot.8,
                snapshot.7,
                purchase.created_at_epoch,
            ));
            buyer_standing = Some(upsert_world_faction_standing(
                &mut league,
                &purchase.buyer_matrix_user_id,
                &snapshot.8,
                1,
                purchase.created_at_epoch,
            ));
            league
                .world
                .world_economy_events
                .push(purchase_event.clone());
            league.world.world_economy_events.push(market_tax_event);
            company = indexes
                .company_index(&purchase.company_id)
                .map(|index| league.world.world_companies[index].clone());
            shop = indexes
                .shop_index(&purchase.shop_id)
                .map(|index| league.world.world_shops[index].clone());
            economy_event = Some(purchase_event);
        } else if released {
            let market_tax_event = WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-market-tax",
                    &format!(
                        "{}:{}:{}",
                        purchase.buyer_matrix_user_id,
                        purchase.purchase_id,
                        purchase.created_at_epoch
                    ),
                ),
                matrix_user_id: purchase.buyer_matrix_user_id.clone(),
                event_kind: "market_tax_sink".to_string(),
                subject_id: purchase.purchase_id.clone(),
                credits_delta: -snapshot
                    .6
                    .get("market_tax_credits")
                    .and_then(Value::as_i64)
                    .unwrap_or(1),
                reputation_delta: 0,
                created_at_epoch: purchase.created_at_epoch,
            };
            league.world.world_economy_events.push(market_tax_event);
        }
        (
            league.clone(),
            purchase,
            work_order,
            company,
            shop,
            economy_event,
            seller_standing,
            buyer_standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_buy").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_listing_purchase",
            "world": "trillionnium_world",
            "purchase": final_snapshot.1,
            "work_order": final_snapshot.2,
            "listing": snapshot.3,
            "company": final_snapshot.3,
            "shop": final_snapshot.4,
            "economy_event": final_snapshot.5,
            "seller_standing": final_snapshot.6,
            "buyer_standing": final_snapshot.7,
            "market_simulation": snapshot.6,
            "buyer_ledger_status": buyer_reserve.status,
            "buyer_ledger_entry_id": buyer_reserve.entry_id,
            "buyer_ledger_error": buyer_reserve.error,
            "ledger_status": settlement.status,
            "ledger_entry_id": settlement.entry_id,
            "ledger_error": settlement.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn buy_world_listing(
    Path(listing_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldListingBuyRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    buy_world_listing_inner(state, listing_id, payload).await
}

pub(super) async fn post_world_web_listing_buy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebListingBuyRequest>,
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
    let listing_id = payload
        .listing_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let request = WorldListingBuyRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        body: payload
            .body
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    };
    let response = buy_world_listing_inner(state, listing_id, request).await;
    if response.status().is_success() {
        Redirect::to("/world?purchase=created").into_response()
    } else {
        response
    }
}

pub(super) async fn deliver_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkDeliverRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let _room_id = payload.room_id.as_deref();
    let judgement =
        judge_league_submission_with_pipeline(&state, &body, "world_work_delivery").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_deliverable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].seller_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another seller", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        if !matches!(
            league.world.world_work_orders[work_index].status.as_str(),
            "open" | "delivery_review_hold"
        ) {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not deliverable", "status": league.world.world_work_orders[work_index].status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id, "purchase_id": work_order_seed.purchase_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        if !world_purchase_seller_settlement_active(&purchase_seed) {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world purchase seller settlement is not active",
                    "work_order_id": work_order_id,
                    "purchase_id": purchase_seed.purchase_id,
                    "purchase_status": purchase_seed.status,
                    "ledger_status": purchase_seed.ledger_status,
                })),
            )
                .into_response();
        }
        let faction_id = "faction-market-guild";
        let delivery_status = if judgement.payout_status == "eligible" {
            "delivered"
        } else {
            "review_hold"
        };
        let delivery = WorldWorkDelivery {
            delivery_id: league_hash_id(
                "world-delivery",
                &format!(
                    "{}:{}:{}",
                    work_order_seed.work_order_id, matrix_user_id, now
                ),
            ),
            work_order_id: work_order_seed.work_order_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
            score: judgement.score,
            judge_status: judgement.judge_status.clone(),
            status: delivery_status.to_string(),
            created_at_epoch: now,
        };
        league.world.world_work_orders[work_index].status = if delivery_status == "delivered" {
            "delivered".to_string()
        } else {
            "delivery_review_hold".to_string()
        };
        let self_dealing_work =
            work_order_seed.buyer_matrix_user_id == work_order_seed.seller_matrix_user_id;
        let reputation_delta = if delivery_status == "delivered" && !self_dealing_work {
            (judgement.score / 10.0).round() as i64
        } else {
            0
        };
        if reputation_delta > 0 {
            if let Some(company_index) = indexes
                .company_index_by_id
                .get(&work_order_seed.company_id)
                .copied()
            {
                if let Some(company) = league.world.world_companies.get_mut(company_index) {
                    company.reputation_score += reputation_delta;
                }
            }
            let mut seller = ensure_league_player(&mut league, &matrix_user_id, None);
            seller.xp += judgement.score.round() as i64;
            seller.reputation += reputation_delta;
            seller.rating += ((judgement.score - 50.0) / 6.0).round() as i64;
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), seller);
        }
        let standing = if reputation_delta > 0 {
            Some(upsert_world_faction_standing(
                &mut league,
                &matrix_user_id,
                faction_id,
                reputation_delta,
                now,
            ))
        } else {
            None
        };
        let economy_event = if reputation_delta > 0 {
            Some(WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!(
                        "{}:{}:{}",
                        matrix_user_id, work_order_seed.work_order_id, now
                    ),
                ),
                matrix_user_id: matrix_user_id.clone(),
                event_kind: "work_delivered".to_string(),
                subject_id: work_order_seed.work_order_id.clone(),
                credits_delta: 0,
                reputation_delta,
                created_at_epoch: now,
            })
        } else {
            None
        };
        league.world.world_work_deliveries.push(delivery.clone());
        if let Some(economy_event) = economy_event.clone() {
            league.world.world_economy_events.push(economy_event);
        }
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            delivery,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_work_deliver").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_delivery",
            "world": "trillionnium_world",
            "work_order": snapshot.1,
            "delivery": snapshot.2,
            "economy_event": snapshot.3,
            "standing": snapshot.4,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn deliver_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkDeliverRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    deliver_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_deliver(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkDeliverRequest>,
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
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = deliver_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkDeliverRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Work delivery package: deliverable, evidence, acceptance checklist, risk review, next action, and self-review.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=delivered").into_response()
    } else {
        response
    }
}

pub(super) async fn accept_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkAcceptRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_acceptable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].buyer_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another buyer", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        if league.world.world_work_orders[work_index].status != "delivered" {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not ready for acceptance", "status": league.world.world_work_orders[work_index].status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        if !world_purchase_seller_settlement_active(&purchase_seed) {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world purchase seller settlement is not active",
                    "work_order_id": work_order_id,
                    "purchase_id": purchase_seed.purchase_id,
                    "purchase_status": purchase_seed.status,
                    "ledger_status": purchase_seed.ledger_status,
                })),
            )
                .into_response();
        }
        let reputation_delta = (work_order_seed.value_score / 20).max(1);
        let acceptance = WorldWorkAcceptance {
            acceptance_id: league_hash_id(
                "world-acceptance",
                &format!(
                    "{}:{}:{}",
                    work_order_seed.work_order_id, matrix_user_id, now
                ),
            ),
            work_order_id: work_order_seed.work_order_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body,
            status: "pending_consume".to_string(),
            reputation_delta,
            created_at_epoch: now,
        };
        league.world.world_work_orders[work_index].status = "accepted_pending_payment".to_string();
        league.world.world_work_acceptances.push(acceptance.clone());
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            purchase_seed,
            acceptance,
            reputation_delta,
        )
    };
    let buyer_consume =
        consume_world_purchase_with_ledger(&state, payload.room_id.as_deref(), &snapshot.2).await;
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let mut work_order = snapshot.1.clone();
        let mut purchase = snapshot.2.clone();
        let mut acceptance = snapshot.3.clone();
        let mut economy_event = None;
        let mut standing = None;
        let buyer_consumed =
            buyer_consume.status == "consumed" || buyer_consume.status == "duplicate";
        purchase.buyer_consume_status = Some(buyer_consume.status.clone());
        purchase.buyer_consume_entry_id = buyer_consume.entry_id.clone();
        purchase.buyer_consume_balance_after = buyer_consume.balance_after;
        purchase.buyer_consume_error = buyer_consume.error.clone();
        purchase.status = if buyer_consumed {
            "completed".to_string()
        } else if buyer_consume.status.starts_with("skipped") {
            "accepted_payment_hold".to_string()
        } else {
            "accepted_payment_failed".to_string()
        };
        work_order.status = if buyer_consumed {
            "completed".to_string()
        } else if buyer_consume.status.starts_with("skipped") {
            "accepted_payment_hold".to_string()
        } else {
            "accepted_payment_failed".to_string()
        };
        acceptance.status = if buyer_consumed {
            "accepted".to_string()
        } else if buyer_consume.status.starts_with("skipped") {
            "accepted_payment_hold".to_string()
        } else {
            "accepted_payment_failed".to_string()
        };
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        indexes.replace_acceptance_by_id(&mut league.world, &acceptance);
        let self_dealing_work = work_order.buyer_matrix_user_id == work_order.seller_matrix_user_id;
        if buyer_consumed && !self_dealing_work {
            if let Some(company_index) = indexes
                .company_index_by_id
                .get(&work_order.company_id)
                .copied()
            {
                if let Some(company) = league.world.world_companies.get_mut(company_index) {
                    company.reputation_score += snapshot.4;
                    company.level = 1 + (company.revenue_score / 100).max(0);
                }
            }
            let mut buyer =
                ensure_league_player(&mut league, &work_order.buyer_matrix_user_id, None);
            buyer.xp += 3;
            buyer.reputation += 1;
            league
                .players_by_matrix_user
                .insert(work_order.buyer_matrix_user_id.clone(), buyer);
            let mut seller =
                ensure_league_player(&mut league, &work_order.seller_matrix_user_id, None);
            seller.xp += snapshot.4;
            seller.reputation += snapshot.4;
            seller.rating += (snapshot.4 / 2).max(1);
            league
                .players_by_matrix_user
                .insert(work_order.seller_matrix_user_id.clone(), seller);
            let accepted_event = WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!(
                        "{}:{}:{}",
                        work_order.buyer_matrix_user_id,
                        acceptance.acceptance_id,
                        acceptance.created_at_epoch
                    ),
                ),
                matrix_user_id: work_order.seller_matrix_user_id.clone(),
                event_kind: "work_accepted".to_string(),
                subject_id: work_order.work_order_id.clone(),
                credits_delta: 0,
                reputation_delta: snapshot.4,
                created_at_epoch: acceptance.created_at_epoch,
            };
            standing = Some(upsert_world_faction_standing(
                &mut league,
                &work_order.seller_matrix_user_id,
                "faction-market-guild",
                snapshot.4,
                acceptance.created_at_epoch,
            ));
            league
                .world
                .world_economy_events
                .push(accepted_event.clone());
            economy_event = Some(accepted_event);
        }
        (
            league.clone(),
            work_order,
            purchase,
            acceptance,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_work_accept").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_acceptance",
            "world": "trillionnium_world",
            "work_order": final_snapshot.1,
            "purchase": final_snapshot.2,
            "acceptance": final_snapshot.3,
            "economy_event": final_snapshot.4,
            "standing": final_snapshot.5,
            "buyer_consume_status": buyer_consume.status,
            "buyer_consume_entry_id": buyer_consume.entry_id,
            "buyer_consume_error": buyer_consume.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn accept_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkAcceptRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    accept_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_accept(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkAcceptRequest>,
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
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = accept_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkAcceptRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Buyer acceptance: confirm customer deliverable, evidence package, quality note, risk controls, next collaboration, reputation confirmation, and self-review.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=accepted").into_response()
    } else {
        response
    }
}

pub(super) async fn reject_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkRejectRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_rejectable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].buyer_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another buyer", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        if !matches!(
            league.world.world_work_orders[work_index].status.as_str(),
            "delivered"
                | "delivery_review_hold"
                | "rejected_refund_hold"
                | "rejected_refund_failed"
                | "rejected_chargeback_failed"
        ) {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not rejectable", "status": league.world.world_work_orders[work_index].status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        let retry_chargeback_only = work_order_seed.status == "rejected_chargeback_failed"
            && matches!(
                purchase_seed.buyer_consume_status.as_deref(),
                Some("refunded")
            );
        let retry_status = if retry_chargeback_only {
            "pending_chargeback"
        } else {
            "pending_refund"
        };
        let rejection = if matches!(
            work_order_seed.status.as_str(),
            "rejected_refund_hold" | "rejected_refund_failed" | "rejected_chargeback_failed"
        ) {
            match league
                .world
                .world_work_rejections
                .iter()
                .rev()
                .find(|rejection| rejection.work_order_id == work_order_seed.work_order_id)
                .cloned()
            {
                Some(mut rejection) => {
                    rejection.body = body;
                    rejection.status = retry_status.to_string();
                    rejection
                }
                None => WorldWorkRejection {
                    rejection_id: league_hash_id(
                        "world-rejection",
                        &format!(
                            "{}:{}:{}",
                            work_order_seed.work_order_id, matrix_user_id, now
                        ),
                    ),
                    work_order_id: work_order_seed.work_order_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    body,
                    status: retry_status.to_string(),
                    refund_status: "pending".to_string(),
                    created_at_epoch: now,
                },
            }
        } else {
            WorldWorkRejection {
                rejection_id: league_hash_id(
                    "world-rejection",
                    &format!(
                        "{}:{}:{}",
                        work_order_seed.work_order_id, matrix_user_id, now
                    ),
                ),
                work_order_id: work_order_seed.work_order_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                body,
                status: retry_status.to_string(),
                refund_status: "pending".to_string(),
                created_at_epoch: now,
            }
        };
        league.world.world_work_orders[work_index].status = if retry_chargeback_only {
            "rejected_pending_chargeback".to_string()
        } else {
            "rejected_pending_refund".to_string()
        };
        if indexes
            .rejection_index_by_id
            .contains_key(&rejection.rejection_id)
        {
            indexes.replace_rejection_by_id(&mut league.world, &rejection);
        } else {
            league.world.world_work_rejections.push(rejection.clone());
        }
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            purchase_seed,
            rejection,
            retry_chargeback_only,
        )
    };
    let buyer_refund = if snapshot.4 {
        LeagueLedgerSettlement {
            status: "refunded".to_string(),
            account_id: snapshot.2.buyer_ledger_account_id.clone(),
            entry_id: snapshot.2.buyer_consume_entry_id.clone(),
            balance_after: snapshot.2.buyer_consume_balance_after,
            error: None,
        }
    } else {
        refund_world_purchase_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3.rejection_id,
        )
        .await
    };
    let buyer_refunded = buyer_refund.status == "refunded" || buyer_refund.status == "duplicate";
    let seller_chargeback = if buyer_refunded {
        chargeback_world_purchase_seller_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3.rejection_id,
        )
        .await
    } else {
        LeagueLedgerSettlement {
            status: "skipped_buyer_not_refunded".to_string(),
            error: Some(
                "seller chargeback skipped because buyer refund did not complete".to_string(),
            ),
            ..Default::default()
        }
    };
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let mut work_order = snapshot.1.clone();
        let mut purchase = snapshot.2.clone();
        let mut rejection = snapshot.3.clone();
        let mut economy_event = None;
        let mut standing = None;
        let seller_charged_back = buyer_refunded
            && matches!(
                seller_chargeback.status.as_str(),
                "seller_chargeback_consumed" | "duplicate"
            );
        let seller_chargeback_cleared = buyer_refunded
            && (seller_charged_back || seller_chargeback.status.starts_with("skipped"));
        purchase.buyer_consume_status = Some(if buyer_refunded {
            "refunded".to_string()
        } else {
            buyer_refund.status.clone()
        });
        purchase.buyer_consume_entry_id = buyer_refund.entry_id.clone();
        purchase.buyer_consume_balance_after = buyer_refund.balance_after;
        purchase.buyer_consume_error = buyer_refund.error.clone();
        purchase.status = if buyer_refunded {
            if seller_chargeback_cleared {
                "rejected_refunded".to_string()
            } else {
                "rejected_chargeback_failed".to_string()
            }
        } else if buyer_refund.status.starts_with("skipped") {
            "rejected_refund_hold".to_string()
        } else {
            "rejected_refund_failed".to_string()
        };
        if buyer_refunded {
            purchase.ledger_status = Some(if seller_charged_back {
                "seller_chargeback_consumed".to_string()
            } else if seller_chargeback.status.starts_with("skipped") {
                seller_chargeback.status.clone()
            } else {
                "seller_chargeback_failed".to_string()
            });
            purchase.ledger_entry_id = seller_chargeback
                .entry_id
                .clone()
                .or_else(|| purchase.ledger_entry_id.clone());
            purchase.ledger_balance_after = seller_chargeback
                .balance_after
                .or(purchase.ledger_balance_after);
            purchase.ledger_error = seller_chargeback.error.clone();
            if seller_chargeback_cleared {
                let rejected_event = WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-econ",
                        &format!(
                            "{}:{}:{}",
                            rejection.matrix_user_id,
                            rejection.rejection_id,
                            rejection.created_at_epoch
                        ),
                    ),
                    matrix_user_id: rejection.matrix_user_id.clone(),
                    event_kind: "work_rejected".to_string(),
                    subject_id: work_order.work_order_id.clone(),
                    credits_delta: -purchase.price_credits,
                    reputation_delta: 0,
                    created_at_epoch: rejection.created_at_epoch,
                };
                standing = Some(upsert_world_faction_standing(
                    &mut league,
                    &rejection.matrix_user_id,
                    "faction-market-guild",
                    1,
                    rejection.created_at_epoch,
                ));
                league
                    .world
                    .world_economy_events
                    .push(rejected_event.clone());
                economy_event = Some(rejected_event);
            }
            if seller_charged_back {
                let seller_net_credits = world_seller_net_credits_for_price(purchase.price_credits);
                if let Some(player) = league
                    .players_by_matrix_user
                    .get_mut(&purchase.seller_matrix_user_id)
                {
                    player.earned_credits =
                        (player.earned_credits - seller_net_credits as f64).max(0.0);
                }
                league.world.world_economy_events.push(WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-seller-chargeback",
                        &format!(
                            "{}:{}:{}",
                            purchase.seller_matrix_user_id,
                            rejection.rejection_id,
                            Utc::now().timestamp()
                        ),
                    ),
                    matrix_user_id: purchase.seller_matrix_user_id.clone(),
                    event_kind: "seller_chargeback".to_string(),
                    subject_id: purchase.purchase_id.clone(),
                    credits_delta: -seller_net_credits,
                    reputation_delta: 0,
                    created_at_epoch: Utc::now().timestamp(),
                });
            }
        }
        work_order.status = purchase.status.clone();
        rejection.refund_status = buyer_refund.status.clone();
        rejection.status = purchase.status.clone();
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        indexes.replace_rejection_by_id(&mut league.world, &rejection);
        (
            league.clone(),
            work_order,
            purchase,
            rejection,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_work_reject").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_rejection",
            "world": "trillionnium_world",
            "work_order": final_snapshot.1,
            "purchase": final_snapshot.2,
            "rejection": final_snapshot.3,
            "economy_event": final_snapshot.4,
            "standing": final_snapshot.5,
            "buyer_refund_status": buyer_refund.status,
            "buyer_refund_entry_id": buyer_refund.entry_id,
            "buyer_refund_error": buyer_refund.error,
            "seller_chargeback_status": seller_chargeback.status,
            "seller_chargeback_entry_id": seller_chargeback.entry_id,
            "seller_chargeback_error": seller_chargeback.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn reject_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkRejectRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    reject_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_reject(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkRejectRequest>,
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
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = reject_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkRejectRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Buyer rejection: delivery is not accepted; record customer deliverable gap, evidence package, refund risk controls, revision requirements, next action, and self-review.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=rejected").into_response()
    } else {
        response
    }
}

pub(super) async fn reopen_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkReopenRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_reopenable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].buyer_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another buyer", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        if !matches!(
            league.world.world_work_orders[work_index].status.as_str(),
            "rejected_refunded" | "rejected_refund_hold" | "rejected_refund_failed"
        ) {
            let status = league.world.world_work_orders[work_index].status.clone();
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not reopenable", "status": status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        if !world_purchase_rejection_settlement_released(&purchase_seed) {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world work rejection settlement is not complete",
                    "work_order_id": work_order_id,
                    "purchase_id": purchase_seed.purchase_id,
                    "purchase_status": purchase_seed.status,
                    "buyer_refund_status": purchase_seed.buyer_consume_status,
                    "seller_chargeback_status": purchase_seed.ledger_status,
                })),
            )
                .into_response();
        }
        let reopen = WorldWorkReopen {
            reopen_id: league_hash_id(
                "world-reopen",
                &format!(
                    "{}:{}:{}:{}",
                    work_order_seed.work_order_id, matrix_user_id, now, body
                ),
            ),
            work_order_id: work_order_seed.work_order_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body,
            status: "pending_reopen_reserve".to_string(),
            reserve_status: "pending".to_string(),
            created_at_epoch: now,
        };
        league.world.world_work_orders[work_index].status = "reopen_pending_reserve".to_string();
        league.world.world_work_reopens.push(reopen.clone());
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            purchase_seed,
            reopen,
        )
    };
    let buyer_reopen_reserve = reserve_reopened_world_purchase_with_ledger(
        &state,
        payload.room_id.as_deref(),
        &snapshot.2,
        &snapshot.3,
    )
    .await;
    let buyer_reserved =
        buyer_reopen_reserve.status == "reserved" || buyer_reopen_reserve.status == "duplicate";
    let seller_reopen_settlement = if buyer_reserved {
        settle_reopened_world_purchase_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3,
        )
        .await
    } else {
        LeagueLedgerSettlement {
            status: "skipped_buyer_reopen_reserve".to_string(),
            error: Some(
                "seller reopen settlement skipped because buyer reserve did not complete"
                    .to_string(),
            ),
            ..Default::default()
        }
    };
    let seller_resettled = matches!(
        seller_reopen_settlement.status.as_str(),
        "reopened_settled" | "duplicate"
    );
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let mut work_order = snapshot.1.clone();
        let mut purchase = snapshot.2.clone();
        let mut reopen = snapshot.3.clone();
        let mut economy_event = None;
        let mut standing = None;
        purchase.buyer_ledger_status = Some(if buyer_reserved {
            "reopened_reserved".to_string()
        } else {
            buyer_reopen_reserve.status.clone()
        });
        purchase.buyer_ledger_entry_id = buyer_reopen_reserve.entry_id.clone();
        purchase.buyer_ledger_balance_after = buyer_reopen_reserve.balance_after;
        purchase.buyer_ledger_error = buyer_reopen_reserve.error.clone();
        if buyer_reserved {
            purchase.ledger_status = Some(seller_reopen_settlement.status.clone());
            purchase.ledger_account_id = seller_reopen_settlement.account_id.clone();
            purchase.ledger_entry_id = seller_reopen_settlement.entry_id.clone();
            purchase.ledger_balance_after = seller_reopen_settlement.balance_after;
            purchase.ledger_error = seller_reopen_settlement.error.clone();
        }
        purchase.status = if buyer_reserved {
            if seller_resettled {
                "reopened_reserved".to_string()
            } else if seller_reopen_settlement.status.starts_with("skipped") {
                "reopen_seller_settlement_pending".to_string()
            } else {
                "reopen_seller_settlement_failed".to_string()
            }
        } else if buyer_reopen_reserve.status.starts_with("skipped") {
            "reopen_reserve_hold".to_string()
        } else {
            "reopen_reserve_failed".to_string()
        };
        work_order.status = if seller_resettled {
            "open".to_string()
        } else {
            purchase.status.clone()
        };
        reopen.reserve_status = buyer_reopen_reserve.status.clone();
        reopen.status = if buyer_reserved {
            if seller_resettled {
                "reopened".to_string()
            } else if seller_reopen_settlement.status.starts_with("skipped") {
                "reopen_seller_settlement_pending".to_string()
            } else {
                "reopen_seller_settlement_failed".to_string()
            }
        } else if buyer_reopen_reserve.status.starts_with("skipped") {
            "reopen_reserve_hold".to_string()
        } else {
            "reopen_reserve_failed".to_string()
        };
        if seller_resettled {
            let reopened_event = WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!(
                        "{}:{}:{}",
                        reopen.matrix_user_id, reopen.reopen_id, reopen.created_at_epoch
                    ),
                ),
                matrix_user_id: reopen.matrix_user_id.clone(),
                event_kind: "work_reopened".to_string(),
                subject_id: work_order.work_order_id.clone(),
                credits_delta: 0,
                reputation_delta: 1,
                created_at_epoch: reopen.created_at_epoch,
            };
            standing = Some(upsert_world_faction_standing(
                &mut league,
                &reopen.matrix_user_id,
                "faction-market-guild",
                1,
                reopen.created_at_epoch,
            ));
            league
                .world
                .world_economy_events
                .push(reopened_event.clone());
            economy_event = Some(reopened_event);
        }
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        indexes.replace_reopen_by_id(&mut league.world, &reopen);
        (
            league.clone(),
            work_order,
            purchase,
            reopen,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_work_reopen").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_reopen",
            "world": "trillionnium_world",
            "work_order": final_snapshot.1,
            "purchase": final_snapshot.2,
            "reopen": final_snapshot.3,
            "economy_event": final_snapshot.4,
            "standing": final_snapshot.5,
            "buyer_reopen_reserve_status": buyer_reopen_reserve.status,
            "buyer_reopen_reserve_entry_id": buyer_reopen_reserve.entry_id,
            "buyer_reopen_reserve_error": buyer_reopen_reserve.error,
            "seller_reopen_settlement_status": seller_reopen_settlement.status,
            "seller_reopen_settlement_entry_id": seller_reopen_settlement.entry_id,
            "seller_reopen_settlement_error": seller_reopen_settlement.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn reopen_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkReopenRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    reopen_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_reopen(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkReopenRequest>,
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
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = reopen_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkReopenRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Buyer reopen: reserve funds again, list customer deliverable revisions, evidence gaps, risk controls, acceptance standard, next redelivery action, and self-review.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=reopened").into_response()
    } else {
        response
    }
}

pub(super) async fn cancel_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkCancelRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_cancellable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].buyer_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another buyer", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        if !matches!(
            league.world.world_work_orders[work_index].status.as_str(),
            "open"
                | "payment_hold"
                | "seller_settlement_pending"
                | "seller_settlement_failed"
                | "reopen_reserve_hold"
                | "reopen_reserve_failed"
                | "reopen_seller_settlement_pending"
                | "reopen_seller_settlement_failed"
                | "cancelled_refund_hold"
                | "cancelled_refund_failed"
                | "cancelled_chargeback_failed"
        ) {
            let status = league.world.world_work_orders[work_index].status.clone();
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not cancellable before delivery", "status": status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        let retry_chargeback_only = work_order_seed.status == "cancelled_chargeback_failed"
            && matches!(
                purchase_seed.buyer_consume_status.as_deref(),
                Some("refunded")
            );
        let retry_status = if retry_chargeback_only {
            "pending_chargeback"
        } else {
            "pending_refund"
        };
        let cancellation = if matches!(
            work_order_seed.status.as_str(),
            "cancelled_refund_hold" | "cancelled_refund_failed" | "cancelled_chargeback_failed"
        ) {
            match league
                .world
                .world_work_cancellations
                .iter()
                .rev()
                .find(|cancellation| cancellation.work_order_id == work_order_seed.work_order_id)
                .cloned()
            {
                Some(mut cancellation) => {
                    cancellation.body = body;
                    cancellation.status = retry_status.to_string();
                    cancellation
                }
                None => WorldWorkCancellation {
                    cancellation_id: league_hash_id(
                        "world-cancel",
                        &format!(
                            "{}:{}:{}:{}",
                            work_order_seed.work_order_id, matrix_user_id, now, body
                        ),
                    ),
                    work_order_id: work_order_seed.work_order_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    body,
                    status: retry_status.to_string(),
                    refund_status: "pending".to_string(),
                    created_at_epoch: now,
                },
            }
        } else {
            WorldWorkCancellation {
                cancellation_id: league_hash_id(
                    "world-cancel",
                    &format!(
                        "{}:{}:{}:{}",
                        work_order_seed.work_order_id, matrix_user_id, now, body
                    ),
                ),
                work_order_id: work_order_seed.work_order_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                body,
                status: retry_status.to_string(),
                refund_status: "pending".to_string(),
                created_at_epoch: now,
            }
        };
        league.world.world_work_orders[work_index].status = if retry_chargeback_only {
            "cancel_pending_chargeback".to_string()
        } else {
            "cancel_pending_refund".to_string()
        };
        if indexes
            .cancellation_index_by_id
            .contains_key(&cancellation.cancellation_id)
        {
            indexes.replace_cancellation_by_id(&mut league.world, &cancellation);
        } else {
            league
                .world
                .world_work_cancellations
                .push(cancellation.clone());
        }
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            purchase_seed,
            cancellation,
            retry_chargeback_only,
        )
    };
    let buyer_cancel_refund = if snapshot.4 {
        LeagueLedgerSettlement {
            status: "refunded".to_string(),
            account_id: snapshot.2.buyer_ledger_account_id.clone(),
            entry_id: snapshot.2.buyer_consume_entry_id.clone(),
            balance_after: snapshot.2.buyer_consume_balance_after,
            error: None,
        }
    } else {
        refund_world_purchase_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3.cancellation_id,
        )
        .await
    };
    let buyer_refunded =
        buyer_cancel_refund.status == "refunded" || buyer_cancel_refund.status == "duplicate";
    let seller_chargeback = if buyer_refunded {
        chargeback_world_purchase_seller_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3.cancellation_id,
        )
        .await
    } else {
        LeagueLedgerSettlement {
            status: "skipped_buyer_not_refunded".to_string(),
            error: Some(
                "seller chargeback skipped because buyer refund did not complete".to_string(),
            ),
            ..Default::default()
        }
    };
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let mut work_order = snapshot.1.clone();
        let mut purchase = snapshot.2.clone();
        let mut cancellation = snapshot.3.clone();
        let mut economy_event = None;
        let mut standing = None;
        let seller_charged_back = buyer_refunded
            && matches!(
                seller_chargeback.status.as_str(),
                "seller_chargeback_consumed" | "duplicate"
            );
        let seller_chargeback_cleared = buyer_refunded
            && (seller_charged_back || seller_chargeback.status.starts_with("skipped"));
        purchase.buyer_consume_status = Some(if buyer_refunded {
            "refunded".to_string()
        } else {
            buyer_cancel_refund.status.clone()
        });
        purchase.buyer_consume_entry_id = buyer_cancel_refund.entry_id.clone();
        purchase.buyer_consume_balance_after = buyer_cancel_refund.balance_after;
        purchase.buyer_consume_error = buyer_cancel_refund.error.clone();
        purchase.status = if buyer_refunded {
            if seller_chargeback_cleared {
                "cancelled_refunded".to_string()
            } else {
                "cancelled_chargeback_failed".to_string()
            }
        } else if buyer_cancel_refund.status.starts_with("skipped") {
            "cancelled_refund_hold".to_string()
        } else {
            "cancelled_refund_failed".to_string()
        };
        if buyer_refunded {
            if seller_charged_back || !seller_chargeback.status.starts_with("skipped") {
                purchase.ledger_status = Some(if seller_charged_back {
                    "seller_chargeback_consumed".to_string()
                } else {
                    "seller_chargeback_failed".to_string()
                });
                purchase.ledger_entry_id = seller_chargeback
                    .entry_id
                    .clone()
                    .or_else(|| purchase.ledger_entry_id.clone());
                purchase.ledger_balance_after = seller_chargeback
                    .balance_after
                    .or(purchase.ledger_balance_after);
                purchase.ledger_error = seller_chargeback.error.clone();
            }
            if seller_chargeback_cleared {
                let cancelled_event = WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-econ",
                        &format!(
                            "{}:{}:{}",
                            cancellation.matrix_user_id,
                            cancellation.cancellation_id,
                            cancellation.created_at_epoch
                        ),
                    ),
                    matrix_user_id: cancellation.matrix_user_id.clone(),
                    event_kind: "work_cancelled".to_string(),
                    subject_id: work_order.work_order_id.clone(),
                    credits_delta: -purchase.price_credits,
                    reputation_delta: 0,
                    created_at_epoch: cancellation.created_at_epoch,
                };
                standing = Some(upsert_world_faction_standing(
                    &mut league,
                    &cancellation.matrix_user_id,
                    "faction-market-guild",
                    1,
                    cancellation.created_at_epoch,
                ));
                league
                    .world
                    .world_economy_events
                    .push(cancelled_event.clone());
                economy_event = Some(cancelled_event);
            }
            if seller_charged_back {
                let seller_net_credits = world_seller_net_credits_for_price(purchase.price_credits);
                if let Some(player) = league
                    .players_by_matrix_user
                    .get_mut(&purchase.seller_matrix_user_id)
                {
                    player.earned_credits =
                        (player.earned_credits - seller_net_credits as f64).max(0.0);
                }
                let now = Utc::now().timestamp();
                league.world.world_economy_events.push(WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-seller-chargeback",
                        &format!(
                            "{}:{}:{}",
                            purchase.seller_matrix_user_id, cancellation.cancellation_id, now
                        ),
                    ),
                    matrix_user_id: purchase.seller_matrix_user_id.clone(),
                    event_kind: "seller_chargeback".to_string(),
                    subject_id: purchase.purchase_id.clone(),
                    credits_delta: -seller_net_credits,
                    reputation_delta: 0,
                    created_at_epoch: now,
                });
            }
        }
        work_order.status = purchase.status.clone();
        cancellation.refund_status = buyer_cancel_refund.status.clone();
        cancellation.status = purchase.status.clone();
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        indexes.replace_cancellation_by_id(&mut league.world, &cancellation);
        (
            league.clone(),
            work_order,
            purchase,
            cancellation,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_work_cancel").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_cancellation",
            "world": "trillionnium_world",
            "work_order": final_snapshot.1,
            "purchase": final_snapshot.2,
            "cancellation": final_snapshot.3,
            "economy_event": final_snapshot.4,
            "standing": final_snapshot.5,
            "buyer_cancel_refund_status": buyer_cancel_refund.status,
            "buyer_cancel_refund_entry_id": buyer_cancel_refund.entry_id,
            "buyer_cancel_refund_error": buyer_cancel_refund.error,
            "seller_chargeback_status": seller_chargeback.status,
            "seller_chargeback_entry_id": seller_chargeback.entry_id,
            "seller_chargeback_error": seller_chargeback.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn cancel_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkCancelRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    cancel_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkCancelRequest>,
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
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = cancel_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkCancelRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Buyer cancel: record customer deliverable status, evidence package, refund risk controls, next action, and self-review before closing the work order.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=cancelled").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_contracts(
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
            "kind": "trillionnium_world_contracts",
            "world": "trillionnium_world",
            "contracts": league.world.world_contracts,
            "completions": league.world.world_contract_completions,
        })),
    )
        .into_response()
}

pub(super) async fn settle_world_contract_completion_with_ledger(
    state: &AppState,
    payload: &WorldContractCompleteRequest,
    matrix_user_id: &str,
    contract: &WorldContract,
    completion: &WorldContractCompletion,
) -> LeagueLedgerSettlement {
    if completion.reward_amount <= 0.0 {
        return LeagueLedgerSettlement {
            status: "skipped_zero_reward".to_string(),
            ..Default::default()
        };
    }
    if completion.payout_status != "eligible" || !completion.anti_cheat_flags.is_empty() {
        return LeagueLedgerSettlement {
            status: "held_review".to_string(),
            error: Some(format!(
                "world contract payout held: status={} flags={}",
                completion.payout_status,
                completion.anti_cheat_flags.join(",")
            )),
            ..Default::default()
        };
    }
    let Some(room_id) = payload
        .room_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_room".to_string(),
            ..Default::default()
        };
    };
    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.to_string(),
        room_id: room_id.to_string(),
        session_id: None,
        org_id: None,
        message: "world contract reward settlement".to_string(),
        capability_id: None,
        account_id: None,
        event_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let resolved_identity = match resolve_matrix_identity(state, &matrix_payload).await {
        Ok(identity) => identity,
        Err(_) => {
            return LeagueLedgerSettlement {
                status: "failed_identity".to_string(),
                error: Some(
                    "matrix identity could not be resolved for world contract settlement"
                        .to_string(),
                ),
                ..Default::default()
            }
        }
    };
    let Some(account_id) = resolved_identity.scope.account_id.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_account".to_string(),
            error: Some("matrix identity did not resolve a ledger account_id".to_string()),
            ..Default::default()
        };
    };
    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_ledger_token".to_string(),
            account_id: Some(account_id),
            error: Some("consumer-entry ledger admin token is not configured".to_string()),
            ..Default::default()
        };
    };
    let url = format!(
        "{}/v1/ledger/grant",
        state.config().ledger_base_url.trim_end_matches('/')
    );
    let body = json!({
        "account_id": account_id,
        "amount": completion.reward_amount,
        "idempotency_key": format!("world_contract_completion:{}", completion.completion_id),
        "reference_id": contract.task_id,
    });
    let response = match state
        .inner
        .http
        .post(url)
        .header("x-admin-token", ledger_admin_token)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_network".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("failed to reach ledger-service: {err}")),
                ..Default::default()
            }
        }
    };
    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_bad_response".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("ledger-service returned non-json response: {err}")),
                ..Default::default()
            }
        }
    };
    if !status.is_success() {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("ledger grant failed");
        return LeagueLedgerSettlement {
            status: if status.as_u16() == 409 {
                "duplicate".to_string()
            } else {
                "failed_ledger".to_string()
            },
            account_id: body
                .get("account_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            error: Some(format!("{}: {error}", status.as_u16())),
            ..Default::default()
        };
    }
    LeagueLedgerSettlement {
        status: "settled".to_string(),
        account_id: value
            .get("account")
            .and_then(|account| account.get("account_id"))
            .and_then(Value::as_str)
            .or_else(|| body.get("account_id").and_then(Value::as_str))
            .map(ToString::to_string),
        entry_id: value
            .get("entry")
            .and_then(|entry| entry.get("entry_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        balance_after: value
            .get("account")
            .and_then(|account| account.get("balance"))
            .and_then(Value::as_f64),
        error: None,
    }
}

pub(super) async fn complete_world_contract_inner(
    state: AppState,
    contract_id: String,
    payload: WorldContractCompleteRequest,
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
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let contract = {
        let league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(contract) = indexes.contract(&league.world, &contract_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world contract not found", "contract_id": contract_id })),
            )
                .into_response();
        };
        let released_completion_exists =
            league
                .world
                .world_contract_completions
                .iter()
                .any(|completion| {
                    completion.contract_id == contract.contract_id
                        && world_contract_completion_released(completion)
                });
        if released_completion_exists || world_contract_completion_final(&contract) {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world contract is already completed",
                    "contract_id": contract.contract_id,
                    "status": contract.status,
                    "cex_status": contract.cex_status,
                })),
            )
                .into_response();
        }
        contract
    };
    if contract.actor_matrix_user_id != matrix_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "world contract can only be completed by its creator",
                "contract_id": contract.contract_id,
            })),
        )
            .into_response();
    }
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_contract").await;
    let mut completion = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let completion_id = league_hash_id(
            "world-contract-completion",
            &format!("{}:{}:{}", contract.contract_id, now, body),
        );
        let completion = WorldContractCompletion {
            completion_id: completion_id.clone(),
            contract_id: contract.contract_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
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
        let released = judgement.payout_status == "eligible";
        if let Some(contract_index) = indexes.contract_index(&contract.contract_id) {
            let stored_contract = &mut league.world.world_contracts[contract_index];
            stored_contract.status = if released {
                "completed_pending_settlement".to_string()
            } else {
                "review_hold".to_string()
            };
            stored_contract.cex_status = Some(if released {
                "settlement_pending".to_string()
            } else {
                "review_hold".to_string()
            });
        }
        league
            .world
            .world_contract_completions
            .push(completion.clone());
        completion
    };
    let settlement = settle_world_contract_completion_with_ledger(
        &state,
        &payload,
        &matrix_user_id,
        &contract,
        &completion,
    )
    .await;
    completion.ledger_status = Some(settlement.status);
    completion.ledger_account_id = settlement.account_id;
    completion.ledger_entry_id = settlement.entry_id;
    completion.ledger_balance_after = settlement.balance_after;
    completion.ledger_error = settlement.error;
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let settlement_completed = matches!(
            completion.ledger_status.as_deref(),
            Some("settled") | Some("duplicate")
        );
        indexes.replace_contract_completion_by_id(&mut league.world, &completion);
        if settlement_completed {
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.earned_credits += completion.reward_amount;
            player.xp += completion.score.round() as i64;
            player.reputation += (completion.score / 8.0).round() as i64;
            player.rating += ((completion.score - 50.0) / 3.0).round() as i64;
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);

            let asset_delta = (completion.score / 5.0).round() as i64;
            if let Some(asset_index) = indexes.latest_asset_index_for_owner(&matrix_user_id) {
                let asset = &mut league.world.world_assets[asset_index];
                asset.value_score += asset_delta.max(1);
                asset.upgrade_points += asset_delta.max(1);
                asset.upgrade_level =
                    asset.upgrade_level.max(1) + (asset.upgrade_points / 60).max(0);
                asset.last_upgrade_kind = Some("contract_completion".to_string());
                asset.status = "upgraded_by_contract".to_string();
            } else {
                league.world.world_assets.push(WorldAsset {
                    asset_id: league_hash_id("world-asset", &completion.completion_id),
                    owner_matrix_user_id: matrix_user_id.clone(),
                    location_id: contract.location_id.clone(),
                    asset_kind: "contract_proof".to_string(),
                    name: "World Contract Proof".to_string(),
                    status: "active".to_string(),
                    value_score: asset_delta.max(1),
                    upgrade_level: 1,
                    upgrade_points: asset_delta.max(1),
                    last_upgrade_kind: Some("contract_completion".to_string()),
                    created_at_epoch: completion.created_at_epoch,
                });
            }
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
                let asset_delta = (completion.score / 5.0).round() as i64;
                stored_contract.value_score += asset_delta.max(1);
            }
            indexes.replace_contract_by_id(&mut league.world, &stored_contract);
        }
        league.clone()
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot, "world_contract_completion").await
    {
        return response;
    }
    let response_contract = {
        let indexes = build_world_indexes(&snapshot.world);
        indexes
            .contract(&snapshot.world, &contract.contract_id)
            .cloned()
    };
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_contract_completion",
            "world": "trillionnium_world",
            "contract": response_contract,
            "completion": completion,
        })),
    )
        .into_response()
}

pub(super) async fn complete_world_contract(
    Path(contract_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldContractCompleteRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    complete_world_contract_inner(state, contract_id, payload).await
}

pub(super) async fn post_world_web_contract_complete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebContractCompleteRequest>,
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
    let contract_id = match payload
        .contract_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    {
        Some(value) => value,
        None => {
            let league = state.inner.league_state.lock().await;
            let indexes = build_world_indexes(&league.world);
            match indexes
                .latest_contract_index_for_actor(&matrix_user_id)
                .and_then(|index| league.world.world_contracts.get(index))
                .map(|contract| contract.contract_id.clone())
            {
                Some(value) => value,
                None => return Redirect::to("/world?contract=missing").into_response(),
            }
        }
    };
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("World contract report: customer deliverable, evidence package, risk review, next step, rating standards, and self-review.")
        .to_string();
    let request = WorldContractCompleteRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        body,
    };
    let response = complete_world_contract_inner(state, contract_id, request).await;
    if response.status().is_success() {
        Redirect::to("/world?contract=completed").into_response()
    } else {
        response
    }
}

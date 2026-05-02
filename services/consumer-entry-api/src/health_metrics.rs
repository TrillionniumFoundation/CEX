use super::*;

const TRILLIONNIUM_WORLD_MATURITY_CONTRACT_VERSION: &str = "trillionnium_world_maturity_axes_v1";
const TRILLIONNIUM_WORLD_CLOSED_BETA_PROTOTYPE_CONTRACT_VERSION: &str =
    "trillionnium_world_closed_beta_prototype_v1";
const TRILLIONNIUM_WORLD_REAL_USER_BETA_CONTRACT_VERSION: &str =
    "trillionnium_world_real_user_beta_v1";
const TRILLIONNIUM_WORLD_PUBLIC_COMMERCIAL_PRODUCT_CONTRACT_VERSION: &str =
    "trillionnium_world_public_commercial_product_v1";

fn maturity_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn maturity_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn maturity_axis_json(
    axis_id: &str,
    label: &str,
    target: &str,
    checks: Vec<(&'static str, bool)>,
) -> Value {
    let total = checks.len();
    let passed = checks.iter().filter(|(_, passed)| *passed).count();
    let percent = if total == 0 {
        0
    } else {
        ((passed * 100) / total) as u64
    };
    let remaining_checks = checks
        .iter()
        .filter_map(|(check_id, passed)| (!*passed).then_some(*check_id))
        .collect::<Vec<_>>();
    let checks_json = checks
        .into_iter()
        .map(|(check_id, passed)| json!({ "check_id": check_id, "passed": passed }))
        .collect::<Vec<_>>();
    json!({
        "axis_id": axis_id,
        "label": label,
        "target": target,
        "percent": percent,
        "status": if percent == 100 { "converged" } else { "in_progress" },
        "passed_checks": passed,
        "total_checks": total,
        "remaining_checks": remaining_checks,
        "checks": checks_json,
    })
}

fn maturity_axis_percent(maturity: &Value, axis_id: &str) -> u64 {
    let axes = maturity.get("axes").unwrap_or(maturity);
    axes.get(axis_id)
        .and_then(|axis| axis.get("percent"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn all_maturity_axes_converged(maturity: &Value, axis_ids: &[&str]) -> bool {
    maturity.get("overall_percent").and_then(Value::as_u64) == Some(100)
        && maturity.get("overall_status").and_then(Value::as_str) == Some("converged")
        && axis_ids
            .iter()
            .all(|axis_id| maturity_axis_percent(maturity, axis_id) == 100)
}

fn mobile_shell_ux_contract_green(app: &Value) -> bool {
    let readiness_checks = app
        .get("mobile_shell_contract")
        .and_then(|contract| contract.get("readiness_checks"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    [
        "four_tab_mobile_shell_visible",
        "mobile_tablist_a11y_visible",
        "keyboard_tab_navigation_visible",
        "global_search_filters_active_tab",
        "search_empty_state_visible",
        "search_clear_and_escape_visible",
        "aria_live_ux_status_visible",
        "offline_feed_fallback_status_visible",
        "web_session_feed_hydration_visible",
        "feed_api_hydration_visible",
    ]
    .iter()
    .all(|expected| readiness_checks.iter().any(|check| check == expected))
}

fn first_maturity_matrix_user_id(league: &LeagueState) -> String {
    league
        .players_by_matrix_user
        .keys()
        .next()
        .cloned()
        .or_else(|| {
            league
                .world
                .world_player_positions
                .values()
                .next()
                .map(|position| position.matrix_user_id.clone())
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string())
}

fn trillionnium_world_maturity_axes_json(
    league: &LeagueState,
    config: &ConsumerEntryConfig,
    profile_ok: bool,
    league_repository_runtime: &Value,
) -> Value {
    let matrix_user_id = first_maturity_matrix_user_id(league);
    let app = client_app_json(league, matrix_user_id.as_str());
    let route_artifacts = build_world_route_artifacts(&league.world);
    let onboarding = app.get("onboarding").cloned().unwrap_or_else(|| json!({}));
    let onboarding_step_count = onboarding
        .get("steps")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let onboarding_acceptance_checks = onboarding
        .get("acceptance_checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let beta_readiness_checks = onboarding
        .get("beta_readiness_checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let app_module_count = app.get("module_count").and_then(Value::as_u64).unwrap_or(0);
    let mobile_shell_ux_green = mobile_shell_ux_contract_green(&app);
    let feed_item_count = app
        .get("feed")
        .and_then(|feed| feed.get("items"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let progression_level = app
        .get("progression")
        .and_then(|progression| progression.get("level"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let route_task_graph_count = route_artifacts.task_views.len();
    let world = &league.world;
    let settled_contract_completion_count = world
        .world_contract_completions
        .iter()
        .filter(|completion| {
            completion.ledger_status.as_deref() == Some("settled")
                || completion.payout_status == "settled"
        })
        .count();
    let reserved_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| purchase.buyer_ledger_status.as_deref() == Some("reserved"))
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| purchase.buyer_consume_status.as_deref() == Some("consumed"))
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| rejection.refund_status == "refunded")
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| cancellation.refund_status == "refunded")
        .count();
    let normalized_db_configured =
        maturity_bool(league_repository_runtime, "normalized_database_configured");
    let normalized_dual_write_active =
        maturity_bool(league_repository_runtime, "normalized_dual_write_active");
    let normalized_read_switch_active =
        maturity_bool(league_repository_runtime, "normalized_read_switch_active");
    let effective_repository_is_normalized =
        maturity_str(league_repository_runtime, "effective_repository")
            == Some("normalized_sql_direct_write_final");
    let source_of_truth_gate_green = maturity_str(
        league_repository_runtime,
        "normalized_read_switch_source_of_truth_gate",
    ) == Some(
        "latest_snapshot_requires_repository_audit_write_set_audit_and_normalized_world_home_and_client_feed_read_models",
    );
    let normalized_read_models_active = maturity_str(
        league_repository_runtime,
        "normalized_source_of_truth_read_models",
    ) == Some("world_home_and_client_feed");
    let direct_write_supported_commands = league_repository_runtime
        .get("normalized_direct_write_supported_commands")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    let first_playable = maturity_axis_json(
        "first_playable",
        "First playable / 可玩闭环",
        "地图焦点 → action → contract → commerce work → delivery/acceptance → reward/feed/next route",
        vec![
            ("map_nodes_visible", world.world_map_nodes.len() >= 12),
            ("player_position_visible", !world.world_player_positions.is_empty()),
            ("onboarding_contract_exposed", onboarding.get("contract_version").and_then(Value::as_str) == Some("trillionnium_first_playable_onboarding_v1")),
            ("onboarding_has_5_steps", onboarding_step_count >= 5),
            ("route_task_graph_next_action_visible", route_task_graph_count > 0 && onboarding_acceptance_checks.iter().any(|check| check == "route_task_graph_next_action_visible")),
            ("world_action_created", !world.world_events.is_empty()),
            ("contract_created", !world.world_contracts.is_empty()),
            ("contract_completion_settled", settled_contract_completion_count > 0),
            ("commerce_work_order_created", !world.world_work_orders.is_empty()),
            ("delivery_accepted_or_feedback_loop", !world.world_work_acceptances.is_empty() || !world.world_work_rejections.is_empty()),
            ("feed_surface_has_items", feed_item_count > 0),
            ("progression_reward_visible", progression_level >= 1),
        ],
    );

    let technical_alpha = maturity_axis_json(
        "technical_alpha",
        "Technical Alpha / 技术 Alpha",
        "normalized SQL dual-write/read-switch 在本地 live runtime 成为有效读写路径，并保留回滚审计",
        vec![
            ("profile_validation_ok", profile_ok),
            ("normalized_database_configured", normalized_db_configured),
            ("normalized_dual_write_active", normalized_dual_write_active),
            ("normalized_read_switch_active", normalized_read_switch_active),
            ("effective_repository_is_normalized", effective_repository_is_normalized),
            ("source_of_truth_gate_green", source_of_truth_gate_green),
            ("world_home_and_client_feed_read_models_active", normalized_read_models_active),
            ("sql_snapshot_path_configured", config.league_sql_snapshot_path.is_some()),
            ("json_state_rollback_path_configured", config.league_state_path.is_some()),
            ("direct_write_commands_cover_world_loop", direct_write_supported_commands >= 10),
        ],
    );

    let beta_readiness = maturity_axis_json(
        "beta_readiness",
        "Beta readiness / 面向真实用户 Beta",
        "Web/Matrix/app/onboarding/feed/commerce failure paths 都可由真实用户连续操作",
        vec![
            (
                "first_playable_axis_converged",
                maturity_axis_percent(
                    &json!({"axes":{"first_playable": first_playable.clone()}}),
                    "first_playable",
                ) == 100,
            ),
            ("app_mobile_modules_ready", app_module_count >= 4),
            (
                "matrix_app_card_onboarding_contract_declared",
                beta_readiness_checks
                    .iter()
                    .any(|check| check == "matrix_app_card_exposes_onboarding"),
            ),
            ("mobile_shell_ux_contract_green", mobile_shell_ux_green),
            ("feed_has_live_items", feed_item_count >= 5),
            (
                "commerce_accept_path_green",
                !world.world_work_acceptances.is_empty() && consumed_purchase_count > 0,
            ),
            (
                "commerce_reject_reopen_path_green",
                !world.world_work_rejections.is_empty()
                    && !world.world_work_reopens.is_empty()
                    && refunded_rejection_count > 0,
            ),
            (
                "commerce_cancel_path_green",
                !world.world_work_cancellations.is_empty() && refunded_cancellation_count > 0,
            ),
            ("progression_level_100", progression_level >= 100),
            ("social_contacts_visible", world.world_entities.len() >= 3),
            (
                "route_graph_has_actionable_tasks",
                route_task_graph_count >= 5,
            ),
        ],
    );

    let full_vision = maturity_axis_json(
        "full_vision",
        "Full vision / 现实镜像开放世界 + 经济 + 多人社交 + 可扩展后端",
        "现实地图、经济闭环、多人/社交入口、阵营关系、路线图和 normalized 后端同时在线",
        vec![
            (
                "real_world_map_dense_region",
                world.world_map_nodes.len() >= 12 && !world.world_player_positions.is_empty(),
            ),
            (
                "open_world_events_and_contracts",
                world.world_events.len() >= 3 && !world.world_contracts.is_empty(),
            ),
            (
                "economy_companies_shops_listings",
                !world.world_companies.is_empty()
                    && !world.world_shops.is_empty()
                    && !world.world_listings.is_empty(),
            ),
            (
                "market_purchases_and_reserves",
                !world.world_purchases.is_empty() && reserved_purchase_count > 0,
            ),
            (
                "work_delivery_acceptance_recovery_loops",
                !world.world_work_deliveries.is_empty()
                    && !world.world_work_acceptances.is_empty()
                    && !world.world_work_rejections.is_empty()
                    && !world.world_work_reopens.is_empty(),
            ),
            (
                "factions_and_standings",
                world.world_factions.len() >= 4 && !world.world_faction_standings.is_empty(),
            ),
            (
                "relationship_graph_active",
                !world.world_relationships.is_empty(),
            ),
            ("route_task_graph_dense", route_task_graph_count >= 10),
            (
                "client_app_feed_social_wallet_progression",
                app_module_count >= 4 && feed_item_count >= 5 && progression_level >= 100,
            ),
            (
                "normalized_scalable_backend_active",
                normalized_read_switch_active
                    && effective_repository_is_normalized
                    && normalized_read_models_active,
            ),
        ],
    );

    let axes = json!({
        "first_playable": first_playable,
        "technical_alpha": technical_alpha,
        "beta_readiness": beta_readiness,
        "full_vision": full_vision,
    });
    let percents = [
        maturity_axis_percent(&axes, "first_playable"),
        maturity_axis_percent(&axes, "technical_alpha"),
        maturity_axis_percent(&axes, "beta_readiness"),
        maturity_axis_percent(&axes, "full_vision"),
    ];
    let overall_percent = percents.iter().sum::<u64>() / percents.len() as u64;
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MATURITY_CONTRACT_VERSION,
        "target": "all_4_axes_100_percent",
        "overall_percent": overall_percent,
        "overall_status": if overall_percent == 100 { "converged" } else { "in_progress" },
        "matrix_user_id": matrix_user_id,
        "axis_order": ["first_playable", "technical_alpha", "beta_readiness", "full_vision"],
        "axes": axes,
    })
}

fn trillionnium_world_closed_beta_prototype_json(
    league: &LeagueState,
    config: &ConsumerEntryConfig,
    profile_ok: bool,
    identity_governance_valid: bool,
    session_auth_governance_valid: bool,
    league_repository_runtime: &Value,
    trillionnium_world_maturity: &Value,
) -> Value {
    let matrix_user_id = first_maturity_matrix_user_id(league);
    let app = client_app_json(league, matrix_user_id.as_str());
    let route_artifacts = build_world_route_artifacts(&league.world);
    let onboarding = app.get("onboarding").cloned().unwrap_or_else(|| json!({}));
    let onboarding_step_count = onboarding
        .get("steps")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let app_module_count = app.get("module_count").and_then(Value::as_u64).unwrap_or(0);
    let mobile_shell_ux_green = mobile_shell_ux_contract_green(&app);
    let feed_item_count = app
        .get("feed")
        .and_then(|feed| feed.get("items"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let progression_level = app
        .get("progression")
        .and_then(|progression| progression.get("level"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let social_contact_count = app
        .get("social")
        .map(|social| {
            social
                .get("contacts")
                .and_then(Value::as_array)
                .map(Vec::len)
                .or_else(|| {
                    social
                        .get("contact_count")
                        .and_then(Value::as_u64)
                        .map(|v| v as usize)
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let wallet_credit_balance = app
        .get("wallet")
        .and_then(|wallet| wallet.get("credit_balance"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let route_task_graph_count = route_artifacts.task_views.len();
    let route_preview_count = route_artifacts
        .preview
        .get("items")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let world = &league.world;

    let settled_contract_completion_count = world
        .world_contract_completions
        .iter()
        .filter(|completion| {
            completion.ledger_status.as_deref() == Some("settled")
                || completion.payout_status == "settled"
        })
        .count();
    let reserved_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| purchase.buyer_ledger_status.as_deref() == Some("reserved"))
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| purchase.buyer_consume_status.as_deref() == Some("consumed"))
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| rejection.refund_status == "refunded")
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| cancellation.refund_status == "refunded")
        .count();

    let normalized_db_configured =
        maturity_bool(league_repository_runtime, "normalized_database_configured");
    let normalized_dual_write_active =
        maturity_bool(league_repository_runtime, "normalized_dual_write_active");
    let normalized_read_switch_active =
        maturity_bool(league_repository_runtime, "normalized_read_switch_active");
    let effective_repository_is_normalized =
        maturity_str(league_repository_runtime, "effective_repository")
            == Some("normalized_sql_direct_write_final");
    let repository_cutover_active =
        maturity_str(league_repository_runtime, "repository_cutover_status")
            == Some("normalized_sql_direct_write_final_cutover_active");
    let normalized_read_models_active = maturity_str(
        league_repository_runtime,
        "normalized_source_of_truth_read_models",
    ) == Some("world_home_and_client_feed");
    let direct_write_supported_commands = league_repository_runtime
        .get("normalized_direct_write_supported_commands")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let direct_write_contract_phase = league_repository_runtime
        .get("normalized_direct_write_contract")
        .and_then(|contract| contract.get("phase"))
        .and_then(Value::as_str)
        == Some("direct_write_final_cutover");

    let session_auth_configured = config.require_session_auth
        && (!config
            .session_auth_secret
            .as_deref()
            .unwrap_or_default()
            .is_empty()
            || !config.session_auth_issuer_secrets.is_empty()
            || !config.session_auth_issuer_keys.is_empty()
            || !config.session_auth_issuer_registry.is_empty())
        && !config.session_auth_allowed_issuers.is_empty()
        && config.session_auth_expected_audience.is_some();
    let web_session_configured = league_web_session_secret(config).is_some()
        && !config.league_web_session_cookie_name.trim().is_empty()
        && config.league_web_session_ttl_secs > 0;

    let maturity_axes = [
        "first_playable",
        "technical_alpha",
        "beta_readiness",
        "full_vision",
    ];
    let maturity_all_axes_converged =
        all_maturity_axes_converged(trillionnium_world_maturity, &maturity_axes);

    let product_loop = maturity_axis_json(
        "product_loop",
        "Closed Beta product loop / 封闭 Beta 主循环",
        "首次用户能从 app/world 入口完成地图探索、行动、委托、交付、结算、feed 和下一步路线",
        vec![
            ("maturity_all_4_axes_converged", maturity_all_axes_converged),
            ("client_app_has_5_modules", app_module_count >= 5),
            ("mobile_shell_ux_contract_green", mobile_shell_ux_green),
            (
                "first_playable_onboarding_complete",
                onboarding_step_count >= 5,
            ),
            ("route_preview_dense", route_preview_count >= 20),
            ("route_task_graph_dense", route_task_graph_count >= 10),
            ("feed_surface_active", feed_item_count >= 5),
            ("progression_level_100", progression_level >= 100),
            ("social_contacts_ready", social_contact_count >= 3),
        ],
    );

    let access_governance = maturity_axis_json(
        "access_governance",
        "Closed Beta access and governance / 封闭访问与治理",
        "入口、防重放、限流、身份绑定、会话签名和 Web session gate 具备封闭 Beta 最小防线",
        vec![
            ("profile_validation_ok", profile_ok),
            ("ingress_token_present", config.ingress_token.is_some()),
            ("session_auth_configured", session_auth_configured),
            (
                "session_auth_governance_valid",
                session_auth_governance_valid,
            ),
            ("identity_governance_valid", identity_governance_valid),
            (
                "identity_binding_audit_log_configured",
                config.identity_binding_audit_log_path.is_some(),
            ),
            (
                "replay_store_configured",
                config.replay_store_path.is_some(),
            ),
            (
                "rate_limit_store_configured",
                config.rate_limit_store_path.is_some(),
            ),
            ("web_session_configured", web_session_configured),
        ],
    );

    let persistence_runtime = maturity_axis_json(
        "persistence_runtime",
        "Closed Beta persistence/runtime / 持久化与运行时",
        "normalized SQL read-switch、typed command writes、snapshot/rollback/audit 和 runtime read models 同时有效",
        vec![
            ("normalized_database_configured", normalized_db_configured),
            ("normalized_dual_write_active", normalized_dual_write_active),
            ("normalized_read_switch_active", normalized_read_switch_active),
            ("effective_repository_is_normalized", effective_repository_is_normalized),
            ("repository_cutover_active", repository_cutover_active),
            ("normalized_read_models_active", normalized_read_models_active),
            ("json_state_rollback_path_configured", config.league_state_path.is_some()),
            ("sql_snapshot_path_configured", config.league_sql_snapshot_path.is_some()),
            ("direct_write_commands_cover_world_loop", direct_write_supported_commands >= 12),
            ("direct_write_contract_declared", direct_write_contract_phase),
        ],
    );

    let world_depth = maturity_axis_json(
        "world_depth",
        "Closed Beta world depth / 世界内容厚度",
        "地图、区域、Agent/NPC、公司、店铺、合约、阵营、关系和经济事件足够支撑封闭 Beta 原型试玩",
        vec![
            (
                "zones_and_locations_ready",
                world.world_zones.len() >= 4 && world.world_locations.len() >= 4,
            ),
            ("map_nodes_dense", world.world_map_nodes.len() >= 12),
            (
                "agent_and_npc_residents_ready",
                world.world_entities.len() >= 3,
            ),
            ("world_events_seeded", world.world_events.len() >= 3),
            (
                "assets_and_upgrades_ready",
                !world.world_assets.is_empty() && !world.world_asset_upgrades.is_empty(),
            ),
            (
                "companies_shops_listings_ready",
                !world.world_companies.is_empty()
                    && !world.world_shops.is_empty()
                    && !world.world_listings.is_empty(),
            ),
            (
                "contracts_and_completions_ready",
                !world.world_contracts.is_empty() && settled_contract_completion_count > 0,
            ),
            (
                "factions_and_standings_ready",
                world.world_factions.len() >= 4 && !world.world_faction_standings.is_empty(),
            ),
            (
                "relationship_graph_ready",
                !world.world_relationships.is_empty(),
            ),
        ],
    );

    let commerce_recovery = maturity_axis_json(
        "commerce_recovery",
        "Closed Beta commerce and recovery / 交易与失败恢复",
        "购买、预留、交付、验收、拒收退款、返工、取消退款和钱包反馈路径全部可演示",
        vec![
            ("wallet_visible", wallet_credit_balance >= 0.0),
            (
                "purchase_reserved",
                !world.world_purchases.is_empty() && reserved_purchase_count > 0,
            ),
            ("work_order_created", !world.world_work_orders.is_empty()),
            (
                "work_delivery_recorded",
                !world.world_work_deliveries.is_empty(),
            ),
            (
                "work_acceptance_consumed",
                !world.world_work_acceptances.is_empty() && consumed_purchase_count > 0,
            ),
            (
                "work_rejection_refunded",
                !world.world_work_rejections.is_empty() && refunded_rejection_count > 0,
            ),
            (
                "work_reopen_rereserved",
                !world.world_work_reopens.is_empty(),
            ),
            (
                "work_cancellation_refunded",
                !world.world_work_cancellations.is_empty() && refunded_cancellation_count > 0,
            ),
            (
                "economy_events_recorded",
                !world.world_economy_events.is_empty(),
            ),
        ],
    );

    let axes = json!({
        "product_loop": product_loop,
        "access_governance": access_governance,
        "persistence_runtime": persistence_runtime,
        "world_depth": world_depth,
        "commerce_recovery": commerce_recovery,
    });
    let axis_order = [
        "product_loop",
        "access_governance",
        "persistence_runtime",
        "world_depth",
        "commerce_recovery",
    ];
    let overall_percent = axis_order
        .iter()
        .map(|axis_id| maturity_axis_percent(&axes, axis_id))
        .sum::<u64>()
        / axis_order.len() as u64;

    json!({
        "contract_version": TRILLIONNIUM_WORLD_CLOSED_BETA_PROTOTYPE_CONTRACT_VERSION,
        "target": "closed_beta_prototype_100_percent",
        "overall_percent": overall_percent,
        "overall_status": if overall_percent == 100 { "converged" } else { "in_progress" },
        "matrix_user_id": matrix_user_id,
        "axis_order": axis_order,
        "requires_live_gates": [
            "scripts/check-trillionnium-world-closed-beta-prototype.sh",
            "scripts/check-trillionnium-league-web-e2e.sh",
            "scripts/check-matrix-live-room-e2e.sh",
            "scripts/check-trillionnium-league-normalized-runtime-dual-write.sh",
            "scripts/check-trillionnium-league-sql-snapshot-db.sh",
            "cargo test --workspace"
        ],
        "axes": axes,
    })
}

fn trillionnium_world_real_user_beta_json(
    league: &LeagueState,
    config: &ConsumerEntryConfig,
    profile_ok: bool,
    identity_governance_valid: bool,
    session_auth_governance_valid: bool,
    league_repository_runtime: &Value,
    trillionnium_world_closed_beta_prototype: &Value,
) -> Value {
    let matrix_user_id = first_maturity_matrix_user_id(league);
    let app = client_app_json(league, matrix_user_id.as_str());
    let route_artifacts = build_world_route_artifacts(&league.world);
    let onboarding = app.get("onboarding").cloned().unwrap_or_else(|| json!({}));
    let onboarding_step_count = onboarding
        .get("steps")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let app_module_count = app.get("module_count").and_then(Value::as_u64).unwrap_or(0);
    let mobile_shell_ux_green = mobile_shell_ux_contract_green(&app);
    let feed_item_count = app
        .get("feed")
        .and_then(|feed| feed.get("items"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let progression_level = app
        .get("progression")
        .and_then(|progression| progression.get("level"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let social_contact_count = app
        .get("social")
        .map(|social| {
            social
                .get("contacts")
                .and_then(Value::as_array)
                .map(Vec::len)
                .or_else(|| {
                    social
                        .get("contact_count")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize)
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let route_task_graph_count = route_artifacts.task_views.len();
    let route_preview_count = route_artifacts
        .preview
        .get("items")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let world = &league.world;
    let settled_contract_completion_count = world
        .world_contract_completions
        .iter()
        .filter(|completion| {
            completion.ledger_status.as_deref() == Some("settled")
                || completion.payout_status == "settled"
        })
        .count();
    let reserved_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| purchase.buyer_ledger_status.as_deref() == Some("reserved"))
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| purchase.buyer_consume_status.as_deref() == Some("consumed"))
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| rejection.refund_status == "refunded")
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| cancellation.refund_status == "refunded")
        .count();
    let normalized_db_configured =
        maturity_bool(league_repository_runtime, "normalized_database_configured");
    let normalized_dual_write_active =
        maturity_bool(league_repository_runtime, "normalized_dual_write_active");
    let normalized_read_switch_active =
        maturity_bool(league_repository_runtime, "normalized_read_switch_active");
    let effective_repository_is_normalized =
        maturity_str(league_repository_runtime, "effective_repository")
            == Some("normalized_sql_direct_write_final");
    let repository_cutover_active =
        maturity_str(league_repository_runtime, "repository_cutover_status")
            == Some("normalized_sql_direct_write_final_cutover_active");
    let normalized_read_models_active = maturity_str(
        league_repository_runtime,
        "normalized_source_of_truth_read_models",
    ) == Some("world_home_and_client_feed");
    let direct_write_supported_commands = league_repository_runtime
        .get("normalized_direct_write_supported_commands")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let direct_write_contract_phase = league_repository_runtime
        .get("normalized_direct_write_contract")
        .and_then(|contract| contract.get("phase"))
        .and_then(Value::as_str)
        == Some("direct_write_final_cutover");
    let closed_beta_axes = [
        "product_loop",
        "access_governance",
        "persistence_runtime",
        "world_depth",
        "commerce_recovery",
    ];
    let closed_beta_all_axes_converged =
        all_maturity_axes_converged(trillionnium_world_closed_beta_prototype, &closed_beta_axes);
    let session_auth_configured = config.require_session_auth
        && (!config
            .session_auth_secret
            .as_deref()
            .unwrap_or_default()
            .is_empty()
            || !config.session_auth_issuer_secrets.is_empty()
            || !config.session_auth_issuer_keys.is_empty()
            || !config.session_auth_issuer_registry.is_empty())
        && !config.session_auth_allowed_issuers.is_empty()
        && config.session_auth_expected_audience.is_some();
    let web_session_required_and_configured = config.league_web_session_required
        && league_web_session_secret(config).is_some()
        && !config.league_web_session_cookie_name.trim().is_empty()
        && config.league_web_session_ttl_secs > 0
        && config.league_web_session_ttl_secs <= 30 * 24 * 60 * 60;
    let rate_limit_policy_sane = config.rate_limit_store_path.is_some()
        && config.rate_limit_window_secs > 0
        && config.rate_limit_max_requests > 0
        && config.rate_limit_user_max_requests > 0
        && config.rate_limit_room_max_requests > 0
        && config.rate_limit_session_max_requests > 0
        && config.rate_limit_org_max_requests > 0;
    let replay_policy_sane = config.replay_store_path.is_some()
        && config.replay_window_secs > 0
        && config.replay_cache_size >= 1024;

    let product_retention = maturity_axis_json(
        "product_retention",
        "Real-user Beta product retention / 长期使用产品闭环",
        "真实用户能反复从 app/world/feed/social/progression 进入并完成下一步任务",
        vec![
            (
                "closed_beta_prototype_converged",
                closed_beta_all_axes_converged,
            ),
            ("client_app_has_5_modules", app_module_count >= 5),
            ("mobile_shell_ux_contract_green", mobile_shell_ux_green),
            ("onboarding_has_5_steps", onboarding_step_count >= 5),
            ("route_preview_dense", route_preview_count >= 20),
            ("route_task_graph_dense", route_task_graph_count >= 10),
            ("feed_surface_has_live_items", feed_item_count >= 5),
            ("social_contacts_ready", social_contact_count >= 3),
            ("progression_level_100", progression_level >= 100),
        ],
    );

    let access_safety = maturity_axis_json(
        "access_safety",
        "Real-user Beta access safety / 真实用户访问安全",
        "非默认密钥、入口保护、身份绑定、会话签名、防重放、限流和 Web session 必须同时在线",
        vec![
            ("profile_validation_ok", profile_ok),
            (
                "gateway_api_key_non_default",
                config.cex_gateway_api_key.trim() != "local-dev-key",
            ),
            (
                "consumer_ingress_token_present",
                config.ingress_token.is_some(),
            ),
            ("session_auth_configured", session_auth_configured),
            (
                "session_auth_governance_valid",
                session_auth_governance_valid,
            ),
            ("identity_binding_required", config.require_identity_binding),
            ("identity_governance_valid", identity_governance_valid),
            (
                "identity_binding_audit_log_configured",
                config.identity_binding_audit_log_path.is_some(),
            ),
            (
                "web_session_required_and_configured",
                web_session_required_and_configured,
            ),
            ("replay_policy_sane", replay_policy_sane),
            ("rate_limit_policy_sane", rate_limit_policy_sane),
        ],
    );

    let durable_persistence = maturity_axis_json(
        "durable_persistence",
        "Real-user Beta durable persistence / 长期运行持久化",
        "normalized SQL read-switch、direct-write、read model、snapshot 和 rollback path 可长期承载用户状态",
        vec![
            ("normalized_database_configured", normalized_db_configured),
            ("normalized_dual_write_active", normalized_dual_write_active),
            ("normalized_read_switch_active", normalized_read_switch_active),
            ("effective_repository_is_normalized", effective_repository_is_normalized),
            ("repository_cutover_active", repository_cutover_active),
            ("normalized_read_models_active", normalized_read_models_active),
            ("direct_write_commands_cover_world_loop", direct_write_supported_commands >= 12),
            ("direct_write_contract_declared", direct_write_contract_phase),
            ("json_state_rollback_path_configured", config.league_state_path.is_some()),
            ("sql_snapshot_path_configured", config.league_sql_snapshot_path.is_some()),
        ],
    );

    let economy_recovery = maturity_axis_json(
        "economy_recovery",
        "Real-user Beta economy recovery / 经济与纠纷恢复",
        "购买、预留、交付、验收、拒收退款、返工、取消退款和经济事件足以支撑长期使用",
        vec![
            (
                "ledger_admin_token_configured",
                config.ledger_admin_token.is_some(),
            ),
            ("purchase_reserved", reserved_purchase_count > 0),
            (
                "work_delivery_recorded",
                !world.world_work_deliveries.is_empty(),
            ),
            ("work_acceptance_consumed", consumed_purchase_count > 0),
            ("work_rejection_refunded", refunded_rejection_count > 0),
            ("work_reopen_recorded", !world.world_work_reopens.is_empty()),
            (
                "work_cancellation_refunded",
                refunded_cancellation_count > 0,
            ),
            (
                "settled_contract_completion",
                settled_contract_completion_count > 0,
            ),
            (
                "economy_events_recorded",
                !world.world_economy_events.is_empty(),
            ),
        ],
    );

    let world_capacity = maturity_axis_json(
        "world_capacity",
        "Real-user Beta world capacity / 世界容量",
        "地图、区域、Agent/NPC、公司、店铺、合约、阵营、关系和任务路线具备持续内容容量",
        vec![
            (
                "zones_and_locations_ready",
                world.world_zones.len() >= 4 && world.world_locations.len() >= 4,
            ),
            ("map_nodes_dense", world.world_map_nodes.len() >= 12),
            (
                "agent_and_npc_residents_ready",
                world.world_entities.len() >= 3,
            ),
            ("world_events_seeded", world.world_events.len() >= 3),
            (
                "companies_shops_listings_ready",
                !world.world_companies.is_empty()
                    && !world.world_shops.is_empty()
                    && !world.world_listings.is_empty(),
            ),
            (
                "contracts_and_completions_ready",
                !world.world_contracts.is_empty() && settled_contract_completion_count > 0,
            ),
            (
                "factions_and_standings_ready",
                world.world_factions.len() >= 4 && !world.world_faction_standings.is_empty(),
            ),
            (
                "relationship_graph_ready",
                !world.world_relationships.is_empty(),
            ),
            (
                "route_graph_has_actionable_tasks",
                route_task_graph_count >= 10,
            ),
        ],
    );

    let ops_runtime = maturity_axis_json(
        "ops_runtime",
        "Real-user Beta ops runtime / 运维可持续性",
        "运行参数、审计、限流、防重放、文本边界和治理 gate 足以纳入长期 Beta 运维巡检",
        vec![
            (
                "max_text_chars_bounded",
                config.max_text_chars >= 1000 && config.max_text_chars <= 16000,
            ),
            (
                "rate_limit_window_configured",
                config.rate_limit_window_secs > 0,
            ),
            (
                "rate_limit_global_configured",
                config.rate_limit_max_requests > 0,
            ),
            (
                "rate_limit_user_configured",
                config.rate_limit_user_max_requests > 0,
            ),
            (
                "rate_limit_room_configured",
                config.rate_limit_room_max_requests > 0,
            ),
            (
                "rate_limit_session_configured",
                config.rate_limit_session_max_requests > 0,
            ),
            (
                "rate_limit_org_configured",
                config.rate_limit_org_max_requests > 0,
            ),
            ("replay_window_configured", config.replay_window_secs > 0),
            (
                "replay_cache_capacity_ready",
                config.replay_cache_size >= 1024,
            ),
            (
                "identity_reload_requires_revision",
                config.identity_binding_reload_require_revision,
            ),
            (
                "identity_reload_requires_approved_revision",
                config.identity_binding_reload_require_approved_revision,
            ),
            (
                "identity_reload_requires_actor",
                config.identity_binding_reload_require_actor,
            ),
            (
                "identity_reload_allowed_actor_present",
                !config.identity_binding_reload_allowed_actors.is_empty(),
            ),
        ],
    );

    let axes = json!({
        "product_retention": product_retention,
        "access_safety": access_safety,
        "durable_persistence": durable_persistence,
        "economy_recovery": economy_recovery,
        "world_capacity": world_capacity,
        "ops_runtime": ops_runtime,
    });
    let axis_order = [
        "product_retention",
        "access_safety",
        "durable_persistence",
        "economy_recovery",
        "world_capacity",
        "ops_runtime",
    ];
    let overall_percent = axis_order
        .iter()
        .map(|axis_id| maturity_axis_percent(&axes, axis_id))
        .sum::<u64>()
        / axis_order.len() as u64;

    json!({
        "contract_version": TRILLIONNIUM_WORLD_REAL_USER_BETA_CONTRACT_VERSION,
        "target": "real_user_long_term_beta_100_percent",
        "overall_percent": overall_percent,
        "overall_status": if overall_percent == 100 { "converged" } else { "in_progress" },
        "matrix_user_id": matrix_user_id,
        "axis_order": axis_order,
        "requires_live_gates": [
            "scripts/check-trillionnium-world-real-user-beta.sh",
            "scripts/check-trillionnium-world-closed-beta-prototype.sh",
            "scripts/check-trillionnium-league-web-e2e.sh",
            "scripts/check-matrix-live-room-e2e.sh",
            "scripts/check-trillionnium-league-normalized-runtime-dual-write.sh",
            "scripts/check-trillionnium-league-sql-snapshot-db.sh",
            "scripts/check-production-readiness.sh",
            "cargo test --workspace"
        ],
        "axes": axes,
    })
}

fn trillionnium_world_public_commercial_product_json(
    league: &LeagueState,
    config: &ConsumerEntryConfig,
    profile_ok: bool,
    identity_governance_valid: bool,
    session_auth_governance_valid: bool,
    league_repository_runtime: &Value,
    trillionnium_world_real_user_beta: &Value,
) -> Value {
    let matrix_user_id = first_maturity_matrix_user_id(league);
    let app = client_app_json(league, matrix_user_id.as_str());
    let route_artifacts = build_world_route_artifacts(&league.world);
    let world = &league.world;
    let real_user_axes = [
        "product_retention",
        "access_safety",
        "durable_persistence",
        "economy_recovery",
        "world_capacity",
        "ops_runtime",
    ];
    let real_user_beta_converged =
        all_maturity_axes_converged(trillionnium_world_real_user_beta, &real_user_axes);
    let app_module_count = app.get("module_count").and_then(Value::as_u64).unwrap_or(0);
    let mobile_shell_ux_green = mobile_shell_ux_contract_green(&app);
    let onboarding_step_count = app
        .get("onboarding")
        .and_then(|onboarding| onboarding.get("steps"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let feed_item_count = app
        .get("feed")
        .and_then(|feed| feed.get("items"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let social_contact_count = app
        .get("social")
        .map(|social| {
            social
                .get("contacts")
                .and_then(Value::as_array)
                .map(Vec::len)
                .or_else(|| {
                    social
                        .get("contact_count")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize)
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let route_preview_count = route_artifacts
        .preview
        .get("items")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let route_task_graph_count = route_artifacts.task_views.len();
    let active_company_count = world
        .world_companies
        .iter()
        .filter(|company| company.status == "active" || company.status == "operating")
        .count();
    let active_shop_count = world
        .world_shops
        .iter()
        .filter(|shop| shop.status == "active" || shop.status == "operating")
        .count();
    let active_listing_count = world
        .world_listings
        .iter()
        .filter(|listing| listing.status == "active" || listing.status == "operating")
        .count();
    let priced_listing_count = world
        .world_listings
        .iter()
        .filter(|listing| listing.price_credits > 0)
        .count();
    let total_company_revenue_score = world
        .world_companies
        .iter()
        .map(|company| company.revenue_score)
        .sum::<i64>();
    let total_shop_gmv_score = world
        .world_shops
        .iter()
        .map(|shop| shop.gross_merchandise_score)
        .sum::<i64>();
    let total_listing_quality_score = world
        .world_listings
        .iter()
        .map(|listing| listing.quality_score)
        .sum::<i64>();
    let reserved_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| purchase.buyer_ledger_status.as_deref() == Some("reserved"))
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| purchase.buyer_consume_status.as_deref() == Some("consumed"))
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| rejection.refund_status == "refunded")
        .count();
    let rereserved_reopen_count = world
        .world_work_reopens
        .iter()
        .filter(|reopen| reopen.reserve_status == "reserved")
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| cancellation.refund_status == "refunded")
        .count();
    let settled_contract_completion_count = world
        .world_contract_completions
        .iter()
        .filter(|completion| {
            completion.ledger_status.as_deref() == Some("settled")
                || completion.payout_status == "settled"
        })
        .count();
    let economy_event_credit_volume = world
        .world_economy_events
        .iter()
        .map(|event| event.credits_delta.abs())
        .sum::<i64>();
    let normalized_db_configured =
        maturity_bool(league_repository_runtime, "normalized_database_configured");
    let normalized_dual_write_active =
        maturity_bool(league_repository_runtime, "normalized_dual_write_active");
    let normalized_read_switch_active =
        maturity_bool(league_repository_runtime, "normalized_read_switch_active");
    let effective_repository_is_normalized =
        maturity_str(league_repository_runtime, "effective_repository")
            == Some("normalized_sql_direct_write_final");
    let repository_cutover_active =
        maturity_str(league_repository_runtime, "repository_cutover_status")
            == Some("normalized_sql_direct_write_final_cutover_active");
    let direct_write_supported_commands = league_repository_runtime
        .get("normalized_direct_write_supported_commands")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let direct_write_contract_phase = league_repository_runtime
        .get("normalized_direct_write_contract")
        .and_then(|contract| contract.get("phase"))
        .and_then(Value::as_str)
        == Some("direct_write_final_cutover");
    let normalized_read_models_active = maturity_str(
        league_repository_runtime,
        "normalized_source_of_truth_read_models",
    ) == Some("world_home_and_client_feed");
    let session_auth_configured = config.require_session_auth
        && (!config
            .session_auth_secret
            .as_deref()
            .unwrap_or_default()
            .is_empty()
            || !config.session_auth_issuer_secrets.is_empty()
            || !config.session_auth_issuer_keys.is_empty()
            || !config.session_auth_issuer_registry.is_empty())
        && !config.session_auth_allowed_issuers.is_empty()
        && config.session_auth_expected_audience.is_some();
    let web_session_required_and_configured = config.league_web_session_required
        && league_web_session_secret(config).is_some()
        && !config.league_web_session_cookie_name.trim().is_empty()
        && config.league_web_session_ttl_secs > 0
        && config.league_web_session_ttl_secs <= 30 * 24 * 60 * 60;
    let rate_limit_policy_sane = config.rate_limit_store_path.is_some()
        && config.rate_limit_window_secs > 0
        && config.rate_limit_max_requests > 0
        && config.rate_limit_user_max_requests > 0
        && config.rate_limit_room_max_requests > 0
        && config.rate_limit_session_max_requests > 0
        && config.rate_limit_org_max_requests > 0;
    let replay_policy_sane = config.replay_store_path.is_some()
        && config.replay_window_secs > 0
        && config.replay_cache_size >= 1024;

    let public_launch_surface = maturity_axis_json(
        "public_launch_surface",
        "Public product launch surface / 公开产品入口",
        "公开用户能从 Web/App/Matrix/世界地图/feed/social/onboarding 进入商业世界并找到下一步",
        vec![
            ("real_user_beta_converged", real_user_beta_converged),
            ("client_app_has_5_modules", app_module_count >= 5),
            ("mobile_shell_ux_contract_green", mobile_shell_ux_green),
            ("onboarding_has_5_steps", onboarding_step_count >= 5),
            ("feed_surface_has_live_items", feed_item_count >= 5),
            ("social_contacts_ready", social_contact_count >= 3),
            ("route_preview_dense", route_preview_count >= 20),
            ("route_task_graph_dense", route_task_graph_count >= 10),
            ("world_map_dense", world.world_map_nodes.len() >= 12),
            ("web_session_required", web_session_required_and_configured),
        ],
    );

    let commercial_engine = maturity_axis_json(
        "commercial_engine",
        "Public commercial engine / 商业化引擎",
        "公司、店铺、定价、购买、交付、验收、拒收、返工、取消、结算和经济事件形成完整商业闭环",
        vec![
            ("active_company_ready", active_company_count > 0),
            ("active_shop_ready", active_shop_count > 0),
            ("active_listing_ready", active_listing_count > 0),
            ("priced_listing_ready", priced_listing_count > 0),
            ("purchase_reserved", reserved_purchase_count > 0),
            (
                "work_delivery_recorded",
                !world.world_work_deliveries.is_empty(),
            ),
            ("work_acceptance_consumed", consumed_purchase_count > 0),
            ("work_rejection_refunded", refunded_rejection_count > 0),
            ("work_reopen_rereserved", rereserved_reopen_count > 0),
            (
                "work_cancellation_refunded",
                refunded_cancellation_count > 0,
            ),
            (
                "settled_contract_completion",
                settled_contract_completion_count > 0,
            ),
            (
                "economy_event_credit_volume",
                economy_event_credit_volume > 0,
            ),
            (
                "company_revenue_score_positive",
                total_company_revenue_score > 0,
            ),
            ("shop_gmv_score_positive", total_shop_gmv_score > 0),
            (
                "listing_quality_score_positive",
                total_listing_quality_score > 0,
            ),
        ],
    );

    let trust_safety = maturity_axis_json(
        "trust_safety",
        "Public commercial trust and safety / 公开商业信任安全",
        "公开商业化必须启用非默认密钥、入口保护、会话签名、身份绑定、治理审批、审计、防重放和限流",
        vec![
            ("profile_validation_ok", profile_ok),
            (
                "gateway_api_key_non_default",
                config.cex_gateway_api_key.trim() != "local-dev-key",
            ),
            (
                "consumer_ingress_token_present",
                config.ingress_token.is_some(),
            ),
            ("session_auth_configured", session_auth_configured),
            (
                "session_auth_governance_valid",
                session_auth_governance_valid,
            ),
            ("identity_binding_required", config.require_identity_binding),
            ("identity_governance_valid", identity_governance_valid),
            (
                "identity_binding_audit_log_configured",
                config.identity_binding_audit_log_path.is_some(),
            ),
            (
                "web_session_required_and_configured",
                web_session_required_and_configured,
            ),
            ("replay_policy_sane", replay_policy_sane),
            ("rate_limit_policy_sane", rate_limit_policy_sane),
        ],
    );

    let durable_scale_ops = maturity_axis_json(
        "durable_scale_ops",
        "Public commercial durable scale ops / 商业化持久化与规模运维",
        "normalized SQL、read-switch、direct-write shadow、read-model、snapshot/rollback、限流和文本边界可支撑公开产品运行",
        vec![
            ("normalized_database_configured", normalized_db_configured),
            ("normalized_dual_write_active", normalized_dual_write_active),
            ("normalized_read_switch_active", normalized_read_switch_active),
            ("effective_repository_is_normalized", effective_repository_is_normalized),
            ("repository_cutover_active", repository_cutover_active),
            ("normalized_read_models_active", normalized_read_models_active),
            ("direct_write_commands_cover_world_loop", direct_write_supported_commands >= 12),
            ("direct_write_contract_declared", direct_write_contract_phase),
            ("json_state_rollback_path_configured", config.league_state_path.is_some()),
            ("sql_snapshot_path_configured", config.league_sql_snapshot_path.is_some()),
            ("max_text_chars_public_bounded", config.max_text_chars >= 1000 && config.max_text_chars <= 16000),
            ("rate_limit_policy_sane", rate_limit_policy_sane),
            ("replay_policy_sane", replay_policy_sane),
        ],
    );

    let growth_network = maturity_axis_json(
        "growth_network",
        "Public commercial growth network / 增长网络",
        "公开商业世界具备玩家、工会、团本、奖励、库存、技能、工具、皮肤、阵营、关系和持续内容网络",
        vec![
            (
                "player_population_ready",
                !league.players_by_matrix_user.is_empty(),
            ),
            ("guilds_ready", !league.guilds.is_empty()),
            (
                "guild_memberships_ready",
                !league.guild_memberships.is_empty(),
            ),
            (
                "raid_contributions_ready",
                !league.raid_contributions.is_empty(),
            ),
            ("raid_rosters_ready", !league.raid_rosters.is_empty()),
            (
                "battles_and_submissions_ready",
                !league.battles.is_empty() && !league.submissions.is_empty(),
            ),
            ("rewards_ready", !league.rewards.is_empty()),
            ("inventory_ready", !league.inventory_items.is_empty()),
            (
                "skills_tools_skins_ready",
                !league.league_skills.is_empty()
                    && !league.league_tools.is_empty()
                    && !league.league_skins.is_empty(),
            ),
            (
                "factions_ready",
                world.world_factions.len() >= 4 && !world.world_faction_standings.is_empty(),
            ),
            (
                "relationship_graph_ready",
                !world.world_relationships.is_empty(),
            ),
            ("world_events_ready", world.world_events.len() >= 3),
            (
                "route_graph_has_actionable_tasks",
                route_task_graph_count >= 10,
            ),
        ],
    );

    let public_world_depth = maturity_axis_json(
        "public_world_depth",
        "Public commercial world depth / 公开商业世界深度",
        "世界拥有可探索区域、地点、居民、资产升级、公司、店铺、合约、地图移动与多路径任务叙事",
        vec![
            (
                "zones_and_locations_ready",
                world.world_zones.len() >= 4 && world.world_locations.len() >= 4,
            ),
            ("map_nodes_dense", world.world_map_nodes.len() >= 12),
            ("entities_ready", world.world_entities.len() >= 3),
            ("assets_ready", !world.world_assets.is_empty()),
            (
                "asset_upgrades_ready",
                !world.world_asset_upgrades.is_empty(),
            ),
            ("companies_ready", !world.world_companies.is_empty()),
            ("shops_ready", !world.world_shops.is_empty()),
            ("contracts_ready", !world.world_contracts.is_empty()),
            (
                "contract_completions_ready",
                settled_contract_completion_count > 0,
            ),
            (
                "player_positions_ready",
                !world.world_player_positions.is_empty(),
            ),
            ("route_preview_dense", route_preview_count >= 20),
            ("route_task_graph_dense", route_task_graph_count >= 10),
        ],
    );

    let axes = json!({
        "public_launch_surface": public_launch_surface,
        "commercial_engine": commercial_engine,
        "trust_safety": trust_safety,
        "durable_scale_ops": durable_scale_ops,
        "growth_network": growth_network,
        "public_world_depth": public_world_depth,
    });
    let axis_order = [
        "public_launch_surface",
        "commercial_engine",
        "trust_safety",
        "durable_scale_ops",
        "growth_network",
        "public_world_depth",
    ];
    let overall_percent = axis_order
        .iter()
        .map(|axis_id| maturity_axis_percent(&axes, axis_id))
        .sum::<u64>()
        / axis_order.len() as u64;

    json!({
        "contract_version": TRILLIONNIUM_WORLD_PUBLIC_COMMERCIAL_PRODUCT_CONTRACT_VERSION,
        "target": "public_commercial_product_100_percent",
        "overall_percent": overall_percent,
        "overall_status": if overall_percent == 100 { "converged" } else { "in_progress" },
        "matrix_user_id": matrix_user_id,
        "axis_order": axis_order,
        "requires_live_gates": [
            "scripts/check-trillionnium-world-public-commercial-product.sh",
            "scripts/check-trillionnium-world-real-user-beta.sh",
            "scripts/check-trillionnium-world-closed-beta-prototype.sh",
            "scripts/check-trillionnium-league-web-e2e.sh",
            "scripts/check-matrix-live-room-e2e.sh",
            "scripts/check-trillionnium-league-normalized-runtime-dual-write.sh",
            "scripts/check-trillionnium-league-sql-snapshot-db.sh",
            "scripts/check-production-readiness.sh",
            "cargo test --workspace"
        ],
        "axes": axes,
    })
}

pub(super) async fn health(State(state): State<AppState>) -> Json<Value> {
    let store = state.inner.identity_binding_store.read().await;
    let audit = state.inner.identity_binding_audit_state.read().await;
    let rate_limits = state.inner.rate_limits.lock().await;
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let session_auth_registry_state = session_auth_issuer_registry_runtime_state(&state);
    let session_auth_registry_approval_state =
        load_session_auth_issuer_registry_revision_approval_state(state.config());
    let session_auth_registry_actor_checks =
        session_auth_issuer_registry_actor_checks_json(state.config());
    let session_auth_registry_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &session_auth_registry_state.metadata,
        &session_auth_registry_approval_state,
    );
    let session_auth_registry_governance_overview =
        session_auth_issuer_registry_governance_overview_json(
            state.config(),
            &session_auth_registry_state.metadata,
            &session_auth_registry_approval_state,
            5,
        );
    let profile_errors = state.config().profile_validation_errors();
    let profile_ok = profile_errors.is_empty();
    let identity_governance_overview =
        identity_governance_overview_json(state.config(), &store, &approval_state, &audit, 5);
    let identity_governance_valid = identity_governance_overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let league_repository_runtime = league_repository_runtime_json(state.config());
    let session_auth_registry_governance_valid = session_auth_registry_governance_overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (
        trillionnium_world_maturity,
        trillionnium_world_closed_beta_prototype,
        trillionnium_world_real_user_beta,
        trillionnium_world_public_commercial_product,
    ) = {
        let league = state.inner.league_state.lock().await;
        let maturity = trillionnium_world_maturity_axes_json(
            &league,
            state.config(),
            profile_ok,
            &league_repository_runtime,
        );
        let closed_beta = trillionnium_world_closed_beta_prototype_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &maturity,
        );
        let real_user_beta = trillionnium_world_real_user_beta_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &closed_beta,
        );
        let public_commercial_product = trillionnium_world_public_commercial_product_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &real_user_beta,
        );
        (
            maturity,
            closed_beta,
            real_user_beta,
            public_commercial_product,
        )
    };
    Json(json!({
        "status": "ok",
        "service": "consumer-entry-api",
        "runtime_profile": state.config().runtime_profile.as_str(),
        "profile_validation": {
            "ok": profile_ok,
            "errors": profile_errors,
            "checks": {
                "ingress_token_present": state.config().ingress_token.is_some(),
                "require_session_auth": state.config().require_session_auth,
                "session_auth_secret_present": state.config().session_auth_secret.is_some(),
                "session_auth_issuer_secret_count": state.config().session_auth_issuer_secrets.len(),
                "session_auth_issuer_key_issuer_count": state.config().session_auth_issuer_keys.len(),
                "session_auth_issuer_key_count": state.config().session_auth_issuer_keys.values().map(|keys| keys.len()).sum::<usize>(),
                "session_auth_issuer_registry_configured": state.config().session_auth_issuer_registry_path.is_some(),
                "session_auth_issuer_registry_loaded": session_auth_registry_state.metadata.load_status == "loaded",
                "session_auth_issuer_registry_revision_present": session_auth_registry_state.metadata.revision.is_some(),
                "session_auth_issuer_registry_issuer_count": session_auth_registry_state.metadata.issuer_count,
                "session_auth_issuer_registry_key_count": session_auth_registry_state.metadata.key_count,
                "session_auth_issuer_registry_approved_revisions_configured": state.config().session_auth_issuer_registry_approved_revisions_path.is_some(),
                "session_auth_issuer_registry_require_approved_revision": state.config().session_auth_issuer_registry_require_approved_revision,
                "session_auth_issuer_registry_require_actor": state.config().session_auth_issuer_registry_require_actor,
                "session_auth_issuer_registry_actor_gate_valid": session_auth_registry_actor_checks.get("valid").and_then(Value::as_bool),
                "session_auth_issuer_registry_approval_loaded": session_auth_registry_approval_state.load_status == "loaded",
                "session_auth_issuer_registry_revision_approved": session_auth_registry_approval_checks.get("current_revision_approved").and_then(Value::as_bool),
                "session_auth_issuer_registry_governance_valid": session_auth_registry_governance_valid,
                "session_auth_allowed_issuer_count": state.config().session_auth_allowed_issuers.len(),
                "session_auth_expected_audience_configured": state.config().session_auth_expected_audience.is_some(),
                "replay_store_configured": state.config().replay_store_path.is_some(),
                "rate_limit_store_configured": state.config().rate_limit_store_path.is_some(),
                "identity_bindings_configured": state.config().identity_bindings_path.is_some(),
                "identity_registry_configured": state.config().identity_registry_path.is_some(),
                "require_identity_binding": state.config().require_identity_binding,
                "gateway_api_key_is_non_default": state.config().cex_gateway_api_key.trim() != "local-dev-key",
                "identity_binding_audit_log_configured": state.config().identity_binding_audit_log_path.is_some(),
                "identity_binding_approved_revisions_configured": state.config().identity_binding_approved_revisions_path.is_some(),
                "reload_requires_revision": state.config().identity_binding_reload_require_revision,
                "reload_requires_approved_revision": state.config().identity_binding_reload_require_approved_revision,
                "reload_requires_actor": state.config().identity_binding_reload_require_actor,
                "reload_allowed_actor_count": state.config().identity_binding_reload_allowed_actors.len(),
                "identity_registry_users": store.product_users.len(),
                "identity_registry_missing_refs": count_missing_product_user_refs(&store),
                "identity_governance_valid": identity_governance_valid,
                "league_normalized_database_configured": state.config().league_normalized_database_url.is_some(),
                "league_normalized_dual_write_enabled": state.config().league_normalized_dual_write_enabled,
                "league_normalized_read_switch_enabled": state.config().league_normalized_read_switch_enabled,
                "league_normalized_final_cutover_enabled": state.config().league_normalized_final_cutover_enabled,
                "league_normalized_dual_write_active": state.config().league_normalized_dual_write_enabled && state.config().league_normalized_database_url.is_some(),
                "league_normalized_read_switch_active": state.config().league_normalized_read_switch_enabled && state.config().league_normalized_database_url.is_some(),
                "league_normalized_final_cutover_active": state.config().league_normalized_final_cutover_enabled
                    && state.config().league_normalized_dual_write_enabled
                    && state.config().league_normalized_database_url.is_some(),
            }
        },
        "league_repository_runtime": league_repository_runtime,
        "trillionnium_world_public_commercial_product": trillionnium_world_public_commercial_product,
        "trillionnium_world_real_user_beta": trillionnium_world_real_user_beta,
        "trillionnium_world_closed_beta_prototype": trillionnium_world_closed_beta_prototype,
        "trillionnium_world_maturity": trillionnium_world_maturity,
        "cex_gateway_base_url": state.config().cex_gateway_base_url,
        "default_capability_id": state.config().default_capability_id,
        "ingress_protected": state.config().ingress_token.is_some(),
        "require_session_auth": state.config().require_session_auth,
        "session_auth": {
            "enabled": state.config().require_session_auth,
            "secret_present": state.config().session_auth_secret.is_some(),
            "issuer_secret_count": state.config().session_auth_issuer_secrets.len(),
            "issuer_secret_issuers": state.config().session_auth_issuer_secrets.keys().cloned().collect::<Vec<_>>(),
            "issuer_key_issuer_count": state.config().session_auth_issuer_keys.len(),
            "issuer_key_count": state.config().session_auth_issuer_keys.values().map(|keys| keys.len()).sum::<usize>(),
            "issuer_key_issuers": state.config().session_auth_issuer_keys.keys().cloned().collect::<Vec<_>>(),
            "issuer_registry_path": state.config().session_auth_issuer_registry_path,
            "issuer_registry_loaded": session_auth_registry_state.metadata.load_status == "loaded",
            "issuer_registry_load_error": session_auth_registry_state.metadata.load_error.clone(),
            "issuer_registry_issuer_count": session_auth_registry_state.metadata.issuer_count,
            "issuer_registry_key_count": session_auth_registry_state.metadata.key_count,
            "issuer_registry_issuers": session_auth_registry_state.registry.keys().cloned().collect::<Vec<_>>(),
            "issuer_registry_metadata": session_auth_registry_state.metadata.clone(),
            "issuer_registry_approved_revisions_path": state
                .config()
                .session_auth_issuer_registry_approved_revisions_path,
            "issuer_registry_approval_required": state
                .config()
                .session_auth_issuer_registry_require_approved_revision,
            "issuer_registry_actor_required": state
                .config()
                .session_auth_issuer_registry_require_actor,
            "issuer_registry_actor_checks": session_auth_registry_actor_checks,
            "issuer_registry_approval_state": session_auth_registry_approval_state,
            "issuer_registry_approval_checks": session_auth_registry_approval_checks,
            "allowed_issuers": state.config().session_auth_allowed_issuers,
            "allowed_issuer_count": state.config().session_auth_allowed_issuers.len(),
            "expected_audience": state.config().session_auth_expected_audience,
            "assertion_header": USER_SESSION_ASSERTION_HEADER,
            "signature_header": USER_SESSION_SIGNATURE_HEADER,
            "max_clock_skew_secs": state.config().session_auth_max_clock_skew_secs,
            "max_ttl_secs": state.config().session_auth_max_ttl_secs,
        },
        "identity_bindings_path": state.config().identity_bindings_path,
        "identity_bindings_enabled": state.config().identity_bindings_path.is_some(),
        "identity_registry_path": state.config().identity_registry_path,
        "identity_registry_enabled": state.config().identity_registry_path.is_some(),
        "require_identity_binding": state.config().require_identity_binding,
        "identity_binding_metadata": {
            "format": store.metadata.format.clone(),
            "version": store.metadata.version,
            "revision": store.metadata.revision.clone(),
            "source_path": store.metadata.source_path.clone(),
            "source_modified_epoch": store.metadata.source_modified_epoch,
            "loaded_at_epoch": store.metadata.loaded_at_epoch,
            "load_status": store.metadata.load_status.clone(),
            "load_error": store.metadata.load_error.clone(),
        },
        "identity_binding_counts": identity_binding_counts_json(&store),
        "identity_source_of_truth": {
            "mode": identity_source_of_truth_mode(&store),
            "product_users": store.product_users.len(),
            "missing_product_user_refs": count_missing_product_user_refs(&store),
        },
        "identity_registry_metadata": {
            "format": store.registry_metadata.format.clone(),
            "version": store.registry_metadata.version,
            "revision": store.registry_metadata.revision.clone(),
            "source_path": store.registry_metadata.source_path.clone(),
            "source_modified_epoch": store.registry_metadata.source_modified_epoch,
            "loaded_at_epoch": store.registry_metadata.loaded_at_epoch,
            "load_status": store.registry_metadata.load_status.clone(),
            "load_error": store.registry_metadata.load_error.clone(),
        },
        "identity_binding_audit": {
            "path": audit.path.clone(),
            "last_event_kind": audit.last_event_kind.clone(),
            "last_event_epoch": audit.last_event_epoch,
            "last_status": audit.last_status.clone(),
            "last_error": audit.last_error.clone(),
            "last_policy_decision": audit.last_policy_decision.clone(),
            "last_policy_reason": audit.last_policy_reason.clone(),
        },
        "identity_binding_reload_policy": {
            "require_revision": state.config().identity_binding_reload_require_revision,
            "reject_same_revision": state.config().identity_binding_reload_reject_same_revision,
            "allow_legacy_format": state.config().identity_binding_reload_allow_legacy_format,
            "require_approved_revision": state.config().identity_binding_reload_require_approved_revision,
            "allow_rollback": state.config().identity_binding_reload_allow_rollback,
            "require_actor": state.config().identity_binding_reload_require_actor,
            "actor_header": state.config().identity_binding_reload_actor_header,
            "allowed_actors_count": state.config().identity_binding_reload_allowed_actors.len(),
        },
        "identity_binding_revision_approval": {
            "source_path": approval_state.source_path,
            "source_modified_epoch": approval_state.source_modified_epoch,
            "loaded_at_epoch": approval_state.loaded_at_epoch,
            "load_status": approval_state.load_status,
            "load_error": approval_state.load_error,
            "version": approval_state.version,
            "revision": approval_state.revision,
            "approved_revisions": approval_state.approved_revisions,
        },
        "identity_governance_overview": identity_governance_overview,
        "session_auth_issuer_registry_governance_overview": session_auth_registry_governance_overview,
        "max_text_chars": state.config().max_text_chars,
        "rate_limit_window_secs": state.config().rate_limit_window_secs,
        "rate_limit_max_requests": state.config().rate_limit_max_requests,
        "rate_limit_user_max_requests": state.config().rate_limit_user_max_requests,
        "rate_limit_room_max_requests": state.config().rate_limit_room_max_requests,
        "rate_limit_session_max_requests": state.config().rate_limit_session_max_requests,
        "rate_limit_org_max_requests": state.config().rate_limit_org_max_requests,
        "rate_limit_store_path": state.config().rate_limit_store_path,
        "rate_limit_store_enabled": state.config().rate_limit_store_path.is_some(),
        "rate_limit_bucket_count": rate_limits.seen.len(),
        "replay_window_secs": state.config().replay_window_secs,
        "replay_cache_size": state.config().replay_cache_size,
        "replay_store_path": state.config().replay_store_path,
        "replay_store_enabled": state.config().replay_store_path.is_some(),
        "metrics": state.inner.metrics.snapshot(),
    }))
}

pub(super) async fn metrics(State(state): State<AppState>) -> Response {
    let profile_ok_bool = state.config().profile_validation_errors().is_empty();
    let profile_ok = if profile_ok_bool { 1 } else { 0 };
    let rate_limit_bucket_count = {
        let rate_limits = state.inner.rate_limits.lock().await;
        rate_limits.seen.len()
    };
    let identity_binding_store = state.inner.identity_binding_store.read().await;
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let governance_overview = identity_governance_overview_json(
        state.config(),
        &identity_binding_store,
        &approval_state,
        &audit,
        5,
    );
    let product_user_count = identity_binding_store.product_users.len();
    let product_user_ref_count =
        count_product_user_refs(&identity_binding_store.bindings.chat_users)
            + count_product_user_refs(&identity_binding_store.bindings.matrix_users);
    let missing_product_user_ref_count = count_missing_product_user_refs(&identity_binding_store);
    let governance_valid = governance_overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let governance_checks = governance_overview
        .get("checks")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let session_auth_registry_state = session_auth_issuer_registry_runtime_state(&state);
    let session_auth_registry_approval_state =
        load_session_auth_issuer_registry_revision_approval_state(state.config());
    let session_auth_registry_actor_checks =
        session_auth_issuer_registry_actor_checks_json(state.config());
    let session_auth_registry_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &session_auth_registry_state.metadata,
        &session_auth_registry_approval_state,
    );
    let session_auth_registry_governance_overview =
        session_auth_issuer_registry_governance_overview_json(
            state.config(),
            &session_auth_registry_state.metadata,
            &session_auth_registry_approval_state,
            5,
        );
    let session_auth_registry_governance_checks = session_auth_registry_governance_overview
        .get("checks")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let session_auth_registry_governance_valid = session_auth_registry_governance_overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let league_repository_runtime = league_repository_runtime_json(state.config());
    let (
        trillionnium_world_maturity,
        trillionnium_world_closed_beta_prototype,
        trillionnium_world_real_user_beta,
        trillionnium_world_public_commercial_product,
    ) = {
        let league = state.inner.league_state.lock().await;
        let maturity = trillionnium_world_maturity_axes_json(
            &league,
            state.config(),
            profile_ok_bool,
            &league_repository_runtime,
        );
        let closed_beta = trillionnium_world_closed_beta_prototype_json(
            &league,
            state.config(),
            profile_ok_bool,
            governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &maturity,
        );
        let real_user_beta = trillionnium_world_real_user_beta_json(
            &league,
            state.config(),
            profile_ok_bool,
            governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &closed_beta,
        );
        let public_commercial_product = trillionnium_world_public_commercial_product_json(
            &league,
            state.config(),
            profile_ok_bool,
            governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &real_user_beta,
        );
        (
            maturity,
            closed_beta,
            real_user_beta,
            public_commercial_product,
        )
    };
    let body = format!(
        concat!(
            "# TYPE cex_consumer_entry_task_create_requests_total counter\n",
            "cex_consumer_entry_task_create_requests_total {}\n",
            "# TYPE cex_consumer_entry_task_lookup_requests_total counter\n",
            "cex_consumer_entry_task_lookup_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_requests_total counter\n",
            "cex_consumer_entry_rate_limited_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_source_scope_requests_total counter\n",
            "cex_consumer_entry_rate_limited_source_scope_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_user_requests_total counter\n",
            "cex_consumer_entry_rate_limited_user_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_room_requests_total counter\n",
            "cex_consumer_entry_rate_limited_room_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_session_requests_total counter\n",
            "cex_consumer_entry_rate_limited_session_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_org_requests_total counter\n",
            "cex_consumer_entry_rate_limited_org_requests_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_failures_total counter\n",
            "cex_consumer_entry_identity_binding_failures_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_matches_total counter\n",
            "cex_consumer_entry_identity_binding_matches_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_reload_requests_total counter\n",
            "cex_consumer_entry_identity_binding_reload_requests_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_reload_successes_total counter\n",
            "cex_consumer_entry_identity_binding_reload_successes_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_reload_rejections_total counter\n",
            "cex_consumer_entry_identity_binding_reload_rejections_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_reload_actor_rejections_total counter\n",
            "cex_consumer_entry_identity_binding_reload_actor_rejections_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_audit_failures_total counter\n",
            "cex_consumer_entry_identity_binding_audit_failures_total {}\n",
            "# TYPE cex_consumer_entry_ingress_auth_failures_total counter\n",
            "cex_consumer_entry_ingress_auth_failures_total {}\n",
            "# TYPE cex_consumer_entry_session_auth_successes_total counter\n",
            "cex_consumer_entry_session_auth_successes_total {}\n",
            "# TYPE cex_consumer_entry_session_auth_failures_total counter\n",
            "cex_consumer_entry_session_auth_failures_total {}\n",
            "# TYPE cex_consumer_entry_replay_hits_total counter\n",
            "cex_consumer_entry_replay_hits_total {}\n",
            "# TYPE cex_consumer_entry_profile_validation_ok gauge\n",
            "cex_consumer_entry_profile_validation_ok {}\n",
            "# TYPE cex_consumer_entry_ingress_protected gauge\n",
            "cex_consumer_entry_ingress_protected {}\n",
            "# TYPE cex_consumer_entry_require_session_auth gauge\n",
            "cex_consumer_entry_require_session_auth {}\n",
            "# TYPE cex_consumer_entry_session_auth_global_secret_present gauge\n",
            "cex_consumer_entry_session_auth_global_secret_present {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_secrets gauge\n",
            "cex_consumer_entry_session_auth_issuer_secrets {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_key_issuers gauge\n",
            "cex_consumer_entry_session_auth_issuer_key_issuers {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_keys gauge\n",
            "cex_consumer_entry_session_auth_issuer_keys {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_configured gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_configured {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_loaded gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_loaded {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_revision_present gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_revision_present {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_issuers gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_issuers {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_keys gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_keys {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_configured gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_configured {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_required gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_required {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_actor_gate_valid gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_actor_gate_valid {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_loaded gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_loaded {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_revision_approved gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_revision_approved {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_governance_valid gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_governance_valid {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_source_valid gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_source_valid {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid {}\n",
            "# TYPE cex_consumer_entry_session_auth_allowed_issuers gauge\n",
            "cex_consumer_entry_session_auth_allowed_issuers {}\n",
            "# TYPE cex_consumer_entry_session_auth_expected_audience_configured gauge\n",
            "cex_consumer_entry_session_auth_expected_audience_configured {}\n",
            "# TYPE cex_consumer_entry_require_identity_binding gauge\n",
            "cex_consumer_entry_require_identity_binding {}\n",
            "# TYPE cex_consumer_entry_identity_registry_users gauge\n",
            "cex_consumer_entry_identity_registry_users {}\n",
            "# TYPE cex_consumer_entry_identity_registry_refs gauge\n",
            "cex_consumer_entry_identity_registry_refs {}\n",
            "# TYPE cex_consumer_entry_identity_registry_missing_refs gauge\n",
            "cex_consumer_entry_identity_registry_missing_refs {}\n",
            "# TYPE cex_consumer_entry_rate_limit_store_enabled gauge\n",
            "cex_consumer_entry_rate_limit_store_enabled {}\n",
            "# TYPE cex_consumer_entry_rate_limit_bucket_count gauge\n",
            "cex_consumer_entry_rate_limit_bucket_count {}\n",
            "# TYPE cex_consumer_entry_identity_governance_valid gauge\n",
            "cex_consumer_entry_identity_governance_valid {}\n",
            "# TYPE cex_consumer_entry_identity_binding_loaded gauge\n",
            "cex_consumer_entry_identity_binding_loaded {}\n",
            "# TYPE cex_consumer_entry_identity_registry_loaded gauge\n",
            "cex_consumer_entry_identity_registry_loaded {}\n",
            "# TYPE cex_consumer_entry_identity_ref_integrity_ok gauge\n",
            "cex_consumer_entry_identity_ref_integrity_ok {}\n",
            "# TYPE cex_consumer_entry_identity_actor_gate_valid gauge\n",
            "cex_consumer_entry_identity_actor_gate_valid {}\n",
            "# TYPE cex_consumer_entry_identity_approval_source_valid gauge\n",
            "cex_consumer_entry_identity_approval_source_valid {}\n",
            "# TYPE cex_consumer_entry_identity_approval_coverage_valid gauge\n",
            "cex_consumer_entry_identity_approval_coverage_valid {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_maturity_overall_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_maturity_overall_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_maturity_first_playable_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_maturity_first_playable_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_maturity_technical_alpha_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_maturity_technical_alpha_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_maturity_beta_readiness_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_maturity_beta_readiness_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_maturity_full_vision_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_maturity_full_vision_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_closed_beta_prototype_overall_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_closed_beta_prototype_overall_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_closed_beta_prototype_product_loop_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_closed_beta_prototype_product_loop_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_closed_beta_prototype_access_governance_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_closed_beta_prototype_access_governance_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_closed_beta_prototype_persistence_runtime_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_closed_beta_prototype_persistence_runtime_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_closed_beta_prototype_world_depth_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_closed_beta_prototype_world_depth_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_closed_beta_prototype_commerce_recovery_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_closed_beta_prototype_commerce_recovery_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_real_user_beta_overall_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_real_user_beta_overall_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_real_user_beta_product_retention_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_real_user_beta_product_retention_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_real_user_beta_access_safety_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_real_user_beta_access_safety_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_real_user_beta_durable_persistence_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_real_user_beta_durable_persistence_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_real_user_beta_economy_recovery_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_real_user_beta_economy_recovery_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_real_user_beta_world_capacity_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_real_user_beta_world_capacity_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_real_user_beta_ops_runtime_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_real_user_beta_ops_runtime_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_public_commercial_product_overall_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_public_commercial_product_overall_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_public_commercial_product_public_launch_surface_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_public_commercial_product_public_launch_surface_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_public_commercial_product_commercial_engine_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_public_commercial_product_commercial_engine_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_public_commercial_product_trust_safety_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_public_commercial_product_trust_safety_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_public_commercial_product_durable_scale_ops_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_public_commercial_product_durable_scale_ops_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_public_commercial_product_growth_network_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_public_commercial_product_growth_network_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_public_commercial_product_public_world_depth_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_public_commercial_product_public_world_depth_percent {}\n"
        ),
        state
            .inner
            .metrics
            .task_create_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .task_lookup_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_source_scope_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_user_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_room_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_session_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_org_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_matches
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_reload_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_reload_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_reload_rejections
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_reload_actor_rejections
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_audit_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .ingress_auth_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .session_auth_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .session_auth_failures
            .load(Ordering::Relaxed),
        state.inner.metrics.replay_hits.load(Ordering::Relaxed),
        profile_ok,
        if state.config().ingress_token.is_some() {
            1
        } else {
            0
        },
        if state.config().require_session_auth {
            1
        } else {
            0
        },
        if state.config().session_auth_secret.is_some() {
            1
        } else {
            0
        },
        state.config().session_auth_issuer_secrets.len(),
        state.config().session_auth_issuer_keys.len(),
        state.config().session_auth_issuer_keys.values().map(|keys| keys.len()).sum::<usize>(),
        if state.config().session_auth_issuer_registry_path.is_some() {
            1
        } else {
            0
        },
        if session_auth_registry_state.metadata.load_status == "loaded" {
            1
        } else {
            0
        },
        if session_auth_registry_state.metadata.revision.is_some() {
            1
        } else {
            0
        },
        session_auth_registry_state.metadata.issuer_count,
        session_auth_registry_state.metadata.key_count,
        if state
            .config()
            .session_auth_issuer_registry_approved_revisions_path
            .is_some()
        {
            1
        } else {
            0
        },
        if state
            .config()
            .session_auth_issuer_registry_require_approved_revision
        {
            1
        } else {
            0
        },
        if session_auth_registry_actor_checks
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if session_auth_registry_approval_state.load_status == "loaded" {
            1
        } else {
            0
        },
        if session_auth_registry_approval_checks
            .get("current_revision_approved")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if session_auth_registry_governance_valid { 1 } else { 0 },
        if session_auth_registry_governance_checks
            .get("approval_source_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if session_auth_registry_governance_checks
            .get("approval_coverage_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        state.config().session_auth_allowed_issuers.len(),
        if state.config().session_auth_expected_audience.is_some() {
            1
        } else {
            0
        },
        if state.config().require_identity_binding {
            1
        } else {
            0
        },
        product_user_count,
        product_user_ref_count,
        missing_product_user_ref_count,
        if state.config().rate_limit_store_path.is_some() {
            1
        } else {
            0
        },
        rate_limit_bucket_count,
        if governance_valid { 1 } else { 0 },
        if governance_checks
            .get("binding_loaded")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("registry_loaded")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("ref_integrity_ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("actor_gate_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("approval_source_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("approval_coverage_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        trillionnium_world_maturity
            .get("overall_percent")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        maturity_axis_percent(&trillionnium_world_maturity, "first_playable"),
        maturity_axis_percent(&trillionnium_world_maturity, "technical_alpha"),
        maturity_axis_percent(&trillionnium_world_maturity, "beta_readiness"),
        maturity_axis_percent(&trillionnium_world_maturity, "full_vision"),
        trillionnium_world_closed_beta_prototype
            .get("overall_percent")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        maturity_axis_percent(&trillionnium_world_closed_beta_prototype, "product_loop"),
        maturity_axis_percent(
            &trillionnium_world_closed_beta_prototype,
            "access_governance",
        ),
        maturity_axis_percent(
            &trillionnium_world_closed_beta_prototype,
            "persistence_runtime",
        ),
        maturity_axis_percent(&trillionnium_world_closed_beta_prototype, "world_depth"),
        maturity_axis_percent(
            &trillionnium_world_closed_beta_prototype,
            "commerce_recovery",
        ),
        trillionnium_world_real_user_beta
            .get("overall_percent")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        maturity_axis_percent(&trillionnium_world_real_user_beta, "product_retention"),
        maturity_axis_percent(&trillionnium_world_real_user_beta, "access_safety"),
        maturity_axis_percent(&trillionnium_world_real_user_beta, "durable_persistence"),
        maturity_axis_percent(&trillionnium_world_real_user_beta, "economy_recovery"),
        maturity_axis_percent(&trillionnium_world_real_user_beta, "world_capacity"),
        maturity_axis_percent(&trillionnium_world_real_user_beta, "ops_runtime"),
        trillionnium_world_public_commercial_product
            .get("overall_percent")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        maturity_axis_percent(
            &trillionnium_world_public_commercial_product,
            "public_launch_surface",
        ),
        maturity_axis_percent(
            &trillionnium_world_public_commercial_product,
            "commercial_engine",
        ),
        maturity_axis_percent(
            &trillionnium_world_public_commercial_product,
            "trust_safety",
        ),
        maturity_axis_percent(
            &trillionnium_world_public_commercial_product,
            "durable_scale_ops",
        ),
        maturity_axis_percent(
            &trillionnium_world_public_commercial_product,
            "growth_network",
        ),
        maturity_axis_percent(
            &trillionnium_world_public_commercial_product,
            "public_world_depth",
        ),
    );
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

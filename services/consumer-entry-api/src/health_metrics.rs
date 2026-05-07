use super::*;

const TRILLIONNIUM_WORLD_MATURITY_CONTRACT_VERSION: &str = "trillionnium_world_maturity_axes_v1";
const TRILLIONNIUM_WORLD_CLOSED_BETA_PROTOTYPE_CONTRACT_VERSION: &str =
    "trillionnium_world_closed_beta_prototype_v1";
const TRILLIONNIUM_WORLD_REAL_USER_BETA_CONTRACT_VERSION: &str =
    "trillionnium_world_real_user_beta_v1";
const TRILLIONNIUM_WORLD_PUBLIC_COMMERCIAL_PRODUCT_CONTRACT_VERSION: &str =
    "trillionnium_world_public_commercial_product_v1";
const TRILLIONNIUM_WORLD_PLAYABILITY_SCORECARD_CONTRACT_VERSION: &str =
    "trillionnium_world_playability_scorecard_v1";

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

fn playability_axis_json(
    axis_id: &str,
    label: &str,
    target: &str,
    checks: Vec<(&'static str, bool)>,
) -> Value {
    let mut axis = maturity_axis_json(axis_id, label, target, checks);
    let percent = axis.get("percent").and_then(Value::as_u64).unwrap_or(0);
    let score = ((percent as f64 / 10.0) * 10.0).round() / 10.0;
    if let Some(object) = axis.as_object_mut() {
        object.insert("score".to_string(), json!(score));
        object.insert("target_score".to_string(), json!(10.0));
        object.insert("score_label".to_string(), json!(format!("{score:.1}/10")));
        object.insert(
            "status".to_string(),
            json!(if percent == 100 {
                "converged"
            } else {
                "in_progress"
            }),
        );
    }
    axis
}

fn playability_axis_score(scorecard: &Value, axis_id: &str) -> f64 {
    let axes = scorecard.get("axes").unwrap_or(scorecard);
    axes.get(axis_id)
        .and_then(|axis| axis.get("score"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn playability_user_metric_score(scorecard: &Value, axis_id: &str) -> f64 {
    let axes = scorecard.get("user_metric_axes").unwrap_or(scorecard);
    axes.get(axis_id)
        .and_then(|axis| axis.get("score"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn all_playability_axes_converged(scorecard: &Value, axis_ids: &[&str]) -> bool {
    scorecard.get("overall_score").and_then(Value::as_f64) == Some(10.0)
        && scorecard.get("overall_status").and_then(Value::as_str) == Some("converged")
        && axis_ids
            .iter()
            .all(|axis_id| playability_axis_score(scorecard, axis_id) == 10.0)
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
    let mobile_shell_contract = app.get("mobile_shell_contract");
    let primary_cta = mobile_shell_contract.and_then(|contract| contract.get("primary_cta"));
    let copy_layering = mobile_shell_contract.and_then(|contract| contract.get("copy_layering"));
    let readiness_checks = mobile_shell_contract
        .and_then(|contract| contract.get("readiness_checks"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let primary_cta_green = primary_cta
        .and_then(|cta| cta.get("contract_version"))
        .and_then(Value::as_str)
        == Some("trillionnium_mobile_single_primary_cta_v1")
        && primary_cta
            .and_then(|cta| cta.get("single_primary_cta"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && primary_cta
            .and_then(|cta| cta.get("bottom_sheet_id"))
            .and_then(Value::as_str)
            == Some("app-mobile-action-sheet")
        && primary_cta
            .and_then(|cta| cta.get("primary_cta_id"))
            .and_then(Value::as_str)
            == Some("app-mobile-primary-cta")
        && primary_cta
            .and_then(|cta| cta.get("target_id"))
            .and_then(Value::as_str)
            == Some("app-map-action-rail");
    let copy_layering_green = copy_layering
        .and_then(|copy| copy.get("contract_version"))
        .and_then(Value::as_str)
        == Some("trillionnium_mobile_copy_layering_v1")
        && copy_layering
            .and_then(|copy| copy.get("summary_id"))
            .and_then(Value::as_str)
            == Some("app-map-copy-summary")
        && copy_layering
            .and_then(|copy| copy.get("details_id"))
            .and_then(Value::as_str)
            == Some("app-map-copy-layer-details")
        && copy_layering
            .and_then(|copy| copy.get("default_state"))
            .and_then(Value::as_str)
            == Some("collapsed");
    primary_cta_green
        && copy_layering_green
        && [
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
            "mobile_bottom_sheet_single_primary_cta_visible",
            "mobile_copy_layering_visible",
            "next_action_rail_visible",
            "playability_coach_visible",
            "p0_next_best_action_visible",
            "p1_strategy_choices_visible",
            "p2_retention_telemetry_visible",
            "failure_recovery_lane_visible",
            "economy_tradeoff_cards_visible",
            "retention_calendar_visible",
            "playability_funnel_visible",
            "anti_cheese_policy_visible",
            "ops_refresh_hooks_visible",
        ]
        .iter()
        .all(|expected| readiness_checks.iter().any(|check| check == expected))
}

fn route_runner_handoff_contract_ready(handoff: Option<&Value>) -> bool {
    handoff.is_some_and(|handoff| {
        handoff.get("contract_version").and_then(Value::as_str)
            == Some("trillionnium_route_runner_handoff_v1")
            && handoff
                .get("supports_route_runner_reward_claim_actions")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && handoff
                .get("supports_route_runner_next_route_actions")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && handoff
                .get("supports_checkpoint_reward_history")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && handoff
                .get("supports_route_runner_lifecycle")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && handoff
                .get("lifecycle_contract_version")
                .and_then(Value::as_str)
                == Some("trillionnium_route_runner_lifecycle_v1")
            && handoff
                .get("runner_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
            && handoff
                .get("reward_claim_action_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
            && handoff
                .get("next_route_action_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
            && handoff
                .get("first_task_id")
                .and_then(Value::as_str)
                .is_some_and(|task_id| !task_id.trim().is_empty())
            && handoff
                .get("first_progress_label")
                .and_then(Value::as_str)
                .is_some_and(|label| !label.trim().is_empty())
            && handoff
                .get("first_reward_claim_status")
                .and_then(Value::as_str)
                .is_some_and(|status| !status.trim().is_empty())
            && handoff
                .get("first_lifecycle_source")
                .and_then(Value::as_str)
                .is_some_and(|source| !source.trim().is_empty())
            && handoff
                .get("first_lifecycle_stage")
                .and_then(Value::as_str)
                .is_some_and(|stage| !stage.trim().is_empty())
            && handoff
                .get("first_lifecycle_status")
                .and_then(Value::as_str)
                .is_some_and(|status| !status.trim().is_empty())
            && handoff
                .get("first_next_route_status")
                .and_then(Value::as_str)
                .is_some_and(|status| !status.trim().is_empty())
            && handoff
                .get("first_next_route_action_body")
                .and_then(Value::as_str)
                .is_some_and(|body| !body.trim().is_empty())
            && handoff
                .get("first_next_route_sequence_summary")
                .and_then(Value::as_str)
                .is_some_and(|summary| !summary.trim().is_empty())
            && handoff
                .get("handoff_prompt")
                .and_then(Value::as_str)
                .is_some_and(|prompt| !prompt.trim().is_empty())
    })
}

fn app_route_runner_handoff_gate_json(app: &Value) -> Value {
    let feed = app.get("feed");
    let feed_source_count = feed
        .and_then(|feed| feed.get("source_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let feed_sources_include_route_runner_handoff = feed
        .and_then(|feed| feed.get("sources"))
        .and_then(Value::as_array)
        .is_some_and(|sources| {
            sources
                .iter()
                .any(|source| source == "route_runner_handoff")
        });
    let feed_route_runner_handoff = feed.and_then(|feed| feed.get("route_runner_handoff"));
    let map_hub_route_runner_handoff = app
        .get("map_hub")
        .and_then(|map_hub| map_hub.get("route_runner_handoff"));
    let feed_route_runner_handoff_green = feed_source_count >= 7
        && feed_sources_include_route_runner_handoff
        && route_runner_handoff_contract_ready(feed_route_runner_handoff);
    let map_hub_route_runner_handoff_green =
        route_runner_handoff_contract_ready(map_hub_route_runner_handoff);

    json!({
        "contract_version": "trillionnium_playability_route_runner_handoff_gate_v1",
        "feed_contract_visible": feed_route_runner_handoff_green,
        "map_hub_contract_visible": map_hub_route_runner_handoff_green,
        "source_count": feed_source_count,
        "sources_include_route_runner_handoff": feed_sources_include_route_runner_handoff,
        "feed_handoff_contract_version": feed_route_runner_handoff.and_then(|handoff| handoff.get("contract_version")).and_then(Value::as_str),
        "map_hub_handoff_contract_version": map_hub_route_runner_handoff.and_then(|handoff| handoff.get("contract_version")).and_then(Value::as_str),
        "lifecycle_contract_version": feed_route_runner_handoff.and_then(|handoff| handoff.get("lifecycle_contract_version")).and_then(Value::as_str),
        "first_lifecycle_source": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_lifecycle_source")).and_then(Value::as_str),
        "first_lifecycle_stage": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_lifecycle_stage")).and_then(Value::as_str),
        "first_lifecycle_status": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_lifecycle_status")).and_then(Value::as_str),
        "runner_count": feed_route_runner_handoff.and_then(|handoff| handoff.get("runner_count")).and_then(Value::as_u64).unwrap_or(0),
        "reward_claim_action_count": feed_route_runner_handoff.and_then(|handoff| handoff.get("reward_claim_action_count")).and_then(Value::as_u64).unwrap_or(0),
        "next_route_action_count": feed_route_runner_handoff.and_then(|handoff| handoff.get("next_route_action_count")).and_then(Value::as_u64).unwrap_or(0),
        "first_reward_claim_status": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_reward_claim_status")).and_then(Value::as_str),
        "first_next_route_status": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_next_route_status")).and_then(Value::as_str),
        "first_next_route_sequence_summary": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_next_route_sequence_summary")).and_then(Value::as_str),
        "handoff_prompt": feed_route_runner_handoff.and_then(|handoff| handoff.get("handoff_prompt")).and_then(Value::as_str),
    })
}

fn is_route_runner_handoff_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_playability_route_runner_handoff_gate_v1")
        && gate
            .get("feed_contract_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("map_hub_contract_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("source_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 7
        && gate
            .get("sources_include_route_runner_handoff")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("feed_handoff_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_route_runner_handoff_v1")
        && gate
            .get("map_hub_handoff_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_route_runner_handoff_v1")
        && gate
            .get("lifecycle_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_route_runner_lifecycle_v1")
        && gate
            .get("runner_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("reward_claim_action_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("next_route_action_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("first_next_route_status")
            .and_then(Value::as_str)
            .is_some_and(|status| !status.trim().is_empty())
        && gate
            .get("first_lifecycle_source")
            .and_then(Value::as_str)
            .is_some_and(|source| !source.trim().is_empty())
        && gate
            .get("first_lifecycle_stage")
            .and_then(Value::as_str)
            .is_some_and(|stage| !stage.trim().is_empty())
        && gate
            .get("first_lifecycle_status")
            .and_then(Value::as_str)
            .is_some_and(|status| !status.trim().is_empty())
        && gate
            .get("first_next_route_sequence_summary")
            .and_then(Value::as_str)
            .is_some_and(|summary| !summary.trim().is_empty())
        && gate
            .get("handoff_prompt")
            .and_then(Value::as_str)
            .is_some_and(|prompt| !prompt.trim().is_empty())
}

fn route_runner_handoff_gate_u64(gate: &Value, key: &str) -> u64 {
    gate.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn gauge_bool(value: bool) -> u64 {
    if value {
        1
    } else {
        0
    }
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

#[derive(Debug, Clone)]
struct HealthWorldProjection {
    matrix_user_id: String,
    app: Value,
    route_artifacts: WorldRouteArtifacts,
}

impl HealthWorldProjection {
    fn new(league: &LeagueState) -> Self {
        let matrix_user_id = first_maturity_matrix_user_id(league);
        Self {
            app: client_app_json(league, matrix_user_id.as_str()),
            route_artifacts: build_world_route_artifacts(&league.world),
            matrix_user_id,
        }
    }
}

fn trillionnium_world_maturity_axes_json(
    league: &LeagueState,
    config: &ConsumerEntryConfig,
    profile_ok: bool,
    league_repository_runtime: &Value,
    projection: &HealthWorldProjection,
) -> Value {
    let matrix_user_id = projection.matrix_user_id.clone();
    let app = projection.app.clone();
    let route_artifacts = projection.route_artifacts.clone();
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

fn trillionnium_world_playability_scorecard_json(
    league: &LeagueState,
    trillionnium_world_maturity: &Value,
    trillionnium_world_closed_beta_prototype: &Value,
    trillionnium_world_real_user_beta: &Value,
    trillionnium_world_public_commercial_product: &Value,
    league_repository_runtime: &Value,
    projection: &HealthWorldProjection,
) -> Value {
    let matrix_user_id = projection.matrix_user_id.clone();
    let app = projection.app.clone();
    let world_home = world_home_json(league);
    let world_home_playability_runtime_green = world_home
        .get("playability_runtime")
        .and_then(|runtime| runtime.get("contract_version"))
        .and_then(Value::as_str)
        == Some("trillionnium_world_playability_runtime_v1")
        && world_home
            .get("playability_runtime")
            .and_then(|runtime| runtime.get("readiness_checks"))
            .and_then(Value::as_array)
            .is_some_and(|checks| checks.len() >= 3);
    let route_artifacts = projection.route_artifacts.clone();
    let onboarding = app.get("onboarding").cloned().unwrap_or_else(|| json!({}));
    let onboarding_steps = onboarding
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let onboarding_acceptance_checks = onboarding
        .get("acceptance_checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mobile_contract = app
        .get("mobile_shell_contract")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mobile_readiness_checks = mobile_contract
        .get("readiness_checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let playability_coach = app
        .get("playability_coach")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let coach_lanes = playability_coach
        .get("lanes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let coach_next_best_actions = playability_coach
        .get("next_best_actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let coach_readiness_checks = playability_coach
        .get("readiness_checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let coach_check = |check_id: &str| coach_readiness_checks.iter().any(|check| check == check_id);
    let coach_lane = |lane_id: &str| {
        coach_lanes.iter().any(|lane| {
            lane.get("lane_id").and_then(Value::as_str) == Some(lane_id)
                && lane
                    .get("player_goal")
                    .and_then(Value::as_str)
                    .is_some_and(|goal| !goal.trim().is_empty())
                && lane
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| !command.trim().is_empty())
        })
    };
    let coach_contract_v1 = playability_coach
        .get("contract_version")
        .and_then(Value::as_str)
        == Some("trillionnium_playability_coach_v1");
    let coach_p0_p1_p2_green = coach_contract_v1
        && coach_lane("p0_first_session")
        && coach_lane("p1_strategy_depth")
        && coach_lane("p2_retention_ops")
        && coach_next_best_actions.len() >= 4
        && coach_check("coach_lanes_cover_p0_p1_p2")
        && coach_check("coach_actions_link_world_panels")
        && coach_check("coach_uses_live_runtime_counts");
    let economy_retention_ops = app
        .get("economy_retention_ops")
        .cloned()
        .or_else(|| playability_coach.get("economy_retention_ops").cloned())
        .unwrap_or_else(|| json!({}));
    let ops_readiness_checks = economy_retention_ops
        .get("readiness_checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let ops_check = |check_id: &str| ops_readiness_checks.iter().any(|check| check == check_id);
    let ops_contract_v1 = economy_retention_ops
        .get("contract_version")
        .and_then(Value::as_str)
        == Some("trillionnium_economy_retention_ops_v1");
    let ops_tradeoff_cards_green = ops_contract_v1
        && ops_check("economy_tradeoff_cards_visible")
        && economy_retention_ops
            .get("economy_tradeoff_cards")
            .and_then(Value::as_array)
            .is_some_and(|cards| cards.len() >= 4);
    let ops_retention_calendar_green = ops_contract_v1
        && ops_check("retention_calendar_visible")
        && ops_check("ops_refresh_hooks_visible")
        && economy_retention_ops
            .get("retention_calendar")
            .and_then(|calendar| calendar.get("season_loop"))
            .and_then(Value::as_str)
            .is_some_and(|loop_copy| !loop_copy.trim().is_empty())
        && economy_retention_ops
            .get("ops_refresh_hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| hooks.len() >= 4);
    let ops_funnel_green = ops_contract_v1
        && ops_check("playability_funnel_visible")
        && economy_retention_ops
            .get("playability_funnel")
            .and_then(|funnel| funnel.get("steps"))
            .and_then(Value::as_array)
            .is_some_and(|steps| steps.len() >= 7)
        && economy_retention_ops
            .get("playability_funnel")
            .and_then(|funnel| funnel.get("total_steps"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            >= 7;
    let ops_anti_cheese_green = ops_contract_v1
        && ops_check("anti_cheese_policy_visible")
        && ops_check("cooldown_policy_visible")
        && ops_check("anti_cheese_gate_enforced_visible")
        && economy_retention_ops
            .get("anti_cheese_policy")
            .and_then(|policy| policy.get("cooldown_seconds"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            >= 300
        && economy_retention_ops
            .get("anti_cheese_policy")
            .and_then(|policy| policy.get("duplicate_gate"))
            .and_then(Value::as_str)
            == Some("review_hold_zero_reward")
        && economy_retention_ops
            .get("anti_cheese_policy")
            .and_then(|policy| policy.get("backend_gate_enforced"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let ops_engine_contracts_green = ops_contract_v1
        && ops_check("backend_outcome_engine_visible")
        && ops_check("market_simulator_visible")
        && ops_check("league_encounter_state_visible")
        && economy_retention_ops
            .get("engine_contracts")
            .and_then(|contracts| contracts.get("world_action_engine"))
            .and_then(Value::as_str)
            == Some("trillionnium_world_action_engine_v1")
        && economy_retention_ops
            .get("engine_contracts")
            .and_then(|contracts| contracts.get("market_simulator"))
            .and_then(Value::as_str)
            == Some("trillionnium_market_simulator_v1")
        && economy_retention_ops
            .get("engine_contracts")
            .and_then(|contracts| contracts.get("league_encounter_state"))
            .and_then(Value::as_str)
            == Some("trillionnium_league_encounter_state_v1");
    let ops_balance_config_green = ops_contract_v1
        && ops_check("balance_config_visible")
        && economy_retention_ops
            .get("playability_balance_config")
            .and_then(|config| config.get("contract_version"))
            .and_then(Value::as_str)
            == Some("trillionnium_playability_balance_config_v1")
        && economy_retention_ops
            .get("playability_balance_config")
            .and_then(|config| config.get("world_action_cooldown_seconds"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            >= 300;
    let ops_persistent_telemetry_green = ops_contract_v1
        && ops_check("persistent_telemetry_stream_visible")
        && economy_retention_ops
            .get("engine_contracts")
            .and_then(|contracts| contracts.get("telemetry_stream"))
            .and_then(Value::as_str)
            == Some("world_economy_events:playability_telemetry");
    let app_module_count = app.get("module_count").and_then(Value::as_u64).unwrap_or(0);
    let mobile_shell_ux_green = mobile_shell_ux_contract_green(&app);
    let feed_item_count = app
        .get("feed")
        .and_then(|feed| feed.get("items"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let feed_api_path_configured = app
        .get("feed")
        .and_then(|feed| feed.get("api_path"))
        .and_then(Value::as_str)
        .is_some_and(|path| !path.trim().is_empty());
    let route_runner_handoff_gate = app_route_runner_handoff_gate_json(&app);
    let route_runner_handoff_gate_green =
        is_route_runner_handoff_gate_green(&route_runner_handoff_gate);
    let feed_route_runner_handoff_green = route_runner_handoff_gate
        .get("feed_contract_visible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let map_hub_route_runner_handoff_green = route_runner_handoff_gate
        .get("map_hub_contract_visible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let progression = app.get("progression").cloned().unwrap_or_else(|| json!({}));
    let progression_level = progression
        .get("level")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let successful_task_count = progression
        .get("successful_task_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let experience_data_points = progression
        .get("experience_data_points")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let unlocked_skill_count = progression
        .get("unlocked_skill_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_tool_count = progression
        .get("unlocked_tool_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_skin_count = progression
        .get("unlocked_skin_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let social_contact_count = app
        .get("social")
        .and_then(|social| social.get("contact_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let route_preview_count = route_artifacts
        .preview
        .get("items")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let route_task_graph_count = route_artifacts.task_views.len();
    let route_tasks_with_next_action = route_artifacts
        .task_views
        .iter()
        .filter(|task| {
            let item = task.to_feed_item();
            item.get("next_opportunity_command")
                .and_then(Value::as_str)
                .is_some_and(|command| !command.trim().is_empty())
                && item
                    .get("next_opportunity_hint")
                    .and_then(Value::as_str)
                    .is_some_and(|hint| !hint.trim().is_empty() && !hint.contains("pending"))
                && item
                    .get("suggested_matrix_command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| !command.trim().is_empty())
        })
        .count();
    let route_contract_version_present = app
        .get("route_contract")
        .and_then(|contract| contract.get("contract_version"))
        .is_some();
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
    let latest_submission = league
        .submissions
        .values()
        .max_by_key(|submission| submission.created_at_epoch);
    let latest_submission_score = latest_submission
        .map(|submission| submission.score)
        .unwrap_or(0.0);
    let latest_submission_reward = latest_submission
        .map(|submission| submission.reward_amount)
        .unwrap_or(0.0);
    let score_event_dimensions = latest_submission
        .map(|submission| {
            submission
                .score_events
                .iter()
                .map(|event| event.dimension.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let has_score_dimension = |dimension: &str| {
        score_event_dimensions
            .iter()
            .any(|candidate| candidate == dimension)
    };
    let score_event_count = latest_submission
        .map(|submission| submission.score_events.len())
        .unwrap_or(0);
    let positive_reward_count = league
        .rewards
        .iter()
        .filter(|reward| reward.amount > 0.0)
        .count();
    let eligible_submission_count = league
        .submissions
        .values()
        .filter(|submission| submission.payout_status.as_deref() == Some("eligible"))
        .count();
    let review_hold_count = league
        .submissions
        .values()
        .filter(|submission| submission.payout_status.as_deref() == Some("review_hold"))
        .count();
    let item_reward_count = league.inventory_items.len();
    let intent_samples = [
        (
            "contract",
            "create a contract task with deliverable evidence risk and next step",
        ),
        (
            "venture",
            "start a venture company with customer operating loop and next action",
        ),
        (
            "craft",
            "craft and build a studio asset with evidence and risk controls",
        ),
        (
            "recruit",
            "recruit a team and hire collaborators for this quest",
        ),
        (
            "market",
            "buy from market shop listing and reserve reward escrow",
        ),
        (
            "explore",
            "explore the city map and inspect nearby world events",
        ),
    ];
    let intent_coverage = intent_samples
        .iter()
        .filter(|(expected, body)| world_action_kind(body).0 == *expected)
        .count();
    let all_product_gates_100 = all_maturity_axes_converged(
        trillionnium_world_maturity,
        &[
            "first_playable",
            "technical_alpha",
            "beta_readiness",
            "full_vision",
        ],
    ) && all_maturity_axes_converged(
        trillionnium_world_closed_beta_prototype,
        &[
            "product_loop",
            "access_governance",
            "persistence_runtime",
            "world_depth",
            "commerce_recovery",
        ],
    ) && all_maturity_axes_converged(
        trillionnium_world_real_user_beta,
        &[
            "product_retention",
            "access_safety",
            "durable_persistence",
            "economy_recovery",
            "world_capacity",
            "ops_runtime",
        ],
    ) && all_maturity_axes_converged(
        trillionnium_world_public_commercial_product,
        &[
            "public_launch_surface",
            "commercial_engine",
            "trust_safety",
            "durable_scale_ops",
            "growth_network",
            "public_world_depth",
        ],
    );

    let repository_final_cutover = league_repository_runtime
        .get("effective_repository")
        .and_then(Value::as_str)
        == Some("normalized_sql_direct_write_final")
        && league_repository_runtime
            .get("repository_cutover_status")
            .and_then(Value::as_str)
            == Some("normalized_sql_direct_write_final_cutover_active");

    let onboarding_axis = playability_axis_json(
        "onboarding_3_minute_loop",
        "Onboarding / 3-minute first quest",
        "A new player can see the main quest, choose focus, start action, submit/rate, and read reward within one guided rail.",
        vec![
            ("onboarding_contract_v1", onboarding.get("contract_version").and_then(Value::as_str) == Some("trillionnium_first_playable_onboarding_v1")),
            ("starter_quest_rail_visible", onboarding.get("rail_id").and_then(Value::as_str) == Some("first_playable_main_quest_rail")),
            ("five_step_loop_documented", onboarding_steps.len() >= 5),
            ("entry_surfaces_include_app_world_matrix", onboarding.get("entry_surfaces").and_then(Value::as_array).is_some_and(|surfaces| surfaces.iter().any(|surface| surface == "/app") && surfaces.iter().any(|surface| surface == "/world") && surfaces.iter().any(|surface| surface == "Matrix /app"))),
            ("map_focus_step_has_command", onboarding_steps.iter().any(|step| step.get("step_id").and_then(Value::as_str) == Some("orient_on_map") && step.get("command").and_then(Value::as_str).is_some_and(|command| command == "/map"))),
            ("world_action_step_prefills_cta", onboarding_steps.iter().any(|step| step.get("step_id").and_then(Value::as_str) == Some("start_world_action") && step.get("textarea_id").and_then(Value::as_str).is_some())),
            ("quest_delivery_step_prefills_cta", onboarding_steps.iter().any(|step| step.get("step_id").and_then(Value::as_str) == Some("quest_delivery") && step.get("textarea_id").and_then(Value::as_str).is_some())),
            ("acceptance_checks_and_coach_cover_reward_route", onboarding_acceptance_checks.iter().any(|check| check == "wallet_progression_feed_updated") && onboarding_acceptance_checks.iter().any(|check| check == "route_task_graph_next_action_visible") && coach_contract_v1 && coach_lane("p0_first_session") && coach_check("p0_next_best_action_visible")),
            ("mobile_shell_ready_for_first_loop", mobile_shell_ux_green),
            ("first_playable_gate_100", maturity_axis_percent(trillionnium_world_maturity, "first_playable") == 100),
        ],
    );

    let intent_axis = playability_axis_json(
        "intent_mapping",
        "Intent mapping / player language to world action",
        "Common player verbs map into contract, venture, craft, recruit, market, and explore actions with safe defaults.",
        vec![
            ("six_core_intents_covered", intent_coverage == intent_samples.len()),
            ("contract_intent_creates_task_path", world_action_kind("contract task deliverable evidence risk next").0 == "contract"),
            ("venture_intent_creates_asset_path", world_action_kind("start venture company operating loop").0 == "venture"),
            ("craft_intent_creates_asset_path", world_action_kind("craft build studio asset").0 == "craft"),
            ("recruit_intent_creates_social_path", world_action_kind("recruit hire team").0 == "recruit"),
            ("market_intent_creates_commerce_path", world_action_kind("market shop buy listing").0 == "market"),
            ("fallback_intent_explores_world", world_action_kind("look around the city map").0 == "explore"),
            ("world_events_recorded", world.world_events.len() >= 3),
            ("relationships_record_player_context", !world.world_relationships.is_empty()),
            ("route_actions_available_from_map_focus", route_tasks_with_next_action >= 5),
        ],
    );

    let quest_axis = playability_axis_json(
        "quest_clarity",
        "Quest clarity / next-best-action route graph",
        "Every active route explains what happened, where it is, and the next action button/command.",
        vec![
            ("route_preview_dense", route_preview_count >= 20),
            ("route_task_graph_dense", route_task_graph_count >= 10),
            ("route_tasks_have_next_actions", route_tasks_with_next_action >= 5),
            ("map_nodes_visible", world.world_map_nodes.len() >= 12),
            ("player_position_visible", !world.world_player_positions.is_empty()),
            ("contracts_link_tasks", !world.world_contracts.is_empty()),
            ("commerce_work_orders_visible", !world.world_work_orders.is_empty()),
            ("deliveries_visible", !world.world_work_deliveries.is_empty()),
            ("feed_items_include_route_context", feed_item_count >= 10 && feed_route_runner_handoff_green),
            ("route_contract_exposed_to_app", route_contract_version_present && route_runner_handoff_gate_green),
        ],
    );

    let scoring_axis = playability_axis_json(
        "scoring_rewards_explainability",
        "Scoring and rewards / explainable rating loop",
        "Submissions expose dimensions, grade, reward amount, payout state, inventory, and ledger-facing reward path.",
        vec![
            ("latest_submission_scored", latest_submission_score > 0.0 && score_event_count >= 6),
            ("latest_submission_reward_positive", latest_submission_reward > 0.0),
            ("score_events_have_core_dimensions", has_score_dimension("delivery_fit") && has_score_dimension("evidence_grounding") && has_score_dimension("risk_control") && has_score_dimension("actionability") && has_score_dimension("craft_polish")),
            ("hidden_tests_or_adapter_visible", has_score_dimension("hidden_tests") || has_score_dimension("llm_judge_adapter")),
            ("score_event_breakdown_rich", score_event_count >= 6),
            ("eligible_submissions_exist", eligible_submission_count > 0),
            ("positive_rewards_exist", positive_reward_count > 0),
            ("inventory_rewards_exist", item_reward_count > 0),
            ("contract_completion_settled", settled_contract_completion_count > 0),
            ("progression_level_reflects_rewards", progression_level >= 100),
        ],
    );

    let feedback_axis = playability_axis_json(
        "feedback_failure_recovery",
        "Feedback and failure recovery / retry without dead ends",
        "Failed or incomplete work can be rejected, refunded, reopened, cancelled, searched, and routed back to the next attempt.",
        vec![
            ("acceptance_path_exists", !world.world_work_acceptances.is_empty()),
            ("rejection_path_exists", !world.world_work_rejections.is_empty()),
            ("reopen_path_exists", !world.world_work_reopens.is_empty()),
            ("cancel_path_exists", !world.world_work_cancellations.is_empty()),
            ("rejection_refund_recorded", refunded_rejection_count > 0),
            ("cancellation_refund_recorded", refunded_cancellation_count > 0),
            ("retry_actions_and_coach_failure_lane_visible", route_tasks_with_next_action >= 5 && coach_check("p0_failure_recovery_copy_visible") && playability_coach.get("failure_recovery").and_then(|recovery| recovery.get("states")).and_then(Value::as_array).is_some_and(|states| states.len() >= 5)),
            ("search_empty_state_visible", mobile_readiness_checks.iter().any(|check| check == "search_empty_state_visible")),
            ("aria_live_status_visible", mobile_readiness_checks.iter().any(|check| check == "aria_live_ux_status_visible")),
            ("review_hold_path_modelled", review_hold_count > 0 || score_event_count >= 6),
        ],
    );

    let economy_axis = playability_axis_json(
        "economy_balance",
        "Economy balance / escrow, consume, refund, grant",
        "Market and commission loops cover listing, escrow reserve, delivery consume, refund, reputation, credits, and economy events.",
        vec![
            ("companies_and_shops_seeded", !world.world_companies.is_empty() && !world.world_shops.is_empty()),
            ("active_listing_ready", !world.world_listings.is_empty()),
            ("purchases_recorded", !world.world_purchases.is_empty()),
            ("escrow_reserve_recorded", reserved_purchase_count > 0),
            ("acceptance_consumes_escrow", consumed_purchase_count > 0),
            ("refunds_recorded", refunded_rejection_count > 0 && refunded_cancellation_count > 0),
            ("economy_events_dense", world.world_economy_events.len() >= 20),
            ("faction_strategy_tradeoffs_visible", !world.world_faction_standings.is_empty() && coach_check("p1_economy_tradeoffs_visible") && playability_coach.get("strategy_depth").and_then(|strategy| strategy.get("economy_choices")).and_then(Value::as_array).is_some_and(|choices| choices.len() >= 4) && ops_tradeoff_cards_green && ops_engine_contracts_green && ops_balance_config_green),
            ("player_rewards_positive", positive_reward_count > 0),
            ("wallet_module_available", app.get("wallet").and_then(|wallet| wallet.get("ledger_actions")).and_then(Value::as_array).is_some_and(|actions| actions.len() >= 4)),
        ],
    );

    let social_axis = playability_axis_json(
        "social_coop",
        "Social and co-op / parties, guilds, contacts",
        "The world can be played with teammates through contacts, guilds, raids, face-duel, faction standings, and shared route context.",
        vec![
            ("agent_contacts_visible", social_contact_count >= 3),
            ("world_entities_visible", world.world_entities.len() >= 3),
            ("guilds_available", league.guilds.len() >= 2),
            ("face_duel_match_available", league.matches.contains_key("face-duel-001")),
            ("guild_raid_match_available", league.matches.contains_key("guild-raid-001")),
            ("factions_available", world.world_factions.len() >= 4),
            ("relationship_and_coop_strategy_visible", !world.world_relationships.is_empty() && coach_check("p1_social_coop_choices_visible") && playability_coach.get("strategy_depth").and_then(|strategy| strategy.get("social_choices")).and_then(Value::as_array).is_some_and(|choices| choices.len() >= 4)),
            ("route_feed_supports_team_context", feed_item_count >= 10),
            ("social_module_available", app_module_count >= 5),
            ("nearby_agents_surface_available", app.get("nearby_agents").and_then(Value::as_array).is_some()),
        ],
    );

    let retention_axis = playability_axis_json(
        "retention_progression",
        "Retention and progression / reasons to return",
        "Players see level, successful quests, unlocks, feed history, route backlog, matches, and durable world growth.",
        vec![
            ("progression_level_100", progression_level >= 100),
            ("successful_tasks_dense", successful_task_count >= 20),
            ("experience_data_points_dense", experience_data_points >= 50),
            ("skills_unlocked", unlocked_skill_count >= 5),
            ("tools_unlocked", unlocked_tool_count >= 4),
            ("skins_unlocked", unlocked_skin_count >= 3),
            ("feed_history_dense", feed_item_count >= 20),
            ("route_backlog_and_daily_return_hook_visible", route_task_graph_count >= 10 && coach_check("p2_daily_return_hook_visible") && playability_coach.get("retention_ops").and_then(|ops| ops.get("daily_return_hooks")).and_then(Value::as_array).is_some_and(|hooks| hooks.len() >= 4) && ops_retention_calendar_green && ops_persistent_telemetry_green),
            ("multiple_match_modes", league.matches.len() >= 4),
            ("world_assets_persist", !world.world_assets.is_empty()),
        ],
    );

    let surface_axis = playability_axis_json(
        "surface_feedback",
        "Surface feedback / UI tells players what changed",
        "The app/world/league surfaces expose live status, feed hydration, map focus, route status, score feedback, and actionable CTAs.",
        vec![
            ("app_has_five_modules", app_module_count >= 5),
            ("mobile_shell_contract_green", mobile_shell_ux_green),
            ("feed_api_hydration_visible", mobile_readiness_checks.iter().any(|check| check == "feed_api_hydration_visible")),
            ("web_session_feed_hydration_visible", mobile_readiness_checks.iter().any(|check| check == "web_session_feed_hydration_visible")),
            ("playability_coach_visible", mobile_readiness_checks.iter().any(|check| check == "next_action_rail_visible") && mobile_readiness_checks.iter().any(|check| check == "playability_coach_visible") && coach_p0_p1_p2_green),
            ("map_focus_visible", onboarding_acceptance_checks.iter().any(|check| check == "map_focus_visible")),
            ("quest_rating_visible", onboarding_acceptance_checks.iter().any(|check| check == "quest_rating_or_feedback_loop_visible")),
            ("reward_feed_visible", onboarding_acceptance_checks.iter().any(|check| check == "wallet_progression_feed_updated")),
            ("league_score_breakdown_available", score_event_count >= 6),
            ("route_status_cards_available", route_tasks_with_next_action >= 5 && map_hub_route_runner_handoff_green),
        ],
    );

    let observability_axis = playability_axis_json(
        "observability_gates",
        "Observability and gates / playability is measurable",
        "Health, metrics, beta, commercial, route, feed, and repository gates make 10/10 playability auditable instead of subjective.",
        vec![
            ("all_existing_product_gates_100", all_product_gates_100),
            ("maturity_overall_100", trillionnium_world_maturity.get("overall_percent").and_then(Value::as_u64) == Some(100)),
            ("closed_beta_overall_100", trillionnium_world_closed_beta_prototype.get("overall_percent").and_then(Value::as_u64) == Some(100)),
            ("real_user_beta_overall_100", trillionnium_world_real_user_beta.get("overall_percent").and_then(Value::as_u64) == Some(100)),
            ("public_commercial_overall_100", trillionnium_world_public_commercial_product.get("overall_percent").and_then(Value::as_u64) == Some(100)),
            ("feed_api_path_configured", feed_api_path_configured && feed_route_runner_handoff_green),
            ("playability_runtime_contracts_exposed", route_contract_version_present && route_runner_handoff_gate_green && coach_p0_p1_p2_green && world_home_playability_runtime_green && ops_contract_v1 && ops_engine_contracts_green && ops_balance_config_green),
            ("mobile_contract_readiness_dense", mobile_readiness_checks.len() >= 10),
            ("scorecard_has_runtime_funnel_data", feed_item_count >= 20 && route_task_graph_count >= 10 && route_runner_handoff_gate_green && ops_funnel_green && ops_persistent_telemetry_green),
            ("repository_backed_world_state_dense", world.world_economy_events.len() >= 20 && !world.world_contract_completions.is_empty()),
        ],
    );

    let user_metric_technical_reliability = playability_axis_json(
        "technical_reliability",
        "技术可靠性 / Technical reliability",
        "Reliability must be backed by runtime gates, normalized persistence, health/metrics, route/feed contracts, and production evidence.",
        vec![
            ("all_existing_product_gates_100", all_product_gates_100),
            ("repository_final_cutover_active", repository_final_cutover),
            ("maturity_gate_100", trillionnium_world_maturity.get("overall_percent").and_then(Value::as_u64) == Some(100)),
            ("real_user_beta_gate_100", trillionnium_world_real_user_beta.get("overall_percent").and_then(Value::as_u64) == Some(100)),
            ("public_commercial_gate_100", trillionnium_world_public_commercial_product.get("overall_percent").and_then(Value::as_u64) == Some(100)),
            ("mobile_shell_contract_green", mobile_shell_ux_green),
            ("feed_api_path_configured", feed_api_path_configured && feed_route_runner_handoff_green),
            ("playability_runtime_contracts_present", route_contract_version_present && route_runner_handoff_gate_green && coach_p0_p1_p2_green && world_home_playability_runtime_green && ops_contract_v1 && ops_engine_contracts_green && ops_balance_config_green),
            ("score_events_runtime_present", score_event_count >= 6),
            ("world_state_dense_enough_for_smoke", feed_item_count >= 20 && route_runner_handoff_gate_green && world.world_economy_events.len() >= 20),
        ],
    );

    let user_metric_first_playable_completeness = playability_axis_json(
        "first_playable_completeness",
        "first playable 完整度 / First playable completeness",
        "The first playable loop is complete only when map focus, action, contract, commission, rating, reward, and next route are all visible.",
        vec![
            ("onboarding_contract_v1", onboarding.get("contract_version").and_then(Value::as_str) == Some("trillionnium_first_playable_onboarding_v1")),
            ("starter_quest_rail_visible", onboarding.get("rail_id").and_then(Value::as_str) == Some("first_playable_main_quest_rail")),
            ("five_step_loop_documented", onboarding_steps.len() >= 5),
            ("acceptance_checks_cover_full_loop", onboarding_acceptance_checks.len() >= 7),
            ("entry_surfaces_include_app_world_matrix", onboarding.get("entry_surfaces").and_then(Value::as_array).is_some_and(|surfaces| surfaces.iter().any(|surface| surface == "/app") && surfaces.iter().any(|surface| surface == "/world") && surfaces.iter().any(|surface| surface == "Matrix /app"))),
            ("coach_next_best_actions_cover_first_loop", coach_lane("p0_first_session") && coach_next_best_actions.iter().any(|action| action.get("metric").and_then(Value::as_str) == Some("first_playable_completeness")) && onboarding_steps.iter().any(|step| step.get("step_id").and_then(Value::as_str) == Some("orient_on_map")) && onboarding_steps.iter().any(|step| step.get("step_id").and_then(Value::as_str) == Some("start_world_action") && step.get("textarea_id").and_then(Value::as_str).is_some()) && onboarding_steps.iter().any(|step| step.get("step_id").and_then(Value::as_str) == Some("quest_delivery") && step.get("textarea_id").and_then(Value::as_str).is_some()) && onboarding_steps.iter().any(|step| step.get("step_id").and_then(Value::as_str) == Some("read_reward_and_next_route"))),
            ("route_preview_and_task_graph_ready", route_preview_count >= 20 && route_task_graph_count >= 10),
            ("commerce_loop_seeded", !world.world_listings.is_empty() && !world.world_purchases.is_empty() && !world.world_work_orders.is_empty()),
            ("rating_reward_loop_seeded", !world.world_work_deliveries.is_empty() && !world.world_work_acceptances.is_empty() && positive_reward_count > 0),
            ("next_route_after_reward_visible", route_tasks_with_next_action >= 5 && route_runner_handoff_gate_green),
        ],
    );

    let user_metric_player_comprehension_cost = playability_axis_json(
        "real_player_comprehension_cost",
        "真实玩家理解成本 / Real player comprehension cost",
        "Players should understand what to do next without reading debug internals: tabs, search, live status, score formula, route status, and recovery copy must be obvious.",
        vec![
            ("four_tab_shell_reduces_navigation_load", mobile_readiness_checks.iter().any(|check| check == "four_tab_mobile_shell_visible")),
            ("active_tab_search_available", mobile_readiness_checks.iter().any(|check| check == "global_search_filters_active_tab")),
            ("empty_state_and_clear_recovery", mobile_readiness_checks.iter().any(|check| check == "search_empty_state_visible") && mobile_readiness_checks.iter().any(|check| check == "search_clear_and_escape_visible")),
            ("live_status_feedback_visible", mobile_readiness_checks.iter().any(|check| check == "aria_live_ux_status_visible")),
            ("coach_reduces_comprehension_cost", mobile_readiness_checks.iter().any(|check| check == "next_action_rail_visible") && coach_check("p0_next_best_action_visible") && coach_check("p0_failure_recovery_copy_visible")),
            ("map_focus_acceptance_visible", onboarding_acceptance_checks.iter().any(|check| check == "map_focus_visible")),
            ("rating_reward_acceptance_visible", onboarding_acceptance_checks.iter().any(|check| check == "quest_rating_or_feedback_loop_visible") && onboarding_acceptance_checks.iter().any(|check| check == "wallet_progression_feed_updated")),
            ("route_graph_has_actionable_commands", route_tasks_with_next_action >= 5),
            ("league_score_breakdown_explainable", score_event_count >= 6 && has_score_dimension("delivery_fit") && has_score_dimension("evidence_grounding") && has_score_dimension("risk_control") && has_score_dimension("actionability")),
            ("feedback_recovery_paths_visible", !world.world_work_rejections.is_empty() && !world.world_work_reopens.is_empty() && !world.world_work_cancellations.is_empty()),
        ],
    );

    let user_metric_long_term_replayability = playability_axis_json(
        "long_term_replayability",
        "长期可重复游玩 / Long-term replayability",
        "Replayability requires durable progression, varied route backlog, multiple modes, feed history, unlocks, world events, and repeatable economy loops.",
        vec![
            ("progression_level_100", progression_level >= 100),
            ("successful_tasks_dense", successful_task_count >= 20),
            ("experience_data_points_dense", experience_data_points >= 50),
            ("skills_tools_skins_unlocked", unlocked_skill_count >= 5 && unlocked_tool_count >= 4 && unlocked_skin_count >= 3),
            ("feed_history_dense", feed_item_count >= 20),
            ("coach_retention_ops_visible", route_task_graph_count >= 10 && coach_lane("p2_retention_ops") && coach_check("p2_telemetry_contract_visible") && ops_retention_calendar_green && ops_anti_cheese_green),
            ("multiple_match_modes", league.matches.len() >= 4),
            ("world_events_dense", world.world_events.len() >= 3 && world.world_economy_events.len() >= 20),
            ("replayable_market_and_work_loops", world.world_listings.len() >= 3 && world.world_work_orders.len() >= 3),
            ("failure_retry_keeps_loop_alive", !world.world_work_rejections.is_empty() && !world.world_work_reopens.is_empty() && !world.world_work_cancellations.is_empty()),
        ],
    );

    let user_metric_economy_social_strategy_depth = playability_axis_json(
        "economy_social_strategy_depth",
        "经济/社交策略深度 / Economy and social strategy depth",
        "Depth requires meaningful choices across market supply, escrow/settlement, refunds, factions, guilds, raids, nearby agents, relationships, and reward inventory.",
        vec![
            ("companies_shops_listings_ready", !world.world_companies.is_empty() && !world.world_shops.is_empty() && !world.world_listings.is_empty()),
            ("purchase_reserve_consume_loop", !world.world_purchases.is_empty() && reserved_purchase_count > 0 && consumed_purchase_count > 0),
            ("refund_and_reopen_strategy_loop", refunded_rejection_count > 0 && refunded_cancellation_count > 0 && !world.world_work_reopens.is_empty()),
            ("settled_contract_rewards", settled_contract_completion_count > 0 && positive_reward_count > 0),
            ("coach_strategy_depth_visible", app.get("wallet").and_then(|wallet| wallet.get("ledger_actions")).and_then(Value::as_array).is_some_and(|actions| actions.len() >= 4) && coach_lane("p1_strategy_depth") && coach_check("p1_economy_tradeoffs_visible") && coach_check("p1_social_coop_choices_visible") && ops_tradeoff_cards_green && ops_engine_contracts_green && ops_balance_config_green),
            ("factions_and_standings_present", world.world_factions.len() >= 4 && !world.world_faction_standings.is_empty()),
            ("relationship_graph_and_nearby_agents", !world.world_relationships.is_empty() && social_contact_count >= 3),
            ("guild_and_raid_coop_modes", league.guilds.len() >= 2 && league.matches.contains_key("guild-raid-001")),
            ("face_to_face_duel_social_mode", league.matches.contains_key("face-duel-001") && world.world_entities.len() >= 3),
            ("inventory_reward_loadout_strategy", item_reward_count > 0 && unlocked_skill_count >= 5 && unlocked_tool_count >= 4),
        ],
    );

    let axes = json!({
        "onboarding_3_minute_loop": onboarding_axis,
        "intent_mapping": intent_axis,
        "quest_clarity": quest_axis,
        "scoring_rewards_explainability": scoring_axis,
        "feedback_failure_recovery": feedback_axis,
        "economy_balance": economy_axis,
        "social_coop": social_axis,
        "retention_progression": retention_axis,
        "surface_feedback": surface_axis,
        "observability_gates": observability_axis,
    });
    let axis_order = [
        "onboarding_3_minute_loop",
        "intent_mapping",
        "quest_clarity",
        "scoring_rewards_explainability",
        "feedback_failure_recovery",
        "economy_balance",
        "social_coop",
        "retention_progression",
        "surface_feedback",
        "observability_gates",
    ];
    let axis_scores = axis_order
        .iter()
        .map(|axis_id| playability_axis_score(&axes, axis_id))
        .collect::<Vec<_>>();
    let overall_score = if axis_scores.is_empty() {
        0.0
    } else {
        ((axis_scores.iter().sum::<f64>() / axis_scores.len() as f64) * 10.0).round() / 10.0
    };
    let overall_percent = ((overall_score * 10.0).round() as u64).min(100);
    let axis_order_vec = axis_order.to_vec();
    let user_metric_axes = json!({
        "technical_reliability": user_metric_technical_reliability,
        "first_playable_completeness": user_metric_first_playable_completeness,
        "real_player_comprehension_cost": user_metric_player_comprehension_cost,
        "long_term_replayability": user_metric_long_term_replayability,
        "economy_social_strategy_depth": user_metric_economy_social_strategy_depth,
    });
    let user_metric_order = [
        "technical_reliability",
        "first_playable_completeness",
        "real_player_comprehension_cost",
        "long_term_replayability",
        "economy_social_strategy_depth",
    ];
    let user_metric_scores = user_metric_order
        .iter()
        .map(|axis_id| playability_axis_score(&user_metric_axes, axis_id))
        .collect::<Vec<_>>();
    let user_metric_overall_score = if user_metric_scores.is_empty() {
        0.0
    } else {
        ((user_metric_scores.iter().sum::<f64>() / user_metric_scores.len() as f64) * 10.0).round()
            / 10.0
    };
    let user_metric_overall_percent = ((user_metric_overall_score * 10.0).round() as u64).min(100);
    let user_metric_order_vec = user_metric_order.to_vec();
    let scorecard = json!({
        "contract_version": TRILLIONNIUM_WORLD_PLAYABILITY_SCORECARD_CONTRACT_VERSION,
        "target": "all_5_user_playability_metrics_score_10_of_10",
        "diagnostic_target": "all_10_playability_sub_axes_score_10_of_10",
        "overall_score": overall_score,
        "overall_percent": overall_percent,
        "overall_status": if overall_score == 10.0 { "converged" } else { "in_progress" },
        "user_metric_overall_score": user_metric_overall_score,
        "user_metric_overall_percent": user_metric_overall_percent,
        "user_metric_overall_status": if user_metric_overall_score == 10.0 { "converged" } else { "in_progress" },
        "score_unit": "0_to_10",
        "matrix_user_id": matrix_user_id,
        "axis_order": axis_order_vec,
        "user_metric_order": user_metric_order_vec,
        "reported_baseline_before_push": {
            "technical_reliability": 8.5,
            "first_playable_completeness": 8.0,
            "real_player_comprehension_cost": 5.5,
            "long_term_replayability": 4.5,
            "economy_social_strategy_depth": 4.0
        },
        "proof_scope": "runtime_state_plus_product_gates_not_subjective_claim",
        "player_loop": "choose map focus → accept bounty/commission → submit result/evidence → rating/reward → next route/retry",
        "route_runner_handoff_gate": route_runner_handoff_gate,
        "axes": axes,
        "user_metric_axes": user_metric_axes,
    });
    let _ = all_playability_axes_converged(&scorecard, &axis_order);
    scorecard
}

fn trillionnium_world_closed_beta_prototype_json(
    league: &LeagueState,
    config: &ConsumerEntryConfig,
    profile_ok: bool,
    identity_governance_valid: bool,
    session_auth_governance_valid: bool,
    league_repository_runtime: &Value,
    trillionnium_world_maturity: &Value,
    projection: &HealthWorldProjection,
) -> Value {
    let matrix_user_id = projection.matrix_user_id.clone();
    let app = projection.app.clone();
    let route_artifacts = projection.route_artifacts.clone();
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
    let route_runner_handoff_gate = app_route_runner_handoff_gate_json(&app);
    let route_runner_handoff_gate_ready =
        is_route_runner_handoff_gate_green(&route_runner_handoff_gate);
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
            (
                "route_task_graph_dense",
                route_task_graph_count >= 10 && route_runner_handoff_gate_ready,
            ),
            (
                "feed_surface_active",
                feed_item_count >= 5 && route_runner_handoff_gate_ready,
            ),
            (
                "route_runner_handoff_gate_visible",
                route_runner_handoff_gate_ready,
            ),
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
            (
                "route_runner_handoff_world_loop_ready",
                route_runner_handoff_gate_ready,
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
        "route_runner_handoff_gate": route_runner_handoff_gate,
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
    projection: &HealthWorldProjection,
) -> Value {
    let matrix_user_id = projection.matrix_user_id.clone();
    let app = projection.app.clone();
    let route_artifacts = projection.route_artifacts.clone();
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
    let route_runner_handoff_gate = app_route_runner_handoff_gate_json(&app);
    let route_runner_handoff_gate_ready =
        is_route_runner_handoff_gate_green(&route_runner_handoff_gate);
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
            (
                "route_runner_handoff_gate_visible",
                route_runner_handoff_gate_ready,
            ),
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
                route_task_graph_count >= 10 && route_runner_handoff_gate_ready,
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
        "route_runner_handoff_gate": route_runner_handoff_gate,
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
    projection: &HealthWorldProjection,
) -> Value {
    let matrix_user_id = projection.matrix_user_id.clone();
    let app = projection.app.clone();
    let route_artifacts = projection.route_artifacts.clone();
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
    let route_runner_handoff_gate = app_route_runner_handoff_gate_json(&app);
    let route_runner_handoff_gate_ready =
        is_route_runner_handoff_gate_green(&route_runner_handoff_gate);
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
            (
                "route_runner_handoff_gate_visible",
                route_runner_handoff_gate_ready,
            ),
            ("social_contacts_ready", social_contact_count >= 3),
            ("route_preview_dense", route_preview_count >= 20),
            (
                "route_task_graph_dense",
                route_task_graph_count >= 10 && route_runner_handoff_gate_ready,
            ),
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
                route_task_graph_count >= 10 && route_runner_handoff_gate_ready,
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
            (
                "route_task_graph_dense",
                route_task_graph_count >= 10 && route_runner_handoff_gate_ready,
            ),
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
        "route_runner_handoff_gate": route_runner_handoff_gate,
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
        trillionnium_world_playability_scorecard,
    ) = {
        let league = state.inner.league_state.lock().await;
        let projection = HealthWorldProjection::new(&league);
        let maturity = trillionnium_world_maturity_axes_json(
            &league,
            state.config(),
            profile_ok,
            &league_repository_runtime,
            &projection,
        );
        let closed_beta = trillionnium_world_closed_beta_prototype_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &maturity,
            &projection,
        );
        let real_user_beta = trillionnium_world_real_user_beta_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &closed_beta,
            &projection,
        );
        let public_commercial_product = trillionnium_world_public_commercial_product_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &real_user_beta,
            &projection,
        );
        let playability_scorecard = trillionnium_world_playability_scorecard_json(
            &league,
            &maturity,
            &closed_beta,
            &real_user_beta,
            &public_commercial_product,
            &league_repository_runtime,
            &projection,
        );
        (
            maturity,
            closed_beta,
            real_user_beta,
            public_commercial_product,
            playability_scorecard,
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
        "trillionnium_world_playability_scorecard": trillionnium_world_playability_scorecard,
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
        trillionnium_world_playability_scorecard,
    ) = {
        let league = state.inner.league_state.lock().await;
        let projection = HealthWorldProjection::new(&league);
        let maturity = trillionnium_world_maturity_axes_json(
            &league,
            state.config(),
            profile_ok_bool,
            &league_repository_runtime,
            &projection,
        );
        let closed_beta = trillionnium_world_closed_beta_prototype_json(
            &league,
            state.config(),
            profile_ok_bool,
            governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &maturity,
            &projection,
        );
        let real_user_beta = trillionnium_world_real_user_beta_json(
            &league,
            state.config(),
            profile_ok_bool,
            governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &closed_beta,
            &projection,
        );
        let public_commercial_product = trillionnium_world_public_commercial_product_json(
            &league,
            state.config(),
            profile_ok_bool,
            governance_valid,
            session_auth_registry_governance_valid,
            &league_repository_runtime,
            &real_user_beta,
            &projection,
        );
        let playability_scorecard = trillionnium_world_playability_scorecard_json(
            &league,
            &maturity,
            &closed_beta,
            &real_user_beta,
            &public_commercial_product,
            &league_repository_runtime,
            &projection,
        );
        (
            maturity,
            closed_beta,
            real_user_beta,
            public_commercial_product,
            playability_scorecard,
        )
    };
    let empty_route_runner_handoff_gate = json!({});
    let playability_route_runner_handoff_gate = trillionnium_world_playability_scorecard
        .get("route_runner_handoff_gate")
        .unwrap_or(&empty_route_runner_handoff_gate);
    let closed_beta_route_runner_handoff_gate = trillionnium_world_closed_beta_prototype
        .get("route_runner_handoff_gate")
        .unwrap_or(&empty_route_runner_handoff_gate);
    let real_user_beta_route_runner_handoff_gate = trillionnium_world_real_user_beta
        .get("route_runner_handoff_gate")
        .unwrap_or(&empty_route_runner_handoff_gate);
    let public_commercial_route_runner_handoff_gate = trillionnium_world_public_commercial_product
        .get("route_runner_handoff_gate")
        .unwrap_or(&empty_route_runner_handoff_gate);
    let playability_route_runner_handoff_gate_green =
        is_route_runner_handoff_gate_green(playability_route_runner_handoff_gate);
    let closed_beta_route_runner_handoff_gate_green =
        is_route_runner_handoff_gate_green(closed_beta_route_runner_handoff_gate);
    let real_user_beta_route_runner_handoff_gate_green =
        is_route_runner_handoff_gate_green(real_user_beta_route_runner_handoff_gate);
    let public_commercial_route_runner_handoff_gate_green =
        is_route_runner_handoff_gate_green(public_commercial_route_runner_handoff_gate);
    let all_route_runner_handoff_gates_green = playability_route_runner_handoff_gate_green
        && closed_beta_route_runner_handoff_gate_green
        && real_user_beta_route_runner_handoff_gate_green
        && public_commercial_route_runner_handoff_gate_green;
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
            "cex_consumer_entry_trillionnium_world_public_commercial_product_public_world_depth_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_playability_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_playability_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_closed_beta_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_closed_beta_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_real_user_beta_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_real_user_beta_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_public_commercial_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_public_commercial_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_all_gates_green gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_all_gates_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_feed_source_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_feed_source_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_runner_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_runner_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_reward_claim_action_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_reward_claim_action_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_next_route_action_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_next_route_action_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_overall_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_overall_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_overall_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_overall_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_onboarding_3_minute_loop_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_onboarding_3_minute_loop_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_intent_mapping_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_intent_mapping_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_quest_clarity_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_quest_clarity_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_scoring_rewards_explainability_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_scoring_rewards_explainability_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_feedback_failure_recovery_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_feedback_failure_recovery_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_economy_balance_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_economy_balance_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_social_coop_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_social_coop_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_retention_progression_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_retention_progression_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_surface_feedback_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_surface_feedback_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_observability_gates_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_observability_gates_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_user_metric_overall_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_user_metric_overall_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_user_metric_overall_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_user_metric_overall_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_technical_reliability_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_technical_reliability_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_first_playable_completeness_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_first_playable_completeness_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_real_player_comprehension_cost_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_real_player_comprehension_cost_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_long_term_replayability_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_long_term_replayability_score {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_playability_scorecard_economy_social_strategy_depth_score gauge\n",
            "cex_consumer_entry_trillionnium_world_playability_scorecard_economy_social_strategy_depth_score {}\n"
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
        gauge_bool(playability_route_runner_handoff_gate_green),
        gauge_bool(closed_beta_route_runner_handoff_gate_green),
        gauge_bool(real_user_beta_route_runner_handoff_gate_green),
        gauge_bool(public_commercial_route_runner_handoff_gate_green),
        gauge_bool(all_route_runner_handoff_gates_green),
        route_runner_handoff_gate_u64(playability_route_runner_handoff_gate, "source_count"),
        route_runner_handoff_gate_u64(playability_route_runner_handoff_gate, "runner_count"),
        route_runner_handoff_gate_u64(
            playability_route_runner_handoff_gate,
            "reward_claim_action_count",
        ),
        route_runner_handoff_gate_u64(
            playability_route_runner_handoff_gate,
            "next_route_action_count",
        ),
        trillionnium_world_playability_scorecard
            .get("overall_score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        trillionnium_world_playability_scorecard
            .get("overall_percent")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        playability_axis_score(
            &trillionnium_world_playability_scorecard,
            "onboarding_3_minute_loop",
        ),
        playability_axis_score(&trillionnium_world_playability_scorecard, "intent_mapping"),
        playability_axis_score(&trillionnium_world_playability_scorecard, "quest_clarity"),
        playability_axis_score(
            &trillionnium_world_playability_scorecard,
            "scoring_rewards_explainability",
        ),
        playability_axis_score(
            &trillionnium_world_playability_scorecard,
            "feedback_failure_recovery",
        ),
        playability_axis_score(&trillionnium_world_playability_scorecard, "economy_balance"),
        playability_axis_score(&trillionnium_world_playability_scorecard, "social_coop"),
        playability_axis_score(
            &trillionnium_world_playability_scorecard,
            "retention_progression",
        ),
        playability_axis_score(&trillionnium_world_playability_scorecard, "surface_feedback"),
        playability_axis_score(&trillionnium_world_playability_scorecard, "observability_gates"),
        trillionnium_world_playability_scorecard
            .get("user_metric_overall_score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        trillionnium_world_playability_scorecard
            .get("user_metric_overall_percent")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        playability_user_metric_score(
            &trillionnium_world_playability_scorecard,
            "technical_reliability",
        ),
        playability_user_metric_score(
            &trillionnium_world_playability_scorecard,
            "first_playable_completeness",
        ),
        playability_user_metric_score(
            &trillionnium_world_playability_scorecard,
            "real_player_comprehension_cost",
        ),
        playability_user_metric_score(
            &trillionnium_world_playability_scorecard,
            "long_term_replayability",
        ),
        playability_user_metric_score(
            &trillionnium_world_playability_scorecard,
            "economy_social_strategy_depth",
        ),
    );
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

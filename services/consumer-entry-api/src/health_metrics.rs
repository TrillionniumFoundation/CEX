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
    let percent = (passed * 100).checked_div(total).unwrap_or_default() as u64;
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

fn cex_trillionnium_world_adapter_readiness_green(readiness: &Value) -> bool {
    readiness.get("contract_version").and_then(Value::as_str)
        == Some("cex_trnm_game_economy_adapter_v1")
        && readiness.get("protocol_contract").and_then(Value::as_str)
            == Some("term_exchange_protocol_v2")
        && readiness.get("domain_contract").and_then(Value::as_str) == Some("trnm_game_economy_v1")
        && readiness.get("status").and_then(Value::as_str)
            == Some("cex_trnm_game_economy_adapter_ready")
        && readiness
            .pointer("/repository/source_of_truth")
            .and_then(Value::as_str)
            == Some("cex_postgres_trnm_economic_intents_and_receipts")
        && readiness
            .pointer("/identity/adapter_contract")
            .and_then(Value::as_str)
            == Some("cex_trnm_game_economy_adapter_v1")
        && readiness
            .pointer("/session/source_of_truth")
            .and_then(Value::as_str)
            == Some("cex_ingress_token_and_optional_signed_session")
        && readiness
            .pointer("/standalone_runtime_adapter_readiness/statuses")
            .and_then(Value::as_array)
            .is_some_and(|statuses| {
                !statuses.is_empty()
                    && statuses.iter().all(|status| {
                        status.get("status").and_then(Value::as_str)
                            == Some("cex_trnm_game_economy_impl_connected")
                            && status
                                .get("production_adapter_trait_ready")
                                .and_then(Value::as_bool)
                                == Some(true)
                    })
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
    let map_readability_lod =
        mobile_shell_contract.and_then(|contract| contract.get("map_readability_lod"));
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
    let map_readability_lod_green = map_readability_lod
        .and_then(|lod| lod.get("contract_version"))
        .and_then(Value::as_str)
        == Some("trillionnium_world_map_readability_lod_v1")
        && map_readability_lod
            .and_then(|lod| lod.get("visible_contract_id"))
            .and_then(Value::as_str)
            == Some("app-map-readability-lod")
        && map_readability_lod
            .and_then(|lod| lod.get("max_primary_cta_count"))
            .and_then(Value::as_u64)
            == Some(1)
        && map_readability_lod
            .and_then(|lod| lod.get("max_summary_chars"))
            .and_then(Value::as_u64)
            .is_some_and(|max_chars| max_chars <= 150)
        && map_readability_lod
            .and_then(|lod| lod.get("max_visible_markers"))
            .and_then(Value::as_u64)
            .is_some_and(|max_markers| max_markers <= 18)
        && map_readability_lod
            .and_then(|lod| lod.get("max_avatar_route_runners"))
            .and_then(Value::as_u64)
            .is_some_and(|max_runners| max_runners <= 6)
        && map_readability_lod
            .and_then(|lod| lod.get("details_default_state"))
            .and_then(Value::as_str)
            == Some("collapsed");
    primary_cta_green
        && copy_layering_green
        && map_readability_lod_green
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
            "map_readability_lod_visible",
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
                .get("supports_route_mastery_progression")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && handoff
                .get("route_mastery_contract_version")
                .and_then(Value::as_str)
                == Some("trillionnium_route_mastery_v1")
            && handoff
                .get("runner_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
            && handoff
                .get("route_mastery_runner_count")
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
                .get("first_route_mastery_xp")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
            && handoff
                .get("first_route_mastery_tier")
                .and_then(Value::as_str)
                .is_some_and(|tier| !tier.trim().is_empty())
            && handoff
                .get("first_route_mastery_next_goal")
                .and_then(Value::as_str)
                .is_some_and(|goal| !goal.trim().is_empty())
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
        "route_mastery_contract_version": feed_route_runner_handoff.and_then(|handoff| handoff.get("route_mastery_contract_version")).and_then(Value::as_str),
        "supports_route_mastery_progression": feed_route_runner_handoff.and_then(|handoff| handoff.get("supports_route_mastery_progression")).and_then(Value::as_bool).unwrap_or(false),
        "route_mastery_runner_count": feed_route_runner_handoff.and_then(|handoff| handoff.get("route_mastery_runner_count")).and_then(Value::as_u64).unwrap_or(0),
        "first_route_mastery_xp": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_route_mastery_xp")).and_then(Value::as_u64).unwrap_or(0),
        "first_route_mastery_tier": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_route_mastery_tier")).and_then(Value::as_str),
        "first_route_mastery_next_goal": feed_route_runner_handoff.and_then(|handoff| handoff.get("first_route_mastery_next_goal")).and_then(Value::as_str),
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
            .get("supports_route_mastery_progression")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("route_mastery_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_route_mastery_v1")
        && gate
            .get("runner_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("route_mastery_runner_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("first_route_mastery_xp")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("first_route_mastery_tier")
            .and_then(Value::as_str)
            .is_some_and(|tier| !tier.trim().is_empty())
        && gate
            .get("first_route_mastery_next_goal")
            .and_then(Value::as_str)
            .is_some_and(|goal| !goal.trim().is_empty())
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

fn app_map_readability_lod_gate_json(app: &Value) -> Value {
    let shell_lod = app
        .get("mobile_shell_contract")
        .and_then(|contract| contract.get("map_readability_lod"));
    let viewport_lod = app
        .get("map_hub")
        .and_then(|hub| hub.get("viewport"))
        .and_then(|viewport| viewport.get("map_readability_lod"));
    let map_hub_viewport = app.get("map_hub").and_then(|hub| hub.get("viewport"));
    let game_layer_semantics = viewport_lod.and_then(|lod| lod.get("game_layer_semantics"));
    let map_subsystem = app.get("world_map_subsystem_contract");
    let transport_delta_contract =
        map_hub_viewport.and_then(|viewport| viewport.get("transport_delta_contract"));
    let runtime_performance_budget = viewport_lod
        .and_then(|lod| lod.get("runtime_performance_budget"))
        .or_else(|| {
            app.get("mobile_shell_contract")
                .and_then(|contract| contract.get("runtime_performance_budget"))
        });
    let semantic_roles = shell_lod
        .and_then(|lod| lod.get("semantic_roles"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    json!({
        "contract_version": "trillionnium_world_map_readability_lod_gate_v1",
        "shell_contract_version": shell_lod.and_then(|lod| lod.get("contract_version")).and_then(Value::as_str),
        "viewport_contract_version": viewport_lod.and_then(|lod| lod.get("contract_version")).and_then(Value::as_str),
        "semantic_layer_contract_version": shell_lod.and_then(|lod| lod.get("semantic_layer_contract_version")).and_then(Value::as_str),
        "viewport_semantic_contract_version": game_layer_semantics.and_then(|semantics| semantics.get("contract_version")).and_then(Value::as_str),
        "visible_contract_id": shell_lod.and_then(|lod| lod.get("visible_contract_id")).and_then(Value::as_str),
        "first_screen_mode": shell_lod.and_then(|lod| lod.get("first_screen_mode")).and_then(Value::as_str),
        "max_primary_cta_count": shell_lod.and_then(|lod| lod.get("max_primary_cta_count")).and_then(Value::as_u64).unwrap_or(0),
        "max_summary_chars": shell_lod.and_then(|lod| lod.get("max_summary_chars")).and_then(Value::as_u64).unwrap_or(0),
        "max_visible_markers": viewport_lod.and_then(|lod| lod.get("object_budget")).and_then(|budget| budget.get("max_visible_markers")).and_then(Value::as_u64).or_else(|| shell_lod.and_then(|lod| lod.get("max_visible_markers")).and_then(Value::as_u64)).unwrap_or(0),
        "visible_markers": viewport_lod.and_then(|lod| lod.get("object_budget")).and_then(|budget| budget.get("visible_markers")).and_then(Value::as_u64).unwrap_or(0),
        "max_avatar_route_runners": viewport_lod.and_then(|lod| lod.get("object_budget")).and_then(|budget| budget.get("max_avatar_route_runners")).and_then(Value::as_u64).or_else(|| shell_lod.and_then(|lod| lod.get("max_avatar_route_runners")).and_then(Value::as_u64)).unwrap_or(0),
        "avatar_route_runners": viewport_lod.and_then(|lod| lod.get("object_budget")).and_then(|budget| budget.get("avatar_route_runners")).and_then(Value::as_u64).unwrap_or(0),
        "within_budget": viewport_lod.and_then(|lod| lod.get("object_budget")).and_then(|budget| budget.get("within_budget")).and_then(Value::as_bool).unwrap_or(false),
        "details_default_state": shell_lod.and_then(|lod| lod.get("details_default_state")).and_then(Value::as_str),
        "semantic_role_count": semantic_roles.len(),
        "muted_osm_context_visible": game_layer_semantics.and_then(|semantics| semantics.get("base_map_treatment")).and_then(|base| base.get("openstreetmap_role")).and_then(Value::as_str) == Some("muted_context_layer"),
        "active_route_contrast_visible": game_layer_semantics.and_then(|semantics| semantics.get("active_route_style")).and_then(|style| style.get("class_name")).and_then(Value::as_str) == Some("trillionnium-active-route-line"),
        "runtime_performance_budget_contract_version": runtime_performance_budget.and_then(|budget| budget.get("contract_version")).and_then(Value::as_str),
        "first_map_interactive_target_ms": runtime_performance_budget.and_then(|budget| budget.get("budget_targets")).and_then(|targets| targets.get("first_map_interactive_target_ms")).and_then(Value::as_u64).or_else(|| runtime_performance_budget.and_then(|budget| budget.get("first_map_interactive_target_ms")).and_then(Value::as_u64)).unwrap_or(0),
        "viewport_refresh_p95_target_ms": runtime_performance_budget.and_then(|budget| budget.get("budget_targets")).and_then(|targets| targets.get("viewport_refresh_p95_target_ms")).and_then(Value::as_u64).or_else(|| runtime_performance_budget.and_then(|budget| budget.get("viewport_refresh_p95_target_ms")).and_then(Value::as_u64)).unwrap_or(0),
        "focus_to_action_rail_target_ms": runtime_performance_budget.and_then(|budget| budget.get("budget_targets")).and_then(|targets| targets.get("focus_to_action_rail_target_ms")).and_then(Value::as_u64).or_else(|| runtime_performance_budget.and_then(|budget| budget.get("focus_to_action_rail_target_ms")).and_then(Value::as_u64)).unwrap_or(0),
        "low_end_mobile_fps_floor": runtime_performance_budget.and_then(|budget| budget.get("budget_targets")).and_then(|targets| targets.get("low_end_mobile_fps_floor")).and_then(Value::as_u64).or_else(|| runtime_performance_budget.and_then(|budget| budget.get("low_end_mobile_fps_floor")).and_then(Value::as_u64)).unwrap_or(0),
        "delta_viewport_updates_required": runtime_performance_budget.and_then(|budget| budget.get("degrade_strategy")).and_then(|strategy| strategy.get("delta_viewport_updates_required")).and_then(Value::as_bool).or_else(|| runtime_performance_budget.and_then(|budget| budget.get("delta_viewport_updates_required")).and_then(Value::as_bool)).unwrap_or(false),
        "map_subsystem_contract_version": map_subsystem.and_then(|subsystem| subsystem.get("contract_version")).and_then(Value::as_str),
        "transport_delta_contract_version": transport_delta_contract.and_then(|transport| transport.get("contract_version")).and_then(Value::as_str),
        "presence_delta_visible": transport_delta_contract.and_then(|transport| transport.get("presence_payload")).and_then(|presence| presence.get("presence_delta_required")).and_then(Value::as_bool).unwrap_or(false),
        "transport_boundary_visible": transport_delta_contract.and_then(|transport| transport.get("transport_boundaries")).and_then(Value::as_object).is_some(),
    })
}

fn is_map_readability_lod_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_world_map_readability_lod_gate_v1")
        && gate.get("shell_contract_version").and_then(Value::as_str)
            == Some("trillionnium_world_map_readability_lod_v1")
        && gate
            .get("viewport_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_readability_lod_v1")
        && gate.get("visible_contract_id").and_then(Value::as_str)
            == Some("app-map-readability-lod")
        && gate.get("max_primary_cta_count").and_then(Value::as_u64) == Some(1)
        && gate
            .get("max_summary_chars")
            .and_then(Value::as_u64)
            .is_some_and(|max_chars| max_chars <= 150)
        && gate
            .get("max_visible_markers")
            .and_then(Value::as_u64)
            .is_some_and(|max_markers| max_markers <= 18)
        && gate
            .get("max_avatar_route_runners")
            .and_then(Value::as_u64)
            .is_some_and(|max_runners| max_runners <= 6)
        && gate
            .get("within_budget")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate.get("details_default_state").and_then(Value::as_str) == Some("collapsed")
        && gate
            .get("semantic_layer_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_game_layer_semantics_v1")
        && gate
            .get("viewport_semantic_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_game_layer_semantics_v1")
        && gate
            .get("semantic_role_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 4
        && gate
            .get("muted_osm_context_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("active_route_contrast_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("runtime_performance_budget_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_runtime_performance_budget_v1")
        && gate
            .get("first_map_interactive_target_ms")
            .and_then(Value::as_u64)
            .is_some_and(|value| value <= 2000)
        && gate
            .get("viewport_refresh_p95_target_ms")
            .and_then(Value::as_u64)
            .is_some_and(|value| value <= 250)
        && gate
            .get("focus_to_action_rail_target_ms")
            .and_then(Value::as_u64)
            .is_some_and(|value| value <= 300)
        && gate
            .get("low_end_mobile_fps_floor")
            .and_then(Value::as_u64)
            .is_some_and(|value| value >= 45)
        && gate
            .get("delta_viewport_updates_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("map_subsystem_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_subsystem_v1")
        && gate
            .get("transport_delta_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_transport_delta_v1")
        && gate
            .get("presence_delta_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("transport_boundary_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn app_route_runner_funnel_telemetry_gate_json(app: &Value) -> Value {
    let telemetry = app.get("route_runner_funnel_telemetry").or_else(|| {
        app.get("economy_retention_ops")
            .and_then(|ops| ops.get("route_runner_funnel_telemetry"))
    });
    let event_counts = telemetry.and_then(|telemetry| telemetry.get("event_counts"));
    let time_to_reward = telemetry.and_then(|telemetry| telemetry.get("time_to_reward"));
    let cohort_quality = telemetry.and_then(|telemetry| telemetry.get("cohort_quality"));
    let funnel_integrity = telemetry.and_then(|telemetry| telemetry.get("funnel_integrity"));
    json!({
        "contract_version": "trillionnium_route_runner_funnel_telemetry_gate_v1",
        "telemetry_contract_version": telemetry.and_then(|telemetry| telemetry.get("contract_version")).and_then(Value::as_str),
        "cohort_quality_contract_version": cohort_quality.and_then(|cohort| cohort.get("contract_version")).and_then(Value::as_str),
        "funnel_integrity_contract_version": funnel_integrity.and_then(|integrity| integrity.get("contract_version")).and_then(Value::as_str),
        "telemetry_stream": telemetry.and_then(|telemetry| telemetry.get("telemetry_stream")).and_then(Value::as_str),
        "route_started_count": event_counts.and_then(|counts| counts.get("route_started")).and_then(Value::as_i64).unwrap_or(0),
        "evidence_submitted_count": event_counts.and_then(|counts| counts.get("evidence_submitted")).and_then(Value::as_i64).unwrap_or(0),
        "reward_claimed_count": event_counts.and_then(|counts| counts.get("reward_claimed")).and_then(Value::as_i64).unwrap_or(0),
        "next_route_opened_count": event_counts.and_then(|counts| counts.get("next_route_opened")).and_then(Value::as_i64).unwrap_or(0),
        "abandoned_or_recovery_count": event_counts.and_then(|counts| counts.get("abandoned_or_recovery")).and_then(Value::as_i64).unwrap_or(0),
        "daily_return_resume_count": event_counts.and_then(|counts| counts.get("daily_return_resume")).and_then(Value::as_i64).unwrap_or(0),
        "reward_to_next_route_conversion_percent": cohort_quality.and_then(|cohort| cohort.get("reward_to_next_route_conversion_percent")).and_then(Value::as_i64).unwrap_or(0),
        "d1_resume_rate_percent": cohort_quality.and_then(|cohort| cohort.get("d1_resume_rate_percent")).and_then(Value::as_i64).unwrap_or(0),
        "route_abandon_or_recovery_rate_percent": cohort_quality.and_then(|cohort| cohort.get("route_abandon_or_recovery_rate_percent")).and_then(Value::as_i64).unwrap_or(0),
        "time_to_first_proof_seconds": cohort_quality.and_then(|cohort| cohort.get("time_to_first_proof_seconds")).and_then(Value::as_i64).unwrap_or(0),
        "time_to_next_route_seconds": cohort_quality.and_then(|cohort| cohort.get("time_to_next_route_seconds")).and_then(Value::as_i64).unwrap_or(0),
        "abandon_reason_breakdown_visible": cohort_quality.and_then(|cohort| cohort.get("abandon_reason_breakdown")).and_then(Value::as_object).is_some(),
        "time_to_reward_seconds": time_to_reward.and_then(|time| time.get("p50_seconds")).and_then(Value::as_i64).unwrap_or(0),
        "time_to_reward_target_seconds": time_to_reward.and_then(|time| time.get("target_seconds")).and_then(Value::as_i64).unwrap_or(0),
        "time_to_reward_within_target": time_to_reward.and_then(|time| time.get("within_target")).and_then(Value::as_bool).unwrap_or(false),
        "sample_count": time_to_reward.and_then(|time| time.get("sample_count")).and_then(Value::as_i64).unwrap_or(0),
        "cohort_denominator_consistent": funnel_integrity.and_then(|integrity| integrity.get("cohort_denominator_consistent")).and_then(Value::as_bool).unwrap_or(false),
        "event_dedupe_policy_visible": funnel_integrity.and_then(|integrity| integrity.get("event_dedupe_policy")).and_then(Value::as_object).is_some(),
        "real_user_route_session_count": funnel_integrity.and_then(|integrity| integrity.get("cohort_denominators")).and_then(|denominators| denominators.get("real_user_route_session_count")).and_then(Value::as_i64).unwrap_or(0),
        "duplicate_route_event_count": funnel_integrity.and_then(|integrity| integrity.get("cohort_denominators")).and_then(|denominators| denominators.get("duplicate_route_event_count")).and_then(Value::as_i64).unwrap_or(0),
        "demo_seed_route_event_count": funnel_integrity.and_then(|integrity| integrity.get("cohort_denominators")).and_then(|denominators| denominators.get("demo_seed_route_event_count")).and_then(Value::as_i64).unwrap_or(0),
        "decision_metric_mode": funnel_integrity.and_then(|integrity| integrity.get("decision_metric_mode")).and_then(Value::as_str),
        "demo_seed_policy_visible": funnel_integrity.and_then(|integrity| integrity.get("demo_seed_policy")).and_then(Value::as_str).is_some_and(|policy| !policy.trim().is_empty()),
        "reward_to_next_route_blockers_visible": funnel_integrity.and_then(|integrity| integrity.get("reward_to_next_route_blockers")).and_then(|blockers| blockers.get("blocked_reason_candidates")).and_then(Value::as_array).is_some_and(|reasons| reasons.len() >= 3),
    })
}

fn is_route_runner_funnel_telemetry_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_route_runner_funnel_telemetry_gate_v1")
        && gate
            .get("telemetry_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_route_runner_funnel_telemetry_v1")
        && gate
            .get("cohort_quality_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_route_runner_funnel_cohort_quality_v1")
        && gate
            .get("funnel_integrity_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_route_runner_funnel_integrity_v1")
        && gate.get("telemetry_stream").and_then(Value::as_str)
            == Some("world_economy_events:playability_telemetry")
        && gate
            .get("route_started_count")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("evidence_submitted_count")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("reward_claimed_count")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("next_route_opened_count")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("abandoned_or_recovery_count")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("daily_return_resume_count")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("time_to_reward_seconds")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("time_to_reward_target_seconds")
            .and_then(Value::as_i64)
            == Some(1800)
        && gate
            .get("reward_to_next_route_conversion_percent")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("d1_resume_rate_percent")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("route_abandon_or_recovery_rate_percent")
            .and_then(Value::as_i64)
            .is_some_and(|percent| (0..=100).contains(&percent))
        && gate
            .get("time_to_first_proof_seconds")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("time_to_next_route_seconds")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("abandon_reason_breakdown_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("cohort_denominator_consistent")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("event_dedupe_policy_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate.get("decision_metric_mode").and_then(Value::as_str)
            == Some("bounded_cohort_rates_with_raw_counts_preserved")
        && gate
            .get("demo_seed_policy_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("reward_to_next_route_blockers_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn app_commercial_operating_dashboard_gate_json(app: &Value) -> Value {
    let dashboard = app.get("commercial_operating_dashboard").or_else(|| {
        app.get("economy_retention_ops")
            .and_then(|ops| ops.get("commercial_operating_dashboard"))
    });
    json!({
        "contract_version": "trillionnium_world_commercial_operating_dashboard_gate_v1",
        "dashboard_contract_version": dashboard.and_then(|dashboard| dashboard.get("contract_version")).and_then(Value::as_str),
        "route_start_to_paid_task_conversion_percent": dashboard.and_then(|dashboard| dashboard.get("route_start_to_paid_task_conversion_percent")).and_then(Value::as_i64).unwrap_or(0),
        "reward_claim_to_next_commission_percent": dashboard.and_then(|dashboard| dashboard.get("reward_claim_to_next_commission_percent")).and_then(Value::as_i64).unwrap_or(0),
        "seller_completion_quality_percent": dashboard.and_then(|dashboard| dashboard.get("seller_completion_quality_percent")).and_then(Value::as_i64).unwrap_or(0),
        "buyer_repeat_order_count": dashboard.and_then(|dashboard| dashboard.get("buyer_repeat_order_count")).and_then(Value::as_i64).unwrap_or(0),
        "dispute_refund_reopen_count": dashboard.and_then(|dashboard| dashboard.get("dispute_refund_reopen_count")).and_then(Value::as_i64).unwrap_or(0),
        "route_recommendation_policy_contract_version": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_policy")).and_then(|policy| policy.get("contract_version")).and_then(Value::as_str),
        "route_recommendation_policy_visible": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_policy")).and_then(|policy| policy.get("ranking_weights")).and_then(Value::as_object).is_some(),
        "route_recommendation_quality_contract_version": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("contract_version")).and_then(Value::as_str).or_else(|| dashboard.and_then(|dashboard| dashboard.get("route_recommendation_policy")).and_then(|policy| policy.get("quality_gate")).and_then(|quality| quality.get("contract_version")).and_then(Value::as_str)),
        "route_recommendation_quality_status": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("status")).and_then(Value::as_str),
        "route_recommendation_quality_score_percent": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("quality_score_percent")).and_then(Value::as_i64).unwrap_or(0),
        "route_recommendation_quality_score_target_percent": 60,
        "route_recommendation_quality_score_ready": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("quality_score_percent")).and_then(Value::as_i64).unwrap_or(0) >= 60,
        "route_recommendation_reward_lift_visible": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("reward_to_next_route_lift_percent")).and_then(Value::as_i64).is_some(),
        "route_recommendation_abandon_risk_visible": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("route_abandon_risk_percent")).and_then(Value::as_i64).is_some(),
        "route_recommendation_denominator_consistent": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("denominator_policy")).and_then(|policy| policy.get("cohort_denominator_consistent")).and_then(Value::as_bool).unwrap_or(false),
        "route_recommendation_raw_counts_preserved": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("denominator_policy")).and_then(|policy| policy.get("raw_counts_preserved")).and_then(Value::as_bool).unwrap_or(false),
        "route_recommendation_risk_controls_visible": dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("risk_controls")).and_then(|controls| controls.get("downrank_high_dispute_routes")).and_then(Value::as_bool).unwrap_or(false)
            && dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("risk_controls")).and_then(|controls| controls.get("warn_before_review_hold_routes")).and_then(Value::as_bool).unwrap_or(false)
            && dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("risk_controls")).and_then(|controls| controls.get("abandon_reason_breakdown_visible")).and_then(Value::as_bool).unwrap_or(false)
            && dashboard.and_then(|dashboard| dashboard.get("route_recommendation_quality")).and_then(|quality| quality.get("risk_controls")).and_then(|controls| controls.get("recommendation_not_marker_density_only")).and_then(Value::as_bool).unwrap_or(false),
    })
}

fn is_commercial_operating_dashboard_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_world_commercial_operating_dashboard_gate_v1")
        && gate
            .get("dashboard_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_commercial_operating_dashboard_v1")
        && gate
            .get("route_start_to_paid_task_conversion_percent")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("reward_claim_to_next_commission_percent")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("seller_completion_quality_percent")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("buyer_repeat_order_count")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("dispute_refund_reopen_count")
            .and_then(Value::as_i64)
            .is_some()
        && gate
            .get("route_recommendation_policy_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_route_recommendation_policy_v1")
        && gate
            .get("route_recommendation_policy_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("route_recommendation_quality_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_route_recommendation_quality_v1")
        && gate
            .get("route_recommendation_quality_status")
            .and_then(Value::as_str)
            == Some("quality_gate_ready")
        && gate
            .get("route_recommendation_quality_score_percent")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            >= gate
                .get("route_recommendation_quality_score_target_percent")
                .and_then(Value::as_i64)
                .unwrap_or(60)
        && gate
            .get("route_recommendation_quality_score_ready")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("route_recommendation_reward_lift_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("route_recommendation_abandon_risk_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("route_recommendation_denominator_consistent")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("route_recommendation_raw_counts_preserved")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("route_recommendation_risk_controls_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn app_future_engine_readiness_gate_json(app: &Value) -> Value {
    let renderer_adapter = app
        .get("real_world_map_engine")
        .and_then(|engine| engine.get("renderer_adapter"));
    let future_engine_readiness =
        renderer_adapter.and_then(|adapter| adapter.get("future_engine_readiness"));
    let planned_upgrade_engine = app
        .get("real_world_map_engine")
        .and_then(|engine| engine.get("planned_upgrade_engine"));
    let required_preconditions = future_engine_readiness
        .and_then(|readiness| readiness.get("required_preconditions"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_precondition = |name: &str| required_preconditions.iter().any(|value| value == name);
    json!({
        "contract_version": "trillionnium_world_future_engine_readiness_gate_v1",
        "readiness_contract_version": future_engine_readiness.and_then(|readiness| readiness.get("contract_version")).and_then(Value::as_str),
        "adapter_id": renderer_adapter.and_then(|adapter| adapter.get("adapter_id")).and_then(Value::as_str),
        "active_engine_id": future_engine_readiness.and_then(|readiness| readiness.get("active_engine_id")).and_then(Value::as_str).or_else(|| renderer_adapter.and_then(|adapter| adapter.get("active_engine_id")).and_then(Value::as_str)),
        "candidate_engine_id": future_engine_readiness.and_then(|readiness| readiness.get("candidate_engine_id")).and_then(Value::as_str).or_else(|| renderer_adapter.and_then(|adapter| adapter.get("future_engine_candidate")).and_then(Value::as_str)),
        "runtime_handle_name": renderer_adapter.and_then(|adapter| adapter.get("runtime_handle_name")).and_then(Value::as_str),
        "planned_upgrade_status": planned_upgrade_engine.and_then(|engine| engine.get("status")).and_then(Value::as_str),
        "planned_upgrade_readiness_contract_version": planned_upgrade_engine.and_then(|engine| engine.get("readiness_contract_version")).and_then(Value::as_str),
        "lod_precondition_visible": has_precondition("map_readability_lod_contract_green"),
        "telemetry_precondition_visible": has_precondition("route_runner_funnel_telemetry_green"),
        "cohort_quality_precondition_visible": has_precondition("route_runner_cohort_quality_green"),
        "world_mobile_entry_parity_precondition_visible": has_precondition("world_mobile_entry_parity_green"),
        "semantic_map_layers_precondition_visible": has_precondition("semantic_map_layers_green"),
        "shadow_renderer_precondition_visible": has_precondition("shadow_renderer_contract_green"),
        "shadow_renderer_contract_version": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("contract_version")).and_then(Value::as_str),
        "shadow_renderer_status": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("status")).and_then(Value::as_str),
        "maplibre_shadow_parity_contract_version": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("maplibre_shadow_parity")).and_then(|parity| parity.get("contract_version")).and_then(Value::as_str),
        "maplibre_shadow_only": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("maplibre_shadow_parity")).and_then(|parity| parity.get("rollout_readiness")).and_then(|rollout| rollout.get("shadow_only")).and_then(Value::as_bool).unwrap_or(false),
        "maplibre_shadow_marker_cluster_popup_focus_parity_visible": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("parity_checks")).and_then(Value::as_array).is_some_and(|checks| checks.iter().any(|value| value == "same_marker_cluster_count") && checks.iter().any(|value| value == "same_popup_semantics") && checks.iter().any(|value| value == "same_focus_and_action_behavior")),
        "maplibre_canary_percent": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("maplibre_shadow_parity")).and_then(|parity| parity.get("rollout_readiness")).and_then(|rollout| rollout.get("canary_percent")).and_then(Value::as_i64).unwrap_or(100),
        "maplibre_canary_starts_at_zero": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("maplibre_shadow_parity")).and_then(|parity| parity.get("rollout_readiness")).and_then(|rollout| rollout.get("canary_percent")).and_then(Value::as_i64).unwrap_or(100) == 0,
        "maplibre_max_canary_percent_without_new_signoff": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("maplibre_shadow_parity")).and_then(|parity| parity.get("rollout_readiness")).and_then(|rollout| rollout.get("max_canary_percent_without_new_signoff")).and_then(Value::as_i64).unwrap_or(100),
        "maplibre_rollback_drill_evidence_required": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("maplibre_shadow_parity")).and_then(|parity| parity.get("rollout_readiness")).and_then(|rollout| rollout.get("rollback_drill_evidence_required")).and_then(Value::as_bool).unwrap_or(false),
        "maplibre_canary_rollback_drill_visible": future_engine_readiness.and_then(|readiness| readiness.get("shadow_renderer_contract")).and_then(|shadow| shadow.get("canary_policy")).and_then(|policy| policy.get("rollback_drill_required")).and_then(Value::as_bool).unwrap_or(false),
        "rollback_plan_visible": future_engine_readiness.and_then(|readiness| readiness.get("rollback_plan")).and_then(|plan| plan.get("candidate_is_shadow_only")).and_then(Value::as_bool).unwrap_or(false),
        "promotion_blocker_count": future_engine_readiness.and_then(|readiness| readiness.get("promotion_blockers")).and_then(Value::as_array).map(Vec::len).unwrap_or(0),
    })
}

fn is_future_engine_readiness_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_world_future_engine_readiness_gate_v1")
        && gate
            .get("readiness_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_future_engine_readiness_v1")
        && gate.get("adapter_id").and_then(Value::as_str) == Some("leaflet_renderer_adapter_v1")
        && gate.get("active_engine_id").and_then(Value::as_str) == Some("leaflet_openstreetmap_v1")
        && gate.get("candidate_engine_id").and_then(Value::as_str) == Some("maplibre_gl_v1")
        && gate.get("runtime_handle_name").and_then(Value::as_str) == Some("mapRuntime")
        && gate.get("planned_upgrade_status").and_then(Value::as_str) == Some("planned_not_active")
        && gate
            .get("planned_upgrade_readiness_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_future_engine_readiness_v1")
        && gate
            .get("lod_precondition_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("telemetry_precondition_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("cohort_quality_precondition_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("world_mobile_entry_parity_precondition_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("semantic_map_layers_precondition_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("shadow_renderer_precondition_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("shadow_renderer_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_renderer_shadow_v1")
        && gate.get("shadow_renderer_status").and_then(Value::as_str)
            == Some("shadow_only_not_user_facing")
        && gate
            .get("maplibre_shadow_parity_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_maplibre_shadow_parity_v1")
        && gate
            .get("maplibre_shadow_only")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("maplibre_shadow_marker_cluster_popup_focus_parity_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("maplibre_canary_percent")
            .and_then(Value::as_i64)
            .unwrap_or(100)
            == 0
        && gate
            .get("maplibre_canary_starts_at_zero")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("maplibre_max_canary_percent_without_new_signoff")
            .and_then(Value::as_i64)
            .unwrap_or(100)
            <= 1
        && gate
            .get("maplibre_rollback_drill_evidence_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("maplibre_canary_rollback_drill_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("rollback_plan_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("promotion_blocker_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 3
}

fn app_openstreetmap_provider_readiness_gate_json(app: &Value) -> Value {
    let geodata = app
        .get("map")
        .and_then(|map| map.get("openstreetmap_geodata"));
    let readiness = geodata.and_then(|geodata| geodata.get("provider_readiness"));
    let live_network_ingestion_enabled = readiness
        .and_then(|readiness| readiness.get("live_network_ingestion_enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let production_ingestion_enabled = readiness
        .and_then(|readiness| readiness.get("production_ingestion_enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    json!({
        "contract_version": "trillionnium_openstreetmap_provider_readiness_gate_v1",
        "geodata_contract_version": geodata.and_then(|geodata| geodata.get("contract_version")).and_then(Value::as_str),
        "provider_contract": geodata.and_then(|geodata| geodata.get("provider_contract")).and_then(Value::as_str),
        "provider_id": geodata.and_then(|geodata| geodata.get("provider_id")).and_then(Value::as_str),
        "provider_mode": geodata.and_then(|geodata| geodata.get("provider_mode")).and_then(Value::as_str),
        "provider_mode_contract_version": geodata.and_then(|geodata| geodata.get("provider_mode_contract_version")).and_then(Value::as_str),
        "readiness_contract_version": readiness.and_then(|readiness| readiness.get("contract_version")).and_then(Value::as_str),
        "readiness_status": readiness.and_then(|readiness| readiness.get("readiness_status")).and_then(Value::as_str),
        "source_of_truth": readiness.and_then(|readiness| readiness.get("source_of_truth")).and_then(Value::as_str).or_else(|| geodata.and_then(|geodata| geodata.get("source_of_truth")).and_then(Value::as_str)),
        "web_role": readiness.and_then(|readiness| readiness.get("web_role")).and_then(Value::as_str).or_else(|| geodata.and_then(|geodata| geodata.get("web_role")).and_then(Value::as_str)),
        "fixture_mode_green": readiness.and_then(|readiness| readiness.get("fixture_mode_green")).and_then(Value::as_bool).unwrap_or(false),
        "fixture_provider_enabled": readiness.and_then(|readiness| readiness.get("fixture_provider_enabled")).and_then(Value::as_bool).unwrap_or(false),
        "fixture_network_ingestion_enabled": readiness.and_then(|readiness| readiness.get("fixture_network_ingestion_enabled")).and_then(Value::as_bool).unwrap_or(true),
        "stable_fixture_identity_coverage_complete": readiness.and_then(|readiness| readiness.get("stable_fixture_identity_coverage_complete")).and_then(Value::as_bool).unwrap_or(false),
        "fixture_node_count": readiness.and_then(|readiness| readiness.get("fixture_node_count")).and_then(Value::as_u64).unwrap_or(0),
        "fixture_layer_feature_count": readiness.and_then(|readiness| readiness.get("fixture_layer_feature_count")).and_then(Value::as_u64).unwrap_or(0),
        "live_modes_fail_closed": readiness.and_then(|readiness| readiness.get("live_modes_fail_closed")).and_then(Value::as_bool).unwrap_or(false),
        "live_network_ingestion_enabled": live_network_ingestion_enabled,
        "network_ingestion_disabled": !live_network_ingestion_enabled,
        "production_ingestion_enabled": production_ingestion_enabled,
        "production_ingestion_disabled": !production_ingestion_enabled,
        "provider_modes_observable": readiness.and_then(|readiness| readiness.get("provider_modes_observable")).and_then(Value::as_bool).unwrap_or(false),
        "fail_closed_mode_count": readiness.and_then(|readiness| readiness.get("fail_closed_mode_count")).and_then(Value::as_u64).unwrap_or(0),
        "expected_fail_closed_mode_count": readiness.and_then(|readiness| readiness.get("expected_fail_closed_mode_count")).and_then(Value::as_u64).unwrap_or(0),
        "overpass_bbox_cache_fail_closed": readiness.and_then(|readiness| readiness.get("overpass_bbox_cache_fail_closed")).and_then(Value::as_bool).unwrap_or(false),
        "geofabrik_extract_import_fail_closed": readiness.and_then(|readiness| readiness.get("geofabrik_extract_import_fail_closed")).and_then(Value::as_bool).unwrap_or(false),
        "vendor_tile_cache_fail_closed": readiness.and_then(|readiness| readiness.get("vendor_tile_cache_fail_closed")).and_then(Value::as_bool).unwrap_or(false),
        "unknown_mode_fail_closed": readiness.and_then(|readiness| readiness.get("unknown_mode_fail_closed")).and_then(Value::as_bool).unwrap_or(false),
        "public_tile_server_production_traffic_allowed": readiness.and_then(|readiness| readiness.get("public_tile_server_production_traffic_allowed")).and_then(Value::as_bool).unwrap_or(true),
        "odbl_tracking_required_before_live": readiness.and_then(|readiness| readiness.get("odbl_tracking_required_before_live")).and_then(Value::as_bool).unwrap_or(false),
        "derived_database_metadata_required_before_live": readiness.and_then(|readiness| readiness.get("derived_database_metadata_required_before_live")).and_then(Value::as_bool).unwrap_or(false),
        "readiness_green": readiness.and_then(|readiness| readiness.get("green")).and_then(Value::as_bool).unwrap_or(false),
    })
}

fn is_openstreetmap_provider_readiness_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_openstreetmap_provider_readiness_gate_v1")
        && gate.get("geodata_contract_version").and_then(Value::as_str)
            == Some("openstreetmap_geodata_v1")
        && gate.get("provider_contract").and_then(Value::as_str)
            == Some("OpenStreetMapDataProvider")
        && gate.get("provider_mode").and_then(Value::as_str) == Some("fixture")
        && gate
            .get("provider_mode_contract_version")
            .and_then(Value::as_str)
            == Some("openstreetmap_provider_mode_v1")
        && gate
            .get("readiness_contract_version")
            .and_then(Value::as_str)
            == Some("openstreetmap_provider_readiness_v1")
        && gate.get("readiness_status").and_then(Value::as_str)
            == Some("fixture_ready_live_fail_closed")
        && gate.get("source_of_truth").and_then(Value::as_str)
            == Some("rust_openstreetmap_data_provider")
        && gate.get("web_role").and_then(Value::as_str) == Some("visualization_input_only")
        && gate
            .get("fixture_mode_green")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("fixture_provider_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !gate
            .get("fixture_network_ingestion_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        && gate
            .get("stable_fixture_identity_coverage_complete")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("fixture_node_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("fixture_layer_feature_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("live_modes_fail_closed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("network_ingestion_disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("production_ingestion_disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("provider_modes_observable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("fail_closed_mode_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= gate
                .get("expected_fail_closed_mode_count")
                .and_then(Value::as_u64)
                .unwrap_or(4)
        && gate
            .get("expected_fail_closed_mode_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 4
        && gate
            .get("overpass_bbox_cache_fail_closed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("geofabrik_extract_import_fail_closed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("vendor_tile_cache_fail_closed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("unknown_mode_fail_closed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !gate
            .get("public_tile_server_production_traffic_allowed")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        && gate
            .get("odbl_tracking_required_before_live")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("derived_database_metadata_required_before_live")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("readiness_green")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn app_openstreetmap_geodata_freshness_gate_json(app: &Value) -> Value {
    let geodata = app
        .get("map")
        .and_then(|map| map.get("openstreetmap_geodata"));
    let freshness = geodata.and_then(|geodata| geodata.get("freshness"));
    let live_ingestion_enabled = freshness
        .and_then(|freshness| freshness.get("live_ingestion_enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    json!({
        "contract_version": "trillionnium_openstreetmap_geodata_freshness_gate_v1",
        "geodata_contract_version": geodata.and_then(|geodata| geodata.get("contract_version")).and_then(Value::as_str),
        "freshness_contract_version": freshness.and_then(|freshness| freshness.get("contract_version")).and_then(Value::as_str),
        "provider_contract": geodata.and_then(|geodata| geodata.get("provider_contract")).and_then(Value::as_str),
        "provider_mode": geodata.and_then(|geodata| geodata.get("provider_mode")).and_then(Value::as_str),
        "freshness_status": freshness.and_then(|freshness| freshness.get("freshness_status")).and_then(Value::as_str),
        "source_of_truth": freshness.and_then(|freshness| freshness.get("source_of_truth")).and_then(Value::as_str).or_else(|| geodata.and_then(|geodata| geodata.get("source_of_truth")).and_then(Value::as_str)),
        "web_role": freshness.and_then(|freshness| freshness.get("web_role")).and_then(Value::as_str).or_else(|| geodata.and_then(|geodata| geodata.get("web_role")).and_then(Value::as_str)),
        "freshness_metric_mode": freshness.and_then(|freshness| freshness.get("freshness_metric_mode")).and_then(Value::as_str),
        "fixture_static_snapshot": freshness.and_then(|freshness| freshness.get("fixture_static_snapshot")).and_then(Value::as_bool).unwrap_or(false),
        "wall_clock_freshness_applies": freshness.and_then(|freshness| freshness.get("wall_clock_freshness_applies")).and_then(Value::as_bool).unwrap_or(true),
        "live_data_freshness_applies": freshness.and_then(|freshness| freshness.get("live_data_freshness_applies")).and_then(Value::as_bool).unwrap_or(true),
        "fixture_snapshot_age_seconds": freshness.and_then(|freshness| freshness.get("fixture_snapshot_age_seconds")).and_then(Value::as_u64).unwrap_or(u64::MAX),
        "fixture_snapshot_max_age_seconds": freshness.and_then(|freshness| freshness.get("fixture_snapshot_max_age_seconds")).and_then(Value::as_u64).unwrap_or(0),
        "fixture_snapshot_age_within_policy": freshness.and_then(|freshness| freshness.get("fixture_snapshot_age_within_policy")).and_then(Value::as_bool).unwrap_or(false),
        "live_ingestion_enabled": live_ingestion_enabled,
        "live_ingestion_disabled": !live_ingestion_enabled,
        "live_snapshot_age_unknown_blocked": freshness.and_then(|freshness| freshness.get("live_snapshot_age_unknown_blocked")).and_then(Value::as_bool).unwrap_or(false),
        "staleness_alarm_active": freshness.and_then(|freshness| freshness.get("staleness_alarm_active")).and_then(Value::as_bool).unwrap_or(true),
        "stale_live_ingestion_blocked": freshness.and_then(|freshness| freshness.get("stale_live_ingestion_blocked")).and_then(Value::as_bool).unwrap_or(false),
        "requires_fresh_import_before_live": freshness.and_then(|freshness| freshness.get("requires_fresh_import_before_live")).and_then(Value::as_bool).unwrap_or(false),
        "freshness_tracking_required_before_live": freshness.and_then(|freshness| freshness.get("freshness_tracking_required_before_live")).and_then(Value::as_bool).unwrap_or(false),
        "derived_database_metadata_contract_version": freshness.and_then(|freshness| freshness.get("derived_database_metadata_contract_version")).and_then(Value::as_str),
        "derived_database_snapshot_id": freshness.and_then(|freshness| freshness.get("derived_database_snapshot_id")).and_then(Value::as_str),
        "node_feature_count": freshness.and_then(|freshness| freshness.get("node_feature_count")).and_then(Value::as_u64).unwrap_or(0),
        "layer_feature_count": freshness.and_then(|freshness| freshness.get("layer_feature_count")).and_then(Value::as_u64).unwrap_or(0),
        "odbl_tracking_visible": freshness.and_then(|freshness| freshness.get("odbl_tracking_visible")).and_then(Value::as_bool).unwrap_or(false),
        "public_tile_server_production_traffic_allowed": freshness.and_then(|freshness| freshness.get("public_tile_server_production_traffic_allowed")).and_then(Value::as_bool).unwrap_or(true),
        "freshness_green": freshness.and_then(|freshness| freshness.get("green")).and_then(Value::as_bool).unwrap_or(false),
    })
}

fn is_openstreetmap_geodata_freshness_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_openstreetmap_geodata_freshness_gate_v1")
        && gate.get("geodata_contract_version").and_then(Value::as_str)
            == Some("openstreetmap_geodata_v1")
        && gate
            .get("freshness_contract_version")
            .and_then(Value::as_str)
            == Some("openstreetmap_geodata_freshness_v1")
        && gate.get("provider_contract").and_then(Value::as_str)
            == Some("OpenStreetMapDataProvider")
        && gate.get("provider_mode").and_then(Value::as_str) == Some("fixture")
        && gate.get("freshness_status").and_then(Value::as_str)
            == Some("fixture_static_fresh_live_stale_blocked")
        && gate.get("source_of_truth").and_then(Value::as_str)
            == Some("rust_openstreetmap_data_provider")
        && gate.get("web_role").and_then(Value::as_str) == Some("visualization_input_only")
        && gate.get("freshness_metric_mode").and_then(Value::as_str)
            == Some("static_fixture_no_wall_clock_decay")
        && gate
            .get("fixture_static_snapshot")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !gate
            .get("wall_clock_freshness_applies")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        && !gate
            .get("live_data_freshness_applies")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        && gate
            .get("fixture_snapshot_age_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX)
            == 0
        && gate
            .get("fixture_snapshot_age_within_policy")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("live_ingestion_disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("live_snapshot_age_unknown_blocked")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !gate
            .get("staleness_alarm_active")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        && gate
            .get("stale_live_ingestion_blocked")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("requires_fresh_import_before_live")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("freshness_tracking_required_before_live")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("derived_database_metadata_contract_version")
            .and_then(Value::as_str)
            == Some("openstreetmap_derived_database_metadata_v1")
        && gate
            .get("derived_database_snapshot_id")
            .and_then(Value::as_str)
            .map(|id| id.starts_with("osm-fixture-v1-"))
            .unwrap_or(false)
        && gate
            .get("node_feature_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("layer_feature_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && gate
            .get("odbl_tracking_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !gate
            .get("public_tile_server_production_traffic_allowed")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        && gate
            .get("freshness_green")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn app_openstreetmap_attribution_presence_gate_json(app: &Value) -> Value {
    let geodata = app
        .get("map")
        .and_then(|map| map.get("openstreetmap_geodata"));
    let attribution = geodata.and_then(|geodata| geodata.get("attribution_presence"));
    let presence_checks = attribution
        .and_then(|attribution| attribution.get("presence_checks"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_presence_check = |needle: &str| {
        presence_checks
            .iter()
            .any(|check| check.as_str() == Some(needle))
    };
    let attribution_required = attribution
        .and_then(|attribution| attribution.get("attribution_required"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let attribution_visible_required = attribution
        .and_then(|attribution| attribution.get("attribution_visible_required"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let derived_database_tracking_required = attribution
        .and_then(|attribution| attribution.get("derived_database_tracking_required"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let odbl_database_obligations = attribution
        .and_then(|attribution| attribution.get("odbl_database_obligations"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let attribution_presence_green = attribution_required
        && attribution_visible_required
        && derived_database_tracking_required
        && odbl_database_obligations
        && has_presence_check("app_shell_static_attribution_node_present")
        && has_presence_check("world_shell_static_attribution_node_present")
        && has_presence_check("leaflet_runtime_attribution_configured")
        && has_presence_check("odbl_database_obligations_visible");
    json!({
        "contract_version": "trillionnium_openstreetmap_attribution_presence_gate_v1",
        "geodata_contract_version": geodata.and_then(|geodata| geodata.get("contract_version")).and_then(Value::as_str),
        "attribution_presence_contract_version": attribution.and_then(|attribution| attribution.get("contract_version")).and_then(Value::as_str),
        "provider_contract": geodata.and_then(|geodata| geodata.get("provider_contract")).and_then(Value::as_str),
        "provider_mode": geodata.and_then(|geodata| geodata.get("provider_mode")).and_then(Value::as_str),
        "source_of_truth": attribution.and_then(|attribution| attribution.get("source_of_truth")).and_then(Value::as_str).or_else(|| geodata.and_then(|geodata| geodata.get("source_of_truth")).and_then(Value::as_str)),
        "web_role": attribution.and_then(|attribution| attribution.get("web_role")).and_then(Value::as_str).or_else(|| geodata.and_then(|geodata| geodata.get("web_role")).and_then(Value::as_str)),
        "attribution": attribution.and_then(|attribution| attribution.get("attribution")).and_then(Value::as_str),
        "database_license": attribution.and_then(|attribution| attribution.get("database_license")).and_then(Value::as_str),
        "attribution_required": attribution_required,
        "attribution_visible_required": attribution_visible_required,
        "derived_database_tracking_required": derived_database_tracking_required,
        "odbl_database_obligations": odbl_database_obligations,
        "public_tile_server_policy": attribution.and_then(|attribution| attribution.get("public_tile_server_policy")).and_then(Value::as_str),
        "live_ingestion_blocked_until_attribution_manifest": attribution.and_then(|attribution| attribution.get("live_ingestion_blocked_until_attribution_manifest")).and_then(Value::as_bool).unwrap_or(false),
        "app_shell_static_attribution_node_present": has_presence_check("app_shell_static_attribution_node_present"),
        "world_shell_static_attribution_node_present": has_presence_check("world_shell_static_attribution_node_present"),
        "leaflet_runtime_attribution_configured": has_presence_check("leaflet_runtime_attribution_configured"),
        "odbl_database_obligations_visible": has_presence_check("odbl_database_obligations_visible"),
        "presence_check_count": presence_checks.len(),
        "attribution_presence_green": attribution_presence_green,
    })
}

fn is_openstreetmap_attribution_presence_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_openstreetmap_attribution_presence_gate_v1")
        && gate.get("geodata_contract_version").and_then(Value::as_str)
            == Some("openstreetmap_geodata_v1")
        && gate
            .get("attribution_presence_contract_version")
            .and_then(Value::as_str)
            == Some("openstreetmap_attribution_presence_v1")
        && gate.get("provider_contract").and_then(Value::as_str)
            == Some("OpenStreetMapDataProvider")
        && gate.get("provider_mode").and_then(Value::as_str) == Some("fixture")
        && gate.get("source_of_truth").and_then(Value::as_str)
            == Some("rust_openstreetmap_data_provider")
        && gate.get("web_role").and_then(Value::as_str) == Some("visualization_input_only")
        && gate.get("attribution").and_then(Value::as_str) == Some("© OpenStreetMap contributors")
        && gate.get("database_license").and_then(Value::as_str) == Some("ODbL-1.0")
        && gate
            .get("attribution_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("attribution_visible_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("derived_database_tracking_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("odbl_database_obligations")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("public_tile_server_policy")
            .and_then(Value::as_str)
            == Some("cache_or_self_host_required_before_production_traffic")
        && gate
            .get("live_ingestion_blocked_until_attribution_manifest")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("app_shell_static_attribution_node_present")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("world_shell_static_attribution_node_present")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("leaflet_runtime_attribution_configured")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("odbl_database_obligations_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("presence_check_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 4
        && gate
            .get("attribution_presence_green")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn app_world_map_runtime_safety_gate_json(app: &Value) -> Value {
    let mobile_contract = app.get("mobile_shell_contract");
    let viewport = app.get("map_hub").and_then(|hub| hub.get("viewport"));
    let viewport_api = viewport.and_then(|viewport| viewport.get("viewport_api"));
    let transport_delta_contract =
        viewport.and_then(|viewport| viewport.get("transport_delta_contract"));
    let runtime_performance_budget = viewport
        .and_then(|viewport| viewport.get("runtime_performance_budget"))
        .or_else(|| {
            viewport
                .and_then(|viewport| viewport.get("map_readability_lod"))
                .and_then(|lod| lod.get("runtime_performance_budget"))
        });
    let rum_slo_contract = viewport
        .and_then(|viewport| viewport.get("rum_slo_contract"))
        .or_else(|| mobile_contract.and_then(|contract| contract.get("rum_slo_contract")));
    let rum_sample_matrix = rum_slo_contract.and_then(|contract| contract.get("sample_matrix"));
    let weak_network_contract = viewport
        .and_then(|viewport| viewport.get("weak_network_resilience"))
        .or_else(|| mobile_contract.and_then(|contract| contract.get("weak_network_resilience")));
    let offline_action_queue =
        weak_network_contract.and_then(|contract| contract.get("offline_action_queue"));
    let location_privacy_contract = viewport
        .and_then(|viewport| viewport.get("location_privacy_contract"))
        .or_else(|| mobile_contract.and_then(|contract| contract.get("location_privacy_contract")));
    let density_scalability =
        runtime_performance_budget.and_then(|budget| budget.get("density_scalability"));
    let gameplay_accessibility = viewport
        .and_then(|viewport| viewport.get("gameplay_accessibility_i18n"))
        .or_else(|| {
            viewport
                .and_then(|viewport| viewport.get("gameplay_layer_contract"))
                .and_then(|contract| contract.get("accessibility_i18n"))
        });
    json!({
        "contract_version": "trillionnium_world_map_runtime_safety_gate_v1",
        "rum_slo_contract_version": rum_slo_contract.and_then(|contract| contract.get("contract_version")).and_then(Value::as_str),
        "rum_slo_quantiles_visible": rum_slo_contract.and_then(|contract| contract.get("required_dimensions")).and_then(|dimensions| dimensions.get("quantiles")).and_then(Value::as_array).is_some_and(|quantiles| quantiles.iter().any(|value| value == "p50") && quantiles.iter().any(|value| value == "p95") && quantiles.iter().any(|value| value == "p99")),
        "rum_slo_surface_split_visible": rum_slo_contract.and_then(|contract| contract.get("required_dimensions")).and_then(|dimensions| dimensions.get("surfaces")).and_then(Value::as_array).is_some_and(|surfaces| surfaces.iter().any(|value| value == "app") && surfaces.iter().any(|value| value == "world")),
        "rum_slo_device_split_visible": rum_slo_contract.and_then(|contract| contract.get("required_dimensions")).and_then(|dimensions| dimensions.get("device_classes")).and_then(Value::as_array).is_some_and(|devices| devices.iter().any(|value| value == "mobile") && devices.iter().any(|value| value == "desktop")),
        "rum_sample_matrix_contract_version": rum_sample_matrix.and_then(|matrix| matrix.get("contract_version")).and_then(Value::as_str),
        "rum_sample_matrix_per_bucket_min_samples": rum_sample_matrix.and_then(|matrix| matrix.get("per_bucket_min_samples")).and_then(Value::as_i64).unwrap_or(0),
        "rum_sample_matrix_cache_network_kinds_visible": rum_sample_matrix.and_then(|matrix| matrix.get("required_sample_kinds")).and_then(Value::as_array).is_some_and(|kinds| kinds.iter().any(|value| value == "cold_cache_interactive") && kinds.iter().any(|value| value == "warm_delta_or_304") && kinds.iter().any(|value| value == "weak_network_cached_snapshot")),
        "weak_network_contract_version": weak_network_contract.and_then(|contract| contract.get("contract_version")).and_then(Value::as_str),
        "weak_network_cached_snapshot_visible": weak_network_contract.and_then(|contract| contract.get("strategy")).and_then(|strategy| strategy.get("local_cached_snapshot_after_network_error")).and_then(Value::as_bool).unwrap_or(false),
        "weak_network_delta_first_visible": weak_network_contract.and_then(|contract| contract.get("strategy")).and_then(|strategy| strategy.get("delta_first")).and_then(Value::as_bool).unwrap_or(false),
        "weak_network_snapshot_fallback_visible": weak_network_contract.and_then(|contract| contract.get("strategy")).and_then(|strategy| strategy.get("snapshot_fallback_after_delta_error")).and_then(Value::as_bool).unwrap_or(false),
        "offline_action_queue_contract_version": offline_action_queue.and_then(|queue| queue.get("contract_version")).and_then(Value::as_str),
        "offline_banner_visible": offline_action_queue.and_then(|queue| queue.get("offline_banner")).and_then(|banner| banner.get("aria_live")).and_then(Value::as_str) == Some("polite"),
        "pending_action_queue_visible": offline_action_queue.and_then(|queue| queue.get("pending_action_queue")).and_then(|pending| pending.get("sync_on_reconnect_required")).and_then(Value::as_bool).unwrap_or(false),
        "conflict_sync_recovery_visible": offline_action_queue.and_then(|queue| queue.get("conflict_recovery")).and_then(|conflict| conflict.get("conflict_banner_required")).and_then(Value::as_bool).unwrap_or(false),
        "location_privacy_contract_version": location_privacy_contract.and_then(|contract| contract.get("contract_version")).and_then(Value::as_str),
        "rum_excludes_lat_lng": location_privacy_contract.and_then(|contract| contract.get("rules")).and_then(|rules| rules.get("rum_payload_excludes_lat_lng")).and_then(Value::as_bool).unwrap_or(false),
        "personalized_map_cache_private": location_privacy_contract.and_then(|contract| contract.get("rules")).and_then(|rules| rules.get("viewport_cache_control")).and_then(Value::as_str) == Some("private") && location_privacy_contract.and_then(|contract| contract.get("rules")).and_then(|rules| rules.get("delta_cache_control")).and_then(Value::as_str) == Some("private"),
        "viewport_api_304_supported": viewport_api.and_then(|api| api.get("not_modified_304_supported")).and_then(Value::as_bool).or_else(|| transport_delta_contract.and_then(|contract| contract.get("entity_delta_cache")).and_then(|cache| cache.get("not_modified_304_compatible")).and_then(Value::as_bool)).unwrap_or(false),
        "entity_delta_cache_contract": viewport_api.and_then(|api| api.get("entity_delta_cache_contract")).and_then(Value::as_str).or_else(|| transport_delta_contract.and_then(|contract| contract.get("entity_delta_cache")).and_then(|cache| cache.get("mode")).and_then(Value::as_str)),
        "changed_group_rendering_required": transport_delta_contract.and_then(|contract| contract.get("entity_delta_cache")).and_then(|cache| cache.get("changed_group_rendering_required")).and_then(Value::as_bool).unwrap_or(false),
        "visible_marker_delta_required": transport_delta_contract.and_then(|contract| contract.get("entity_delta_cache")).and_then(|cache| cache.get("visible_marker_delta_required")).and_then(Value::as_bool).unwrap_or(false),
        "marker_cluster_delta_required": transport_delta_contract.and_then(|contract| contract.get("entity_delta_cache")).and_then(|cache| cache.get("marker_cluster_delta_required")).and_then(Value::as_bool).unwrap_or(false),
        "viewport_request_abort_visible": runtime_performance_budget.and_then(|budget| budget.get("degrade_strategy")).and_then(|strategy| strategy.get("abort_previous_viewport_request")).and_then(Value::as_bool).unwrap_or(false),
        "deferred_card_render_visible": runtime_performance_budget.and_then(|budget| budget.get("degrade_strategy")).and_then(|strategy| strategy.get("defer_noncritical_card_render")).and_then(Value::as_bool).unwrap_or(false),
        "marker_cluster_policy_visible": runtime_performance_budget.and_then(|budget| budget.get("degrade_strategy")).and_then(|strategy| strategy.get("cluster_markers_before_hiding")).and_then(Value::as_bool).unwrap_or(false),
        "density_scalability_contract_version": density_scalability.and_then(|density| density.get("contract_version")).and_then(Value::as_str),
        "projection_cache_strategy_visible": density_scalability.and_then(|density| density.get("backend_projection")).and_then(|backend| backend.get("spatial_tile_cache_required")).and_then(Value::as_bool).unwrap_or(false) && density_scalability.and_then(|density| density.get("backend_projection")).and_then(|backend| backend.get("server_timing_header_required")).and_then(Value::as_bool).unwrap_or(false),
        "frontend_virtualization_visible": density_scalability.and_then(|density| density.get("frontend_virtualization")).and_then(|frontend| frontend.get("virtualize_dense_cards_required")).and_then(Value::as_bool).unwrap_or(false),
        "adaptive_density_scheduler_visible": density_scalability.and_then(|density| density.get("adaptive_density_scheduler")).and_then(|scheduler| scheduler.get("actions")).and_then(Value::as_array).is_some_and(|actions| actions.iter().any(|value| value == "cluster_markers") && actions.iter().any(|value| value == "defer_cards")),
        "gameplay_accessibility_contract_version": gameplay_accessibility.and_then(|contract| contract.get("contract_version")).and_then(Value::as_str),
        "screen_reader_reduced_motion_touch_targets_visible": gameplay_accessibility.and_then(|contract| contract.get("accessibility")).and_then(|a11y| a11y.get("screen_reader_labels_required")).and_then(Value::as_bool).unwrap_or(false) && gameplay_accessibility.and_then(|contract| contract.get("accessibility")).and_then(|a11y| a11y.get("reduced_motion_required")).and_then(Value::as_bool).unwrap_or(false) && gameplay_accessibility.and_then(|contract| contract.get("accessibility")).and_then(|a11y| a11y.get("touch_target_min_px")).and_then(Value::as_i64).unwrap_or(0) >= 44,
    })
}

fn is_world_map_runtime_safety_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_world_map_runtime_safety_gate_v1")
        && gate.get("rum_slo_contract_version").and_then(Value::as_str)
            == Some("trillionnium_world_map_rum_slo_v1")
        && gate
            .get("rum_slo_quantiles_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("rum_slo_surface_split_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("rum_slo_device_split_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("rum_sample_matrix_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_real_user_rum_matrix_v1")
        && gate
            .get("rum_sample_matrix_per_bucket_min_samples")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            >= 1
        && gate
            .get("rum_sample_matrix_cache_network_kinds_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("weak_network_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_weak_network_resilience_v1")
        && gate
            .get("weak_network_cached_snapshot_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("weak_network_delta_first_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("weak_network_snapshot_fallback_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("offline_action_queue_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_offline_action_queue_v1")
        && gate
            .get("offline_banner_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("pending_action_queue_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("conflict_sync_recovery_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("location_privacy_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_location_privacy_v1")
        && gate
            .get("rum_excludes_lat_lng")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("personalized_map_cache_private")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("viewport_api_304_supported")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("entity_delta_cache_contract")
            .and_then(Value::as_str)
            == Some("entity_group_versioned_delta_v1")
        && gate
            .get("changed_group_rendering_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("visible_marker_delta_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("marker_cluster_delta_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("viewport_request_abort_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("deferred_card_render_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("marker_cluster_policy_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("density_scalability_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_density_scalability_v1")
        && gate
            .get("projection_cache_strategy_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("frontend_virtualization_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("adaptive_density_scheduler_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("gameplay_accessibility_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_gameplay_accessibility_i18n_v1")
        && gate
            .get("screen_reader_reduced_motion_touch_targets_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn world_map_rum_slo_gate_from_metrics(metrics_snapshot: &Value) -> Value {
    metrics_snapshot
        .get("world_map_rum")
        .and_then(|rum| rum.get("slo_gate"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "contract_version": "trillionnium_world_map_rum_slo_v1",
                "sample_count": 0,
                "green": true,
                "status": "no_samples_yet_contract_ready"
            })
        })
}

fn is_world_map_rum_slo_metrics_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_world_map_rum_slo_v1")
        && gate.get("green").and_then(Value::as_bool).unwrap_or(false)
        && gate
            .get("split_by_surface")
            .and_then(Value::as_array)
            .is_none_or(|surfaces| {
                surfaces.iter().any(|value| value == "app")
                    && surfaces.iter().any(|value| value == "world")
            })
        && gate
            .get("split_by_device_class")
            .and_then(Value::as_array)
            .is_none_or(|devices| {
                devices.iter().any(|value| value == "mobile")
                    && devices.iter().any(|value| value == "desktop")
            })
        && gate
            .get("split_by_sample_kind")
            .and_then(Value::as_array)
            .is_none_or(|kinds| {
                kinds.iter().any(|value| value == "cold_cache_interactive")
                    && kinds.iter().any(|value| value == "warm_delta_or_304")
                    && kinds
                        .iter()
                        .any(|value| value == "weak_network_cached_snapshot")
            })
        && gate
            .get("sample_matrix_contract_version")
            .and_then(Value::as_str)
            .is_none_or(|value| value == "trillionnium_world_map_real_user_rum_matrix_v1")
        && gate
            .get("per_bucket_min_samples")
            .and_then(Value::as_u64)
            .is_none_or(|value| value >= 1)
        && gate
            .get("sample_matrix_missing_bucket_count")
            .and_then(Value::as_u64)
            .is_none_or(|missing| {
                gate.get("enforcement_status").and_then(Value::as_str)
                    == Some("warming_until_min_samples")
                    || missing == 0
            })
}

fn world_map_delta_cache_gate_from_metrics(metrics_snapshot: &Value) -> Value {
    let delta = metrics_snapshot
        .get("world_map_delta")
        .cloned()
        .unwrap_or_else(|| json!({}));
    json!({
        "contract_version": "trillionnium_world_map_delta_cache_gate_v1",
        "transport_delta_contract_version": delta.get("contract_version").and_then(Value::as_str),
        "entity_delta_cache_contract": delta.get("entity_delta_cache_contract").and_then(Value::as_str),
        "requests": delta.get("requests").and_then(Value::as_u64).unwrap_or(0),
        "noop_responses": delta.get("noop_responses").and_then(Value::as_u64).unwrap_or(0),
        "snapshot_fallbacks": delta.get("snapshot_fallbacks").and_then(Value::as_u64).unwrap_or(0),
        "failures": delta.get("failures").and_then(Value::as_u64).unwrap_or(0),
        "snapshot_fallback_failure_rate_percent": delta.get("snapshot_fallback_failure_rate_percent").and_then(Value::as_u64).unwrap_or(0),
        "failure_rate_within_target": delta.get("failure_rate_within_target").and_then(Value::as_bool).unwrap_or(false),
        "noop_and_snapshot_fallback_are_not_failures": true,
        "etag_304_compatible": true,
    })
}

fn is_world_map_delta_cache_gate_green(gate: &Value) -> bool {
    gate.get("contract_version").and_then(Value::as_str)
        == Some("trillionnium_world_map_delta_cache_gate_v1")
        && gate
            .get("transport_delta_contract_version")
            .and_then(Value::as_str)
            == Some("trillionnium_world_map_transport_delta_v1")
        && gate
            .get("entity_delta_cache_contract")
            .and_then(Value::as_str)
            == Some("entity_group_versioned_delta_v1")
        && gate
            .get("failure_rate_within_target")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("noop_and_snapshot_fallback_are_not_failures")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && gate
            .get("etag_304_compatible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn route_runner_handoff_gate_u64(gate: &Value, key: &str) -> u64 {
    gate.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn gate_i64(gate: &Value, key: &str) -> i64 {
    gate.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn gauge_bool(value: bool) -> u64 {
    if value {
        1
    } else {
        0
    }
}

fn first_maturity_matrix_user_id(league: &LeagueState) -> String {
    let mut candidates = league
        .players_by_matrix_user
        .keys()
        .cloned()
        .chain(
            league
                .world
                .world_player_positions
                .values()
                .map(|position| position.matrix_user_id.clone()),
        )
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();

    let settled_reward_ids = league
        .rewards
        .iter()
        .filter(|reward| league_reward_ledger_released_from_state(league, reward))
        .map(|reward| reward.reward_id.clone())
        .collect::<HashSet<_>>();
    let mut successful_task_counts: HashMap<String, i64> = HashMap::new();
    let mut experience_data_points: HashMap<String, i64> = HashMap::new();
    let inc = |map: &mut HashMap<String, i64>, matrix_user_id: &str, amount: i64| {
        *map.entry(matrix_user_id.to_string()).or_insert(0) += amount;
    };

    for submission in league.submissions.values() {
        let payout_status = submission.payout_status.as_deref().unwrap_or("eligible");
        let released = payout_status == "eligible" || payout_status == "approved_release";
        let reward_id = league_hash_id("reward", &submission.submission_id);
        if submission.score >= 60.0 && released && settled_reward_ids.contains(&reward_id) {
            inc(&mut successful_task_counts, &submission.matrix_user_id, 1);
        }
        inc(&mut experience_data_points, &submission.matrix_user_id, 1);
    }
    for completion in &league.world.world_contract_completions {
        if completion.score >= 60.0
            && completion.payout_status == "eligible"
            && world_commerce_routes::world_contract_completion_released(&league.world, completion)
        {
            inc(&mut successful_task_counts, &completion.matrix_user_id, 1);
        }
    }
    for delivery in &league.world.world_work_deliveries {
        if delivery.score >= 60.0 && delivery.status == "delivered" {
            inc(&mut successful_task_counts, &delivery.matrix_user_id, 1);
        }
    }
    for acceptance in &league.world.world_work_acceptances {
        if acceptance.status == "accepted" {
            inc(&mut successful_task_counts, &acceptance.matrix_user_id, 1);
        }
    }
    for upgrade in &league.world.world_asset_upgrades {
        if upgrade.score >= 60.0 && upgrade.status == "upgraded" {
            inc(&mut successful_task_counts, &upgrade.matrix_user_id, 1);
        }
    }
    for battle in league.battles.values() {
        inc(&mut experience_data_points, &battle.matrix_user_id, 1);
    }
    for event in &league.world.world_events {
        inc(&mut experience_data_points, &event.actor_matrix_user_id, 1);
    }
    for asset in &league.world.world_assets {
        inc(&mut experience_data_points, &asset.owner_matrix_user_id, 1);
    }
    for company in &league.world.world_companies {
        inc(
            &mut experience_data_points,
            &company.owner_matrix_user_id,
            1,
        );
    }
    for listing in &league.world.world_listings {
        inc(
            &mut experience_data_points,
            &listing.owner_matrix_user_id,
            1,
        );
    }
    for purchase in &league.world.world_purchases {
        inc(
            &mut experience_data_points,
            &purchase.buyer_matrix_user_id,
            1,
        );
        if purchase.seller_matrix_user_id != purchase.buyer_matrix_user_id {
            inc(
                &mut experience_data_points,
                &purchase.seller_matrix_user_id,
                1,
            );
        }
    }

    candidates.sort_by(|left, right| {
        let left_score = (
            *successful_task_counts.get(left).unwrap_or(&0),
            *experience_data_points.get(left).unwrap_or(&0),
        );
        let right_score = (
            *successful_task_counts.get(right).unwrap_or(&0),
            *experience_data_points.get(right).unwrap_or(&0),
        );
        right_score.cmp(&left_score).then_with(|| left.cmp(right))
    });
    candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| "@alice:local.dev".to_string())
}

#[derive(Debug, Clone)]
struct HealthWorldProjection {
    matrix_user_id: String,
    app: Value,
    world_home: Value,
    route_artifacts: WorldRouteArtifacts,
    world_home_receipt_overlay_green: bool,
    world_home_receipt_overlay_error: Option<String>,
    client_app_receipt_overlay_green: bool,
    client_app_receipt_overlay_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HealthWorldReadinessBundleCacheKey {
    generation: u64,
    profile_ok: bool,
    identity_governance_valid: bool,
    session_auth_registry_governance_valid: bool,
    league_repository_runtime_fingerprint: String,
}

#[derive(Debug, Clone)]
pub(super) struct HealthWorldReadinessBundle {
    maturity: Value,
    closed_beta_prototype: Value,
    real_user_beta: Value,
    public_commercial_product: Value,
    playability_scorecard: Value,
}

#[derive(Debug, Clone)]
pub(super) struct HealthWorldReadinessBundleCache {
    key: HealthWorldReadinessBundleCacheKey,
    bundle: HealthWorldReadinessBundle,
}

impl HealthWorldProjection {
    fn new(
        league: &LeagueState,
        world_home_read_model: Option<&Value>,
        client_feed_read_model: Option<&Value>,
        normalized_read_switch_active: bool,
        world_home_read_model_error: Option<String>,
        client_feed_read_model_error: Option<String>,
    ) -> Self {
        let matrix_user_id = first_maturity_matrix_user_id(league);
        let mut world_home = world_home_json(league);
        let mut app = client_app_json(league, matrix_user_id.as_str());
        let mut world_home_receipt_overlay_error = world_home_read_model_error;
        let mut world_home_receipt_overlay_green = !normalized_read_switch_active;
        if let Some(read_model) = world_home_read_model {
            match apply_normalized_world_home_receipt_read_model(&mut world_home, read_model) {
                Ok(()) => {
                    let home_source = world_home
                        .get("normalized_receipt_read_model")
                        .and_then(|marker| marker.get("source"))
                        .and_then(Value::as_str);
                    let projection_source = world_home
                        .get("term_exchange_receipt_projection")
                        .and_then(|projection| projection.get("runtime_read_model_source"))
                        .and_then(Value::as_str);
                    world_home_receipt_overlay_green = home_source
                        == Some("normalized_sql_world_home_read_model")
                        && projection_source == Some("normalized_sql_world_home_read_model");
                    if !world_home_receipt_overlay_green {
                        world_home_receipt_overlay_error = Some(
                            "world-home normalized receipt overlay markers were missing"
                                .to_string(),
                        );
                    }
                }
                Err(err) => {
                    world_home_receipt_overlay_green = false;
                    world_home_receipt_overlay_error = Some(err);
                }
            }
        } else if normalized_read_switch_active && world_home_receipt_overlay_error.is_none() {
            world_home_receipt_overlay_error = Some(
                "normalized world-home read model unavailable for world-home overlay".to_string(),
            );
        }
        if let Some(err) = world_home_receipt_overlay_error.as_ref() {
            if let Some(obj) = world_home.as_object_mut() {
                obj.insert(
                    "normalized_receipt_read_model_error".to_string(),
                    json!({
                        "source": "normalized_sql_world_home_read_model",
                        "message": err,
                    }),
                );
            }
        }
        let mut client_app_receipt_overlay_error = client_feed_read_model_error;
        let mut client_app_receipt_overlay_green = !normalized_read_switch_active;
        if let Some(read_model) = client_feed_read_model {
            match apply_normalized_client_app_receipt_read_model(&mut app, read_model) {
                Ok(()) => {
                    let app_source = app
                        .get("normalized_receipt_read_model")
                        .and_then(|marker| marker.get("source"))
                        .and_then(Value::as_str);
                    let feed_source = app
                        .get("feed")
                        .and_then(|feed| feed.get("normalized_receipt_read_model"))
                        .and_then(|marker| marker.get("source"))
                        .and_then(Value::as_str);
                    client_app_receipt_overlay_green = app_source
                        == Some("normalized_sql_client_app_feed_overlay")
                        && feed_source == Some("normalized_sql_client_feed_read_model");
                    if !client_app_receipt_overlay_green {
                        client_app_receipt_overlay_error = Some(
                            "client-app normalized receipt overlay markers were missing"
                                .to_string(),
                        );
                    }
                }
                Err(err) => {
                    client_app_receipt_overlay_green = false;
                    client_app_receipt_overlay_error = Some(err);
                }
            }
        } else if normalized_read_switch_active && client_app_receipt_overlay_error.is_none() {
            client_app_receipt_overlay_error = Some(
                "normalized client-feed read model unavailable for client-app overlay".to_string(),
            );
        }
        if let Some(err) = client_app_receipt_overlay_error.as_ref() {
            if let Some(obj) = app.as_object_mut() {
                obj.insert(
                    "normalized_receipt_read_model_error".to_string(),
                    json!({
                        "source": "normalized_sql_client_app_feed_overlay",
                        "message": err,
                    }),
                );
            }
        }
        Self {
            app,
            world_home,
            route_artifacts: build_world_route_artifacts(&league.world),
            matrix_user_id,
            world_home_receipt_overlay_green,
            world_home_receipt_overlay_error,
            client_app_receipt_overlay_green,
            client_app_receipt_overlay_error,
        }
    }

    fn normalized_receipt_read_models_green(&self) -> bool {
        self.world_home_receipt_overlay_green && self.client_app_receipt_overlay_green
    }
}

async fn trillionnium_world_readiness_bundle(
    state: &AppState,
    profile_ok: bool,
    identity_governance_valid: bool,
    session_auth_registry_governance_valid: bool,
    league_repository_runtime: &Value,
) -> HealthWorldReadinessBundle {
    let key = HealthWorldReadinessBundleCacheKey {
        generation: state
            .inner
            .health_world_readiness_cache_generation
            .load(Ordering::Relaxed),
        profile_ok,
        identity_governance_valid,
        session_auth_registry_governance_valid,
        league_repository_runtime_fingerprint: league_repository_runtime.to_string(),
    };
    {
        let cache = state.inner.health_world_readiness_cache.lock().await;
        if let Some(cache) = cache.as_ref() {
            if cache.key == key {
                return cache.bundle.clone();
            }
        }
    }

    let normalized_read_switch_active = state.config().league_normalized_read_switch_enabled
        && state.config().league_normalized_database_url.is_some();
    let world_home_read_model_result =
        load_normalized_repository_world_home_read_model_for_runtime(state.config()).await;
    let world_home_read_model_error = match &world_home_read_model_result {
        Ok(Some(_)) => None,
        Ok(None) if normalized_read_switch_active => Some(
            "normalized world-home read model not loaded while read switch is active".to_string(),
        ),
        Ok(None) => None,
        Err(err) => Some(err.clone()),
    };
    let world_home_read_model = world_home_read_model_result.ok().flatten();
    let client_feed_read_model_result =
        load_normalized_repository_client_feed_read_model_for_runtime(state.config()).await;
    let client_feed_read_model_error = match &client_feed_read_model_result {
        Ok(Some(_)) => None,
        Ok(None) if normalized_read_switch_active => Some(
            "normalized client-feed read model not loaded while read switch is active".to_string(),
        ),
        Ok(None) => None,
        Err(err) => Some(err.clone()),
    };
    let client_feed_read_model = client_feed_read_model_result.ok().flatten();

    let bundle = {
        let league = state.inner.league_state.lock().await;
        let projection = HealthWorldProjection::new(
            &league,
            world_home_read_model.as_ref(),
            client_feed_read_model.as_ref(),
            normalized_read_switch_active,
            world_home_read_model_error,
            client_feed_read_model_error,
        );
        let maturity = trillionnium_world_maturity_axes_json(
            &league,
            state.config(),
            profile_ok,
            league_repository_runtime,
            &projection,
        );
        let closed_beta_prototype = trillionnium_world_closed_beta_prototype_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            league_repository_runtime,
            &maturity,
            &projection,
        );
        let real_user_beta = trillionnium_world_real_user_beta_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            league_repository_runtime,
            &closed_beta_prototype,
            &projection,
        );
        let public_commercial_product = trillionnium_world_public_commercial_product_json(
            &league,
            state.config(),
            profile_ok,
            identity_governance_valid,
            session_auth_registry_governance_valid,
            league_repository_runtime,
            &real_user_beta,
            &projection,
        );
        let playability_scorecard = trillionnium_world_playability_scorecard_json(
            &league,
            &maturity,
            &closed_beta_prototype,
            &real_user_beta,
            &public_commercial_product,
            league_repository_runtime,
            &projection,
        );
        HealthWorldReadinessBundle {
            maturity,
            closed_beta_prototype,
            real_user_beta,
            public_commercial_product,
            playability_scorecard,
        }
    };

    let mut cache = state.inner.health_world_readiness_cache.lock().await;
    *cache = Some(HealthWorldReadinessBundleCache {
        key,
        bundle: bundle.clone(),
    });
    bundle
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
            world_commerce_routes::world_contract_completion_released(world, completion)
        })
        .count();
    let reserved_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_reserve_active(world, purchase)
        })
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_consume_completed(world, purchase)
        })
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| {
            world_commerce_routes::world_work_rejection_refund_completed(world, rejection)
        })
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| {
            world_commerce_routes::world_work_cancellation_refund_completed(world, cancellation)
        })
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
        "latest_snapshot_requires_repository_audit_write_set_audit_and_normalized_world_home_client_feed_and_client_app_read_models",
    );
    let normalized_read_models_active = maturity_str(
        league_repository_runtime,
        "normalized_source_of_truth_read_models",
    ) == Some("world_home_client_feed_and_client_app")
        && projection.normalized_receipt_read_models_green();
    let world_home_receipt_overlay_green = projection.world_home_receipt_overlay_green;
    let world_home_receipt_overlay_error_absent =
        projection.world_home_receipt_overlay_error.is_none();
    let client_app_receipt_overlay_green = projection.client_app_receipt_overlay_green;
    let client_app_receipt_overlay_error_absent =
        projection.client_app_receipt_overlay_error.is_none();
    let cex_trillionnium_world_adapter_readiness =
        cex_trillionnium_world_adapter_readiness_json_for_league(league);
    let cex_trillionnium_world_adapter_green =
        cex_trillionnium_world_adapter_readiness_green(&cex_trillionnium_world_adapter_readiness);
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
            ("world_home_client_feed_and_client_app_read_models_active", normalized_read_models_active),
            ("world_home_receipt_overlay_green", world_home_receipt_overlay_green),
            ("world_home_receipt_overlay_error_absent", world_home_receipt_overlay_error_absent),
            ("client_app_receipt_overlay_green", client_app_receipt_overlay_green),
            ("client_app_receipt_overlay_error_absent", client_app_receipt_overlay_error_absent),
            ("cex_trillionnium_world_runtime_adapter_green", cex_trillionnium_world_adapter_green),
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
        "cex_trillionnium_world_runtime_adapter_green": cex_trillionnium_world_adapter_green,
        "cex_trillionnium_world_runtime_adapter_readiness": cex_trillionnium_world_adapter_readiness,
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
    let world_home = projection.world_home.clone();
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
    let map_readability_lod_gate = app_map_readability_lod_gate_json(&app);
    let map_readability_lod_gate_green =
        is_map_readability_lod_gate_green(&map_readability_lod_gate);
    let route_runner_funnel_telemetry_gate = app_route_runner_funnel_telemetry_gate_json(&app);
    let route_runner_funnel_telemetry_gate_green =
        is_route_runner_funnel_telemetry_gate_green(&route_runner_funnel_telemetry_gate);
    let commercial_operating_dashboard_gate = app_commercial_operating_dashboard_gate_json(&app);
    let commercial_operating_dashboard_gate_green =
        is_commercial_operating_dashboard_gate_green(&commercial_operating_dashboard_gate);
    let future_engine_readiness_gate = app_future_engine_readiness_gate_json(&app);
    let future_engine_readiness_gate_green =
        is_future_engine_readiness_gate_green(&future_engine_readiness_gate);
    let openstreetmap_provider_readiness_gate =
        app_openstreetmap_provider_readiness_gate_json(&app);
    let openstreetmap_provider_readiness_gate_green =
        is_openstreetmap_provider_readiness_gate_green(&openstreetmap_provider_readiness_gate);
    let openstreetmap_geodata_freshness_gate = app_openstreetmap_geodata_freshness_gate_json(&app);
    let openstreetmap_geodata_freshness_gate_green =
        is_openstreetmap_geodata_freshness_gate_green(&openstreetmap_geodata_freshness_gate);
    let openstreetmap_attribution_presence_gate =
        app_openstreetmap_attribution_presence_gate_json(&app);
    let openstreetmap_attribution_presence_gate_green =
        is_openstreetmap_attribution_presence_gate_green(&openstreetmap_attribution_presence_gate);
    let world_map_runtime_safety_gate = app_world_map_runtime_safety_gate_json(&app);
    let world_map_runtime_safety_gate_green =
        is_world_map_runtime_safety_gate_green(&world_map_runtime_safety_gate);
    let ops_persistent_telemetry_green = ops_contract_v1
        && ops_check("persistent_telemetry_stream_visible")
        && ops_check("route_runner_funnel_telemetry_visible")
        && ops_check("time_to_reward_visible")
        && ops_check("daily_return_resume_visible")
        && economy_retention_ops
            .get("engine_contracts")
            .and_then(|contracts| contracts.get("telemetry_stream"))
            .and_then(Value::as_str)
            == Some("world_economy_events:playability_telemetry")
        && route_runner_funnel_telemetry_gate_green
        && commercial_operating_dashboard_gate_green;
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
            world_commerce_routes::world_contract_completion_released(world, completion)
        })
        .count();
    let reserved_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_reserve_active(world, purchase)
        })
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_consume_completed(world, purchase)
        })
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| {
            world_commerce_routes::world_work_rejection_refund_completed(world, rejection)
        })
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| {
            world_commerce_routes::world_work_cancellation_refund_completed(world, cancellation)
        })
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
            ("route_backlog_and_daily_return_hook_visible", route_task_graph_count >= 10 && coach_check("p2_daily_return_hook_visible") && playability_coach.get("retention_ops").and_then(|ops| ops.get("daily_return_hooks")).and_then(Value::as_array).is_some_and(|hooks| hooks.len() >= 4) && ops_retention_calendar_green && ops_persistent_telemetry_green && route_runner_funnel_telemetry_gate_green && commercial_operating_dashboard_gate_green),
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
            ("map_readability_lod_contract_green", map_readability_lod_gate_green),
            ("openstreetmap_provider_readiness_section_visible", openstreetmap_provider_readiness_gate_green),
            ("openstreetmap_geodata_freshness_section_visible", openstreetmap_geodata_freshness_gate_green),
            ("openstreetmap_attribution_presence_section_visible", openstreetmap_attribution_presence_gate_green),
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
            ("playability_runtime_contracts_exposed", route_contract_version_present && route_runner_handoff_gate_green && coach_p0_p1_p2_green && world_home_playability_runtime_green && ops_contract_v1 && ops_engine_contracts_green && ops_balance_config_green && map_readability_lod_gate_green && route_runner_funnel_telemetry_gate_green && commercial_operating_dashboard_gate_green && future_engine_readiness_gate_green && openstreetmap_provider_readiness_gate_green && openstreetmap_geodata_freshness_gate_green && openstreetmap_attribution_presence_gate_green && world_map_runtime_safety_gate_green),
            ("mobile_contract_readiness_dense", mobile_readiness_checks.len() >= 10),
            ("scorecard_has_runtime_funnel_data", feed_item_count >= 20 && route_task_graph_count >= 10 && route_runner_handoff_gate_green && ops_funnel_green && ops_persistent_telemetry_green && route_runner_funnel_telemetry_gate_green && commercial_operating_dashboard_gate_green),
            ("future_engine_readiness_contract_visible", future_engine_readiness_gate_green),
            ("openstreetmap_provider_readiness_gate_green", openstreetmap_provider_readiness_gate_green),
            ("openstreetmap_geodata_freshness_gate_green", openstreetmap_geodata_freshness_gate_green),
            ("openstreetmap_attribution_presence_gate_green", openstreetmap_attribution_presence_gate_green),
            ("world_map_rum_delta_weak_privacy_gates_visible", world_map_runtime_safety_gate_green),
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
            ("playability_runtime_contracts_present", route_contract_version_present && route_runner_handoff_gate_green && coach_p0_p1_p2_green && world_home_playability_runtime_green && ops_contract_v1 && ops_engine_contracts_green && ops_balance_config_green && map_readability_lod_gate_green && route_runner_funnel_telemetry_gate_green && commercial_operating_dashboard_gate_green && future_engine_readiness_gate_green && openstreetmap_provider_readiness_gate_green && openstreetmap_geodata_freshness_gate_green && openstreetmap_attribution_presence_gate_green && world_map_runtime_safety_gate_green),
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
            ("coach_retention_ops_visible", route_task_graph_count >= 10 && coach_lane("p2_retention_ops") && coach_check("p2_telemetry_contract_visible") && ops_retention_calendar_green && ops_anti_cheese_green && route_runner_funnel_telemetry_gate_green && commercial_operating_dashboard_gate_green),
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
        "map_readability_lod_gate": map_readability_lod_gate,
        "route_runner_funnel_telemetry_gate": route_runner_funnel_telemetry_gate,
        "commercial_operating_dashboard_gate": commercial_operating_dashboard_gate,
        "future_engine_readiness_gate": future_engine_readiness_gate,
        "openstreetmap_provider_readiness_gate": openstreetmap_provider_readiness_gate,
        "openstreetmap_geodata_freshness_gate": openstreetmap_geodata_freshness_gate,
        "openstreetmap_attribution_presence_gate": openstreetmap_attribution_presence_gate,
        "world_map_runtime_safety_gate": world_map_runtime_safety_gate,
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
            world_commerce_routes::world_contract_completion_released(world, completion)
        })
        .count();
    let reserved_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_reserve_active(world, purchase)
        })
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_consume_completed(world, purchase)
        })
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| {
            world_commerce_routes::world_work_rejection_refund_completed(world, rejection)
        })
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| {
            world_commerce_routes::world_work_cancellation_refund_completed(world, cancellation)
        })
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
    ) == Some("world_home_client_feed_and_client_app")
        && projection.normalized_receipt_read_models_green();
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
            world_commerce_routes::world_contract_completion_released(world, completion)
        })
        .count();
    let reserved_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_reserve_active(world, purchase)
        })
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_consume_completed(world, purchase)
        })
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| {
            world_commerce_routes::world_work_rejection_refund_completed(world, rejection)
        })
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| {
            world_commerce_routes::world_work_cancellation_refund_completed(world, cancellation)
        })
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
    ) == Some("world_home_client_feed_and_client_app")
        && projection.normalized_receipt_read_models_green();
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
    let commercial_operating_dashboard_gate = app_commercial_operating_dashboard_gate_json(&app);
    let commercial_operating_dashboard_gate_green =
        is_commercial_operating_dashboard_gate_green(&commercial_operating_dashboard_gate);
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
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_reserve_active(world, purchase)
        })
        .count();
    let consumed_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            world_commerce_routes::world_purchase_buyer_consume_completed(world, purchase)
        })
        .count();
    let refunded_rejection_count = world
        .world_work_rejections
        .iter()
        .filter(|rejection| {
            world_commerce_routes::world_work_rejection_refund_completed(world, rejection)
        })
        .count();
    let rereserved_reopen_count = world
        .world_work_reopens
        .iter()
        .filter(|reopen| world_commerce_routes::world_work_reopen_reserve_completed(world, reopen))
        .count();
    let refunded_cancellation_count = world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| {
            world_commerce_routes::world_work_cancellation_refund_completed(world, cancellation)
        })
        .count();
    let settled_contract_completion_count = world
        .world_contract_completions
        .iter()
        .filter(|completion| {
            world_commerce_routes::world_contract_completion_released(world, completion)
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
    ) == Some("world_home_client_feed_and_client_app")
        && projection.normalized_receipt_read_models_green();
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
                "commercial_operating_dashboard_visible",
                commercial_operating_dashboard_gate_green,
            ),
            (
                "route_start_to_paid_task_conversion_visible",
                commercial_operating_dashboard_gate
                    .get("route_start_to_paid_task_conversion_percent")
                    .and_then(Value::as_i64)
                    .is_some(),
            ),
            (
                "reward_claim_to_next_commission_visible",
                commercial_operating_dashboard_gate
                    .get("reward_claim_to_next_commission_percent")
                    .and_then(Value::as_i64)
                    .is_some(),
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
        "commercial_operating_dashboard_gate": commercial_operating_dashboard_gate,
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
    let trillionnium_world_readiness = trillionnium_world_readiness_bundle(
        &state,
        profile_ok,
        identity_governance_valid,
        session_auth_registry_governance_valid,
        &league_repository_runtime,
    )
    .await;
    let trillionnium_world_maturity = trillionnium_world_readiness.maturity.clone();
    let trillionnium_world_closed_beta_prototype =
        trillionnium_world_readiness.closed_beta_prototype.clone();
    let trillionnium_world_real_user_beta = trillionnium_world_readiness.real_user_beta.clone();
    let trillionnium_world_public_commercial_product = trillionnium_world_readiness
        .public_commercial_product
        .clone();
    let trillionnium_world_playability_scorecard =
        trillionnium_world_readiness.playability_scorecard;
    let metrics_snapshot = state.inner.metrics.snapshot();
    let world_map_rum_slo_metrics_gate = world_map_rum_slo_gate_from_metrics(&metrics_snapshot);
    let world_map_delta_cache_gate = world_map_delta_cache_gate_from_metrics(&metrics_snapshot);
    let empty_scorecard_gate = json!({});
    let world_map_runtime_safety_gate = trillionnium_world_playability_scorecard
        .get("world_map_runtime_safety_gate")
        .unwrap_or(&empty_scorecard_gate)
        .clone();
    let openstreetmap_provider_readiness_gate = trillionnium_world_playability_scorecard
        .get("openstreetmap_provider_readiness_gate")
        .unwrap_or(&empty_scorecard_gate)
        .clone();
    let openstreetmap_geodata_freshness_gate = trillionnium_world_playability_scorecard
        .get("openstreetmap_geodata_freshness_gate")
        .unwrap_or(&empty_scorecard_gate)
        .clone();
    let openstreetmap_attribution_presence_gate = trillionnium_world_playability_scorecard
        .get("openstreetmap_attribution_presence_gate")
        .unwrap_or(&empty_scorecard_gate)
        .clone();
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
        "trillionnium_openstreetmap_provider_readiness_gate": openstreetmap_provider_readiness_gate,
        "trillionnium_openstreetmap_geodata_freshness_gate": openstreetmap_geodata_freshness_gate,
        "trillionnium_openstreetmap_attribution_presence_gate": openstreetmap_attribution_presence_gate,
        "trillionnium_world_map_runtime_safety_gate": world_map_runtime_safety_gate,
        "trillionnium_world_map_rum_slo_gate": world_map_rum_slo_metrics_gate,
        "trillionnium_world_map_delta_cache_gate": world_map_delta_cache_gate,
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
        "metrics": metrics_snapshot,
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
    let trillionnium_world_readiness = trillionnium_world_readiness_bundle(
        &state,
        profile_ok_bool,
        governance_valid,
        session_auth_registry_governance_valid,
        &league_repository_runtime,
    )
    .await;
    let trillionnium_world_maturity = trillionnium_world_readiness.maturity.clone();
    let trillionnium_world_closed_beta_prototype =
        trillionnium_world_readiness.closed_beta_prototype.clone();
    let trillionnium_world_real_user_beta = trillionnium_world_readiness.real_user_beta.clone();
    let trillionnium_world_public_commercial_product = trillionnium_world_readiness
        .public_commercial_product
        .clone();
    let trillionnium_world_playability_scorecard =
        trillionnium_world_readiness.playability_scorecard;
    let cex_trillionnium_world_runtime_adapter_green = trillionnium_world_maturity
        .get("cex_trillionnium_world_runtime_adapter_green")
        .and_then(Value::as_bool)
        .unwrap_or(false);
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
    let route_runner_handoff_mastery_contract_visible = playability_route_runner_handoff_gate
        .get("route_mastery_contract_version")
        .and_then(Value::as_str)
        == Some("trillionnium_route_mastery_v1");
    let route_runner_handoff_mastery_tier_visible = playability_route_runner_handoff_gate
        .get("first_route_mastery_tier")
        .and_then(Value::as_str)
        .map(|tier| !tier.trim().is_empty())
        .unwrap_or(false);
    let route_runner_handoff_mastery_next_goal_evidence_visible =
        playability_route_runner_handoff_gate
            .get("first_route_mastery_next_goal")
            .and_then(Value::as_str)
            .map(|goal| goal.to_ascii_lowercase().contains("evidence"))
            .unwrap_or(false);
    let empty_scorecard_gate = json!({});
    let map_readability_lod_gate = trillionnium_world_playability_scorecard
        .get("map_readability_lod_gate")
        .unwrap_or(&empty_scorecard_gate);
    let route_runner_funnel_telemetry_gate = trillionnium_world_playability_scorecard
        .get("route_runner_funnel_telemetry_gate")
        .unwrap_or(&empty_scorecard_gate);
    let commercial_operating_dashboard_gate = trillionnium_world_playability_scorecard
        .get("commercial_operating_dashboard_gate")
        .unwrap_or(&empty_scorecard_gate);
    let future_engine_readiness_gate = trillionnium_world_playability_scorecard
        .get("future_engine_readiness_gate")
        .unwrap_or(&empty_scorecard_gate);
    let openstreetmap_provider_readiness_gate = trillionnium_world_playability_scorecard
        .get("openstreetmap_provider_readiness_gate")
        .unwrap_or(&empty_scorecard_gate);
    let openstreetmap_geodata_freshness_gate = trillionnium_world_playability_scorecard
        .get("openstreetmap_geodata_freshness_gate")
        .unwrap_or(&empty_scorecard_gate);
    let openstreetmap_attribution_presence_gate = trillionnium_world_playability_scorecard
        .get("openstreetmap_attribution_presence_gate")
        .unwrap_or(&empty_scorecard_gate);
    let world_map_runtime_safety_gate = trillionnium_world_playability_scorecard
        .get("world_map_runtime_safety_gate")
        .unwrap_or(&empty_scorecard_gate);
    let map_readability_lod_gate_green =
        is_map_readability_lod_gate_green(map_readability_lod_gate);
    let route_runner_funnel_telemetry_gate_green =
        is_route_runner_funnel_telemetry_gate_green(route_runner_funnel_telemetry_gate);
    let commercial_operating_dashboard_gate_green =
        is_commercial_operating_dashboard_gate_green(commercial_operating_dashboard_gate);
    let future_engine_readiness_gate_green =
        is_future_engine_readiness_gate_green(future_engine_readiness_gate);
    let openstreetmap_provider_readiness_gate_green =
        is_openstreetmap_provider_readiness_gate_green(openstreetmap_provider_readiness_gate);
    let openstreetmap_geodata_freshness_gate_green =
        is_openstreetmap_geodata_freshness_gate_green(openstreetmap_geodata_freshness_gate);
    let openstreetmap_attribution_presence_gate_green =
        is_openstreetmap_attribution_presence_gate_green(openstreetmap_attribution_presence_gate);
    let world_map_runtime_safety_gate_green =
        is_world_map_runtime_safety_gate_green(world_map_runtime_safety_gate);
    let metrics_snapshot = state.inner.metrics.snapshot();
    let world_map_rum_snapshot = metrics_snapshot
        .get("world_map_rum")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let world_map_delta_snapshot = metrics_snapshot
        .get("world_map_delta")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let world_map_rum_slo_metrics_gate = world_map_rum_slo_gate_from_metrics(&metrics_snapshot);
    let world_map_delta_cache_gate = world_map_delta_cache_gate_from_metrics(&metrics_snapshot);
    let world_map_rum_slo_metrics_gate_green =
        is_world_map_rum_slo_metrics_gate_green(&world_map_rum_slo_metrics_gate);
    let world_map_delta_cache_gate_green =
        is_world_map_delta_cache_gate_green(&world_map_delta_cache_gate);
    let world_map_rum_sample_matrix_gate_green = world_map_rum_slo_metrics_gate
        .get("sample_matrix_raw_green")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let world_map_density_scalability_gate_green = world_map_runtime_safety_gate
        .get("density_scalability_contract_version")
        .and_then(Value::as_str)
        == Some("trillionnium_world_map_density_scalability_v1")
        && world_map_runtime_safety_gate
            .get("projection_cache_strategy_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && world_map_runtime_safety_gate
            .get("frontend_virtualization_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && world_map_runtime_safety_gate
            .get("adaptive_density_scheduler_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let world_map_offline_action_queue_gate_green = world_map_runtime_safety_gate
        .get("offline_action_queue_contract_version")
        .and_then(Value::as_str)
        == Some("trillionnium_world_map_offline_action_queue_v1")
        && world_map_runtime_safety_gate
            .get("offline_banner_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && world_map_runtime_safety_gate
            .get("pending_action_queue_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && world_map_runtime_safety_gate
            .get("conflict_sync_recovery_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let world_map_gameplay_accessibility_i18n_gate_green = world_map_runtime_safety_gate
        .get("gameplay_accessibility_contract_version")
        .and_then(Value::as_str)
        == Some("trillionnium_world_map_gameplay_accessibility_i18n_v1")
        && world_map_runtime_safety_gate
            .get("screen_reader_reduced_motion_touch_targets_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let world_map_maplibre_shadow_parity_gate_green = future_engine_readiness_gate
        .get("maplibre_shadow_parity_contract_version")
        .and_then(Value::as_str)
        == Some("trillionnium_world_map_maplibre_shadow_parity_v1")
        && future_engine_readiness_gate
            .get("maplibre_shadow_only")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && future_engine_readiness_gate
            .get("maplibre_shadow_marker_cluster_popup_focus_parity_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let world_map_maplibre_canary_rollback_ready = future_engine_readiness_gate
        .get("maplibre_canary_percent")
        .and_then(Value::as_i64)
        .unwrap_or(100)
        == 0
        && future_engine_readiness_gate
            .get("maplibre_canary_starts_at_zero")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && future_engine_readiness_gate
            .get("maplibre_max_canary_percent_without_new_signoff")
            .and_then(Value::as_i64)
            .unwrap_or(100)
            <= 1
        && future_engine_readiness_gate
            .get("maplibre_rollback_drill_evidence_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && future_engine_readiness_gate
            .get("maplibre_canary_rollback_drill_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && future_engine_readiness_gate
            .get("rollback_plan_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && future_engine_readiness_gate
            .get("shadow_renderer_status")
            .and_then(Value::as_str)
            == Some("shadow_only_not_user_facing");
    let world_route_recommendation_quality_gate_green = commercial_operating_dashboard_gate
        .get("route_recommendation_quality_contract_version")
        .and_then(Value::as_str)
        == Some("trillionnium_world_route_recommendation_quality_v1")
        && commercial_operating_dashboard_gate
            .get("route_recommendation_quality_status")
            .and_then(Value::as_str)
            == Some("quality_gate_ready")
        && commercial_operating_dashboard_gate
            .get("route_recommendation_quality_score_percent")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            >= commercial_operating_dashboard_gate
                .get("route_recommendation_quality_score_target_percent")
                .and_then(Value::as_i64)
                .unwrap_or(60)
        && commercial_operating_dashboard_gate
            .get("route_recommendation_quality_score_ready")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && commercial_operating_dashboard_gate
            .get("route_recommendation_reward_lift_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && commercial_operating_dashboard_gate
            .get("route_recommendation_abandon_risk_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && commercial_operating_dashboard_gate
            .get("route_recommendation_denominator_consistent")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && commercial_operating_dashboard_gate
            .get("route_recommendation_raw_counts_preserved")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && commercial_operating_dashboard_gate
            .get("route_recommendation_risk_controls_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
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
            "# TYPE cex_consumer_entry_game_account_register_successes_total counter\n",
            "cex_consumer_entry_game_account_register_successes_total {}\n",
            "# TYPE cex_consumer_entry_game_account_login_successes_total counter\n",
            "cex_consumer_entry_game_account_login_successes_total {}\n",
            "# TYPE cex_consumer_entry_game_account_login_failures_total counter\n",
            "cex_consumer_entry_game_account_login_failures_total {}\n",
            "# TYPE cex_consumer_entry_game_account_logout_successes_total counter\n",
            "cex_consumer_entry_game_account_logout_successes_total {}\n",
            "# TYPE cex_consumer_entry_game_account_profile_updates_total counter\n",
            "cex_consumer_entry_game_account_profile_updates_total {}\n",
            "# TYPE cex_consumer_entry_game_account_password_change_successes_total counter\n",
            "cex_consumer_entry_game_account_password_change_successes_total {}\n",
            "# TYPE cex_consumer_entry_game_account_password_change_failures_total counter\n",
            "cex_consumer_entry_game_account_password_change_failures_total {}\n",
            "# TYPE cex_consumer_entry_game_account_session_refresh_successes_total counter\n",
            "cex_consumer_entry_game_account_session_refresh_successes_total {}\n",
            "# TYPE cex_consumer_entry_game_account_session_revoke_successes_total counter\n",
            "cex_consumer_entry_game_account_session_revoke_successes_total {}\n",
            "# TYPE cex_consumer_entry_game_account_auth_rate_limited_total counter\n",
            "cex_consumer_entry_game_account_auth_rate_limited_total {}\n",
            "# TYPE cex_consumer_entry_replay_hits_total counter\n",
            "cex_consumer_entry_replay_hits_total {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_samples_total counter\n",
            "cex_consumer_entry_trillionnium_world_map_rum_samples_total {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_first_interactive_max_ms gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_first_interactive_max_ms {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_viewport_refresh_max_ms gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_viewport_refresh_max_ms {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_delta_requests_total counter\n",
            "cex_consumer_entry_trillionnium_world_map_delta_requests_total {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_delta_noop_responses_total counter\n",
            "cex_consumer_entry_trillionnium_world_map_delta_noop_responses_total {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_delta_snapshot_fallbacks_total counter\n",
            "cex_consumer_entry_trillionnium_world_map_delta_snapshot_fallbacks_total {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_delta_failures_total counter\n",
            "cex_consumer_entry_trillionnium_world_map_delta_failures_total {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_slo_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_slo_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_slo_raw_split_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_slo_raw_split_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_slo_sample_count gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_slo_sample_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_slo_enforcement_active gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_slo_enforcement_active {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_slo_warming gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_slo_warming {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_raw_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_raw_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_coverage_count gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_coverage_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_missing_bucket_count gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_sample_matrix_missing_bucket_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_first_interactive_p95_ms gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_first_interactive_p95_ms {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_viewport_refresh_p95_ms gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_viewport_refresh_p95_ms {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_rum_focus_to_action_p95_ms gauge\n",
            "cex_consumer_entry_trillionnium_world_map_rum_focus_to_action_p95_ms {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_delta_failure_rate_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_map_delta_failure_rate_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_delta_cache_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_delta_cache_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_runtime_safety_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_runtime_safety_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_weak_network_resilience_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_weak_network_resilience_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_location_privacy_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_location_privacy_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_density_scalability_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_density_scalability_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_offline_action_queue_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_offline_action_queue_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_gameplay_accessibility_i18n_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_gameplay_accessibility_i18n_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_maplibre_shadow_parity_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_maplibre_shadow_parity_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_maplibre_shadow_only gauge\n",
            "cex_consumer_entry_trillionnium_world_map_maplibre_shadow_only {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_maplibre_canary_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_map_maplibre_canary_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_maplibre_max_canary_percent_without_new_signoff gauge\n",
            "cex_consumer_entry_trillionnium_world_map_maplibre_max_canary_percent_without_new_signoff {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_maplibre_canary_starts_at_zero gauge\n",
            "cex_consumer_entry_trillionnium_world_map_maplibre_canary_starts_at_zero {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_maplibre_rollback_drill_evidence_required gauge\n",
            "cex_consumer_entry_trillionnium_world_map_maplibre_rollback_drill_evidence_required {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_maplibre_canary_rollback_ready gauge\n",
            "cex_consumer_entry_trillionnium_world_map_maplibre_canary_rollback_ready {}\n",
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
            "# TYPE cex_consumer_entry_trillionnium_world_runtime_adapter_green gauge\n",
            "cex_consumer_entry_trillionnium_world_runtime_adapter_green {}\n",
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
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_contract_visible gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_contract_visible {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_runner_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_runner_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_first_route_mastery_xp gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_first_route_mastery_xp {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_tier_visible gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_tier_visible {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_next_goal_evidence_visible gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_next_goal_evidence_visible {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_readability_lod_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_map_readability_lod_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_readability_lod_visible_marker_budget gauge\n",
            "cex_consumer_entry_trillionnium_world_map_readability_lod_visible_marker_budget {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_readability_lod_visible_markers gauge\n",
            "cex_consumer_entry_trillionnium_world_map_readability_lod_visible_markers {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_readability_lod_avatar_runner_budget gauge\n",
            "cex_consumer_entry_trillionnium_world_map_readability_lod_avatar_runner_budget {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_map_readability_lod_avatar_runners gauge\n",
            "cex_consumer_entry_trillionnium_world_map_readability_lod_avatar_runners {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_telemetry_contract_visible gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_telemetry_contract_visible {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_route_started_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_route_started_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_evidence_submitted_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_evidence_submitted_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_reward_claimed_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_reward_claimed_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_next_route_opened_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_next_route_opened_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_abandoned_or_recovery_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_abandoned_or_recovery_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_time_to_reward_seconds gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_time_to_reward_seconds {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_daily_return_resume_count gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_daily_return_resume_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_reward_to_next_route_percent gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_reward_to_next_route_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_d1_resume_percent gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_d1_resume_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_route_abandon_or_recovery_percent gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_route_abandon_or_recovery_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_time_to_first_proof_seconds gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_time_to_first_proof_seconds {}\n",
            "# TYPE cex_consumer_entry_trillionnium_route_runner_funnel_time_to_next_route_seconds gauge\n",
            "cex_consumer_entry_trillionnium_route_runner_funnel_time_to_next_route_seconds {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_commercial_operating_dashboard_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_commercial_operating_dashboard_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_quality_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_quality_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_target_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_target_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_ready gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_quality_score_ready {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_reward_lift_visible gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_reward_lift_visible {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_abandon_risk_visible gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_abandon_risk_visible {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_denominator_consistent gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_denominator_consistent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_raw_counts_preserved gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_raw_counts_preserved {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_route_recommendation_risk_controls_visible gauge\n",
            "cex_consumer_entry_trillionnium_world_route_recommendation_risk_controls_visible {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_commercial_route_start_to_paid_task_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_commercial_route_start_to_paid_task_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_commercial_reward_claim_to_next_commission_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_commercial_reward_claim_to_next_commission_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_commercial_seller_completion_quality_percent gauge\n",
            "cex_consumer_entry_trillionnium_world_commercial_seller_completion_quality_percent {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_commercial_buyer_repeat_order_count gauge\n",
            "cex_consumer_entry_trillionnium_world_commercial_buyer_repeat_order_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_commercial_dispute_refund_reopen_count gauge\n",
            "cex_consumer_entry_trillionnium_world_commercial_dispute_refund_reopen_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_future_engine_readiness_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_world_future_engine_readiness_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_world_future_engine_promotion_blocker_count gauge\n",
            "cex_consumer_entry_trillionnium_world_future_engine_promotion_blocker_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_provider_readiness_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_provider_readiness_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_provider_fail_closed_mode_count gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_provider_fail_closed_mode_count {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_geodata_freshness_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_geodata_freshness_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_geodata_fixture_snapshot_age_seconds gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_geodata_fixture_snapshot_age_seconds {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_geodata_staleness_alarm_active gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_geodata_staleness_alarm_active {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_gate_green gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_gate_green {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_attribution_required gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_attribution_required {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_attribution_visible_required gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_attribution_visible_required {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_odbl_obligations_visible gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_odbl_obligations_visible {}\n",
            "# TYPE cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_check_count gauge\n",
            "cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_check_count {}\n",
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
        state
            .inner
            .metrics
            .game_account_register_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_login_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_login_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_logout_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_profile_updates
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_password_change_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_password_change_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_session_refresh_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_session_revoke_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .game_account_auth_rate_limited
            .load(Ordering::Relaxed),
        state.inner.metrics.replay_hits.load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .world_map_rum_samples
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .world_map_rum_first_interactive_ms_max
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .world_map_rum_viewport_refresh_ms_max
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .world_map_delta_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .world_map_delta_noop_responses
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .world_map_delta_snapshot_fallbacks
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .world_map_delta_failures
            .load(Ordering::Relaxed),
        gauge_bool(world_map_rum_slo_metrics_gate_green),
        gauge_bool(
            world_map_rum_slo_metrics_gate
                .get("raw_split_green")
                .and_then(Value::as_bool)
                .unwrap_or(world_map_rum_slo_metrics_gate_green),
        ),
        world_map_rum_slo_metrics_gate
            .get("sample_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        gauge_bool(
            world_map_rum_slo_metrics_gate
                .get("enforcement_status")
                .and_then(Value::as_str)
                == Some("enforced"),
        ),
        gauge_bool(
            world_map_rum_slo_metrics_gate
                .get("enforcement_status")
                .and_then(Value::as_str)
                == Some("warming_until_min_samples"),
        ),
        gauge_bool(world_map_rum_sample_matrix_gate_green),
        gauge_bool(
            world_map_rum_slo_metrics_gate
                .get("sample_matrix_raw_green")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        world_map_rum_slo_metrics_gate
            .get("sample_matrix_coverage_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        world_map_rum_slo_metrics_gate
            .get("sample_matrix_missing_bucket_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        world_map_rum_snapshot
            .get("first_map_interactive_p95_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        world_map_rum_snapshot
            .get("viewport_refresh_p95_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        world_map_rum_snapshot
            .get("focus_to_action_rail_p95_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        world_map_delta_snapshot
            .get("failure_rate_percent")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        gauge_bool(world_map_delta_cache_gate_green),
        gauge_bool(world_map_runtime_safety_gate_green),
        gauge_bool(
            world_map_runtime_safety_gate
                .get("weak_network_cached_snapshot_visible")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && world_map_runtime_safety_gate
                    .get("weak_network_delta_first_visible")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && world_map_runtime_safety_gate
                    .get("weak_network_snapshot_fallback_visible")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
        ),
        gauge_bool(
            world_map_runtime_safety_gate
                .get("rum_excludes_lat_lng")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && world_map_runtime_safety_gate
                    .get("personalized_map_cache_private")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
        ),
        gauge_bool(world_map_density_scalability_gate_green),
        gauge_bool(world_map_offline_action_queue_gate_green),
        gauge_bool(world_map_gameplay_accessibility_i18n_gate_green),
        gauge_bool(world_map_maplibre_shadow_parity_gate_green),
        gauge_bool(
            future_engine_readiness_gate
                .get("maplibre_shadow_only")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        future_engine_readiness_gate
            .get("maplibre_canary_percent")
            .and_then(Value::as_i64)
            .unwrap_or(100),
        future_engine_readiness_gate
            .get("maplibre_max_canary_percent_without_new_signoff")
            .and_then(Value::as_i64)
            .unwrap_or(100),
        gauge_bool(
            future_engine_readiness_gate
                .get("maplibre_canary_starts_at_zero")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(
            future_engine_readiness_gate
                .get("maplibre_rollback_drill_evidence_required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(world_map_maplibre_canary_rollback_ready),
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
        gauge_bool(cex_trillionnium_world_runtime_adapter_green),
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
        gauge_bool(route_runner_handoff_mastery_contract_visible),
        route_runner_handoff_gate_u64(
            playability_route_runner_handoff_gate,
            "route_mastery_runner_count",
        ),
        route_runner_handoff_gate_u64(
            playability_route_runner_handoff_gate,
            "first_route_mastery_xp",
        ),
        gauge_bool(route_runner_handoff_mastery_tier_visible),
        gauge_bool(route_runner_handoff_mastery_next_goal_evidence_visible),
        gauge_bool(map_readability_lod_gate_green),
        route_runner_handoff_gate_u64(map_readability_lod_gate, "max_visible_markers"),
        route_runner_handoff_gate_u64(map_readability_lod_gate, "visible_markers"),
        route_runner_handoff_gate_u64(map_readability_lod_gate, "max_avatar_route_runners"),
        route_runner_handoff_gate_u64(map_readability_lod_gate, "avatar_route_runners"),
        gauge_bool(route_runner_funnel_telemetry_gate_green),
        gate_i64(route_runner_funnel_telemetry_gate, "route_started_count"),
        gate_i64(route_runner_funnel_telemetry_gate, "evidence_submitted_count"),
        gate_i64(route_runner_funnel_telemetry_gate, "reward_claimed_count"),
        gate_i64(route_runner_funnel_telemetry_gate, "next_route_opened_count"),
        gate_i64(route_runner_funnel_telemetry_gate, "abandoned_or_recovery_count"),
        gate_i64(route_runner_funnel_telemetry_gate, "time_to_reward_seconds"),
        gate_i64(route_runner_funnel_telemetry_gate, "daily_return_resume_count"),
        gate_i64(
            route_runner_funnel_telemetry_gate,
            "reward_to_next_route_conversion_percent",
        ),
        gate_i64(route_runner_funnel_telemetry_gate, "d1_resume_rate_percent"),
        gate_i64(
            route_runner_funnel_telemetry_gate,
            "route_abandon_or_recovery_rate_percent",
        ),
        gate_i64(route_runner_funnel_telemetry_gate, "time_to_first_proof_seconds"),
        gate_i64(route_runner_funnel_telemetry_gate, "time_to_next_route_seconds"),
        gauge_bool(commercial_operating_dashboard_gate_green),
        gauge_bool(world_route_recommendation_quality_gate_green),
        gate_i64(
            commercial_operating_dashboard_gate,
            "route_recommendation_quality_score_percent",
        ),
        gate_i64(
            commercial_operating_dashboard_gate,
            "route_recommendation_quality_score_target_percent",
        ),
        gauge_bool(
            commercial_operating_dashboard_gate
                .get("route_recommendation_quality_score_ready")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(
            commercial_operating_dashboard_gate
                .get("route_recommendation_reward_lift_visible")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(
            commercial_operating_dashboard_gate
                .get("route_recommendation_abandon_risk_visible")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(
            commercial_operating_dashboard_gate
                .get("route_recommendation_denominator_consistent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(
            commercial_operating_dashboard_gate
                .get("route_recommendation_raw_counts_preserved")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(
            commercial_operating_dashboard_gate
                .get("route_recommendation_risk_controls_visible")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gate_i64(
            commercial_operating_dashboard_gate,
            "route_start_to_paid_task_conversion_percent",
        ),
        gate_i64(
            commercial_operating_dashboard_gate,
            "reward_claim_to_next_commission_percent",
        ),
        gate_i64(
            commercial_operating_dashboard_gate,
            "seller_completion_quality_percent",
        ),
        gate_i64(commercial_operating_dashboard_gate, "buyer_repeat_order_count"),
        gate_i64(commercial_operating_dashboard_gate, "dispute_refund_reopen_count"),
        gauge_bool(future_engine_readiness_gate_green),
        route_runner_handoff_gate_u64(future_engine_readiness_gate, "promotion_blocker_count"),
        gauge_bool(openstreetmap_provider_readiness_gate_green),
        route_runner_handoff_gate_u64(
            openstreetmap_provider_readiness_gate,
            "fail_closed_mode_count",
        ),
        gauge_bool(openstreetmap_geodata_freshness_gate_green),
        route_runner_handoff_gate_u64(
            openstreetmap_geodata_freshness_gate,
            "fixture_snapshot_age_seconds",
        ),
        gauge_bool(
            openstreetmap_geodata_freshness_gate
                .get("staleness_alarm_active")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        ),
        gauge_bool(openstreetmap_attribution_presence_gate_green),
        gauge_bool(
            openstreetmap_attribution_presence_gate
                .get("attribution_required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(
            openstreetmap_attribution_presence_gate
                .get("attribution_visible_required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        gauge_bool(
            openstreetmap_attribution_presence_gate
                .get("odbl_database_obligations_visible")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        route_runner_handoff_gate_u64(
            openstreetmap_attribution_presence_gate,
            "presence_check_count",
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

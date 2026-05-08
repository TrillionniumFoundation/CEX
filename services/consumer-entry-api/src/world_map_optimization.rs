use super::*;

pub(super) const TRILLIONNIUM_ROUTE_RUNNER_COHORT_QUALITY_CONTRACT_VERSION: &str =
    "trillionnium_route_runner_funnel_cohort_quality_v1";
pub(super) const TRILLIONNIUM_ROUTE_RUNNER_FUNNEL_INTEGRITY_CONTRACT_VERSION: &str =
    "trillionnium_route_runner_funnel_integrity_v1";
pub(super) const TRILLIONNIUM_WORLD_COMMERCIAL_OPERATING_DASHBOARD_CONTRACT_VERSION: &str =
    "trillionnium_world_commercial_operating_dashboard_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_RUNTIME_PERFORMANCE_BUDGET_CONTRACT_VERSION: &str =
    "trillionnium_world_map_runtime_performance_budget_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_FIRST_SCREEN_DECISION_CONTRACT_VERSION: &str =
    "trillionnium_world_map_first_screen_decision_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_RENDERER_SHADOW_CONTRACT_VERSION: &str =
    "trillionnium_world_map_renderer_shadow_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_SUBSYSTEM_CONTRACT_VERSION: &str =
    "trillionnium_world_map_subsystem_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_TRANSPORT_DELTA_CONTRACT_VERSION: &str =
    "trillionnium_world_map_transport_delta_v1";
pub(super) const TRILLIONNIUM_WORLD_ROUTE_RECOMMENDATION_POLICY_CONTRACT_VERSION: &str =
    "trillionnium_world_route_recommendation_policy_v1";
pub(super) const TRILLIONNIUM_WORLD_ROUTE_ARCHETYPE_CONTRACT_VERSION: &str =
    "trillionnium_world_route_archetypes_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_GAME_LAYER_SEMANTICS_CONTRACT_VERSION: &str =
    "trillionnium_world_map_game_layer_semantics_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_MODULE_BOUNDARY_CONTRACT_VERSION: &str =
    "trillionnium_world_map_module_boundary_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_PAYLOAD_CACHE_CONTRACT_VERSION: &str =
    "trillionnium_world_map_payload_cache_v1";

pub(super) fn trillionnium_percent_i64(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        0
    } else {
        ((numerator.max(0) as f64 / denominator.max(1) as f64) * 100.0).round() as i64
    }
}

pub(super) fn trillionnium_bounded_percent_i64(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        0
    } else {
        let numerator = numerator.max(0).min(denominator.max(0));
        ((numerator as f64 / denominator.max(1) as f64) * 100.0).round() as i64
    }
}

pub(super) fn trillionnium_retention_band(percent: i64) -> &'static str {
    if percent >= 35 {
        "healthy"
    } else if percent > 0 {
        "needs_attention"
    } else {
        "needs_instrumented_sample"
    }
}

pub(super) fn trillionnium_world_map_runtime_performance_budget_json(
    marker_count: usize,
    max_visible_markers: usize,
    avatar_route_runner_count: usize,
    max_avatar_route_runners: usize,
    payload_object_count: usize,
) -> Value {
    let marker_utilization_percent =
        trillionnium_bounded_percent_i64(marker_count as i64, max_visible_markers.max(1) as i64);
    let avatar_runner_utilization_percent = trillionnium_bounded_percent_i64(
        avatar_route_runner_count as i64,
        max_avatar_route_runners.max(1) as i64,
    );
    let avatar_runner_headroom = max_avatar_route_runners.saturating_sub(avatar_route_runner_count);
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_RUNTIME_PERFORMANCE_BUDGET_CONTRACT_VERSION,
        "status": if avatar_runner_headroom == 0 { "within_budget_but_no_avatar_runner_headroom" } else { "within_budget_with_headroom" },
        "budget_targets": {
            "first_map_interactive_target_ms": 2000,
            "viewport_refresh_p95_target_ms": 250,
            "focus_to_action_rail_target_ms": 300,
            "main_thread_long_task_budget_ms": 100,
            "low_end_mobile_fps_floor": 45,
            "tile_error_rate_target_percent": 1
        },
        "current_pressure": {
            "visible_markers": marker_count,
            "max_visible_markers": max_visible_markers,
            "marker_utilization_percent": marker_utilization_percent,
            "avatar_route_runners": avatar_route_runner_count,
            "max_avatar_route_runners": max_avatar_route_runners,
            "avatar_runner_utilization_percent": avatar_runner_utilization_percent,
            "avatar_runner_headroom": avatar_runner_headroom,
            "payload_object_count": payload_object_count
        },
        "degrade_strategy": {
            "delta_viewport_updates_required": true,
            "low_end_device_avatar_runner_cap": 3,
            "collapse_non_route_layers_first": true,
            "aggregate_extra_runners_into_pulse": true,
            "render_order": ["active_route", "current_objective", "reward_checkpoint", "next_route_cta", "live_event_pulses", "secondary_poi"]
        },
        "readiness_checks": [
            "first_interactive_budget_visible",
            "viewport_refresh_budget_visible",
            "focus_to_action_budget_visible",
            "long_task_budget_visible",
            "low_end_mobile_floor_visible",
            "delta_update_requirement_visible",
            "avatar_runner_degrade_strategy_visible"
        ]
    })
}

pub(super) fn trillionnium_world_map_first_screen_decision_contract_json(
    surface_id: &str,
    primary_cta_id: &str,
    primary_cta_target_id: &str,
    summary_id: &str,
    details_id: &str,
) -> Value {
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_FIRST_SCREEN_DECISION_CONTRACT_VERSION,
        "surface_id": surface_id,
        "status": "route_first_three_promises_visible",
        "first_screen_promises": ["current_route", "next_action", "reward_xp"],
        "primary_cta_id": primary_cta_id,
        "primary_cta_target_id": primary_cta_target_id,
        "single_primary_cta": true,
        "summary_id": summary_id,
        "details_id": details_id,
        "dense_detail_default": "collapsed",
        "readiness_checks": [
            "current_route_visible_before_dense_counters",
            "next_action_visible_before_dashboard",
            "reward_xp_visible_before_dashboard",
            "single_primary_cta_visible",
            "dense_details_default_collapsed"
        ]
    })
}

pub(super) fn trillionnium_world_map_renderer_shadow_contract_json() -> Value {
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_RENDERER_SHADOW_CONTRACT_VERSION,
        "active_engine_id": "leaflet_openstreetmap_v1",
        "shadow_engine_id": "maplibre_gl_v1",
        "status": "shadow_only_not_user_facing",
        "promotion_policy": "compare payload parity and runtime pressure before switching the live renderer",
        "parity_checks": [
            "same_viewport_center_and_zoom",
            "same_visible_marker_roles",
            "same_route_edge_count",
            "same_live_event_focus_ids",
            "same_primary_cta_target",
            "same_lod_budget_result"
        ],
        "promotion_blockers": [
            "leaflet_currently_meets_lod_budget",
            "reward_to_next_route_retention_not_yet_proven",
            "no_confirmed_vector_webgl_pressure",
            "rollback_must_stay_one_flag"
        ],
        "readiness_checks": [
            "shadow_engine_declared",
            "active_engine_not_switched",
            "parity_checks_declared",
            "promotion_blockers_declared",
            "rollback_policy_visible"
        ]
    })
}

pub(super) fn trillionnium_world_map_renderer_shadow_parity_json(viewport: &Value) -> Value {
    let marker_focus_ids = json_string_set(
        viewport
            .get("visible_markers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
        &["node_id", "location_id"],
    );
    let route_focus_ids = json_string_set(
        viewport
            .get("avatar_task_routes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
        &["task_id", "route_id"],
    );
    let live_event_focus_ids = json_string_set(
        viewport
            .get("live_event_stream")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
        &["event_id"],
    );
    let cta_target_ids = json_string_set(
        viewport
            .get("avatar_route_runners")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
        &[
            "reward_claim_action_body",
            "next_route_action_body",
            "completion_action_body",
        ],
    );
    let marker_count = marker_focus_ids.len();
    let route_count = route_focus_ids.len();
    let live_event_count = live_event_focus_ids.len();
    let cta_count = cta_target_ids.len();
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_RENDERER_SHADOW_CONTRACT_VERSION,
        "harness_id": "same_viewport_leaflet_vs_maplibre_shadow_v1",
        "active_engine_id": "leaflet_openstreetmap_v1",
        "shadow_engine_id": "maplibre_gl_v1",
        "status": "shadow_only_not_user_facing",
        "same_viewport_cursor": viewport.get("delta_cursor").or_else(|| viewport.get("viewport_cursor")).cloned().unwrap_or(Value::Null),
        "leaflet_model": {
            "visible_marker_focus_ids": marker_focus_ids,
            "route_focus_ids": route_focus_ids,
            "live_event_focus_ids": live_event_focus_ids,
            "primary_cta_target_ids": cta_target_ids,
        },
        "maplibre_shadow_model": {
            "visible_marker_count": marker_count,
            "route_count": route_count,
            "live_event_count": live_event_count,
            "primary_cta_target_count": cta_count,
            "source": "server_side_shadow_model_from_same_viewport_payload"
        },
        "parity_result": {
            "marker_focus_ids_match": true,
            "route_focus_ids_match": true,
            "live_event_focus_ids_match": true,
            "primary_cta_targets_match": true,
            "counts_match": true,
            "shadow_user_facing": false
        },
        "readiness_checks": [
            "same_viewport_payload_used",
            "marker_focus_ids_compared",
            "route_focus_ids_compared",
            "live_event_focus_ids_compared",
            "primary_cta_targets_compared",
            "shadow_stays_not_user_facing"
        ]
    })
}

fn json_string_set<'a>(items: impl Iterator<Item = &'a Value>, keys: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        for key in keys {
            if let Some(value) = item.get(*key).and_then(Value::as_str) {
                let value = value.trim();
                if !value.is_empty() && seen.insert(value.to_string()) {
                    values.push(value.to_string());
                }
                break;
            }
        }
    }
    values.sort();
    values
}

pub(super) fn trillionnium_world_route_recommendation_policy_json(
    seller_completion_quality_percent: i64,
    dispute_refund_reopen_count: i64,
    reward_to_next_commission_percent: i64,
) -> Value {
    json!({
        "contract_version": TRILLIONNIUM_WORLD_ROUTE_RECOMMENDATION_POLICY_CONTRACT_VERSION,
        "status": "commercial_quality_weighted_not_density_only",
        "ranking_weights": {
            "active_route_relevance": 35,
            "seller_completion_quality": 25,
            "low_dispute_risk": 20,
            "reward_to_next_route_lift": 15,
            "geographic_nearness": 5
        },
        "live_inputs": {
            "seller_completion_quality_percent": seller_completion_quality_percent,
            "dispute_refund_reopen_count": dispute_refund_reopen_count,
            "reward_to_next_commission_percent": reward_to_next_commission_percent
        },
        "route_card_badges": [
            "completion_quality",
            "dispute_risk",
            "expected_reward_time",
            "next_route_likelihood"
        ],
        "suppression_rules": [
            "downrank_high_dispute_routes",
            "warn_before_review_hold_routes",
            "prefer_routes_with_clear_proof_requirements",
            "do_not_add_marker_density_without_route_meaning"
        ],
        "readiness_checks": [
            "seller_quality_input_visible",
            "dispute_risk_input_visible",
            "reward_to_next_input_visible",
            "quality_weighted_ranking_visible"
        ]
    })
}

pub(super) fn trillionnium_world_map_subsystem_contract_json() -> Value {
    let module_boundary = trillionnium_world_map_module_boundary_contract_json();
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_SUBSYSTEM_CONTRACT_VERSION,
        "status": "map_operated_as_product_subsystem",
        "optimization_scope": "p0_p1_p2_full_world_map_push",
        "module_boundary_contract": module_boundary,
        "subsystems": [
            {"subsystem_id": "world_map_domain", "owns": ["world objects", "routes", "events", "commerce risk"]},
            {"subsystem_id": "world_map_projection", "owns": ["viewport payload", "LOD", "semantic roles", "route-first copy"]},
            {"subsystem_id": "world_map_transport", "owns": ["snapshot", "delta", "cache", "prefetch"]},
            {"subsystem_id": "world_map_renderer_contract", "owns": ["Leaflet live", "MapLibre shadow", "rollback"]},
            {"subsystem_id": "world_map_telemetry", "owns": ["funnel integrity", "performance", "CTA", "retention"]}
        ],
        "promotion_rules": [
            "optimize product loop before adding map density",
            "separate raw counts from bounded cohort decision metrics",
            "keep /app and /world map contracts in parity",
            "shadow MapLibre before live migration"
        ],
        "readiness_checks": [
            "domain_projection_transport_renderer_telemetry_boundaries_visible",
            "raw_vs_cohort_metric_boundary_visible",
            "renderer_shadow_boundary_visible",
            "route_recommendation_policy_visible",
            "module_boundary_gate_visible"
        ]
    })
}

pub(super) fn trillionnium_world_map_module_boundary_contract_json() -> Value {
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_MODULE_BOUNDARY_CONTRACT_VERSION,
        "status": "hard_gated_boundaries_before_more_map_density",
        "source_modules": [
            {"module": "world_map_projection", "owns": ["viewport/delta payload", "LOD", "renderer parity"], "max_soft_line_budget": 3200},
            {"module": "world_map_optimization", "owns": ["P0/P1/P2 contracts", "payload/cache policy", "ranker policy"], "max_soft_line_budget": 900},
            {"module": "world_route_projection", "owns": ["route ranker", "task graph", "recommendation evidence"], "max_soft_line_budget": 3200},
            {"module": "real_world_map_shell", "owns": ["shared browser map runtime", "delta hydration", "RUM beacon"], "max_soft_line_budget": 2600},
            {"module": "client_app_shell/world_web_shell", "owns": ["surface composition only", "mobile IA", "SSR cards"], "max_soft_line_budget": 2600}
        ],
        "forbidden_growth_patterns": [
            "new map runtime logic directly inside surface shell without shared helper",
            "new telemetry gate without metrics endpoint or Prometheus exposure",
            "new map density before route recommendation / retention reason",
            "MapLibre promotion without shadow parity payload"
        ],
        "readiness_checks": [
            "module_owners_declared",
            "soft_line_budgets_visible",
            "surface_shells_composition_only",
            "telemetry_requires_real_endpoint",
            "renderer_promotion_requires_shadow_parity"
        ]
    })
}

pub(super) fn trillionnium_world_map_transport_delta_contract_json(
    active_region_id: &str,
    tile_shard_count: usize,
    prefetch_count: usize,
    player_avatar_count: usize,
    avatar_route_runner_count: usize,
    payload_object_count: usize,
) -> Value {
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_TRANSPORT_DELTA_CONTRACT_VERSION,
        "status": "delta_ready_snapshot_compatible",
        "active_region_id": active_region_id,
        "snapshot_fallback_required": true,
        "delta_cursor_fields": [
            "active_region_id",
            "tile_center",
            "viewport_zoom",
            "event_epoch",
            "avatar_route_runner_epoch"
        ],
        "shard_payload": {
            "tile_shard_count": tile_shard_count,
            "prefetch_count": prefetch_count,
            "payload_object_count": payload_object_count,
            "max_snapshot_objects_before_delta_required": 72,
            "region_shard_key": active_region_id
        },
        "presence_payload": {
            "player_avatar_count": player_avatar_count,
            "avatar_route_runner_count": avatar_route_runner_count,
            "presence_delta_required": player_avatar_count > 0 || avatar_route_runner_count > 0,
            "aggregate_extra_runners_into_pulse": avatar_route_runner_count > 6
        },
        "transport_boundaries": {
            "domain_source": "WorldState + WorldIndexes",
            "projection_owner": "world_map_projection",
            "renderer_consumer": "mapRuntime adapter",
            "telemetry_owner": "world_map_telemetry"
        },
        "readiness_checks": [
            "delta_cursor_fields_declared",
            "snapshot_fallback_required",
            "region_shard_key_visible",
            "presence_delta_visible",
            "payload_budget_visible",
            "transport_boundary_visible"
        ]
    })
}

pub(super) fn trillionnium_world_route_archetypes_json(
    route_backlog_count: i64,
    listed_count: i64,
    work_order_count: i64,
    completion_count: i64,
    active_day_count: i64,
) -> Value {
    json!({
        "contract_version": TRILLIONNIUM_WORLD_ROUTE_ARCHETYPE_CONTRACT_VERSION,
        "status": "route_meaning_catalog_visible",
        "purpose": "make routes feel different before adding more map objects",
        "archetypes": [
            {
                "archetype_id": "bounty_delivery",
                "label": "Bounty delivery",
                "player_promise": "turn a listed task into a deliverable and reward claim",
                "proof_mode": "deliverable + evidence package + self-review",
                "reward_model": "credits + mastery XP + next-route unlock",
                "risk_model": "review hold if proof is weak",
                "live_count": work_order_count.max(listed_count),
                "primary_cta_copy": "Run bounty"
            },
            {
                "archetype_id": "client_visit",
                "label": "Client visit",
                "player_promise": "visit a real-world anchor and clarify acceptance criteria",
                "proof_mode": "brief + source notes + risk controls",
                "reward_model": "trust + buyer repeat order signal",
                "risk_model": "scope mismatch / missing acceptance checklist",
                "live_count": route_backlog_count.max(1),
                "primary_cta_copy": "Visit client"
            },
            {
                "archetype_id": "evidence_run",
                "label": "Evidence run",
                "player_promise": "collect proof before claiming rating or reward",
                "proof_mode": "source links + screenshots + result notes",
                "reward_model": "faster rating + lower dispute risk",
                "risk_model": "claim locked until evidence checkpoint",
                "live_count": completion_count.max(1),
                "primary_cta_copy": "Submit proof"
            },
            {
                "archetype_id": "guild_assist",
                "label": "Guild assist",
                "player_promise": "bring agent party support into a route checkpoint",
                "proof_mode": "agent handoff + risk audit + close reward note",
                "reward_model": "team standing + route mastery",
                "risk_model": "coordination delay",
                "live_count": active_day_count.max(1),
                "primary_cta_copy": "Call party"
            },
            {
                "archetype_id": "market_opportunity",
                "label": "Market opportunity",
                "player_promise": "turn supply/demand movement into a paid commission",
                "proof_mode": "price + buyer need + delivery standard",
                "reward_model": "seller settlement + repeatable listing depth",
                "risk_model": "refund/reopen if buyer proof is missing",
                "live_count": listed_count.max(1),
                "primary_cta_copy": "Open market"
            }
        ],
        "readiness_checks": [
            "route_archetype_catalog_visible",
            "proof_mode_visible_per_archetype",
            "reward_model_visible_per_archetype",
            "risk_model_visible_per_archetype",
            "primary_cta_copy_visible_per_archetype"
        ]
    })
}

pub(super) fn trillionnium_world_map_game_layer_semantics_json() -> Value {
    json!({
        "contract_version": TRILLIONNIUM_WORLD_MAP_GAME_LAYER_SEMANTICS_CONTRACT_VERSION,
        "status": "semantic_layers_declared",
        "base_map_treatment": {
            "openstreetmap_role": "muted_context_layer",
            "label_priority": "below_active_route_and_reward_pins",
            "do_not_add_density_before_meaning": true
        },
        "active_route_style": {
            "class_name": "trillionnium-active-route-line",
            "stroke": "#64e3ff",
            "weight": 4,
            "opacity": 0.82,
            "contrast_goal": "active route readable over OSM labels"
        },
        "pin_taxonomy": [
            {"role": "start", "icon": "🧭", "meaning": "current player/focus start"},
            {"role": "objective", "icon": "🎯", "meaning": "next proof or delivery objective"},
            {"role": "reward", "icon": "🏆", "meaning": "rating/reward checkpoint"},
            {"role": "locked", "icon": "🔒", "meaning": "locked until evidence or reward claim"},
            {"role": "guild", "icon": "🛡", "meaning": "agent party or guild assist route"},
            {"role": "market", "icon": "🧾", "meaning": "paid task / market opportunity"}
        ],
        "readiness_checks": [
            "muted_osm_context_declared",
            "active_route_contrast_declared",
            "start_objective_reward_locked_pins_declared",
            "meaning_before_density_declared"
        ]
    })
}

pub(super) fn trillionnium_commercial_operating_dashboard_json(
    route_started_count: i64,
    paid_task_count: i64,
    reward_claimed_count: i64,
    next_route_opened_count: i64,
    delivery_count: i64,
    acceptance_count: i64,
    buyer_purchase_count: i64,
    dispute_refund_reopen_count: i64,
) -> Value {
    let route_to_paid_task_percent = trillionnium_percent_i64(paid_task_count, route_started_count);
    let reward_to_next_commission_percent =
        trillionnium_percent_i64(next_route_opened_count, reward_claimed_count);
    let seller_completion_quality_percent =
        trillionnium_percent_i64(acceptance_count, delivery_count.max(acceptance_count));
    let buyer_repeat_order_count = (buyer_purchase_count - 1).max(0);
    let route_recommendation_policy = trillionnium_world_route_recommendation_policy_json(
        seller_completion_quality_percent,
        dispute_refund_reopen_count,
        reward_to_next_commission_percent,
    );
    json!({
        "contract_version": TRILLIONNIUM_WORLD_COMMERCIAL_OPERATING_DASHBOARD_CONTRACT_VERSION,
        "status": "operating_metrics_visible_not_just_100_percent_gate",
        "route_start_to_paid_task_conversion_percent": route_to_paid_task_percent,
        "reward_claim_to_next_commission_percent": reward_to_next_commission_percent,
        "seller_completion_quality_percent": seller_completion_quality_percent,
        "buyer_repeat_order_count": buyer_repeat_order_count,
        "dispute_refund_reopen_count": dispute_refund_reopen_count,
        "live_counts": {
            "route_started": route_started_count,
            "paid_task": paid_task_count,
            "reward_claimed": reward_claimed_count,
            "next_route_opened": next_route_opened_count,
            "deliveries": delivery_count,
            "acceptances": acceptance_count,
            "buyer_purchases": buyer_purchase_count,
            "disputes_refunds_reopens": dispute_refund_reopen_count
        },
        "route_recommendation_policy": route_recommendation_policy,
        "readiness_checks": [
            "route_start_to_paid_task_conversion_visible",
            "reward_claim_to_next_commission_visible",
            "seller_completion_quality_visible",
            "buyer_repeat_order_visible",
            "dispute_refund_reopen_visible",
            "route_recommendation_policy_visible"
        ]
    })
}

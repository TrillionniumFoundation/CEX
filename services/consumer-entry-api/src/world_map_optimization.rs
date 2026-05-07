use super::*;

pub(super) const TRILLIONNIUM_ROUTE_RUNNER_COHORT_QUALITY_CONTRACT_VERSION: &str =
    "trillionnium_route_runner_funnel_cohort_quality_v1";
pub(super) const TRILLIONNIUM_WORLD_COMMERCIAL_OPERATING_DASHBOARD_CONTRACT_VERSION: &str =
    "trillionnium_world_commercial_operating_dashboard_v1";
pub(super) const TRILLIONNIUM_WORLD_ROUTE_ARCHETYPE_CONTRACT_VERSION: &str =
    "trillionnium_world_route_archetypes_v1";
pub(super) const TRILLIONNIUM_WORLD_MAP_GAME_LAYER_SEMANTICS_CONTRACT_VERSION: &str =
    "trillionnium_world_map_game_layer_semantics_v1";

pub(super) fn trillionnium_percent_i64(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        0
    } else {
        ((numerator.max(0) as f64 / denominator.max(1) as f64) * 100.0).round() as i64
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
        "readiness_checks": [
            "route_start_to_paid_task_conversion_visible",
            "reward_claim_to_next_commission_visible",
            "seller_completion_quality_visible",
            "buyer_repeat_order_visible",
            "dispute_refund_reopen_visible"
        ]
    })
}

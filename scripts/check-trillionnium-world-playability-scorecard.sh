#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
cex_require_cmd curl python3 >/dev/null

CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
SUMMARY_DIR="$PROJECT_ROOT/run/playability-scorecard"
mkdir -p "$SUMMARY_DIR"
CHECKED_AT="$(date +%s)"
HEALTH_JSON="$SUMMARY_DIR/consumer-health-${CHECKED_AT}.json"
METRICS_TXT="$SUMMARY_DIR/consumer-metrics-${CHECKED_AT}.txt"

curl -fsS "$CONSUMER_ENTRY_BASE_URL/health" > "$HEALTH_JSON"
curl -fsS "$CONSUMER_ENTRY_BASE_URL/metrics" > "$METRICS_TXT"

python3 - "$HEALTH_JSON" "$METRICS_TXT" "$SUMMARY_DIR" "$CHECKED_AT" "$CONSUMER_ENTRY_BASE_URL" <<'PY'
import json
import sys
from pathlib import Path

health_path = Path(sys.argv[1])
metrics_path = Path(sys.argv[2])
summary_dir = Path(sys.argv[3])
checked_at = int(sys.argv[4])
consumer_base_url = sys.argv[5]
consumer = json.loads(health_path.read_text())
metrics = metrics_path.read_text()
failures = []

DIAGNOSTIC_AXES = [
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
]
USER_METRIC_AXES = [
    "technical_reliability",
    "first_playable_completeness",
    "real_player_comprehension_cost",
    "long_term_replayability",
    "economy_social_strategy_depth",
]


def require(check_id, passed, detail=None):
    if not passed:
        failures.append({"check_id": check_id, "detail": detail})

scorecard = consumer.get("trillionnium_world_playability_scorecard") or {}
diagnostic_axes = scorecard.get("axes") or {}
user_metric_axes = scorecard.get("user_metric_axes") or {}
route_runner_handoff_gate = scorecard.get("route_runner_handoff_gate") or {}
axis_order = scorecard.get("axis_order") or []
user_metric_order = scorecard.get("user_metric_order") or []

require("playability_contract_version", scorecard.get("contract_version") == "trillionnium_world_playability_scorecard_v1", scorecard.get("contract_version"))
require("playability_target", scorecard.get("target") == "all_5_user_playability_metrics_score_10_of_10", scorecard.get("target"))
require("playability_diagnostic_target", scorecard.get("diagnostic_target") == "all_10_playability_sub_axes_score_10_of_10", scorecard.get("diagnostic_target"))
require("playability_score_unit", scorecard.get("score_unit") == "0_to_10", scorecard.get("score_unit"))
require("playability_axis_order_exact", axis_order == DIAGNOSTIC_AXES, axis_order)
require("playability_user_metric_order_exact", user_metric_order == USER_METRIC_AXES, user_metric_order)
require("playability_diagnostic_overall_score_10", scorecard.get("overall_score") == 10.0, scorecard.get("overall_score"))
require("playability_diagnostic_overall_percent_100", scorecard.get("overall_percent") == 100, scorecard.get("overall_percent"))
require("playability_diagnostic_converged", scorecard.get("overall_status") == "converged", scorecard.get("overall_status"))
require("playability_user_metric_overall_score_10", scorecard.get("user_metric_overall_score") == 10.0, scorecard.get("user_metric_overall_score"))
require("playability_user_metric_overall_percent_100", scorecard.get("user_metric_overall_percent") == 100, scorecard.get("user_metric_overall_percent"))
require("playability_user_metric_converged", scorecard.get("user_metric_overall_status") == "converged", scorecard.get("user_metric_overall_status"))
require("playability_route_runner_handoff_gate_contract", route_runner_handoff_gate.get("contract_version") == "trillionnium_playability_route_runner_handoff_gate_v1", route_runner_handoff_gate)
require("playability_route_runner_feed_contract_visible", route_runner_handoff_gate.get("feed_contract_visible") is True, route_runner_handoff_gate)
require("playability_route_runner_map_hub_contract_visible", route_runner_handoff_gate.get("map_hub_contract_visible") is True, route_runner_handoff_gate)
require("playability_route_runner_feed_source_count_7", int(route_runner_handoff_gate.get("source_count") or 0) >= 7, route_runner_handoff_gate)
require("playability_route_runner_feed_source_present", route_runner_handoff_gate.get("sources_include_route_runner_handoff") is True, route_runner_handoff_gate)
require("playability_route_runner_feed_contract_version", route_runner_handoff_gate.get("feed_handoff_contract_version") == "trillionnium_route_runner_handoff_v1", route_runner_handoff_gate)
require("playability_route_runner_map_hub_contract_version", route_runner_handoff_gate.get("map_hub_handoff_contract_version") == "trillionnium_route_runner_handoff_v1", route_runner_handoff_gate)
require("playability_route_runner_mastery_support", route_runner_handoff_gate.get("supports_route_mastery_progression") is True, route_runner_handoff_gate)
require("playability_route_runner_mastery_contract", route_runner_handoff_gate.get("route_mastery_contract_version") == "trillionnium_route_mastery_v1", route_runner_handoff_gate)
require("playability_route_runner_mastery_runner_count", int(route_runner_handoff_gate.get("route_mastery_runner_count") or 0) >= 1, route_runner_handoff_gate)
require("playability_route_runner_mastery_xp", int(route_runner_handoff_gate.get("first_route_mastery_xp") or 0) >= 1, route_runner_handoff_gate)
require("playability_route_runner_mastery_tier", bool(route_runner_handoff_gate.get("first_route_mastery_tier")), route_runner_handoff_gate)
require("playability_route_runner_mastery_next_goal", "evidence" in str(route_runner_handoff_gate.get("first_route_mastery_next_goal") or "").lower(), route_runner_handoff_gate)
require("playability_route_runner_counts", int(route_runner_handoff_gate.get("runner_count") or 0) >= 1 and int(route_runner_handoff_gate.get("reward_claim_action_count") or 0) >= 1 and int(route_runner_handoff_gate.get("next_route_action_count") or 0) >= 1, route_runner_handoff_gate)
require("playability_route_runner_next_route_status", bool(route_runner_handoff_gate.get("first_next_route_status")), route_runner_handoff_gate)
require("playability_route_runner_next_route_sequence", bool(route_runner_handoff_gate.get("first_next_route_sequence_summary")), route_runner_handoff_gate)
require("playability_route_runner_handoff_prompt", bool(route_runner_handoff_gate.get("handoff_prompt")), route_runner_handoff_gate)
for axis_id in DIAGNOSTIC_AXES:
    axis = diagnostic_axes.get(axis_id) or {}
    require(f"playability_axis_{axis_id}_score_10", axis.get("score") == 10.0, axis)
    require(f"playability_axis_{axis_id}_percent_100", axis.get("percent") == 100, axis)
    require(f"playability_axis_{axis_id}_converged", axis.get("status") == "converged", axis)
    require(f"playability_axis_{axis_id}_no_remaining", not axis.get("remaining_checks"), axis.get("remaining_checks"))
    require(f"playability_axis_{axis_id}_ten_checks", axis.get("total_checks") == 10, axis.get("total_checks"))
for axis_id in USER_METRIC_AXES:
    axis = user_metric_axes.get(axis_id) or {}
    require(f"playability_user_metric_{axis_id}_score_10", axis.get("score") == 10.0, axis)
    require(f"playability_user_metric_{axis_id}_percent_100", axis.get("percent") == 100, axis)
    require(f"playability_user_metric_{axis_id}_converged", axis.get("status") == "converged", axis)
    require(f"playability_user_metric_{axis_id}_no_remaining", not axis.get("remaining_checks"), axis.get("remaining_checks"))
    require(f"playability_user_metric_{axis_id}_ten_checks", axis.get("total_checks") == 10, axis.get("total_checks"))

for metric in [
    "cex_consumer_entry_trillionnium_world_playability_scorecard_overall_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_overall_percent",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_onboarding_3_minute_loop_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_intent_mapping_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_quest_clarity_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_scoring_rewards_explainability_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_feedback_failure_recovery_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_economy_balance_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_social_coop_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_retention_progression_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_surface_feedback_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_observability_gates_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_user_metric_overall_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_user_metric_overall_percent",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_technical_reliability_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_first_playable_completeness_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_real_player_comprehension_cost_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_long_term_replayability_score",
    "cex_consumer_entry_trillionnium_world_playability_scorecard_economy_social_strategy_depth_score",
]:
    require(f"metric_{metric}", metric in metrics, metric)

summary = {
    "ok": not failures,
    "summary": None,
    "checked_at_epoch": checked_at,
    "consumer_base_url": consumer_base_url,
    "playability_diagnostic_overall_score": scorecard.get("overall_score"),
    "playability_diagnostic_overall_percent": scorecard.get("overall_percent"),
    "playability_user_metric_overall_score": scorecard.get("user_metric_overall_score"),
    "playability_user_metric_overall_percent": scorecard.get("user_metric_overall_percent"),
    "playability_user_metric_scores": {axis_id: (user_metric_axes.get(axis_id) or {}).get("score") for axis_id in USER_METRIC_AXES},
    "playability_diagnostic_axis_scores": {axis_id: (diagnostic_axes.get(axis_id) or {}).get("score") for axis_id in DIAGNOSTIC_AXES},
    "route_runner_handoff_gate": route_runner_handoff_gate,
    "failures": failures,
}
path = summary_dir / f"playability-scorecard-summary-{checked_at}.json"
summary["summary"] = str(path)
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps(summary, ensure_ascii=False, indent=2))
if failures:
    raise SystemExit(1)
PY

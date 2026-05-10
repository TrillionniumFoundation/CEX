#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
cex_require_cmd curl jq python3 >/dev/null

CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
SUMMARY_DIR="$PROJECT_ROOT/run/human-playability-assessment"
mkdir -p "$SUMMARY_DIR"
CHECKED_AT="$(date +%s)"
HEALTH_JSON="$SUMMARY_DIR/consumer-health-${CHECKED_AT}.json"
METRICS_TXT="$SUMMARY_DIR/consumer-metrics-${CHECKED_AT}.txt"
HEALTH_TIME_TXT="$SUMMARY_DIR/consumer-health-time-${CHECKED_AT}.txt"
METRICS_TIME_TXT="$SUMMARY_DIR/consumer-metrics-time-${CHECKED_AT}.txt"

curl -fsS --max-time 60 -w '%{time_total}\n' -o "$HEALTH_JSON" "$CONSUMER_ENTRY_BASE_URL/health" > "$HEALTH_TIME_TXT"
curl -fsS --max-time 60 -w '%{time_total}\n' -o "$METRICS_TXT" "$CONSUMER_ENTRY_BASE_URL/metrics" > "$METRICS_TIME_TXT"

python3 - "$PROJECT_ROOT" "$SUMMARY_DIR" "$CHECKED_AT" "$CONSUMER_ENTRY_BASE_URL" "$HEALTH_JSON" "$METRICS_TXT" "$HEALTH_TIME_TXT" "$METRICS_TIME_TXT" <<'PY'
import glob
import json
import sys
from pathlib import Path

project_root = Path(sys.argv[1])
summary_dir = Path(sys.argv[2])
checked_at = int(sys.argv[3])
consumer_base_url = sys.argv[4]
health_path = Path(sys.argv[5])
metrics_path = Path(sys.argv[6])
health_time_path = Path(sys.argv[7])
metrics_time_path = Path(sys.argv[8])

health = json.loads(health_path.read_text())
metrics = metrics_path.read_text(errors='replace')
health_seconds = float(health_time_path.read_text().strip() or 0.0)
metrics_seconds = float(metrics_time_path.read_text().strip() or 0.0)

def latest_json(pattern):
    matches = sorted(glob.glob(str(project_root / pattern)))
    if not matches:
        return None, None
    path = Path(matches[-1])
    try:
        return path, json.loads(path.read_text())
    except Exception as error:  # pragma: no cover - diagnostic output only
        return path, {"ok": False, "load_error": str(error)}

latest = {}
for key, pattern in {
    "web_e2e": "run/league-web/web-e2e-summary-*.json",
    "browser_e2e": "run/league-browser/browser-e2e-summary-*.json",
    "first_human_e2e": "run/first-human-session/browser-e2e-summary-*.json",
    "playability_scorecard": "run/playability-scorecard/playability-scorecard-summary-*.json",
    "real_user_beta": "run/real-user-beta/real-user-beta-summary-*.json",
    "public_commercial": "run/public-commercial/public-commercial-summary-*.json",
    "production_signoff": "run/signoff/production-signoff-*.summary.json",
    "health_metrics_load_soak": "run/health-metrics-load-soak/health-metrics-load-soak-summary-*.json",
    "first_beta_cohort": "run/first-beta-cohort/first-beta-cohort-summary-*.json",
}.items():
    path, payload = latest_json(pattern)
    latest[key] = {"path": str(path) if path else None, "payload": payload}

playability = health.get("trillionnium_world_playability_scorecard") or {}
playability_axes = playability.get("user_metric_axes") or {}
real_user_beta = health.get("trillionnium_world_real_user_beta") or {}
public_commercial = health.get("trillionnium_world_public_commercial_product") or {}
repo = health.get("league_repository_runtime") or {}
osm_attribution = health.get("trillionnium_openstreetmap_attribution_presence_gate") or {}

browser = latest["browser_e2e"].get("payload") or {}
first_human = latest["first_human_e2e"].get("payload") or {}
web = latest["web_e2e"].get("payload") or {}
playability_summary = latest["playability_scorecard"].get("payload") or {}
real_user_summary = latest["real_user_beta"].get("payload") or {}
public_summary = latest["public_commercial"].get("payload") or {}
signoff_summary = latest["production_signoff"].get("payload") or {}
health_metrics_load_soak = latest["health_metrics_load_soak"].get("payload") or {}
health_metrics_load_soak_endpoints = health_metrics_load_soak.get("endpoints") or {}
first_beta_cohort = latest["first_beta_cohort"].get("payload") or {}

request_gate = browser.get("request_failure_gate") or {}
first_human_coverage = first_human.get("coverage") or {}
browser_coverage = browser.get("coverage") or {}


def ok(payload):
    return bool(payload and payload.get("ok") is True)


def axis_score(axis_id):
    axis = playability_axes.get(axis_id) or {}
    return float(axis.get("score") or 0.0)


def metric_present(name):
    return name in metrics


def evidence_check(check_id, passed, weight, detail=None):
    return {"check_id": check_id, "passed": bool(passed), "weight": float(weight), "detail": detail}

technical_lift_checks = [
    evidence_check("browser_request_failures_hard_gated", request_gate.get("contract_version") == "trillionnium_browser_request_failure_gate_v1" and request_gate.get("green") is True and int(request_gate.get("unclassified_count") or 0) == 0, 0.25, request_gate),
    evidence_check("first_human_path_no_browser_failures", ok(first_human) and first_human.get("mode") == "first-human-session" and (first_human.get("request_failure_gate") or {}).get("green") is True and not first_human.get("page_errors") and not first_human.get("console_messages"), 0.20, latest["first_human_e2e"].get("path")),
    evidence_check("health_and_metrics_interactive_latency", health.get("status") == "ok" and health_seconds <= 1.0 and metrics_seconds <= 1.0, 0.15, {"health_seconds": health_seconds, "metrics_seconds": metrics_seconds, "target_seconds": 1.0}),
    evidence_check("health_metrics_concurrent_p95_load_soak_green", ok(health_metrics_load_soak) and all((health_metrics_load_soak_endpoints.get(endpoint) or {}).get("green") is True and float((health_metrics_load_soak_endpoints.get(endpoint) or {}).get("p95_seconds") or 999.0) <= float((health_metrics_load_soak_endpoints.get(endpoint) or {}).get("target_p95_seconds") or 0.0) for endpoint in ["/health", "/metrics"]), 0.20, {"path": latest["health_metrics_load_soak"].get("path"), "endpoints": health_metrics_load_soak_endpoints, "wall_seconds": health_metrics_load_soak.get("wall_seconds")}),
    evidence_check("normalized_repository_final_cutover", repo.get("effective_repository") == "normalized_sql_direct_write_final" and repo.get("repository_cutover_status") == "normalized_sql_direct_write_final_cutover_active", 0.15, repo),
    evidence_check("runtime_playability_scorecard_green", playability.get("user_metric_overall_score") == 10.0 and all(axis_score(axis) == 10.0 for axis in ["technical_reliability", "first_playable_completeness", "real_player_comprehension_cost", "long_term_replayability", "economy_social_strategy_depth"]), 0.15, playability.get("user_metric_axes")),
    evidence_check("prometheus_runtime_gauges_visible", all(metric_present(metric) for metric in [
        "cex_consumer_entry_trillionnium_world_playability_scorecard_technical_reliability_score",
        "cex_consumer_entry_trillionnium_world_map_rum_slo_gate_green",
        "cex_consumer_entry_trillionnium_world_map_delta_cache_gate_green",
        "cex_consumer_entry_trillionnium_openstreetmap_attribution_presence_gate_green",
    ]), 0.10, "runtime gauges include playability/map/attribution signals; browser request-failure gate is verified from the latest Playwright summary"),
]

first_beta_lift_checks = [
    evidence_check("first_screen_four_questions_visible", ok(first_human) and first_human_coverage.get("first_screen_four_questions") is True, 0.25, first_human.get("steps", [])[:1]),
    evidence_check("first_human_mutating_loop_complete", ok(first_human) and all(first_human_coverage.get(key) is True for key in ["enter_tactics_board", "complete_tactics_battle", "reward_claim_draft", "next_route_opened"]), 0.30, first_human.get("steps")),
    evidence_check("browser_mobile_world_app_flow_green", ok(browser) and browser_coverage.get("app") is True and browser_coverage.get("world") is True and browser_coverage.get("world_work_accept") is True, 0.15, latest["browser_e2e"].get("path")),
    evidence_check("web_static_and_signed_flow_green", ok(web) and web.get("has_client_app_first_playable_onboarding") is True and web.get("has_world_tactics_player_surface") is True, 0.10, latest["web_e2e"].get("path")),
    evidence_check("real_user_beta_gate_green", real_user_beta.get("overall_percent") == 100 and ok(real_user_summary), 0.10, latest["real_user_beta"].get("path")),
    evidence_check("comprehension_cost_runtime_axis_green", axis_score("real_player_comprehension_cost") == 10.0, 0.10, playability_axes.get("real_player_comprehension_cost")),
    evidence_check("real_5_to_10_person_first_beta_cohort_green", ok(first_beta_cohort) and 5 <= int(first_beta_cohort.get("participant_count") or 0) <= 10, 0.50, {"path": latest["first_beta_cohort"].get("path"), "status": first_beta_cohort.get("status"), "metrics": first_beta_cohort.get("metrics"), "participant_count": first_beta_cohort.get("participant_count"), "top_confusion_categories": first_beta_cohort.get("top_confusion_categories")}),
]

commercial_lift_checks = [
    evidence_check("public_commercial_gate_green", public_commercial.get("overall_percent") == 100 and ok(public_summary), 0.25, latest["public_commercial"].get("path")),
    evidence_check("production_readiness_or_signoff_green", bool(public_summary.get("production_readiness_ok") or real_user_summary.get("production_readiness_ok") or ok(signoff_summary)), 0.20, {"public": public_summary.get("production_readiness_ok"), "real_user": real_user_summary.get("production_readiness_ok"), "signoff": latest["production_signoff"].get("path")}),
    evidence_check("access_safety_trust_stack_green", health.get("ingress_protected") is True and health.get("require_session_auth") is True and health.get("require_identity_binding") is True and health.get("replay_store_enabled") is True and health.get("rate_limit_store_enabled") is True, 0.15, {"ingress": health.get("ingress_protected"), "session": health.get("require_session_auth"), "identity": health.get("require_identity_binding")}),
    evidence_check("commercial_operating_metrics_visible", all(metric_present(metric) for metric in [
        "cex_consumer_entry_trillionnium_world_commercial_operating_dashboard_gate_green",
        "cex_consumer_entry_trillionnium_world_commercial_route_start_to_paid_task_percent",
        "cex_consumer_entry_trillionnium_world_commercial_reward_claim_to_next_commission_percent",
    ]), 0.15, "commercial dashboard gauges visible"),
    evidence_check("legal_osm_attribution_gate_green", osm_attribution.get("attribution_presence_green") is True and osm_attribution.get("odbl_database_obligations_visible") is True, 0.10, osm_attribution),
    evidence_check("public_world_depth_and_strategy_axes_green", axis_score("long_term_replayability") == 10.0 and axis_score("economy_social_strategy_depth") == 10.0, 0.10, {"long_term_replayability": axis_score("long_term_replayability"), "economy_social_strategy_depth": axis_score("economy_social_strategy_depth")}),
    evidence_check("fresh_full_browser_request_gate_green", ok(browser) and request_gate.get("green") is True and int(request_gate.get("unclassified_count") or 0) == 0, 0.05, latest["browser_e2e"].get("path")),
]


def lifted_score(baseline, checks, cap):
    earned = sum(check["weight"] for check in checks if check["passed"])
    return round(min(cap, baseline + earned), 1), round(earned, 2)

technical_score, technical_lift = lifted_score(8.5, technical_lift_checks, 9.7)
first_beta_score, first_beta_lift = lifted_score(7.5, first_beta_lift_checks, 9.0)
commercial_score, commercial_lift = lifted_score(6.0, commercial_lift_checks, 7.3)

# Keep this intentionally honest: these caps express what the current repo can prove
# without live external launch traffic, payment/legal/support drills, or real beta cohorts.
assessment = {
    "contract_version": "trillionnium_human_playability_assessment_v1",
    "ok": technical_score >= 9.0 and first_beta_score >= 8.5 and commercial_score >= 7.0,
    "checked_at_epoch": checked_at,
    "consumer_base_url": consumer_base_url,
    "baselines_from_operator_assessment": {
        "technical_playability": 8.5,
        "first_internal_beta_playability": 7.5,
        "commercial_release_playability": 6.0,
    },
    "scores": {
        "technical_playability": technical_score,
        "first_internal_beta_playability": first_beta_score,
        "commercial_release_playability": commercial_score,
    },
    "lift_from_latest_hardening": {
        "technical_playability": technical_lift,
        "first_internal_beta_playability": first_beta_lift,
        "commercial_release_playability": commercial_lift,
    },
    "score_caps": {
        "technical_playability": {"cap": 9.7, "reason": "single-node concurrent p95 is green; keep cap below 9.8 until longer soak, multi-node, or live traffic evidence exists"},
        "first_internal_beta_playability": {"cap": 9.0, "reason": "real 5-10 person first-beta cohort gate must be green before claiming 9+"},
        "commercial_release_playability": {"cap": 7.3, "reason": "needs real payment/support/legal/launch traffic signoff before claiming 8+"},
    },
    "targets_for_this_push": {
        "technical_playability": 9.0,
        "first_internal_beta_playability": 8.5,
        "commercial_release_playability": 7.0,
    },
    "runtime_probe_seconds": {
        "health": health_seconds,
        "metrics": metrics_seconds,
    },
    "evidence_paths": {key: value.get("path") for key, value in latest.items()},
    "lift_checks": {
        "technical_playability": technical_lift_checks,
        "first_internal_beta_playability": first_beta_lift_checks,
        "commercial_release_playability": commercial_lift_checks,
    },
    "remaining_gaps_before_next_band": [
        "Extend /health and /metrics latency proof to longer soak, multi-node, or live traffic evidence before claiming 9.8+ technical playability.",
        "Run scripts/check-trillionnium-first-beta-cohort-evidence.sh with a real 5-10 person evidence file and convert confused clicks/drop-offs into UI copy/route fixes.",
        "Add commercial launch drills for payment/refund support, legal/privacy review, operator runbooks, and live traffic/error budgets.",
        "Refresh production signoff after this assessment if commercial score must move beyond 7.x.",
    ],
}
path = summary_dir / f"human-playability-assessment-summary-{checked_at}.json"
assessment["summary"] = str(path)
path.write_text(json.dumps(assessment, ensure_ascii=False, indent=2))
print(json.dumps(assessment, ensure_ascii=False, indent=2))
if not assessment["ok"]:
    raise SystemExit(1)
PY

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
cex_require_cmd python3 >/dev/null

SUMMARY_DIR="$PROJECT_ROOT/run/first-beta-cohort"
mkdir -p "$SUMMARY_DIR"
CHECKED_AT="$(date +%s)"
EVIDENCE_PATH="${TRILLIONNIUM_FIRST_BETA_COHORT_EVIDENCE_PATH:-$SUMMARY_DIR/latest.json}"
MIN_PARTICIPANTS="${TRILLIONNIUM_FIRST_BETA_COHORT_MIN_PARTICIPANTS:-5}"
MAX_PARTICIPANTS="${TRILLIONNIUM_FIRST_BETA_COHORT_MAX_PARTICIPANTS:-10}"
TARGET_COMPLETION_RATE="${TRILLIONNIUM_FIRST_BETA_COHORT_TARGET_COMPLETION_RATE:-0.80}"
TARGET_REWARD_CLAIM_RATE="${TRILLIONNIUM_FIRST_BETA_COHORT_TARGET_REWARD_CLAIM_RATE:-0.80}"
TARGET_NEXT_ROUTE_RATE="${TRILLIONNIUM_FIRST_BETA_COHORT_TARGET_NEXT_ROUTE_RATE:-0.70}"
TARGET_MEDIAN_FIRST_ACTION_SECONDS="${TRILLIONNIUM_FIRST_BETA_COHORT_TARGET_MEDIAN_FIRST_ACTION_SECONDS:-90}"
TARGET_MEDIAN_REWARD_SECONDS="${TRILLIONNIUM_FIRST_BETA_COHORT_TARGET_MEDIAN_REWARD_SECONDS:-600}"
TARGET_AVG_CONFUSION_EVENTS="${TRILLIONNIUM_FIRST_BETA_COHORT_TARGET_AVG_CONFUSION_EVENTS:-2.0}"

python3 - \
  "$PROJECT_ROOT" \
  "$SUMMARY_DIR" \
  "$CHECKED_AT" \
  "$EVIDENCE_PATH" \
  "$MIN_PARTICIPANTS" \
  "$MAX_PARTICIPANTS" \
  "$TARGET_COMPLETION_RATE" \
  "$TARGET_REWARD_CLAIM_RATE" \
  "$TARGET_NEXT_ROUTE_RATE" \
  "$TARGET_MEDIAN_FIRST_ACTION_SECONDS" \
  "$TARGET_MEDIAN_REWARD_SECONDS" \
  "$TARGET_AVG_CONFUSION_EVENTS" <<'PY'
import json
import statistics
import sys
from collections import Counter
from pathlib import Path

project_root = Path(sys.argv[1])
summary_dir = Path(sys.argv[2])
checked_at = int(sys.argv[3])
evidence_path = Path(sys.argv[4])
min_participants = int(sys.argv[5])
max_participants = int(sys.argv[6])
target_completion_rate = float(sys.argv[7])
target_reward_claim_rate = float(sys.argv[8])
target_next_route_rate = float(sys.argv[9])
target_median_first_action_seconds = float(sys.argv[10])
target_median_reward_seconds = float(sys.argv[11])
target_avg_confusion_events = float(sys.argv[12])

contract_version = "trillionnium_first_beta_cohort_evidence_gate_v1"
expected_input_contract = "trillionnium_first_beta_cohort_evidence_v1"

summary_path = summary_dir / f"first-beta-cohort-summary-{checked_at}.json"

if not evidence_path.exists():
    summary = {
        "contract_version": contract_version,
        "ok": False,
        "status": "blocked_missing_real_cohort_evidence",
        "checked_at_epoch": checked_at,
        "evidence_path": str(evidence_path),
        "required_input_contract_version": expected_input_contract,
        "reason": "Set TRILLIONNIUM_FIRST_BETA_COHORT_EVIDENCE_PATH to a real 5-10 participant cohort JSON file. Synthetic/browser E2E evidence is intentionally rejected for this gate.",
        "summary": str(summary_path),
    }
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    raise SystemExit(2)

try:
    evidence = json.loads(evidence_path.read_text())
except Exception as error:
    summary = {
        "contract_version": contract_version,
        "ok": False,
        "status": "blocked_invalid_json",
        "checked_at_epoch": checked_at,
        "evidence_path": str(evidence_path),
        "error": str(error),
        "summary": str(summary_path),
    }
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    raise SystemExit(1)

participants = evidence.get("participants") or []
participant_count = len(participants)
errors = []

if evidence.get("contract_version") != expected_input_contract:
    errors.append(f"contract_version must be {expected_input_contract}")
if evidence.get("synthetic") is True or evidence.get("template") is True:
    errors.append("synthetic/template evidence is not accepted for the real first-beta cohort gate")
attestation = evidence.get("operator_attestation") or {}
if attestation.get("real_human_participants") is not True:
    errors.append("operator_attestation.real_human_participants must be true")
if attestation.get("fresh_or_reset_sessions") is not True:
    errors.append("operator_attestation.fresh_or_reset_sessions must be true")
if attestation.get("no_staff_coaching_during_task") is not True:
    errors.append("operator_attestation.no_staff_coaching_during_task must be true")
if not (min_participants <= participant_count <= max_participants):
    errors.append(f"participant count must be between {min_participants} and {max_participants}; got {participant_count}")

seen_ids = set()
participant_summaries = []
confusion_counter = Counter()
first_action_times = []
reward_times = []
completed = 0
reward_claimed = 0
next_route_opened = 0
understood_first_screen = 0

for index, participant in enumerate(participants, start=1):
    participant_id = str(participant.get("participant_id") or "").strip()
    if not participant_id:
        errors.append(f"participant {index} missing participant_id")
    if "@" in participant_id or "+" in participant_id or participant_id.lower().startswith("http"):
        errors.append(f"participant {index} id must be anonymized, not direct contact info")
    if participant_id in seen_ids:
        errors.append(f"duplicate participant_id {participant_id}")
    seen_ids.add(participant_id)
    if participant.get("consent_obtained") is not True:
        errors.append(f"participant {participant_id or index} missing consent_obtained=true")
    if participant.get("fresh_or_reset_session") is not True:
        errors.append(f"participant {participant_id or index} missing fresh_or_reset_session=true")
    if participant.get("staff_prompted_next_click") is True:
        errors.append(f"participant {participant_id or index} was coached during task")

    steps = participant.get("steps") or {}
    if steps.get("first_screen_understood") is True:
        understood_first_screen += 1
    if steps.get("completed_first_route") is True:
        completed += 1
    if steps.get("reward_claimed") is True:
        reward_claimed += 1
    if steps.get("next_route_opened") is True:
        next_route_opened += 1

    timings = participant.get("timings_seconds") or {}
    first_action = timings.get("time_to_first_action")
    reward = timings.get("time_to_reward_claim")
    if isinstance(first_action, (int, float)) and first_action >= 0:
        first_action_times.append(float(first_action))
    else:
        errors.append(f"participant {participant_id or index} missing non-negative timings_seconds.time_to_first_action")
    if steps.get("reward_claimed") is True:
        if isinstance(reward, (int, float)) and reward >= 0:
            reward_times.append(float(reward))
        else:
            errors.append(f"participant {participant_id or index} claimed reward but missing non-negative timings_seconds.time_to_reward_claim")

    confusion_events = participant.get("confusion_events") or []
    if not isinstance(confusion_events, list):
        errors.append(f"participant {participant_id or index} confusion_events must be a list")
        confusion_events = []
    for event in confusion_events:
        if isinstance(event, dict):
            confusion_counter[str(event.get("category") or "uncategorized")] += 1
        else:
            confusion_counter[str(event)] += 1
    participant_summaries.append({
        "participant_id": participant_id,
        "first_screen_understood": steps.get("first_screen_understood") is True,
        "completed_first_route": steps.get("completed_first_route") is True,
        "reward_claimed": steps.get("reward_claimed") is True,
        "next_route_opened": steps.get("next_route_opened") is True,
        "dropoff_step": participant.get("dropoff_step"),
        "confusion_event_count": len(confusion_events),
    })

def rate(count):
    return round(count / participant_count, 4) if participant_count else 0.0

def median(values):
    return round(statistics.median(values), 3) if values else None

completion_rate = rate(completed)
reward_claim_rate = rate(reward_claimed)
next_route_rate = rate(next_route_opened)
first_screen_understood_rate = rate(understood_first_screen)
avg_confusion_events = round(sum(item["confusion_event_count"] for item in participant_summaries) / participant_count, 3) if participant_count else 0.0
median_first_action = median(first_action_times)
median_reward = median(reward_times)

metric_checks = [
    {
        "check_id": "cohort_size_5_to_10",
        "passed": min_participants <= participant_count <= max_participants,
        "detail": {"participant_count": participant_count, "min": min_participants, "max": max_participants},
    },
    {
        "check_id": "completion_rate_target",
        "passed": completion_rate >= target_completion_rate,
        "detail": {"completion_rate": completion_rate, "target": target_completion_rate},
    },
    {
        "check_id": "reward_claim_rate_target",
        "passed": reward_claim_rate >= target_reward_claim_rate,
        "detail": {"reward_claim_rate": reward_claim_rate, "target": target_reward_claim_rate},
    },
    {
        "check_id": "next_route_rate_target",
        "passed": next_route_rate >= target_next_route_rate,
        "detail": {"next_route_rate": next_route_rate, "target": target_next_route_rate},
    },
    {
        "check_id": "median_first_action_target",
        "passed": median_first_action is not None and median_first_action <= target_median_first_action_seconds,
        "detail": {"median_first_action_seconds": median_first_action, "target": target_median_first_action_seconds},
    },
    {
        "check_id": "median_reward_target",
        "passed": median_reward is not None and median_reward <= target_median_reward_seconds,
        "detail": {"median_reward_seconds": median_reward, "target": target_median_reward_seconds},
    },
    {
        "check_id": "confusion_event_budget",
        "passed": avg_confusion_events <= target_avg_confusion_events,
        "detail": {"avg_confusion_events": avg_confusion_events, "target": target_avg_confusion_events},
    },
]

passed_metrics = all(check["passed"] for check in metric_checks)
ok = not errors and passed_metrics
summary = {
    "contract_version": contract_version,
    "ok": ok,
    "status": "green" if ok else "needs_fix_or_more_cohort_evidence",
    "checked_at_epoch": checked_at,
    "evidence_path": str(evidence_path),
    "input_contract_version": evidence.get("contract_version"),
    "operator_attestation": attestation,
    "participant_count": participant_count,
    "metrics": {
        "first_screen_understood_rate": first_screen_understood_rate,
        "completion_rate": completion_rate,
        "reward_claim_rate": reward_claim_rate,
        "next_route_rate": next_route_rate,
        "median_first_action_seconds": median_first_action,
        "median_reward_seconds": median_reward,
        "avg_confusion_events": avg_confusion_events,
    },
    "metric_checks": metric_checks,
    "top_confusion_categories": confusion_counter.most_common(10),
    "participant_summaries": participant_summaries,
    "errors": errors,
    "summary": str(summary_path),
}
summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps(summary, ensure_ascii=False, indent=2))
if not ok:
    raise SystemExit(1)
PY

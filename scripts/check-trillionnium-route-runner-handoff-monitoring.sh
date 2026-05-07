#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

METADATA_PATH="${CEX_MONITORING_DEPLOY_METADATA_PATH:-$CEX_PROJECT_ROOT/run/monitoring-live-target/metadata/monitoring-deploy-metadata.yml}"
PROMETHEUS_BUNDLE_PATH=""
ALERTMANAGER_BUNDLE_PATH=""
DASHBOARD_PATH="$CEX_PROJECT_ROOT/ops/monitoring/grafana/trillionnium-route-runner-handoff-dashboard.example.json"
MAX_AGE_SECONDS="${CEX_MONITORING_DEPLOY_MAX_AGE_SECONDS:-86400}"
SUMMARY_FILE=""
CHECK_DASHBOARD="true"
QUIET="false"
CONTRACT_VERSION="trillionnium_route_runner_handoff_monitoring_contract_check_v1"

usage() {
  cat <<'EOF'
Usage: scripts/check-trillionnium-route-runner-handoff-monitoring.sh [--metadata <path>] [--prometheus-bundle <path>] [--alertmanager-bundle <path>] [--dashboard <path>] [--no-dashboard] [--max-age-seconds <n>] [--summary-file <path>] [--quiet]

Validates the Trillionnium route-runner handoff monitoring contract:
  - monitoring deploy post-action metadata is successful and fresh,
  - live Prometheus bundle includes all CexTrillionniumRouteRunnerHandoff* alerts,
  - alerts use the required route-runner handoff metrics and product-ops labels,
  - Alertmanager routes component=trillionnium-route-runner-handoff before generic product-edge / severity fallbacks,
  - optional Grafana dashboard example consumes the direct handoff gauges.

Defaults read the repo-local live-target deploy metadata:
  run/monitoring-live-target/metadata/monitoring-deploy-metadata.yml

Options:
  --metadata <path>             Monitoring deploy metadata path
  --prometheus-bundle <path>    Override Prometheus bundle path instead of metadata deployedPath
  --alertmanager-bundle <path>  Override Alertmanager bundle path instead of metadata deployedPath
  --dashboard <path>            Dashboard JSON path to validate
  --no-dashboard                Skip dashboard JSON validation
  --max-age-seconds <n>         Max metadata deployedAt age; 0 disables freshness check
  --summary-file <path>         Write JSON evidence summary
  --contract-version <value>    Override summary contract_version
  --quiet                       Suppress OK/FAIL text; rely on exit code/summary
  -h, --help                    Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --metadata)
      [[ $# -ge 2 ]] || { echo "Error: --metadata requires a path" >&2; exit 2; }
      METADATA_PATH="$2"
      shift 2
      ;;
    --prometheus-bundle)
      [[ $# -ge 2 ]] || { echo "Error: --prometheus-bundle requires a path" >&2; exit 2; }
      PROMETHEUS_BUNDLE_PATH="$2"
      shift 2
      ;;
    --alertmanager-bundle)
      [[ $# -ge 2 ]] || { echo "Error: --alertmanager-bundle requires a path" >&2; exit 2; }
      ALERTMANAGER_BUNDLE_PATH="$2"
      shift 2
      ;;
    --dashboard)
      [[ $# -ge 2 ]] || { echo "Error: --dashboard requires a path" >&2; exit 2; }
      DASHBOARD_PATH="$2"
      CHECK_DASHBOARD="true"
      shift 2
      ;;
    --no-dashboard)
      CHECK_DASHBOARD="false"
      shift
      ;;
    --max-age-seconds)
      [[ $# -ge 2 ]] || { echo "Error: --max-age-seconds requires a value" >&2; exit 2; }
      MAX_AGE_SECONDS="$2"
      shift 2
      ;;
    --summary-file)
      [[ $# -ge 2 ]] || { echo "Error: --summary-file requires a path" >&2; exit 2; }
      SUMMARY_FILE="$2"
      shift 2
      ;;
    --contract-version)
      [[ $# -ge 2 ]] || { echo "Error: --contract-version requires a value" >&2; exit 2; }
      CONTRACT_VERSION="$2"
      shift 2
      ;;
    --quiet)
      QUIET="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ! [[ "$MAX_AGE_SECONDS" =~ ^[0-9]+$ ]]; then
  echo "Error: --max-age-seconds must be an integer" >&2
  exit 2
fi

cex_require_cmd python3

summary_tmp=""
if [[ -z "$SUMMARY_FILE" ]]; then
  summary_tmp="$(mktemp)"
  SUMMARY_FILE="$summary_tmp"
fi

status=0
python3 - "$CEX_PROJECT_ROOT" "$METADATA_PATH" "$PROMETHEUS_BUNDLE_PATH" "$ALERTMANAGER_BUNDLE_PATH" "$DASHBOARD_PATH" "$CHECK_DASHBOARD" "$MAX_AGE_SECONDS" "$SUMMARY_FILE" "$CONTRACT_VERSION" <<'PY' || status=$?
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
import json
import sys
import yaml

repo_root = Path(sys.argv[1])
metadata_arg = sys.argv[2]
prometheus_arg = sys.argv[3]
alertmanager_arg = sys.argv[4]
dashboard_arg = sys.argv[5]
check_dashboard = sys.argv[6].lower() == 'true'
max_age_seconds = int(sys.argv[7])
summary_path = Path(sys.argv[8])
contract_version = sys.argv[9]
if not summary_path.is_absolute():
    summary_path = repo_root / summary_path

def resolve(path_value: str | None, base: Path | None = None) -> Path | None:
    if not path_value:
        return None
    path = Path(str(path_value))
    if path.is_absolute():
        return path
    if base is not None and (base / path).exists():
        return base / path
    return repo_root / path

failures: list[str] = []
metadata_path = resolve(metadata_arg)
metadata = {}
metadata_required = not (prometheus_arg and alertmanager_arg)
if metadata_path and metadata_path.exists():
    metadata = yaml.safe_load(metadata_path.read_text()) or {}
elif metadata_required:
    failures.append(f'monitoring deploy metadata missing: {metadata_path}')

post_deploy_overall = (((metadata.get('postDeployActions') or {}).get('overall')) or {})
post_deploy_ok = bool(post_deploy_overall.get('successful') is True and post_deploy_overall.get('requiresAttention') is not True)
if metadata and not post_deploy_ok:
    failures.append('monitoring deploy post action is not successful')

deployed_at = metadata.get('deployedAt')
deployed_age_seconds = None
if metadata and deployed_at:
    parsed = datetime.fromisoformat(str(deployed_at).replace('Z', '+00:00'))
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    deployed_age_seconds = int((datetime.now(timezone.utc) - parsed.astimezone(timezone.utc)).total_seconds())
    if max_age_seconds > 0 and (deployed_age_seconds < 0 or deployed_age_seconds > max_age_seconds):
        failures.append(f'monitoring deploy metadata is stale age={deployed_age_seconds}s max={max_age_seconds}s')
elif metadata:
    failures.append('monitoring deploy metadata missing deployedAt')

def metadata_deployed_path(section: str) -> Path | None:
    deployed = metadata.get('deployed') or {}
    raw = ((deployed.get(section) or {}).get('deployedPath'))
    return resolve(raw, metadata_path.parent if metadata_path else None)

prometheus_path = resolve(prometheus_arg) if prometheus_arg else metadata_deployed_path('prometheus')
alertmanager_path = resolve(alertmanager_arg) if alertmanager_arg else metadata_deployed_path('alertmanager')
dashboard_path = resolve(dashboard_arg) if dashboard_arg else None

for section, path in [('prometheus', prometheus_path), ('alertmanager', alertmanager_path)]:
    if not path or not path.exists():
        failures.append(f'missing deployed monitoring artifact: {section} {path}')

prometheus = {}
alertmanager = {}
if prometheus_path and prometheus_path.exists():
    prometheus = yaml.safe_load(prometheus_path.read_text()) or {}
if alertmanager_path and alertmanager_path.exists():
    alertmanager = yaml.safe_load(alertmanager_path.read_text()) or {}

expected_alerts = {
    'CexTrillionniumRouteRunnerHandoffAllGatesNotGreen': {
        'metric': 'cex_consumer_entry_trillionnium_route_runner_handoff_all_gates_green',
        'expr_fragments': ['== 0'],
        'for': '5m',
        'severity': 'critical',
        'route_hint': 'page',
    },
    'CexTrillionniumRouteRunnerHandoffFeedSourceMissing': {
        'metric': 'cex_consumer_entry_trillionnium_route_runner_handoff_feed_source_count',
        'expr_fragments': ['< 7'],
        'for': '5m',
        'severity': 'critical',
        'route_hint': 'page',
    },
    'CexTrillionniumRouteRunnerHandoffRunnerCountZero': {
        'metric': 'cex_consumer_entry_trillionnium_route_runner_handoff_runner_count',
        'expr_fragments': ['== 0'],
        'for': '10m',
        'severity': 'warning',
        'route_hint': 'chat',
    },
    'CexTrillionniumRouteRunnerHandoffActionsMissing': {
        'metric': 'cex_consumer_entry_trillionnium_route_runner_handoff_reward_claim_action_count',
        'extra_metrics': ['cex_consumer_entry_trillionnium_route_runner_handoff_next_route_action_count'],
        'expr_fragments': ['== 0', ' or '],
        'for': '10m',
        'severity': 'warning',
        'route_hint': 'chat',
    },
    'CexTrillionniumRouteRunnerHandoffRouteMasteryMissing': {
        'metric': 'cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_contract_visible',
        'extra_metrics': [
            'cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_runner_count',
            'cex_consumer_entry_trillionnium_route_runner_handoff_first_route_mastery_xp',
        ],
        'expr_fragments': ['== 0', ' or '],
        'for': '10m',
        'severity': 'warning',
        'route_hint': 'chat',
    },
}

rules_by_alert: dict[str, dict] = {}
for group in prometheus.get('groups') or []:
    for rule in group.get('rules') or []:
        name = rule.get('alert')
        if name:
            rules_by_alert[name] = rule

alert_results = {}
for name, expected in expected_alerts.items():
    rule = rules_by_alert.get(name)
    labels = (rule or {}).get('labels') or {}
    expr = str((rule or {}).get('expr') or '')
    metric_visible = expected['metric'] in expr
    extra_metrics = expected.get('extra_metrics') or []
    extra_metrics_visible = all(metric in expr for metric in extra_metrics)
    expr_fragments = expected.get('expr_fragments') or []
    expr_semantics_ok = all(fragment in expr for fragment in expr_fragments)
    duration_ok = str((rule or {}).get('for') or '') == expected['for']
    labels_ok = (
        labels.get('severity') == expected['severity']
        and labels.get('service') == 'consumer-entry-api'
        and labels.get('family') == 'product-edge'
        and labels.get('component') == 'trillionnium-route-runner-handoff'
        and labels.get('owner') == 'product-ops'
        and labels.get('route_hint') == expected['route_hint']
    )
    annotations = (rule or {}).get('annotations') or {}
    annotations_ok = annotations.get('runbook') == 'docs/operator-runbook-v1.md' and bool(annotations.get('first_action'))
    result = {
        'present': rule is not None,
        'metric_visible': metric_visible,
        'extra_metrics': extra_metrics,
        'extra_metrics_visible': extra_metrics_visible,
        'expr': expr,
        'expr_fragments': expr_fragments,
        'expr_semantics_ok': expr_semantics_ok,
        'for': (rule or {}).get('for'),
        'duration_ok': duration_ok,
        'severity': labels.get('severity'),
        'service': labels.get('service'),
        'family': labels.get('family'),
        'component': labels.get('component'),
        'owner': labels.get('owner'),
        'route_hint': labels.get('route_hint'),
        'labels_ok': labels_ok,
        'annotations_ok': annotations_ok,
    }
    result['ok'] = bool(
        result['present']
        and metric_visible
        and extra_metrics_visible
        and expr_semantics_ok
        and duration_ok
        and labels_ok
        and annotations_ok
    )
    alert_results[name] = result
    if not result['ok']:
        failures.append(f'prometheus alert contract invalid: {name}')

routes = ((alertmanager.get('route') or {}).get('routes')) or []
def matcher_set(route: dict) -> set[str]:
    return set(str(matcher) for matcher in (route.get('matchers') or []))

handoff_matchers = {
    'service="consumer-entry-api"',
    'family="product-edge"',
    'component="trillionnium-route-runner-handoff"',
}
handoff_route_index = None
generic_product_edge_index = None
generic_severity_index = None
for idx, route in enumerate(routes):
    matchers = matcher_set(route)
    if handoff_matchers.issubset(matchers) and handoff_route_index is None:
        handoff_route_index = idx
    if matchers == {'family="product-edge"'} and generic_product_edge_index is None:
        generic_product_edge_index = idx
    if matchers in ({'severity="critical"'}, {'severity="warning"'}) and generic_severity_index is None:
        generic_severity_index = idx

route_order_ok = (
    handoff_route_index is not None
    and generic_product_edge_index is not None
    and handoff_route_index < generic_product_edge_index
    and (generic_severity_index is None or handoff_route_index < generic_severity_index)
)
if handoff_route_index is None:
    failures.append('alertmanager handoff component route missing')
if generic_product_edge_index is None:
    failures.append('alertmanager generic product-edge route missing')
if handoff_route_index is not None and generic_product_edge_index is not None and handoff_route_index > generic_product_edge_index:
    failures.append('alertmanager handoff component route must precede generic product-edge route')
if handoff_route_index is not None and generic_severity_index is not None and handoff_route_index > generic_severity_index:
    failures.append('alertmanager handoff component route must precede generic severity fallback routes')

dashboard_required_metrics = [
    'cex_consumer_entry_trillionnium_route_runner_handoff_all_gates_green',
    'cex_consumer_entry_trillionnium_route_runner_handoff_playability_gate_green',
    'cex_consumer_entry_trillionnium_route_runner_handoff_closed_beta_gate_green',
    'cex_consumer_entry_trillionnium_route_runner_handoff_real_user_beta_gate_green',
    'cex_consumer_entry_trillionnium_route_runner_handoff_public_commercial_gate_green',
    'cex_consumer_entry_trillionnium_route_runner_handoff_feed_source_count',
    'cex_consumer_entry_trillionnium_route_runner_handoff_runner_count',
    'cex_consumer_entry_trillionnium_route_runner_handoff_reward_claim_action_count',
    'cex_consumer_entry_trillionnium_route_runner_handoff_next_route_action_count',
    'cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_contract_visible',
    'cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_runner_count',
    'cex_consumer_entry_trillionnium_route_runner_handoff_first_route_mastery_xp',
]
expected_dashboard = {
    'uid': 'cex-trillionnium-route-runner-handoff',
    'title': 'CEX / Trillionnium Route-Runner Handoff',
    'tags': {'cex', 'trillionnium', 'route-runner', 'handoff'},
    'panels': {
        'All handoff gates': {
            'type': 'stat',
            'metrics': ['cex_consumer_entry_trillionnium_route_runner_handoff_all_gates_green'],
            'threshold_values': [1],
        },
        'Gate family': {
            'type': 'stat',
            'metrics': [
                'cex_consumer_entry_trillionnium_route_runner_handoff_playability_gate_green',
                'cex_consumer_entry_trillionnium_route_runner_handoff_closed_beta_gate_green',
                'cex_consumer_entry_trillionnium_route_runner_handoff_real_user_beta_gate_green',
                'cex_consumer_entry_trillionnium_route_runner_handoff_public_commercial_gate_green',
            ],
            'threshold_values': [1],
        },
        'Feed source count (expected >= 7)': {
            'type': 'timeseries',
            'metrics': ['cex_consumer_entry_trillionnium_route_runner_handoff_feed_source_count'],
            'threshold_values': [1, 7],
        },
        'Runner / reward / next-route action counts': {
            'type': 'timeseries',
            'metrics': [
                'cex_consumer_entry_trillionnium_route_runner_handoff_runner_count',
                'cex_consumer_entry_trillionnium_route_runner_handoff_reward_claim_action_count',
                'cex_consumer_entry_trillionnium_route_runner_handoff_next_route_action_count',
            ],
            'threshold_values': [1],
        },
        'Route mastery progression': {
            'type': 'timeseries',
            'metrics': [
                'cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_contract_visible',
                'cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_runner_count',
                'cex_consumer_entry_trillionnium_route_runner_handoff_first_route_mastery_xp',
            ],
            'threshold_values': [1],
        },
    },
}
dashboard_result = {
    'checked': check_dashboard,
    'path': str(dashboard_path) if dashboard_path else None,
    'present': bool(dashboard_path and dashboard_path.exists()),
    'missing_metrics': [],
    'uid_ok': True,
    'title_ok': True,
    'tags_ok': True,
    'panel_results': {},
    'ok': True,
}
if check_dashboard:
    if not dashboard_path or not dashboard_path.exists():
        dashboard_result['ok'] = False
        failures.append(f'dashboard missing: {dashboard_path}')
    else:
        dashboard = json.loads(dashboard_path.read_text())
        exprs = []
        panels_by_title = {}
        for panel in dashboard.get('panels') or []:
            title = panel.get('title')
            if title:
                panels_by_title[str(title)] = panel
            for target in panel.get('targets') or []:
                expr = target.get('expr')
                if expr:
                    exprs.append(str(expr))
        missing_metrics = [metric for metric in dashboard_required_metrics if metric not in exprs]
        dashboard_result['uid_ok'] = dashboard.get('uid') == expected_dashboard['uid']
        dashboard_result['title_ok'] = dashboard.get('title') == expected_dashboard['title']
        dashboard_result['tags_ok'] = expected_dashboard['tags'].issubset(set(dashboard.get('tags') or []))
        dashboard_result['missing_metrics'] = missing_metrics
        if not dashboard_result['uid_ok']:
            failures.append('dashboard uid drifted')
        if not dashboard_result['title_ok']:
            failures.append('dashboard title drifted')
        if not dashboard_result['tags_ok']:
            failures.append('dashboard tags missing handoff taxonomy')
        if missing_metrics:
            failures.append('dashboard missing handoff metrics: ' + ','.join(missing_metrics))
        for title, expected_panel in expected_dashboard['panels'].items():
            panel = panels_by_title.get(title)
            panel_exprs = [str(target.get('expr')) for target in ((panel or {}).get('targets') or []) if target.get('expr')]
            thresholds = (((panel or {}).get('fieldConfig') or {}).get('defaults') or {}).get('thresholds') or {}
            threshold_values = [step.get('value') for step in thresholds.get('steps') or [] if step.get('value') is not None]
            panel_result = {
                'present': panel is not None,
                'type': (panel or {}).get('type'),
                'type_ok': (panel or {}).get('type') == expected_panel['type'],
                'metrics_ok': all(metric in panel_exprs for metric in expected_panel['metrics']),
                'threshold_values': threshold_values,
                'thresholds_ok': all(value in threshold_values for value in expected_panel['threshold_values']),
            }
            panel_result['ok'] = bool(
                panel_result['present']
                and panel_result['type_ok']
                and panel_result['metrics_ok']
                and panel_result['thresholds_ok']
            )
            dashboard_result['panel_results'][title] = panel_result
            if not panel_result['ok']:
                failures.append(f'dashboard panel contract invalid: {title}')
        dashboard_result['ok'] = (
            dashboard_result['uid_ok']
            and dashboard_result['title_ok']
            and dashboard_result['tags_ok']
            and not missing_metrics
            and all(result.get('ok') for result in dashboard_result['panel_results'].values())
        )

summary = {
    'ok': not failures,
    'contract_version': contract_version,
    'metadata_path': str(metadata_path) if metadata_path else None,
    'prometheus_bundle_path': str(prometheus_path) if prometheus_path else None,
    'alertmanager_bundle_path': str(alertmanager_path) if alertmanager_path else None,
    'dashboard_path': str(dashboard_path) if dashboard_path else None,
    'deployed_at': deployed_at,
    'deployed_age_seconds': deployed_age_seconds,
    'post_deploy_overall': post_deploy_overall,
    'alert_results': alert_results,
    'alert_names': list(expected_alerts.keys()),
    'handoff_route_index': handoff_route_index,
    'generic_product_edge_route_index': generic_product_edge_index,
    'generic_severity_route_index': generic_severity_index,
    'route_order_ok': route_order_ok,
    'dashboard': dashboard_result,
    'failures': failures,
}
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n')
if failures:
    raise SystemExit(1)
PY

if [[ "$QUIET" != "true" ]]; then
  if [[ "$status" -eq 0 ]]; then
    echo "OK route-runner handoff monitoring contract ($SUMMARY_FILE)"
  else
    echo "FAIL route-runner handoff monitoring contract ($SUMMARY_FILE)" >&2
    python3 - "$SUMMARY_FILE" <<'PY' >&2 || true
from pathlib import Path
import json, sys
summary = json.loads(Path(sys.argv[1]).read_text())
for failure in summary.get('failures') or []:
    print(f"  - {failure}")
PY
  fi
fi

if [[ -n "$summary_tmp" ]]; then
  rm -f "$summary_tmp"
fi
exit "$status"

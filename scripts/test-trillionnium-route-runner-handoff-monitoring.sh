#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"

cex_require_cmd python3

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

PROMETHEUS_GOOD="$TMP_DIR/prometheus.yml"
ALERTMANAGER_GOOD="$TMP_DIR/alertmanager.yml"
DASHBOARD_GOOD="$TMP_DIR/dashboard.json"
METADATA_GOOD="$TMP_DIR/metadata.yml"
METADATA_STALE="$TMP_DIR/metadata-stale.yml"

cp "$CEX_PROJECT_ROOT/ops/monitoring/prometheus/minimal-wrapper-monitoring-bundle.example.yml" "$PROMETHEUS_GOOD"
cp "$CEX_PROJECT_ROOT/ops/monitoring/alertmanager/minimal-wrapper-monitoring-bundle.example.yml" "$ALERTMANAGER_GOOD"
cp "$CEX_PROJECT_ROOT/ops/monitoring/grafana/trillionnium-route-runner-handoff-dashboard.example.json" "$DASHBOARD_GOOD"

python3 - "$METADATA_GOOD" "$METADATA_STALE" "$PROMETHEUS_GOOD" "$ALERTMANAGER_GOOD" <<'PY'
from datetime import datetime, timezone
from pathlib import Path
import sys
import yaml

good_path = Path(sys.argv[1])
stale_path = Path(sys.argv[2])
prometheus_path = Path(sys.argv[3]).resolve()
alertmanager_path = Path(sys.argv[4]).resolve()

def metadata(deployed_at: str):
    return {
        'version': 1,
        'deployedAt': deployed_at,
        'mode': 'copy',
        'bundleKind': 'all',
        'deployed': {
            'prometheus': {'deployedPath': str(prometheus_path)},
            'alertmanager': {'deployedPath': str(alertmanager_path)},
        },
        'postDeployActions': {
            'overall': {
                'status': 'success',
                'severity': 'ok',
                'requiresAttention': False,
                'successful': True,
            }
        },
    }

good_path.write_text(yaml.safe_dump(metadata(datetime.now(timezone.utc).isoformat()), sort_keys=False))
stale_path.write_text(yaml.safe_dump(metadata('2020-01-01T00:00:00+00:00'), sort_keys=False))
PY

assert_ok() {
  local name="$1"
  shift
  local summary="$TMP_DIR/${name}.json"
  "$SCRIPT_DIR/check-trillionnium-route-runner-handoff-monitoring.sh" \
    --metadata "$METADATA_GOOD" \
    --prometheus-bundle "$PROMETHEUS_GOOD" \
    --alertmanager-bundle "$ALERTMANAGER_GOOD" \
    --dashboard "$DASHBOARD_GOOD" \
    --summary-file "$summary" \
    --quiet \
    "$@"
  python3 - "$summary" <<'PY'
from pathlib import Path
import json, sys
summary = json.loads(Path(sys.argv[1]).read_text())
if summary.get('ok') is not True:
    raise SystemExit(f"expected ok summary, got failures={summary.get('failures')}")
PY
  printf 'OK %s\n' "$name"
}

assert_fail_contains() {
  local name="$1"
  local expected="$2"
  shift 2
  local summary="$TMP_DIR/${name}.json"
  if "$SCRIPT_DIR/check-trillionnium-route-runner-handoff-monitoring.sh" \
    --metadata "$METADATA_GOOD" \
    --prometheus-bundle "$PROMETHEUS_GOOD" \
    --alertmanager-bundle "$ALERTMANAGER_GOOD" \
    --dashboard "$DASHBOARD_GOOD" \
    --summary-file "$summary" \
    --quiet \
    "$@"; then
    echo "FAIL $name unexpectedly passed" >&2
    exit 1
  fi
  python3 - "$summary" "$expected" <<'PY'
from pathlib import Path
import json, sys
summary = json.loads(Path(sys.argv[1]).read_text())
expected = sys.argv[2]
failures = summary.get('failures') or []
if summary.get('ok') is not False:
    raise SystemExit('expected ok=false summary')
if expected not in '\n'.join(failures):
    raise SystemExit(f"expected failure containing {expected!r}, got {failures}")
PY
  printf 'OK %s failed as expected\n' "$name"
}

assert_ok positive

PROMETHEUS_BAD_OWNER="$TMP_DIR/prometheus-bad-owner.yml"
python3 - "$PROMETHEUS_GOOD" "$PROMETHEUS_BAD_OWNER" <<'PY'
from pathlib import Path
import sys
import yaml

data = yaml.safe_load(Path(sys.argv[1]).read_text())
for group in data.get('groups') or []:
    for rule in group.get('rules') or []:
        if rule.get('alert') == 'CexTrillionniumRouteRunnerHandoffAllGatesNotGreen':
            rule.setdefault('labels', {})['owner'] = 'wrong-team'
Path(sys.argv[2]).write_text(yaml.safe_dump(data, sort_keys=False))
PY
assert_fail_contains bad_owner 'CexTrillionniumRouteRunnerHandoffAllGatesNotGreen' --prometheus-bundle "$PROMETHEUS_BAD_OWNER"

PROMETHEUS_BAD_EXPR="$TMP_DIR/prometheus-bad-expr.yml"
python3 - "$PROMETHEUS_GOOD" "$PROMETHEUS_BAD_EXPR" <<'PY'
from pathlib import Path
import sys
import yaml

data = yaml.safe_load(Path(sys.argv[1]).read_text())
for group in data.get('groups') or []:
    for rule in group.get('rules') or []:
        if rule.get('alert') == 'CexTrillionniumRouteRunnerHandoffFeedSourceMissing':
            rule['expr'] = 'cex_consumer_entry_trillionnium_route_runner_handoff_feed_source_count > 7'
Path(sys.argv[2]).write_text(yaml.safe_dump(data, sort_keys=False))
PY
assert_fail_contains bad_expr 'CexTrillionniumRouteRunnerHandoffFeedSourceMissing' --prometheus-bundle "$PROMETHEUS_BAD_EXPR"

PROMETHEUS_BAD_MASTERY_EXPR="$TMP_DIR/prometheus-bad-mastery-expr.yml"
python3 - "$PROMETHEUS_GOOD" "$PROMETHEUS_BAD_MASTERY_EXPR" <<'PY'
from pathlib import Path
import sys
import yaml

data = yaml.safe_load(Path(sys.argv[1]).read_text())
for group in data.get('groups') or []:
    for rule in group.get('rules') or []:
        if rule.get('alert') == 'CexTrillionniumRouteRunnerHandoffRouteMasteryMissing':
            rule['expr'] = 'cex_consumer_entry_trillionnium_route_runner_handoff_route_mastery_contract_visible == 0'
Path(sys.argv[2]).write_text(yaml.safe_dump(data, sort_keys=False))
PY
assert_fail_contains bad_mastery_expr 'CexTrillionniumRouteRunnerHandoffRouteMasteryMissing' --prometheus-bundle "$PROMETHEUS_BAD_MASTERY_EXPR"

PROMETHEUS_BAD_RUM_EXPR="$TMP_DIR/prometheus-bad-rum-expr.yml"
python3 - "$PROMETHEUS_GOOD" "$PROMETHEUS_BAD_RUM_EXPR" <<'PY'
from pathlib import Path
import sys
import yaml

data = yaml.safe_load(Path(sys.argv[1]).read_text())
for group in data.get('groups') or []:
    for rule in group.get('rules') or []:
        if rule.get('alert') == 'CexTrillionniumWorldMapRumSloWarmupRawRed':
            rule['expr'] = 'cex_consumer_entry_trillionnium_world_map_rum_slo_warming == 0'
Path(sys.argv[2]).write_text(yaml.safe_dump(data, sort_keys=False))
PY
assert_fail_contains bad_rum_expr 'CexTrillionniumWorldMapRumSloWarmupRawRed' --prometheus-bundle "$PROMETHEUS_BAD_RUM_EXPR"

ALERTMANAGER_BAD_ORDER="$TMP_DIR/alertmanager-bad-order.yml"
python3 - "$ALERTMANAGER_GOOD" "$ALERTMANAGER_BAD_ORDER" <<'PY'
from pathlib import Path
import sys
import yaml

data = yaml.safe_load(Path(sys.argv[1]).read_text())
routes = data['route']['routes']
# Move the generic critical severity fallback before the handoff component route.
for idx, route in enumerate(routes):
    if set(route.get('matchers') or []) == {'severity="critical"'}:
        generic = routes.pop(idx)
        break
else:
    raise SystemExit('generic critical route not found')
for idx, route in enumerate(routes):
    if 'component="trillionnium-route-runner-handoff"' in set(route.get('matchers') or []):
        routes.insert(idx, generic)
        break
else:
    raise SystemExit('handoff component route not found')
Path(sys.argv[2]).write_text(yaml.safe_dump(data, sort_keys=False))
PY
assert_fail_contains bad_route_order 'generic severity fallback' --alertmanager-bundle "$ALERTMANAGER_BAD_ORDER"

DASHBOARD_BAD_METRIC="$TMP_DIR/dashboard-bad-metric.json"
python3 - "$DASHBOARD_GOOD" "$DASHBOARD_BAD_METRIC" <<'PY'
from pathlib import Path
import json
import sys

data = json.loads(Path(sys.argv[1]).read_text())
for panel in data.get('panels') or []:
    for target in panel.get('targets') or []:
        if target.get('expr') == 'cex_consumer_entry_trillionnium_route_runner_handoff_next_route_action_count':
            target['expr'] = 'cex_consumer_entry_trillionnium_route_runner_handoff_next_route_action_count_removed'
Path(sys.argv[2]).write_text(json.dumps(data, indent=2, sort_keys=True) + '\n')
PY
assert_fail_contains bad_dashboard_metric 'dashboard missing handoff metrics' --dashboard "$DASHBOARD_BAD_METRIC"

DASHBOARD_BAD_THRESHOLD="$TMP_DIR/dashboard-bad-threshold.json"
python3 - "$DASHBOARD_GOOD" "$DASHBOARD_BAD_THRESHOLD" <<'PY'
from pathlib import Path
import json
import sys

data = json.loads(Path(sys.argv[1]).read_text())
for panel in data.get('panels') or []:
    if panel.get('title') == 'Feed source count (expected >= 7)':
        steps = panel['fieldConfig']['defaults']['thresholds']['steps']
        for step in steps:
            if step.get('value') == 7:
                step['value'] = 6
Path(sys.argv[2]).write_text(json.dumps(data, indent=2, sort_keys=True) + '\n')
PY
assert_fail_contains bad_dashboard_threshold 'dashboard panel contract invalid: Feed source count' --dashboard "$DASHBOARD_BAD_THRESHOLD"

if "$SCRIPT_DIR/check-trillionnium-route-runner-handoff-monitoring.sh" \
  --metadata "$METADATA_STALE" \
  --prometheus-bundle "$PROMETHEUS_GOOD" \
  --alertmanager-bundle "$ALERTMANAGER_GOOD" \
  --dashboard "$DASHBOARD_GOOD" \
  --summary-file "$TMP_DIR/stale.json" \
  --max-age-seconds 1 \
  --quiet; then
  echo 'FAIL stale_metadata unexpectedly passed' >&2
  exit 1
fi
python3 - "$TMP_DIR/stale.json" <<'PY'
from pathlib import Path
import json, sys
summary = json.loads(Path(sys.argv[1]).read_text())
if not any('stale' in failure for failure in summary.get('failures') or []):
    raise SystemExit(f"expected stale failure, got {summary.get('failures')}")
PY
printf 'OK stale_metadata failed as expected\n'

printf 'ROUTE_RUNNER_HANDOFF_MONITORING_CONTRACT_TESTS_OK\n'

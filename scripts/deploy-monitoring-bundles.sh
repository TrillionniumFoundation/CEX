#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

MANIFEST_PATH="ops/monitoring/monitoring-bundle-manifest.example.yml"
SOURCE_OVERLAY_ROOT="run/monitoring-overlay"
DEPLOY_ROOT="run/monitoring-live-target"
PROMETHEUS_RULES_DIR=""
ALERTMANAGER_CONFIG_DIR=""
METADATA_DIR=""
BUNDLE_KIND="all"
INCLUDE_FOCUSED="false"
OVERLAY_FIRST="true"
FORCE="false"
MODE="symlink"
RELOAD_AFTER_DEPLOY="false"
RELOAD_MODE="auto"
RELOAD_TIMEOUT_SECS="10"
RELOAD_FAILURE_POLICY="fail"
RELOAD_PROMETHEUS_URL=""
RELOAD_ALERTMANAGER_URL=""
RELOAD_PROMETHEUS_COMMAND=""
RELOAD_ALERTMANAGER_COMMAND=""
RELOAD_PROMETHEUS_RESTART_COMMAND=""
RELOAD_ALERTMANAGER_RESTART_COMMAND=""
RELOAD_DRY_RUN="false"
SKIP_RELOAD_PROMETHEUS="false"
SKIP_RELOAD_ALERTMANAGER="false"
VERIFY_AFTER_DEPLOY="false"
VERIFY_MODE="auto"
VERIFY_TIMEOUT_SECS="10"
VERIFY_ATTEMPTS="1"
VERIFY_DELAY_SECS="1"
VERIFY_PROMETHEUS_URL=""
VERIFY_ALERTMANAGER_URL=""
VERIFY_PROMETHEUS_COMMAND=""
VERIFY_ALERTMANAGER_COMMAND=""
VERIFY_DRY_RUN="false"
SKIP_VERIFY_PROMETHEUS="false"
SKIP_VERIFY_ALERTMANAGER="false"
RELOAD_SUMMARY_FILE=""
VERIFY_SUMMARY_FILE=""

usage() {
  cat <<'EOF'
Usage: scripts/deploy-monitoring-bundles.sh [--manifest <path>] [--from-overlay-root <dir>] [--deploy-root <dir>] [--prometheus-rules-dir <dir>] [--alertmanager-config-dir <dir>] [--metadata-dir <dir>] [--bundle all|prometheus|alertmanager] [--include-focused] [--mode symlink|copy] [--reload] [--reload-mode auto|http|command] [--reload-timeout-secs <n>] [--reload-failure-policy fail|restart] [--reload-prometheus-url <url>] [--reload-alertmanager-url <url>] [--reload-prometheus-command <cmd>] [--reload-alertmanager-command <cmd>] [--reload-prometheus-restart-command <cmd>] [--reload-alertmanager-restart-command <cmd>] [--reload-dry-run] [--skip-reload-prometheus] [--skip-reload-alertmanager] [--verify] [--verify-mode auto|http|command] [--verify-timeout-secs <n>] [--verify-attempts <n>] [--verify-delay-secs <n>] [--verify-prometheus-url <url>] [--verify-alertmanager-url <url>] [--verify-prometheus-command <cmd>] [--verify-alertmanager-command <cmd>] [--verify-dry-run] [--skip-verify-prometheus] [--skip-verify-alertmanager] [--force]

Deploys the current monitoring overlay into a live-target directory layout.

Default behavior:
  - runs scripts/overlay-monitoring-bundles.sh into run/monitoring-overlay
  - deploys into run/monitoring-live-target with this layout:
      <deploy-root>/prometheus/rules.d/cex-monitoring-bundle.rules.yml
      <deploy-root>/alertmanager/conf.d/cex-monitoring-bundle.yml
      <deploy-root>/metadata/monitoring-bundle-manifest.yml
      <deploy-root>/metadata/monitoring-export-metadata.yml
      <deploy-root>/metadata/monitoring-install-metadata.yml
      <deploy-root>/metadata/monitoring-link-metadata.yml
      <deploy-root>/metadata/monitoring-deploy-metadata.yml

Options:
  --manifest <path>              Override manifest path (default: ops/monitoring/monitoring-bundle-manifest.example.yml)
  --from-overlay-root <dir>      Reuse an existing overlay root instead of regenerating run/monitoring-overlay
  --deploy-root <dir>            Deploy destination root (default: run/monitoring-live-target)
  --prometheus-rules-dir <dir>   Override deploy-root/prometheus/rules.d
  --alertmanager-config-dir <dir> Override deploy-root/alertmanager/conf.d
  --metadata-dir <dir>           Override deploy-root/metadata
  --bundle <kind>                One of: all, prometheus, alertmanager (default: all)
  --include-focused              Also deploy focused component files under <deploy-root>/focused/
  --mode <symlink|copy>          Deployment mode (default: symlink)
  --reload                       Trigger Prometheus / Alertmanager reload after deploy
  --reload-mode <kind>           One of: auto, http, command (default: auto)
  --reload-timeout-secs <n>      HTTP timeout passed to reload helper (default: 10)
  --reload-failure-policy <kind> One of: fail, restart (default: fail)
  --reload-prometheus-url <url>  Override Prometheus reload URL
  --reload-alertmanager-url <url> Override Alertmanager reload URL
  --reload-prometheus-command <cmd> Override Prometheus reload command
  --reload-alertmanager-command <cmd> Override Alertmanager reload command
  --reload-prometheus-restart-command <cmd> Override Prometheus restart fallback command
  --reload-alertmanager-restart-command <cmd> Override Alertmanager restart fallback command
  --reload-dry-run               Print reload actions without executing them
  --skip-reload-prometheus       Skip Prometheus reload when --reload is enabled
  --skip-reload-alertmanager     Skip Alertmanager reload when --reload is enabled
  --verify                       Verify Prometheus / Alertmanager health after deploy/reload
  --verify-mode <kind>           One of: auto, http, command (default: auto)
  --verify-timeout-secs <n>      HTTP timeout passed to verify helper (default: 10)
  --verify-attempts <n>          Verification attempts per target (default: 1)
  --verify-delay-secs <n>        Delay between failed verification attempts (default: 1)
  --verify-prometheus-url <url>  Override Prometheus verify URL
  --verify-alertmanager-url <url> Override Alertmanager verify URL
  --verify-prometheus-command <cmd> Override Prometheus verify command
  --verify-alertmanager-command <cmd> Override Alertmanager verify command
  --verify-dry-run               Print verification actions without executing them
  --skip-verify-prometheus       Skip Prometheus verification when --verify is enabled
  --skip-verify-alertmanager     Skip Alertmanager verification when --verify is enabled
  --force                        Replace existing files/symlinks in target dirs
  -h, --help                     Show this help

Examples:
  ./scripts/deploy-monitoring-bundles.sh
  ./scripts/deploy-monitoring-bundles.sh --deploy-root /tmp/cex-monitoring-live-target --include-focused
  ./scripts/deploy-monitoring-bundles.sh --from-overlay-root /tmp/cex-monitoring-overlay --mode copy
  ./scripts/deploy-monitoring-bundles.sh --reload --reload-dry-run
  ./scripts/deploy-monitoring-bundles.sh --reload --reload-mode command --reload-prometheus-command 'systemctl reload prometheus' --reload-alertmanager-command 'systemctl reload alertmanager'
  ./scripts/deploy-monitoring-bundles.sh --reload --reload-mode command --reload-failure-policy restart --reload-prometheus-command 'systemctl reload prometheus' --reload-prometheus-restart-command 'systemctl restart prometheus'
  ./scripts/deploy-monitoring-bundles.sh --reload --verify --verify-attempts 5 --verify-delay-secs 2
  ./scripts/deploy-monitoring-bundles.sh --prometheus-rules-dir /etc/prometheus/rules.d --alertmanager-config-dir /etc/alertmanager/conf.d --metadata-dir /var/lib/cex-monitoring/metadata
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      [[ $# -ge 2 ]] || { echo "Error: --manifest requires a path" >&2; exit 2; }
      MANIFEST_PATH="$2"
      shift 2
      ;;
    --from-overlay-root)
      [[ $# -ge 2 ]] || { echo "Error: --from-overlay-root requires a directory" >&2; exit 2; }
      SOURCE_OVERLAY_ROOT="$2"
      OVERLAY_FIRST="false"
      shift 2
      ;;
    --deploy-root)
      [[ $# -ge 2 ]] || { echo "Error: --deploy-root requires a directory" >&2; exit 2; }
      DEPLOY_ROOT="$2"
      shift 2
      ;;
    --prometheus-rules-dir)
      [[ $# -ge 2 ]] || { echo "Error: --prometheus-rules-dir requires a directory" >&2; exit 2; }
      PROMETHEUS_RULES_DIR="$2"
      shift 2
      ;;
    --alertmanager-config-dir)
      [[ $# -ge 2 ]] || { echo "Error: --alertmanager-config-dir requires a directory" >&2; exit 2; }
      ALERTMANAGER_CONFIG_DIR="$2"
      shift 2
      ;;
    --metadata-dir)
      [[ $# -ge 2 ]] || { echo "Error: --metadata-dir requires a directory" >&2; exit 2; }
      METADATA_DIR="$2"
      shift 2
      ;;
    --bundle)
      [[ $# -ge 2 ]] || { echo "Error: --bundle requires one of all|prometheus|alertmanager" >&2; exit 2; }
      BUNDLE_KIND="$2"
      shift 2
      ;;
    --include-focused)
      INCLUDE_FOCUSED="true"
      shift
      ;;
    --mode)
      [[ $# -ge 2 ]] || { echo "Error: --mode requires symlink|copy" >&2; exit 2; }
      MODE="$2"
      shift 2
      ;;
    --reload)
      RELOAD_AFTER_DEPLOY="true"
      shift
      ;;
    --reload-mode)
      [[ $# -ge 2 ]] || { echo "Error: --reload-mode requires auto|http|command" >&2; exit 2; }
      RELOAD_MODE="$2"
      shift 2
      ;;
    --reload-timeout-secs)
      [[ $# -ge 2 ]] || { echo "Error: --reload-timeout-secs requires a value" >&2; exit 2; }
      RELOAD_TIMEOUT_SECS="$2"
      shift 2
      ;;
    --reload-failure-policy)
      [[ $# -ge 2 ]] || { echo "Error: --reload-failure-policy requires fail|restart" >&2; exit 2; }
      RELOAD_FAILURE_POLICY="$2"
      shift 2
      ;;
    --reload-prometheus-url)
      [[ $# -ge 2 ]] || { echo "Error: --reload-prometheus-url requires a value" >&2; exit 2; }
      RELOAD_PROMETHEUS_URL="$2"
      shift 2
      ;;
    --reload-alertmanager-url)
      [[ $# -ge 2 ]] || { echo "Error: --reload-alertmanager-url requires a value" >&2; exit 2; }
      RELOAD_ALERTMANAGER_URL="$2"
      shift 2
      ;;
    --reload-prometheus-command)
      [[ $# -ge 2 ]] || { echo "Error: --reload-prometheus-command requires a value" >&2; exit 2; }
      RELOAD_PROMETHEUS_COMMAND="$2"
      shift 2
      ;;
    --reload-alertmanager-command)
      [[ $# -ge 2 ]] || { echo "Error: --reload-alertmanager-command requires a value" >&2; exit 2; }
      RELOAD_ALERTMANAGER_COMMAND="$2"
      shift 2
      ;;
    --reload-prometheus-restart-command)
      [[ $# -ge 2 ]] || { echo "Error: --reload-prometheus-restart-command requires a value" >&2; exit 2; }
      RELOAD_PROMETHEUS_RESTART_COMMAND="$2"
      shift 2
      ;;
    --reload-alertmanager-restart-command)
      [[ $# -ge 2 ]] || { echo "Error: --reload-alertmanager-restart-command requires a value" >&2; exit 2; }
      RELOAD_ALERTMANAGER_RESTART_COMMAND="$2"
      shift 2
      ;;
    --reload-dry-run)
      RELOAD_DRY_RUN="true"
      shift
      ;;
    --skip-reload-prometheus)
      SKIP_RELOAD_PROMETHEUS="true"
      shift
      ;;
    --skip-reload-alertmanager)
      SKIP_RELOAD_ALERTMANAGER="true"
      shift
      ;;
    --verify)
      VERIFY_AFTER_DEPLOY="true"
      shift
      ;;
    --verify-mode)
      [[ $# -ge 2 ]] || { echo "Error: --verify-mode requires auto|http|command" >&2; exit 2; }
      VERIFY_MODE="$2"
      shift 2
      ;;
    --verify-timeout-secs)
      [[ $# -ge 2 ]] || { echo "Error: --verify-timeout-secs requires a value" >&2; exit 2; }
      VERIFY_TIMEOUT_SECS="$2"
      shift 2
      ;;
    --verify-attempts)
      [[ $# -ge 2 ]] || { echo "Error: --verify-attempts requires a value" >&2; exit 2; }
      VERIFY_ATTEMPTS="$2"
      shift 2
      ;;
    --verify-delay-secs)
      [[ $# -ge 2 ]] || { echo "Error: --verify-delay-secs requires a value" >&2; exit 2; }
      VERIFY_DELAY_SECS="$2"
      shift 2
      ;;
    --verify-prometheus-url)
      [[ $# -ge 2 ]] || { echo "Error: --verify-prometheus-url requires a value" >&2; exit 2; }
      VERIFY_PROMETHEUS_URL="$2"
      shift 2
      ;;
    --verify-alertmanager-url)
      [[ $# -ge 2 ]] || { echo "Error: --verify-alertmanager-url requires a value" >&2; exit 2; }
      VERIFY_ALERTMANAGER_URL="$2"
      shift 2
      ;;
    --verify-prometheus-command)
      [[ $# -ge 2 ]] || { echo "Error: --verify-prometheus-command requires a value" >&2; exit 2; }
      VERIFY_PROMETHEUS_COMMAND="$2"
      shift 2
      ;;
    --verify-alertmanager-command)
      [[ $# -ge 2 ]] || { echo "Error: --verify-alertmanager-command requires a value" >&2; exit 2; }
      VERIFY_ALERTMANAGER_COMMAND="$2"
      shift 2
      ;;
    --verify-dry-run)
      VERIFY_DRY_RUN="true"
      shift
      ;;
    --skip-verify-prometheus)
      SKIP_VERIFY_PROMETHEUS="true"
      shift
      ;;
    --skip-verify-alertmanager)
      SKIP_VERIFY_ALERTMANAGER="true"
      shift
      ;;
    --force)
      FORCE="true"
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

case "$BUNDLE_KIND" in
  all|prometheus|alertmanager) ;;
  *)
    echo "Error: --bundle must be one of all|prometheus|alertmanager" >&2
    exit 2
    ;;
esac

case "$MODE" in
  symlink|copy) ;;
  *)
    echo "Error: --mode must be symlink or copy" >&2
    exit 2
    ;;
esac

case "$RELOAD_MODE" in
  auto|http|command) ;;
  *)
    echo "Error: --reload-mode must be auto, http, or command" >&2
    exit 2
    ;;
esac

case "$RELOAD_FAILURE_POLICY" in
  fail|restart) ;;
  *)
    echo "Error: --reload-failure-policy must be fail or restart" >&2
    exit 2
    ;;
esac

case "$VERIFY_MODE" in
  auto|http|command) ;;
  *)
    echo "Error: --verify-mode must be auto, http, or command" >&2
    exit 2
    ;;
esac

if [[ -n "$METADATA_DIR" ]]; then
  DEPLOY_METADATA_PATH="$METADATA_DIR/monitoring-deploy-metadata.yml"
else
  DEPLOY_METADATA_PATH="$DEPLOY_ROOT/metadata/monitoring-deploy-metadata.yml"
fi

if [[ "$RELOAD_AFTER_DEPLOY" == "true" ]]; then
  RELOAD_SUMMARY_FILE="$(mktemp)"
fi

if [[ "$VERIFY_AFTER_DEPLOY" == "true" ]]; then
  VERIFY_SUMMARY_FILE="$(mktemp)"
fi

cleanup_summary_files() {
  if [[ -n "$RELOAD_SUMMARY_FILE" && -f "$RELOAD_SUMMARY_FILE" ]]; then
    rm -f "$RELOAD_SUMMARY_FILE"
  fi
  if [[ -n "$VERIFY_SUMMARY_FILE" && -f "$VERIFY_SUMMARY_FILE" ]]; then
    rm -f "$VERIFY_SUMMARY_FILE"
  fi
}

update_deploy_metadata_post_actions() {
  python3 - "$REPO_ROOT" "$DEPLOY_METADATA_PATH" "$RELOAD_AFTER_DEPLOY" "$RELOAD_SUMMARY_FILE" "$VERIFY_AFTER_DEPLOY" "$VERIFY_SUMMARY_FILE" <<'PY'
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
import sys
import yaml

repo_root = Path(sys.argv[1])
metadata_path = Path(sys.argv[2])
if not metadata_path.is_absolute():
    metadata_path = repo_root / metadata_path
reload_requested = sys.argv[3].lower() == 'true'
reload_summary_arg = sys.argv[4]
verify_requested = sys.argv[5].lower() == 'true'
verify_summary_arg = sys.argv[6]

if not metadata_path.exists():
    raise SystemExit(f"Error: deploy metadata not found: {metadata_path}")

metadata = yaml.safe_load(metadata_path.read_text(encoding='utf-8')) or {}

def load_summary(path_arg: str):
    if not path_arg:
        return None
    path = Path(path_arg)
    if not path.exists() or path.stat().st_size == 0:
        return None
    return yaml.safe_load(path.read_text(encoding='utf-8'))

reload_summary = load_summary(reload_summary_arg)
verify_summary = load_summary(verify_summary_arg)

reload_state = {
    'requested': reload_requested,
    'completed': reload_summary is not None,
    'successful': None if reload_summary is None else bool(reload_summary.get('overallSuccess')),
    'summary': reload_summary,
}
verify_state = {
    'requested': verify_requested,
    'completed': verify_summary is not None,
    'successful': None if verify_summary is None else bool(verify_summary.get('overallSuccess')),
    'summary': verify_summary,
}

states = {
    'reload': reload_state,
    'verify': verify_state,
}
requested_actions = [name for name, state in states.items() if state['requested']]
completed_actions = [name for name, state in states.items() if state['requested'] and state['completed']]
failed_actions = [name for name, state in states.items() if state['requested'] and state['completed'] and state['successful'] is False]
pending_actions = [name for name, state in states.items() if state['requested'] and not state['completed']]

if not requested_actions:
    overall_status = 'deploy_only'
    overall_success = True
    overall_summary_display = 'deploy only'
elif pending_actions:
    overall_status = 'incomplete'
    overall_success = False
    overall_summary_display = ', '.join(
        [f'{name} pending' for name in pending_actions] +
        [f'{name} ok' for name in completed_actions if name not in failed_actions]
    )
elif failed_actions:
    overall_success = False
    if failed_actions == ['reload']:
        overall_status = 'reload_failed'
    elif failed_actions == ['verify']:
        overall_status = 'verify_failed'
    else:
        overall_status = 'post_actions_failed'
    overall_summary_display = ', '.join(
        [f'{name} failed' if name in failed_actions else f'{name} ok' for name in requested_actions]
    )
else:
    overall_status = 'success'
    overall_success = True
    overall_summary_display = ', '.join(f'{name} ok' for name in requested_actions)

if overall_status == 'success':
    overall_severity = 'ok'
    overall_next_action_hint = 'none'
elif overall_status == 'deploy_only':
    overall_severity = 'warn'
    overall_next_action_hint = 'run deploy with --reload --verify when targets are ready'
elif overall_status == 'incomplete':
    overall_severity = 'warn'
    overall_next_action_hint = 'inspect helper summaries and rerun incomplete post actions'
elif overall_status == 'reload_failed':
    overall_severity = 'error'
    overall_next_action_hint = 'inspect reload summary/logs and retry reload or restart targets'
elif overall_status == 'verify_failed':
    overall_severity = 'error'
    overall_next_action_hint = 'inspect target health/logs and rerun verify'
else:
    overall_severity = 'error'
    overall_next_action_hint = 'inspect reload/verify summaries and recover failed post actions'

overall_requires_attention = overall_severity != 'ok'
overall_operator_display = f"{overall_severity}/{overall_status}, {overall_summary_display}"

metadata['postDeployActions'] = {
    'updatedAt': datetime.now(timezone.utc).isoformat(),
    'overall': {
        'status': overall_status,
        'severity': overall_severity,
        'requiresAttention': overall_requires_attention,
        'nextActionHint': overall_next_action_hint,
        'successful': overall_success,
        'requestedActions': requested_actions,
        'completedActions': completed_actions,
        'failedActions': failed_actions,
        'pendingActions': pending_actions,
        'summaryDisplay': overall_summary_display,
        'operatorDisplay': overall_operator_display,
    },
    'reload': reload_state,
    'verify': verify_state,
}

metadata_path.write_text(
    '# Generated by scripts/deploy-monitoring-bundles.sh\n\n' + yaml.safe_dump(metadata, sort_keys=False, allow_unicode=True),
    encoding='utf-8',
)
PY
}

trap cleanup_summary_files EXIT

if [[ "$OVERLAY_FIRST" == "true" ]]; then
  overlay_cmd=(
    "$SCRIPT_DIR/overlay-monitoring-bundles.sh"
    --manifest "$MANIFEST_PATH"
    --target-root "$SOURCE_OVERLAY_ROOT"
    --bundle "$BUNDLE_KIND"
    --force
  )
  if [[ "$INCLUDE_FOCUSED" == "true" ]]; then
    overlay_cmd+=(--include-focused)
  fi
  "${overlay_cmd[@]}" >/dev/null
fi

python3 - "$REPO_ROOT" "$SOURCE_OVERLAY_ROOT" "$DEPLOY_ROOT" "$PROMETHEUS_RULES_DIR" "$ALERTMANAGER_CONFIG_DIR" "$METADATA_DIR" "$BUNDLE_KIND" "$INCLUDE_FOCUSED" "$MODE" "$FORCE" <<'PY'
from __future__ import annotations

import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path
import yaml

repo_root = Path(sys.argv[1])
source_overlay_arg = sys.argv[2]
deploy_root_arg = sys.argv[3]
prometheus_rules_arg = sys.argv[4]
alertmanager_config_arg = sys.argv[5]
metadata_arg = sys.argv[6]
bundle_kind = sys.argv[7]
include_focused = sys.argv[8].lower() == 'true'
mode = sys.argv[9]
force = sys.argv[10].lower() == 'true'

source_overlay_root = Path(source_overlay_arg)
if not source_overlay_root.is_absolute():
    source_overlay_root = repo_root / source_overlay_root
if not source_overlay_root.exists():
    raise SystemExit(f"Error: source overlay root not found: {source_overlay_root}")

deploy_root = Path(deploy_root_arg)
if not deploy_root.is_absolute():
    deploy_root = repo_root / deploy_root

def resolve_optional_dir(value: str, default: Path) -> Path:
    if not value:
        return default
    path = Path(value)
    return path if path.is_absolute() else repo_root / path

prometheus_rules_dir = resolve_optional_dir(prometheus_rules_arg, deploy_root / 'prometheus' / 'rules.d')
alertmanager_config_dir = resolve_optional_dir(alertmanager_config_arg, deploy_root / 'alertmanager' / 'conf.d')
metadata_dir = resolve_optional_dir(metadata_arg, deploy_root / 'metadata')
focused_root = deploy_root / 'focused'


def ensure_parent(path: Path):
    path.parent.mkdir(parents=True, exist_ok=True)


def remove_existing(path: Path):
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.is_dir():
        shutil.rmtree(path)


def deploy_path(src: Path, dst: Path):
    if not src.exists() and not src.is_symlink():
        raise SystemExit(f"Error: source path missing for deploy: {src}")
    ensure_parent(dst)
    if dst.exists() or dst.is_symlink():
        if not force:
            raise SystemExit(f"Error: target exists (use --force to replace): {dst}")
        remove_existing(dst)
    if mode == 'symlink':
        dst.symlink_to(src)
    else:
        shutil.copy2(src, dst)


def rel(path: Path, root: Path) -> str:
    return str(path.relative_to(root)) if path.is_relative_to(root) else str(path)


deployed = {
    'prometheus': None,
    'alertmanager': None,
    'metadata': {},
    'focused': {
        'prometheus': [],
        'alertmanager': [],
    },
}

target_dirs = {
    'deployRoot': str(deploy_root),
    'prometheusRulesDir': str(prometheus_rules_dir),
    'alertmanagerConfigDir': str(alertmanager_config_dir),
    'metadataDir': str(metadata_dir),
    'focusedRoot': str(focused_root),
}

if bundle_kind in ('all', 'prometheus'):
    src = source_overlay_root / 'prometheus' / 'cex-monitoring-bundle.rules.yml'
    dst = prometheus_rules_dir / 'cex-monitoring-bundle.rules.yml'
    deploy_path(src, dst)
    deployed['prometheus'] = {
        'source': str(src),
        'deployedPath': str(dst),
        'deployRootRelativePath': rel(dst, deploy_root),
    }

if bundle_kind in ('all', 'alertmanager'):
    src = source_overlay_root / 'alertmanager' / 'cex-monitoring-bundle.yml'
    dst = alertmanager_config_dir / 'cex-monitoring-bundle.yml'
    deploy_path(src, dst)
    deployed['alertmanager'] = {
        'source': str(src),
        'deployedPath': str(dst),
        'deployRootRelativePath': rel(dst, deploy_root),
    }

metadata_files = [
    'monitoring-bundle-manifest.yml',
    'monitoring-export-metadata.yml',
    'monitoring-install-metadata.yml',
    'monitoring-link-metadata.yml',
]
for filename in metadata_files:
    src = source_overlay_root / 'metadata' / filename
    dst = metadata_dir / filename
    deploy_path(src, dst)
    deployed['metadata'][filename] = {
        'source': str(src),
        'deployedPath': str(dst),
        'deployRootRelativePath': rel(dst, deploy_root),
    }

if include_focused:
    if bundle_kind in ('all', 'prometheus'):
        src_root = source_overlay_root / 'focused' / 'prometheus'
        if src_root.exists():
            for src in sorted(src_root.glob('*.yml')):
                dst = focused_root / 'prometheus' / src.name
                deploy_path(src, dst)
                deployed['focused']['prometheus'].append({
                    'source': str(src),
                    'deployedPath': str(dst),
                    'deployRootRelativePath': rel(dst, deploy_root),
                })
    if bundle_kind in ('all', 'alertmanager'):
        src_root = source_overlay_root / 'focused' / 'alertmanager'
        if src_root.exists():
            for src in sorted(src_root.glob('*.yml')):
                dst = focused_root / 'alertmanager' / src.name
                deploy_path(src, dst)
                deployed['focused']['alertmanager'].append({
                    'source': str(src),
                    'deployedPath': str(dst),
                    'deployRootRelativePath': rel(dst, deploy_root),
                })

deploy_metadata = {
    'version': 1,
    'deployedAt': datetime.now(timezone.utc).isoformat(),
    'mode': mode,
    'bundleKind': bundle_kind,
    'includeFocused': include_focused,
    'sourceOverlayRoot': str(source_overlay_root),
    'targetDirs': target_dirs,
    'deployed': deployed,
}

deploy_metadata_path = metadata_dir / 'monitoring-deploy-metadata.yml'
ensure_parent(deploy_metadata_path)
if deploy_metadata_path.exists() or deploy_metadata_path.is_symlink():
    if not force:
        raise SystemExit(f"Error: target exists (use --force to replace): {deploy_metadata_path}")
    remove_existing(deploy_metadata_path)
deploy_metadata_path.write_text(
    '# Generated by scripts/deploy-monitoring-bundles.sh\n\n' + yaml.safe_dump(deploy_metadata, sort_keys=False, allow_unicode=True),
    encoding='utf-8',
)

print(f"deployed metadata -> {deploy_metadata_path}")
if deployed['prometheus']:
    print(f"deployed prometheus bundle -> {deployed['prometheus']['deployedPath']}")
if deployed['alertmanager']:
    print(f"deployed alertmanager bundle -> {deployed['alertmanager']['deployedPath']}")
if include_focused:
    print(
        "deployed focused components -> "
        f"prometheus={len(deployed['focused']['prometheus'])}, "
        f"alertmanager={len(deployed['focused']['alertmanager'])}"
    )
PY

if [[ "$RELOAD_AFTER_DEPLOY" == "true" ]]; then
  reload_cmd=(
    "$SCRIPT_DIR/reload-monitoring-targets.sh"
    --mode "$RELOAD_MODE"
    --timeout-secs "$RELOAD_TIMEOUT_SECS"
    --failure-policy "$RELOAD_FAILURE_POLICY"
    --summary-file "$RELOAD_SUMMARY_FILE"
  )
  if [[ -n "$RELOAD_PROMETHEUS_URL" ]]; then
    reload_cmd+=(--prometheus-url "$RELOAD_PROMETHEUS_URL")
  fi
  if [[ -n "$RELOAD_ALERTMANAGER_URL" ]]; then
    reload_cmd+=(--alertmanager-url "$RELOAD_ALERTMANAGER_URL")
  fi
  if [[ -n "$RELOAD_PROMETHEUS_COMMAND" ]]; then
    reload_cmd+=(--prometheus-command "$RELOAD_PROMETHEUS_COMMAND")
  fi
  if [[ -n "$RELOAD_ALERTMANAGER_COMMAND" ]]; then
    reload_cmd+=(--alertmanager-command "$RELOAD_ALERTMANAGER_COMMAND")
  fi
  if [[ -n "$RELOAD_PROMETHEUS_RESTART_COMMAND" ]]; then
    reload_cmd+=(--prometheus-restart-command "$RELOAD_PROMETHEUS_RESTART_COMMAND")
  fi
  if [[ -n "$RELOAD_ALERTMANAGER_RESTART_COMMAND" ]]; then
    reload_cmd+=(--alertmanager-restart-command "$RELOAD_ALERTMANAGER_RESTART_COMMAND")
  fi
  if [[ "$RELOAD_DRY_RUN" == "true" ]]; then
    reload_cmd+=(--dry-run)
  fi
  if [[ "$SKIP_RELOAD_PROMETHEUS" == "true" ]]; then
    reload_cmd+=(--skip-prometheus)
  fi
  if [[ "$SKIP_RELOAD_ALERTMANAGER" == "true" ]]; then
    reload_cmd+=(--skip-alertmanager)
  fi
  if "${reload_cmd[@]}"; then
    reload_exit=0
  else
    reload_exit=$?
  fi
  update_deploy_metadata_post_actions
  if [[ "$reload_exit" -ne 0 ]]; then
    exit "$reload_exit"
  fi
fi

if [[ "$VERIFY_AFTER_DEPLOY" == "true" ]]; then
  verify_cmd=(
    "$SCRIPT_DIR/verify-monitoring-targets.sh"
    --mode "$VERIFY_MODE"
    --timeout-secs "$VERIFY_TIMEOUT_SECS"
    --attempts "$VERIFY_ATTEMPTS"
    --delay-secs "$VERIFY_DELAY_SECS"
    --summary-file "$VERIFY_SUMMARY_FILE"
  )
  if [[ -n "$VERIFY_PROMETHEUS_URL" ]]; then
    verify_cmd+=(--prometheus-url "$VERIFY_PROMETHEUS_URL")
  fi
  if [[ -n "$VERIFY_ALERTMANAGER_URL" ]]; then
    verify_cmd+=(--alertmanager-url "$VERIFY_ALERTMANAGER_URL")
  fi
  if [[ -n "$VERIFY_PROMETHEUS_COMMAND" ]]; then
    verify_cmd+=(--prometheus-command "$VERIFY_PROMETHEUS_COMMAND")
  fi
  if [[ -n "$VERIFY_ALERTMANAGER_COMMAND" ]]; then
    verify_cmd+=(--alertmanager-command "$VERIFY_ALERTMANAGER_COMMAND")
  fi
  if [[ "$VERIFY_DRY_RUN" == "true" ]]; then
    verify_cmd+=(--dry-run)
  fi
  if [[ "$SKIP_VERIFY_PROMETHEUS" == "true" ]]; then
    verify_cmd+=(--skip-prometheus)
  fi
  if [[ "$SKIP_VERIFY_ALERTMANAGER" == "true" ]]; then
    verify_cmd+=(--skip-alertmanager)
  fi
  if "${verify_cmd[@]}"; then
    verify_exit=0
  else
    verify_exit=$?
  fi
  update_deploy_metadata_post_actions
  if [[ "$verify_exit" -ne 0 ]]; then
    exit "$verify_exit"
  fi
fi

if [[ "$RELOAD_AFTER_DEPLOY" != "true" && "$VERIFY_AFTER_DEPLOY" != "true" ]]; then
  update_deploy_metadata_post_actions
fi

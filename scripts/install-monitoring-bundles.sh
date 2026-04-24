#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

MANIFEST_PATH="ops/monitoring/monitoring-bundle-manifest.example.yml"
INSTALL_DIR="run/monitoring-install"
BUNDLE_KIND="all"
INCLUDE_FOCUSED="false"
FROM_EXPORT_DIR=""
STAGING_DIR=""

usage() {
  cat <<'EOF'
Usage: scripts/install-monitoring-bundles.sh [--manifest <path>] [--install-dir <dir>] [--bundle all|prometheus|alertmanager] [--include-focused] [--from-export-dir <dir>]

Installs the current monitoring bundles into a repo-local install layout.

Default behavior:
  - exports the current manifest-driven bundles to a temporary staging directory
  - installs them into:
      <install-dir>/prometheus/cex-monitoring-bundle.rules.yml
      <install-dir>/alertmanager/cex-monitoring-bundle.yml
      <install-dir>/metadata/monitoring-bundle-manifest.yml
      <install-dir>/metadata/monitoring-export-metadata.yml
      <install-dir>/metadata/monitoring-install-metadata.yml

Options:
  --manifest <path>       Override manifest path (default: ops/monitoring/monitoring-bundle-manifest.example.yml)
  --install-dir <dir>     Install destination root (default: run/monitoring-install)
  --bundle <kind>         One of: all, prometheus, alertmanager (default: all)
  --include-focused       Also install focused component files under focused/prometheus|alertmanager/
  --from-export-dir <dir> Reuse an existing export directory instead of running export-monitoring-bundles.sh
  -h, --help              Show this help

Examples:
  ./scripts/install-monitoring-bundles.sh
  ./scripts/install-monitoring-bundles.sh --install-dir /tmp/cex-monitoring-install
  ./scripts/install-monitoring-bundles.sh --include-focused
  ./scripts/install-monitoring-bundles.sh --from-export-dir /tmp/cex-monitoring-export
EOF
}

cleanup() {
  if [[ -n "$STAGING_DIR" && -d "$STAGING_DIR" ]]; then
    rm -rf "$STAGING_DIR"
  fi
}
trap cleanup EXIT

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      [[ $# -ge 2 ]] || { echo "Error: --manifest requires a path" >&2; exit 2; }
      MANIFEST_PATH="$2"
      shift 2
      ;;
    --install-dir)
      [[ $# -ge 2 ]] || { echo "Error: --install-dir requires a directory" >&2; exit 2; }
      INSTALL_DIR="$2"
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
    --from-export-dir)
      [[ $# -ge 2 ]] || { echo "Error: --from-export-dir requires a directory" >&2; exit 2; }
      FROM_EXPORT_DIR="$2"
      shift 2
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

if [[ -n "$FROM_EXPORT_DIR" ]]; then
  EXPORT_DIR="$FROM_EXPORT_DIR"
else
  STAGING_DIR="$(mktemp -d "${TMPDIR:-/tmp}/cex-monitoring-export.XXXXXX")"
  EXPORT_DIR="$STAGING_DIR"
  "$SCRIPT_DIR/export-monitoring-bundles.sh" \
    --manifest "$MANIFEST_PATH" \
    --output-dir "$EXPORT_DIR" \
    --bundle "$BUNDLE_KIND" \
    $([[ "$INCLUDE_FOCUSED" == "true" ]] && printf '%s' '--include-focused') \
    >/dev/null
fi

python3 - "$REPO_ROOT" "$EXPORT_DIR" "$INSTALL_DIR" "$BUNDLE_KIND" "$INCLUDE_FOCUSED" <<'PY'
from __future__ import annotations

import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path
import yaml

repo_root = Path(sys.argv[1])
export_arg = sys.argv[2]
install_arg = sys.argv[3]
bundle_kind = sys.argv[4]
include_focused = sys.argv[5].lower() == 'true'

export_dir = Path(export_arg)
if not export_dir.is_absolute():
    export_dir = repo_root / export_dir
if not export_dir.exists():
    raise SystemExit(f"Error: export directory not found: {export_dir}")

install_dir = Path(install_arg)
if not install_dir.is_absolute():
    install_dir = repo_root / install_dir
install_dir.mkdir(parents=True, exist_ok=True)

metadata_src = export_dir / 'monitoring-export-metadata.yml'
manifest_src = export_dir / 'monitoring-bundle-manifest.yml'
if not metadata_src.exists():
    raise SystemExit(f"Error: export metadata not found: {metadata_src}")
if not manifest_src.exists():
    raise SystemExit(f"Error: exported manifest not found: {manifest_src}")

with metadata_src.open('r', encoding='utf-8') as f:
    export_metadata = yaml.safe_load(f) or {}

installed = {
    'prometheus': None,
    'alertmanager': None,
    'metadata': {},
    'focused': {
        'prometheus': [],
        'alertmanager': [],
    },
}


def rel(path: Path) -> str:
    return str(path.relative_to(install_dir)) if path.is_relative_to(install_dir) else str(path)


def copy_required(src: Path, dst: Path):
    if not src.exists():
        raise SystemExit(f"Error: required export file missing: {src}")
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)

if bundle_kind in ('all', 'prometheus'):
    src = export_dir / 'prometheus-bundle.yml'
    dst = install_dir / 'prometheus' / 'cex-monitoring-bundle.rules.yml'
    copy_required(src, dst)
    installed['prometheus'] = {
        'source': str(src.relative_to(export_dir)),
        'installedPath': rel(dst),
    }

if bundle_kind in ('all', 'alertmanager'):
    src = export_dir / 'alertmanager-bundle.yml'
    dst = install_dir / 'alertmanager' / 'cex-monitoring-bundle.yml'
    copy_required(src, dst)
    installed['alertmanager'] = {
        'source': str(src.relative_to(export_dir)),
        'installedPath': rel(dst),
    }

metadata_dir = install_dir / 'metadata'
metadata_dir.mkdir(parents=True, exist_ok=True)
manifest_dst = metadata_dir / 'monitoring-bundle-manifest.yml'
export_metadata_dst = metadata_dir / 'monitoring-export-metadata.yml'
copy_required(manifest_src, manifest_dst)
copy_required(metadata_src, export_metadata_dst)
installed['metadata'] = {
    'manifestInstalledPath': rel(manifest_dst),
    'exportMetadataInstalledPath': rel(export_metadata_dst),
}

if include_focused:
    if bundle_kind in ('all', 'prometheus'):
        src_root = export_dir / 'focused' / 'prometheus'
        dst_root = install_dir / 'focused' / 'prometheus'
        if src_root.exists():
            dst_root.mkdir(parents=True, exist_ok=True)
            for src in sorted(src_root.glob('*.yml')):
                dst = dst_root / src.name
                shutil.copy2(src, dst)
                installed['focused']['prometheus'].append({
                    'source': str(src.relative_to(export_dir)),
                    'installedPath': rel(dst),
                })
    if bundle_kind in ('all', 'alertmanager'):
        src_root = export_dir / 'focused' / 'alertmanager'
        dst_root = install_dir / 'focused' / 'alertmanager'
        if src_root.exists():
            dst_root.mkdir(parents=True, exist_ok=True)
            for src in sorted(src_root.glob('*.yml')):
                dst = dst_root / src.name
                shutil.copy2(src, dst)
                installed['focused']['alertmanager'].append({
                    'source': str(src.relative_to(export_dir)),
                    'installedPath': rel(dst),
                })

install_metadata = {
    'version': 1,
    'installedAt': datetime.now(timezone.utc).isoformat(),
    'installRoot': str(install_dir),
    'bundleKind': bundle_kind,
    'includeFocused': include_focused,
    'exportSourceDir': str(export_dir),
    'exportMetadata': export_metadata,
    'installed': installed,
}

install_metadata_path = metadata_dir / 'monitoring-install-metadata.yml'
install_metadata_path.write_text(
    '# Generated by scripts/install-monitoring-bundles.sh\n\n' + yaml.safe_dump(install_metadata, sort_keys=False, allow_unicode=True),
    encoding='utf-8',
)
installed['metadata']['installMetadataInstalledPath'] = rel(install_metadata_path)

print(f"installed metadata -> {rel(install_metadata_path)}")
if installed['prometheus']:
    print(f"installed prometheus bundle -> {installed['prometheus']['installedPath']}")
if installed['alertmanager']:
    print(f"installed alertmanager bundle -> {installed['alertmanager']['installedPath']}")
if include_focused:
    print(
        "installed focused components -> "
        f"prometheus={len(installed['focused']['prometheus'])}, "
        f"alertmanager={len(installed['focused']['alertmanager'])}"
    )
PY

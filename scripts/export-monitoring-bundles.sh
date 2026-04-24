#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

MANIFEST_PATH="ops/monitoring/monitoring-bundle-manifest.example.yml"
OUTPUT_DIR="run/monitoring-export"
BUNDLE_KIND="all"
INCLUDE_FOCUSED="false"
ASSEMBLE_FIRST="true"

usage() {
  cat <<'EOF'
Usage: scripts/export-monitoring-bundles.sh [--manifest <path>] [--output-dir <dir>] [--bundle all|prometheus|alertmanager] [--include-focused] [--no-assemble]

Exports the current manifest-driven monitoring bundles into an install-friendly output directory.

Default behavior:
  - reads ops/monitoring/monitoring-bundle-manifest.example.yml
  - runs scripts/assemble-monitoring-bundles.sh first
  - exports combined bundle files into:
      <output-dir>/prometheus-bundle.yml
      <output-dir>/alertmanager-bundle.yml
      <output-dir>/monitoring-bundle-manifest.yml
      <output-dir>/monitoring-export-metadata.yml

Options:
  --manifest <path>      Override manifest path (default: ops/monitoring/monitoring-bundle-manifest.example.yml)
  --output-dir <dir>     Export destination (default: run/monitoring-export)
  --bundle <kind>        One of: all, prometheus, alertmanager (default: all)
  --include-focused      Also export focused component files under focused/prometheus|alertmanager/
  --no-assemble          Skip assemble step and export current files as-is
  -h, --help             Show this help

Examples:
  ./scripts/export-monitoring-bundles.sh
  ./scripts/export-monitoring-bundles.sh --output-dir /tmp/cex-monitoring-export
  ./scripts/export-monitoring-bundles.sh --include-focused
  ./scripts/export-monitoring-bundles.sh --bundle prometheus --no-assemble
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      [[ $# -ge 2 ]] || { echo "Error: --manifest requires a path" >&2; exit 2; }
      MANIFEST_PATH="$2"
      shift 2
      ;;
    --output-dir)
      [[ $# -ge 2 ]] || { echo "Error: --output-dir requires a directory" >&2; exit 2; }
      OUTPUT_DIR="$2"
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
    --no-assemble)
      ASSEMBLE_FIRST="false"
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

if [[ "$ASSEMBLE_FIRST" == "true" ]]; then
  "$SCRIPT_DIR/assemble-monitoring-bundles.sh" --manifest "$MANIFEST_PATH" --bundle "$BUNDLE_KIND" >/dev/null
fi

python3 - "$REPO_ROOT" "$MANIFEST_PATH" "$OUTPUT_DIR" "$BUNDLE_KIND" "$INCLUDE_FOCUSED" <<'PY'
from __future__ import annotations

import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path
import yaml

repo_root = Path(sys.argv[1])
manifest_arg = sys.argv[2]
output_arg = sys.argv[3]
bundle_kind = sys.argv[4]
include_focused = sys.argv[5].lower() == 'true'

manifest_path = Path(manifest_arg)
if not manifest_path.is_absolute():
    manifest_path = repo_root / manifest_path
if not manifest_path.exists():
    raise SystemExit(f"Error: manifest not found: {manifest_path}")

output_dir = Path(output_arg)
if not output_dir.is_absolute():
    output_dir = repo_root / output_dir
output_dir.mkdir(parents=True, exist_ok=True)

manifest = yaml.safe_load(manifest_path.read_text(encoding='utf-8')) or {}
if not isinstance(manifest, dict):
    raise SystemExit("Error: manifest must be a YAML object")


def repo_path(rel_or_abs: str) -> Path:
    path = Path(rel_or_abs)
    return path if path.is_absolute() else repo_root / path


def rel_for_output(path: Path) -> str:
    return str(path.relative_to(output_dir)) if path.is_relative_to(output_dir) else str(path)


exported = {
    'prometheus': None,
    'alertmanager': None,
    'focused': {
        'prometheus': [],
        'alertmanager': [],
    },
}

manifest_export_path = output_dir / 'monitoring-bundle-manifest.yml'
shutil.copy2(manifest_path, manifest_export_path)

if bundle_kind in ('all', 'prometheus'):
    src = repo_path(manifest['prometheus']['combinedBundle'])
    if not src.exists():
        raise SystemExit(f"Error: prometheus combined bundle not found: {src}")
    dst = output_dir / 'prometheus-bundle.yml'
    shutil.copy2(src, dst)
    exported['prometheus'] = {
        'source': str(src.relative_to(repo_root)) if src.is_relative_to(repo_root) else str(src),
        'exportedPath': rel_for_output(dst),
    }

if bundle_kind in ('all', 'alertmanager'):
    src = repo_path(manifest['alertmanager']['combinedBundle'])
    if not src.exists():
        raise SystemExit(f"Error: alertmanager combined bundle not found: {src}")
    dst = output_dir / 'alertmanager-bundle.yml'
    shutil.copy2(src, dst)
    exported['alertmanager'] = {
        'source': str(src.relative_to(repo_root)) if src.is_relative_to(repo_root) else str(src),
        'exportedPath': rel_for_output(dst),
    }

if include_focused:
    if bundle_kind in ('all', 'prometheus'):
        dst_root = output_dir / 'focused' / 'prometheus'
        dst_root.mkdir(parents=True, exist_ok=True)
        for entry in manifest['prometheus'].get('files', []):
            src = repo_path(entry['path'])
            dst = dst_root / Path(entry['path']).name
            shutil.copy2(src, dst)
            exported['focused']['prometheus'].append({
                'source': entry['path'],
                'exportedPath': rel_for_output(dst),
            })
    if bundle_kind in ('all', 'alertmanager'):
        dst_root = output_dir / 'focused' / 'alertmanager'
        dst_root.mkdir(parents=True, exist_ok=True)
        for entry in manifest['alertmanager'].get('files', []):
            src = repo_path(entry['path'])
            dst = dst_root / Path(entry['path']).name
            shutil.copy2(src, dst)
            exported['focused']['alertmanager'].append({
                'source': entry['path'],
                'exportedPath': rel_for_output(dst),
            })

metadata = {
    'version': 1,
    'bundle': manifest.get('bundle'),
    'exportedAt': datetime.now(timezone.utc).isoformat(),
    'manifestSource': str(manifest_path.relative_to(repo_root)) if manifest_path.is_relative_to(repo_root) else str(manifest_path),
    'manifestExportedPath': rel_for_output(manifest_export_path),
    'bundleKind': bundle_kind,
    'includeFocused': include_focused,
    'exported': exported,
    'managementScripts': manifest.get('managementScripts') or [],
    'bridgeScripts': manifest.get('bridgeScripts') or [],
    'notes': manifest.get('notes') or [],
}

metadata_path = output_dir / 'monitoring-export-metadata.yml'
metadata_path.write_text(
    '# Generated by scripts/export-monitoring-bundles.sh\n\n' + yaml.safe_dump(metadata, sort_keys=False, allow_unicode=True),
    encoding='utf-8',
)

print(f"exported manifest -> {rel_for_output(manifest_export_path)}")
if exported['prometheus']:
    print(f"exported prometheus bundle -> {exported['prometheus']['exportedPath']}")
if exported['alertmanager']:
    print(f"exported alertmanager bundle -> {exported['alertmanager']['exportedPath']}")
if include_focused:
    prom_count = len(exported['focused']['prometheus'])
    alert_count = len(exported['focused']['alertmanager'])
    print(f"exported focused components -> prometheus={prom_count}, alertmanager={alert_count}")
print(f"exported metadata -> {rel_for_output(metadata_path)}")
PY

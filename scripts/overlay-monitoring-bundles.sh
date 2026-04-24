#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

MANIFEST_PATH="ops/monitoring/monitoring-bundle-manifest.example.yml"
SOURCE_INSTALL_DIR="run/monitoring-install"
TARGET_ROOT="run/monitoring-overlay"
BUNDLE_KIND="all"
INCLUDE_FOCUSED="false"
INSTALL_FIRST="true"
FORCE="false"

usage() {
  cat <<'EOF'
Usage: scripts/overlay-monitoring-bundles.sh [--manifest <path>] [--from-install-dir <dir>] [--target-root <dir>] [--bundle all|prometheus|alertmanager] [--include-focused] [--no-install] [--force]

Creates a symlink-based overlay from a monitoring install layout into another target root.

Default behavior:
  - runs scripts/install-monitoring-bundles.sh into run/monitoring-install
  - creates a symlink overlay under run/monitoring-overlay
  - mirrors the install layout using symlinks:
      <target-root>/prometheus/cex-monitoring-bundle.rules.yml
      <target-root>/alertmanager/cex-monitoring-bundle.yml
      <target-root>/metadata/monitoring-bundle-manifest.yml
      <target-root>/metadata/monitoring-export-metadata.yml
      <target-root>/metadata/monitoring-install-metadata.yml
      <target-root>/metadata/monitoring-link-metadata.yml

Options:
  --manifest <path>        Override manifest path (default: ops/monitoring/monitoring-bundle-manifest.example.yml)
  --from-install-dir <dir> Reuse an existing install layout instead of regenerating run/monitoring-install
  --target-root <dir>      Overlay destination root (default: run/monitoring-overlay)
  --bundle <kind>          One of: all, prometheus, alertmanager (default: all)
  --include-focused        Also overlay focused component files under focused/prometheus|alertmanager/
  --no-install             Skip install step and reuse the source install dir as-is
  --force                  Replace existing files/symlinks in the target root
  -h, --help               Show this help

Examples:
  ./scripts/overlay-monitoring-bundles.sh
  ./scripts/overlay-monitoring-bundles.sh --target-root /tmp/cex-monitoring-overlay --include-focused
  ./scripts/overlay-monitoring-bundles.sh --from-install-dir /tmp/cex-monitoring-install --target-root /tmp/cex-monitoring-live
  ./scripts/overlay-monitoring-bundles.sh --force
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      [[ $# -ge 2 ]] || { echo "Error: --manifest requires a path" >&2; exit 2; }
      MANIFEST_PATH="$2"
      shift 2
      ;;
    --from-install-dir)
      [[ $# -ge 2 ]] || { echo "Error: --from-install-dir requires a directory" >&2; exit 2; }
      SOURCE_INSTALL_DIR="$2"
      INSTALL_FIRST="false"
      shift 2
      ;;
    --target-root)
      [[ $# -ge 2 ]] || { echo "Error: --target-root requires a directory" >&2; exit 2; }
      TARGET_ROOT="$2"
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
    --no-install)
      INSTALL_FIRST="false"
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

if [[ "$INSTALL_FIRST" == "true" ]]; then
  install_cmd=(
    "$SCRIPT_DIR/install-monitoring-bundles.sh"
    --manifest "$MANIFEST_PATH"
    --install-dir "$SOURCE_INSTALL_DIR"
    --bundle "$BUNDLE_KIND"
  )
  if [[ "$INCLUDE_FOCUSED" == "true" ]]; then
    install_cmd+=(--include-focused)
  fi
  "${install_cmd[@]}" >/dev/null
fi

python3 - "$REPO_ROOT" "$SOURCE_INSTALL_DIR" "$TARGET_ROOT" "$BUNDLE_KIND" "$INCLUDE_FOCUSED" "$FORCE" <<'PY'
from __future__ import annotations

import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path
import yaml

repo_root = Path(sys.argv[1])
source_install_arg = sys.argv[2]
target_root_arg = sys.argv[3]
bundle_kind = sys.argv[4]
include_focused = sys.argv[5].lower() == 'true'
force = sys.argv[6].lower() == 'true'

source_install_dir = Path(source_install_arg)
if not source_install_dir.is_absolute():
    source_install_dir = repo_root / source_install_dir
if not source_install_dir.exists():
    raise SystemExit(f"Error: source install dir not found: {source_install_dir}")

target_root = Path(target_root_arg)
if not target_root.is_absolute():
    target_root = repo_root / target_root


def ensure_parent(path: Path):
    path.parent.mkdir(parents=True, exist_ok=True)


def remove_existing(path: Path):
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.is_dir():
        shutil.rmtree(path)


def link_path(src: Path, dst: Path):
    if not src.exists():
        raise SystemExit(f"Error: source file missing for overlay: {src}")
    ensure_parent(dst)
    if dst.exists() or dst.is_symlink():
        if not force:
            raise SystemExit(f"Error: target exists (use --force to replace): {dst}")
        remove_existing(dst)
    dst.symlink_to(src.resolve())


def rel(path: Path) -> str:
    return str(path.relative_to(target_root)) if path.is_relative_to(target_root) else str(path)


linked = {
    'prometheus': None,
    'alertmanager': None,
    'metadata': {},
    'focused': {
        'prometheus': [],
        'alertmanager': [],
    },
}

if bundle_kind in ('all', 'prometheus'):
    src = source_install_dir / 'prometheus' / 'cex-monitoring-bundle.rules.yml'
    dst = target_root / 'prometheus' / 'cex-monitoring-bundle.rules.yml'
    link_path(src, dst)
    linked['prometheus'] = {
        'source': str(src),
        'linkedPath': rel(dst),
    }

if bundle_kind in ('all', 'alertmanager'):
    src = source_install_dir / 'alertmanager' / 'cex-monitoring-bundle.yml'
    dst = target_root / 'alertmanager' / 'cex-monitoring-bundle.yml'
    link_path(src, dst)
    linked['alertmanager'] = {
        'source': str(src),
        'linkedPath': rel(dst),
    }

metadata_pairs = [
    ('monitoring-bundle-manifest.yml', 'manifestLinkedPath'),
    ('monitoring-export-metadata.yml', 'exportMetadataLinkedPath'),
    ('monitoring-install-metadata.yml', 'installMetadataLinkedPath'),
]
for filename, field in metadata_pairs:
    src = source_install_dir / 'metadata' / filename
    dst = target_root / 'metadata' / filename
    link_path(src, dst)
    linked['metadata'][field] = rel(dst)

if include_focused:
    if bundle_kind in ('all', 'prometheus'):
        src_root = source_install_dir / 'focused' / 'prometheus'
        if src_root.exists():
            for src in sorted(src_root.glob('*.yml')):
                dst = target_root / 'focused' / 'prometheus' / src.name
                link_path(src, dst)
                linked['focused']['prometheus'].append({
                    'source': str(src),
                    'linkedPath': rel(dst),
                })
    if bundle_kind in ('all', 'alertmanager'):
        src_root = source_install_dir / 'focused' / 'alertmanager'
        if src_root.exists():
            for src in sorted(src_root.glob('*.yml')):
                dst = target_root / 'focused' / 'alertmanager' / src.name
                link_path(src, dst)
                linked['focused']['alertmanager'].append({
                    'source': str(src),
                    'linkedPath': rel(dst),
                })

link_metadata = {
    'version': 1,
    'linkedAt': datetime.now(timezone.utc).isoformat(),
    'sourceInstallDir': str(source_install_dir),
    'targetRoot': str(target_root),
    'bundleKind': bundle_kind,
    'includeFocused': include_focused,
    'linked': linked,
}

link_metadata_path = target_root / 'metadata' / 'monitoring-link-metadata.yml'
ensure_parent(link_metadata_path)
if link_metadata_path.exists() or link_metadata_path.is_symlink():
    if not force:
        raise SystemExit(f"Error: target exists (use --force to replace): {link_metadata_path}")
    remove_existing(link_metadata_path)
link_metadata_path.write_text(
    '# Generated by scripts/overlay-monitoring-bundles.sh\n\n' + yaml.safe_dump(link_metadata, sort_keys=False, allow_unicode=True),
    encoding='utf-8',
)
linked['metadata']['linkMetadataPath'] = rel(link_metadata_path)

print(f"linked metadata -> {rel(link_metadata_path)}")
if linked['prometheus']:
    print(f"linked prometheus bundle -> {linked['prometheus']['linkedPath']}")
if linked['alertmanager']:
    print(f"linked alertmanager bundle -> {linked['alertmanager']['linkedPath']}")
if include_focused:
    print(
        "linked focused components -> "
        f"prometheus={len(linked['focused']['prometheus'])}, "
        f"alertmanager={len(linked['focused']['alertmanager'])}"
    )
PY

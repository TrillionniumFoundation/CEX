#!/usr/bin/env python3
"""Verify vendored Chain bytes and the narrowly approved downstream patch ledger.

This proves repository source identity only. It does not compile the vendor crates,
verify upstream governance, or grant production authorization.
"""
from __future__ import annotations
import hashlib, json, os, stat, subprocess, sys
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = Path('vendor/trnm-chain-vendor-manifest.json')
PATCHES = Path('vendor/trnm-chain-downstream-patches-v1.json')
SCHEMA = 'cex.vendor-provenance-check.v1'
MAX_FILE_BYTES = 8 * 1024 * 1024


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in pairs:
        if key in out:
            raise ValueError('duplicate_json_field')
        out[key] = value
    return out


def load_json(path: Path) -> Any:
    return json.loads(path.read_text('utf-8'), object_pairs_hook=unique_object)


def git_blob_sha(data: bytes) -> str:
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()


def read_regular(root: Path, relative: str) -> bytes:
    path = root / relative
    if path.is_symlink():
        raise ValueError('linked_vendor_path:' + relative)
    info = path.stat()
    if not stat.S_ISREG(info.st_mode):
        raise ValueError('nonregular_vendor_path:' + relative)
    if info.st_mode & 0o111:
        raise ValueError('executable_vendor_path:' + relative)
    if info.st_size > MAX_FILE_BYTES:
        raise ValueError('vendor_file_too_large:' + relative)
    return path.read_bytes()


def default_tree_provider(root: Path, relative: str) -> str:
    process = subprocess.run(
        ['git', 'rev-parse', 'HEAD:' + relative], cwd=root, stdin=subprocess.DEVNULL,
        capture_output=True, text=True, timeout=10, check=False,
    )
    if process.returncode != 0:
        raise ValueError('git_tree_unavailable:' + relative)
    value = process.stdout.strip()
    if len(value) != 40 or any(c not in '0123456789abcdef' for c in value):
        raise ValueError('invalid_git_tree:' + relative)
    return value


def validate(
    root: Path,
    manifest: dict[str, Any],
    ledger: dict[str, Any],
    tree_provider: Callable[[Path, str], str] | None = None,
) -> list[str]:
    problems: list[str] = []
    if manifest.get('schema') != 'hepta.vendor.trnm_chain_crates.v1':
        problems.append('manifest_schema')
    if ledger.get('schema') != 'cex.trnm-chain-downstream-patches.v1':
        problems.append('patch_ledger_schema')
    if ledger.get('production_authorization') != 'not_granted':
        problems.append('patch_ledger_authority')
    if ledger.get('source_manifest') != MANIFEST.as_posix():
        problems.append('patch_ledger_manifest_binding')
    patches = ledger.get('patches')
    crates = manifest.get('crates')
    if not isinstance(patches, list) or not isinstance(crates, dict):
        return problems + ['invalid_vendor_documents']

    patch_map: dict[tuple[str, str], dict[str, Any]] = {}
    for item in patches:
        if not isinstance(item, dict):
            problems.append('invalid_patch_record')
            continue
        key = (item.get('package'), item.get('path'))
        if not all(isinstance(value, str) and value for value in key) or key in patch_map:
            problems.append('duplicate_or_invalid_patch_identity')
            continue
        patch_map[key] = item
        for field in (
            'source_git_tree', 'source_git_blob', 'vendored_git_tree',
            'vendored_git_blob', 'introduced_by_cex_commit',
        ):
            value = item.get(field)
            if not isinstance(value, str) or len(value) != 40 or any(
                c not in '0123456789abcdef' for c in value
            ):
                problems.append('invalid_patch_' + field + ':' + str(key))
        if item.get('runtime_behavior_changed') is not False:
            problems.append('runtime_patch_not_allowed:' + str(key))
        if item.get('scope') != 'test_only':
            problems.append('non_test_patch_not_allowed:' + str(key))
        if item.get('upstream_identity_unchanged') is not True:
            problems.append('upstream_identity_rewritten:' + str(key))
        disposition = item.get('required_rebase_disposition')
        if not isinstance(disposition, str) or not disposition.strip():
            problems.append('missing_rebase_disposition:' + str(key))

    seen: set[tuple[str, str]] = set()
    for package, record in crates.items():
        files = record.get('files') if isinstance(record, dict) else None
        if not isinstance(files, dict):
            problems.append('invalid_file_manifest:' + package)
            continue
        crate_root = root / 'vendor' / package
        actual: list[str] = []
        if crate_root.is_dir():
            for path in crate_root.rglob('*'):
                if path.is_file() or path.is_symlink():
                    actual.append(path.relative_to(crate_root).as_posix())
        if set(actual) != set(files):
            problems.append('vendor_file_set_mismatch:' + package)
        for relative, expected_sha256 in files.items():
            key = (package, relative)
            seen.add(key)
            try:
                data = read_regular(root, f'vendor/{package}/{relative}')
            except (OSError, ValueError) as error:
                problems.append(str(error))
                continue
            patch = patch_map.get(key)
            if patch:
                if patch.get('source_git_tree') != record.get('source_git_tree'):
                    problems.append('patch_source_tree_mismatch:' + package)
                if patch.get('source_sha256') != expected_sha256:
                    problems.append('patch_source_digest_mismatch:' + package + '/' + relative)
                if git_blob_sha(data) != patch.get('vendored_git_blob'):
                    problems.append('patched_blob_mismatch:' + package + '/' + relative)
            elif hashlib.sha256(data).hexdigest() != expected_sha256:
                problems.append('vendor_sha256_mismatch:' + package + '/' + relative)
        package_patches = [item for (pkg, _), item in patch_map.items() if pkg == package]
        if package_patches and tree_provider is not None:
            try:
                observed = tree_provider(root, 'vendor/' + package)
            except (OSError, ValueError, subprocess.TimeoutExpired) as error:
                problems.append(str(error))
                observed = None
            expected_trees = {item.get('vendored_git_tree') for item in package_patches}
            if len(expected_trees) != 1 or observed not in expected_trees:
                problems.append('patched_tree_mismatch:' + package)
    for key in patch_map:
        if key not in seen:
            problems.append('patch_references_unknown_manifest_file:' + str(key))
    return problems


def main() -> int:
    result: dict[str, Any] = {
        'schema': SCHEMA,
        'status': 'failed',
        'checker_may_grant_production_authorization': False,
        'production_authorization': 'not_granted',
        'problems': [],
    }
    try:
        manifest = load_json(ROOT / MANIFEST)
        ledger = load_json(ROOT / PATCHES)
        result['problems'] = validate(ROOT, manifest, ledger, default_tree_provider)
        result['status'] = 'ok' if not result['problems'] else 'failed'
    except (OSError, ValueError, json.JSONDecodeError):
        result['problems'] = ['vendor_provenance_inputs_invalid']
    print(json.dumps(result, sort_keys=True, indent=2))
    return 0 if result['status'] == 'ok' else 1


if __name__ == '__main__':
    raise SystemExit(main())

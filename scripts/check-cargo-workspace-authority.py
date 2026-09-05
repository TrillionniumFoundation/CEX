#!/usr/bin/env python3
"""Compare Cargo's actual workspace and targets with the documentation catalog.

Requires the real Cargo executable and a complete source tree. Unit tests may
call the pure validator with fixtures; the CLI has no metadata-file/fake-success
mode. This observation is not compilation, hosted attestation, or release approval.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tomllib
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MAX_FILE_BYTES = 8 * 1024 * 1024
MAX_METADATA_BYTES = 32 * 1024 * 1024
MAX_TARGET_LAYOUT_ENTRIES = 100_000
CATALOG = 'docs/module-catalog-v1.json'
SCHEMA = 'cex.cargo-workspace-authority.v1'


class ContractError(RuntimeError):
    pass


def require(condition: bool, code: str) -> None:
    if not condition:
        raise ContractError(code)


def local_path(root: Path, value: str, *, absolute: bool = False) -> Path:
    require(isinstance(value, str) and bool(value), 'invalid_repository_path')
    require(not any(ord(c) < 32 for c in value), 'invalid_repository_path')
    path = Path(value)
    require('..' not in path.parts, 'repository_path_escape')
    if absolute:
        require(path.is_absolute(), 'metadata_path_not_absolute')
    else:
        require(not path.is_absolute() and '\\' not in value and path.as_posix() == value,
                'noncanonical_repository_path')
        path = root / path
    try:
        path.relative_to(root)
    except ValueError:
        raise ContractError('repository_path_escape') from None
    require(not any(p.is_symlink() for p in (path, *path.parents)), 'linked_repository_path')
    return path


def read(root: Path, relative: str) -> bytes:
    path = local_path(root, relative)
    try:
        mode = path.stat()
        require(stat.S_ISREG(mode.st_mode) and mode.st_size <= MAX_FILE_BYTES,
                'invalid_source_file:' + relative)
        data = path.read_bytes()
    except OSError:
        raise ContractError('source_unavailable:' + relative) from None
    require(len(data) <= MAX_FILE_BYTES, 'source_too_large:' + relative)
    return data


def unique_json(raw: bytes) -> Any:
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 'duplicate_json_field')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs)


def snapshot(root: Path) -> tuple[dict[str, bytes], dict[str, dict]]:
    data = {p: read(root, p) for p in ('Cargo.toml', 'Cargo.lock', CATALOG)}
    manifest = tomllib.loads(data['Cargo.toml'].decode('utf-8'))
    workspace = manifest.get('workspace', {})
    explicit = workspace.get('members')
    require(isinstance(explicit, list) and bool(explicit), 'explicit_members_required')
    require(all(isinstance(p, str) for p in explicit), 'invalid_explicit_member')
    require(len(set(explicit)) == len(explicit), 'duplicate_explicit_member')
    # This repository uses an exact, auditable member list rather than globs.
    for member in explicit:
        require(not any(c in member for c in '*?['), 'workspace_glob_not_supported')
        local_path(root, member)
    catalog = unique_json(data[CATALOG])
    require(isinstance(catalog, dict) and catalog.get('schema') == 'cex.module-catalog.v1',
            'invalid_module_catalog')
    require(catalog.get('production_authorization') == 'not_granted', 'catalog_authority_overclaim')
    entries = catalog.get('modules')
    require(isinstance(entries, list), 'module_entries_required')
    by_member, names = {}, set()
    for item in entries:
        require(isinstance(item, dict), 'invalid_module_entry')
        member, name = item.get('workspace_member'), item.get('package')
        require(isinstance(member, str) and isinstance(name, str) and bool(name), 'invalid_module_identity')
        require(member not in by_member and name not in names, 'duplicate_module_identity')
        local_path(root, member)
        by_member[member] = item
        names.add(name)
    require(set(explicit) == set(by_member), 'explicit_member_catalog_mismatch')
    for member, item in by_member.items():
        manifest_path = member + '/Cargo.toml'
        data[manifest_path] = read(root, manifest_path)
        package = tomllib.loads(data[manifest_path].decode('utf-8')).get('package', {})
        require(package.get('name') == item['package'], 'manifest_package_mismatch:' + member)
        document = item.get('documentation')
        require(isinstance(document, str) and document.startswith('docs/modules/'), 'invalid_module_document')
        data[document] = read(root, document)
        sources = item.get('source_entrypoints')
        require(isinstance(sources, list) and bool(sources), 'source_entrypoints_required:' + member)
        require(all(isinstance(p, str) for p in sources) and len(set(sources)) == len(sources),
                'invalid_source_entrypoints:' + member)
        for source in sources:
            require(source.startswith(member + '/'), 'entrypoint_outside_module:' + member)
            data[source] = read(root, source)
    return data, by_member


def validate_metadata(root: Path, metadata: Any, entries: dict[str, dict]) -> dict:
    require(isinstance(metadata, dict) and type(metadata.get('version')) is int and metadata['version'] == 1, 'metadata_schema_mismatch')
    require(metadata.get('workspace_root') == str(root), 'metadata_workspace_root_mismatch')
    member_ids, packages = metadata.get('workspace_members'), metadata.get('packages')
    require(isinstance(member_ids, list) and bool(member_ids), 'metadata_members_missing')
    require(all(isinstance(x, str) for x in member_ids) and len(set(member_ids)) == len(member_ids),
            'metadata_duplicate_member_id')
    require(isinstance(packages, list), 'metadata_packages_missing')
    by_id = {}
    for package in packages:
        require(isinstance(package, dict) and isinstance(package.get('id'), str), 'invalid_metadata_package')
        require(package['id'] not in by_id, 'metadata_duplicate_package_id')
        by_id[package['id']] = package
    observed, target_count = {}, 0
    target_kinds = {'bin', 'lib', 'rlib', 'dylib', 'cdylib', 'staticlib', 'proc-macro',
                    'test', 'example', 'bench', 'custom-build'}
    for package_id in member_ids:
        require(package_id in by_id, 'metadata_member_package_missing')
        package = by_id[package_id]
        require(package.get('source') is None, 'workspace_member_not_local')
        path = local_path(root, package.get('manifest_path'), absolute=True)
        require(path.name == 'Cargo.toml' and path.is_file(), 'metadata_manifest_missing')
        member = path.parent.relative_to(root).as_posix()
        require(member not in observed, 'metadata_duplicate_member_path')
        require(member in entries, 'cargo_member_not_catalogued:' + member)
        require(package.get('name') == entries[member]['package'], 'cargo_package_name_mismatch:' + member)
        observed[member] = package['name']
        targets = package.get('targets')
        require(isinstance(targets, list) and bool(targets), 'cargo_targets_missing:' + member)
        seen_targets = set()
        for target in targets:
            require(isinstance(target, dict) and isinstance(target.get('name'), str), 'invalid_cargo_target')
            kinds = target.get('kind')
            require(isinstance(kinds, list) and bool(kinds) and
                    all(isinstance(k, str) and k in target_kinds for k in kinds), 'unsupported_cargo_target_kind')
            source = local_path(root, target.get('src_path'), absolute=True)
            relative = source.relative_to(root).as_posix()
            identity = (target['name'], tuple(kinds), relative)
            require(identity not in seen_targets, 'duplicate_cargo_target')
            seen_targets.add(identity)
            require(relative.startswith(member + '/') and source.is_file(), 'cargo_target_source_missing:' + member)
            require(relative in entries[member]['source_entrypoints'], 'cargo_target_not_documented:' + relative)
            target_count += 1
    require(set(observed) == set(entries), 'catalog_member_not_in_cargo')
    return {'member_count': len(observed), 'target_count': target_count,
            'members': dict(sorted(observed.items()))}



def target_layout(root: Path, entries: dict[str, dict]) -> dict[str, Any]:
    """Snapshot conventional target discovery locations, not Cargo semantics.

    Cargo remains the authority for which candidates are targets. Capturing
    directory existence/names/types additionally detects a target introduced
    after metadata was emitted, even if that path was never in the catalog.
    This is a before/after check in a private checkout, not atomic filesystem
    isolation against an adversary who can restore intermediate changes.
    """
    facts: dict[str, Any] = {}
    count = 0

    def scan(relative: str) -> list[tuple[str, str]]:
        nonlocal count
        if relative in facts:
            return facts[relative] or []
        path = local_path(root, relative)
        try:
            mode = path.lstat().st_mode
        except FileNotFoundError:
            facts[relative] = None
            return []
        require(stat.S_ISDIR(mode), 'target_layout_not_directory:' + relative)
        values = []
        with os.scandir(path) as iterator:
            for item in iterator:
                count += 1
                require(count <= MAX_TARGET_LAYOUT_ENTRIES, 'target_layout_budget_exceeded')
                local_path(root, relative + '/' + item.name)
                mode = item.stat(follow_symlinks=False).st_mode
                require(stat.S_ISREG(mode) or stat.S_ISDIR(mode), 'target_layout_nonregular:' + relative)
                values.append((item.name, 'directory' if stat.S_ISDIR(mode) else 'file'))
        facts[relative] = sorted(values)
        return values

    for member in sorted(entries):
        scan(member)  # Includes default build.rs discovery and root existence.
        scan(member + '/src')  # Includes conventional lib.rs/main.rs.
        for relative in ('src/bin', 'tests', 'examples', 'benches'):
            directory = member + '/' + relative
            for name, kind in scan(directory):
                if kind == 'directory':
                    scan(directory + '/' + name)  # Includes nested main.rs.
    return facts


def execute(root: Path, environment: dict[str, str]) -> dict:
    root = root.absolute()
    require(not root.is_symlink(), 'linked_workspace_root')
    cargo = shutil.which('cargo', path=environment.get('PATH'))
    require(cargo is not None, 'cargo_required_not_executed')
    before, entries = snapshot(root)
    layout_before = target_layout(root, entries)
    try:
        process = subprocess.run([cargo, 'metadata', '--locked', '--no-deps', '--format-version', '1',
                                  '--manifest-path', str(root / 'Cargo.toml')],
                                 cwd=root, env=environment, capture_output=True, timeout=120, check=False)
    except (OSError, subprocess.TimeoutExpired):
        raise ContractError('cargo_metadata_could_not_complete') from None
    # A trusted Cargo/toolchain is required. Raw stderr can contain environment
    # details; do not republish it as a public artifact. This is not a process sandbox.
    require(process.returncode == 0, 'cargo_metadata_nonzero_exit')
    require(len(process.stdout) <= MAX_METADATA_BYTES, 'cargo_metadata_too_large')
    result = validate_metadata(root, unique_json(process.stdout), entries)
    after, _ = snapshot(root)
    require(layout_before == target_layout(root, entries), 'workspace_target_layout_changed')
    require(before == after, 'workspace_sources_changed_during_metadata')
    result['target_layout_sha256'] = hashlib.sha256(
        json.dumps(layout_before, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    result['input_sha256'] = {p: hashlib.sha256(b).hexdigest() for p, b in sorted(before.items())}
    return result


def main() -> int:
    result = {'schema': SCHEMA, 'status': 'failed', 'cargo_metadata_succeeded': False,
              'compilation_proven': False, 'production_authorization': 'not_granted'}
    try:
        require(len(sys.argv) == 1, 'unexpected_arguments')
        result.update(execute(ROOT, dict(os.environ)))
        result.update(status='ok', cargo_metadata_succeeded=True)
    except (ContractError, OSError, ValueError, TypeError, KeyError) as error:
        result['error'] = str(error) if isinstance(error, ContractError) else 'invalid_workspace_input'
    print(json.dumps(result, sort_keys=True, indent=2))
    return 0 if result['status'] == 'ok' else 1


if __name__ == '__main__':
    raise SystemExit(main())

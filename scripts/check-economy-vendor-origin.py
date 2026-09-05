#!/usr/bin/env python3
"""Validate the economy vendor import record without inventing a missing origin.

Default mode fails until an immutable external repository/commit/tree is recorded.
`--contract-only` validates that an unresolved record is honest and bound to the
current CEX import/tree. Neither mode grants production authorization.
"""
from __future__ import annotations
import argparse, json, subprocess, sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
RECORD = ROOT / 'vendor/trnm-economy-vendor-origin-v1.json'
SCHEMA = 'cex.trnm-economy-vendor-origin-check.v1'
EXPECTED_IMPORT = 'ce681c60d7e09a25f468815a6e7ae6081e8e1a12'
EXPECTED_PREVIOUS = '../Trillionnium/trillionnium/crates/trnm-economy-protocol'


def unique_object(pairs):
    out = {}
    for key, value in pairs:
        if key in out:
            raise ValueError('duplicate_json_field')
        out[key] = value
    return out


def git(*args: str) -> str:
    process = subprocess.run(['git', '-C', str(ROOT), *args], stdin=subprocess.DEVNULL,
                             capture_output=True, text=True, timeout=10, check=False)
    if process.returncode != 0:
        raise ValueError('git_identity_unavailable')
    return process.stdout.strip()


def validate(record: dict[str, Any], *, require_resolved: bool, identities: dict[str, str]) -> list[str]:
    problems: list[str] = []
    if record.get('schema') != 'cex.trnm-economy-vendor-origin.v1': problems.append('record_schema')
    if record.get('production_authorization') != 'not_granted': problems.append('authority_overclaim')
    if record.get('package') != 'trnm-economy-protocol' or record.get('package_version') != '2.4.0':
        problems.append('package_identity')
    if record.get('current_path') != 'vendor/trnm-economy-protocol': problems.append('current_path')
    if record.get('imported_by_cex_commit') != EXPECTED_IMPORT: problems.append('import_commit')
    if record.get('previous_external_path') != EXPECTED_PREVIOUS: problems.append('previous_external_path')
    if record.get('current_git_tree') != identities.get('tree'): problems.append('current_tree_drift')
    files = record.get('current_files')
    if not isinstance(files, dict) or set(files) != {'Cargo.toml', 'src/lib.rs'}:
        problems.append('current_file_set')
    else:
        if files.get('Cargo.toml') != identities.get('Cargo.toml'): problems.append('cargo_blob_drift')
        if files.get('src/lib.rs') != identities.get('src/lib.rs'): problems.append('lib_blob_drift')
    external = record.get('external_origin')
    if not isinstance(external, dict):
        problems.append('external_origin_shape')
    else:
        status = external.get('status')
        values = [external.get('repository'), external.get('commit'), external.get('git_tree')]
        if status == 'unresolved':
            if any(value is not None for value in values): problems.append('partial_unresolved_origin')
            if require_resolved: problems.append('external_origin_unresolved')
        elif status == 'resolved':
            repository, commit, tree = values
            if not isinstance(repository, str) or not repository.strip(): problems.append('external_repository_missing')
            for label, value in [('commit', commit), ('git_tree', tree)]:
                if not isinstance(value, str) or len(value) != 40 or any(c not in '0123456789abcdef' for c in value):
                    problems.append('external_' + label + '_invalid')
        else:
            problems.append('external_origin_status')
    requirement = record.get('resolution_requirement')
    if not isinstance(requirement, str) or not requirement.strip(): problems.append('resolution_requirement')
    return problems


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--contract-only', action='store_true')
    args = parser.parse_args()
    result = {'schema': SCHEMA, 'status': 'failed', 'provenance_resolved': False,
              'checker_may_grant_production_authorization': False,
              'production_authorization': 'not_granted', 'problems': []}
    try:
        record = json.loads(RECORD.read_text('utf-8'), object_pairs_hook=unique_object)
        identities = {
            'tree': git('rev-parse', 'HEAD:vendor/trnm-economy-protocol'),
            'Cargo.toml': git('rev-parse', 'HEAD:vendor/trnm-economy-protocol/Cargo.toml'),
            'src/lib.rs': git('rev-parse', 'HEAD:vendor/trnm-economy-protocol/src/lib.rs'),
        }
        result['problems'] = validate(record, require_resolved=not args.contract_only, identities=identities)
        external = record.get('external_origin', {}) if isinstance(record, dict) else {}
        result['provenance_resolved'] = external.get('status') == 'resolved' and not result['problems']
        result['status'] = 'ok' if not result['problems'] else 'failed'
    except (OSError, ValueError, TypeError, json.JSONDecodeError, subprocess.TimeoutExpired):
        result['problems'] = ['origin_record_or_git_identity_invalid']
    print(json.dumps(result, sort_keys=True, indent=2))
    return 0 if result['status'] == 'ok' else 1


if __name__ == '__main__':
    raise SystemExit(main())

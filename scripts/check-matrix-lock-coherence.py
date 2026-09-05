#!/usr/bin/env python3
"""Check Matrix direct-dependency lock edges without changing Cargo.lock.

This preflight detects stale workspace package entries. It does NOT replace
Cargo resolution, checksums, feature unification, compilation or locked tests.
"""
from __future__ import annotations

import json
from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PACKAGES = ('matrix-bot-poller', 'matrix-bot-relay')


def direct_dependencies(manifest: dict) -> set[str]:
    result = set()
    sections = [manifest]
    targets = manifest.get('target', {})
    if not isinstance(targets, dict):
        raise ValueError('invalid target dependency declarations')
    sections.extend(targets.values())
    for section in sections:
        if not isinstance(section, dict):
            raise ValueError('invalid dependency section')
        for kind in ('dependencies', 'dev-dependencies', 'build-dependencies'):
            dependencies = section.get(kind, {})
            if not isinstance(dependencies, dict):
                raise ValueError('invalid dependency table')
            for name, declaration in dependencies.items():
                if isinstance(declaration, dict):
                    name = declaration.get('package', name)
                elif not isinstance(declaration, str):
                    raise ValueError('invalid dependency declaration')
                if not isinstance(name, str) or not name:
                    raise ValueError('invalid dependency name')
                result.add(name)
    return result


def check(lock_text: str, manifests: dict[str, str]) -> list[dict]:
    lock = tomllib.loads(lock_text)
    rows = lock.get('package')
    if not isinstance(rows, list) or not rows:
        raise ValueError('Cargo.lock has no package entries')
    if set(manifests) != set(PACKAGES):
        raise ValueError('both exact Matrix manifests are required')
    findings = []
    for name in PACKAGES:
        manifest = tomllib.loads(manifests[name])
        if manifest.get('package', {}).get('name') != name:
            raise ValueError('Matrix manifest package identity mismatch')
        candidates = [row for row in rows if isinstance(row, dict)
                      and row.get('name') == name and 'source' not in row]
        if len(candidates) != 1:
            raise ValueError('Matrix workspace package must appear exactly once')
        dependencies = candidates[0].get('dependencies', [])
        if not isinstance(dependencies, list) or not all(isinstance(d, str) and d.strip() for d in dependencies):
            raise ValueError('invalid Cargo.lock dependency list')
        if len(dependencies) != len(set(dependencies)):
            raise ValueError('duplicate Cargo.lock dependency reference')
        actual = {dependency.split()[0] for dependency in dependencies}
        expected = direct_dependencies(manifest)
        if actual != expected:
            findings.append({'package': name, 'missing': sorted(expected - actual),
                             'unexpected': sorted(actual - expected)})
    return findings


def main() -> int:
    try:
        findings = check((ROOT / 'Cargo.lock').read_text(encoding='utf-8'), {
            name: (ROOT / 'apps' / name / 'Cargo.toml').read_text(encoding='utf-8')
            for name in PACKAGES
        })
    except (OSError, ValueError):
        print('Matrix lock preflight failed: complete readable lock/manifests required', file=sys.stderr)
        return 1
    print(json.dumps({'schema': 'cex.matrix-direct-lock-edges.v1',
                      'status': 'failed' if findings else 'ok', 'findings': findings,
                      'cargo_resolution_proven': False, 'production_authorization': 'not_granted'}, indent=2))
    return bool(findings)


if __name__ == '__main__':
    raise SystemExit(main())

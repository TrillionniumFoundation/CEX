#!/usr/bin/env python3
"""Read-only exact-source Cargo metadata for root and production Docker workspaces."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import runpy
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
RUSTLS_CHECKSUM = '0d41d731c7d2f962d1ccc364cec258de3c0e93b38c2fb3ba97ac74513048d634'
DOCKER = {
    'hepta': 'services/hepta-research-league/docker',
    'paper': 'services/paper-raid-bff/docker',
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def command(*args: str, cwd: Path) -> bytes:
    return subprocess.check_output(args, cwd=cwd, timeout=900)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_source(root: Path, sha: str, tree: str) -> None:
    require(bool(re.fullmatch('[0-9a-f]{40}', sha)), 'invalid source SHA')
    require(bool(re.fullmatch('[0-9a-f]{40}', tree)), 'invalid source tree')
    require(command('git', 'rev-parse', 'HEAD', cwd=root).decode().strip() == sha, 'source SHA mismatch')
    require(command('git', 'rev-parse', 'HEAD^{tree}', cwd=root).decode().strip() == tree, 'source tree mismatch')
    command('git', 'diff', '--exit-code', 'HEAD', '--', cwd=root)


def safe_relative(name: str) -> bool:
    path = PurePosixPath(name)
    return bool(name) and not path.is_absolute() and all(part not in {'.', '..'} for part in name.split('/'))


def snapshot(root: Path, sha: str, destination: Path) -> None:
    archive = command('git', 'archive', '--format=tar', sha, cwd=root)
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        for entry in source:
            name = entry.name.rstrip('/')
            require(safe_relative(name), 'unsafe archive path')
            require(entry.isdir() or entry.isfile(), 'source archive contains a link or special file')
            target = destination / name
            if entry.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                stream = source.extractfile(entry)
                require(stream is not None, 'archive file has no bytes')
                target.write_bytes(stream.read())
                target.chmod(entry.mode & 0o777)


def members(workspace: Path) -> list[str]:
    value = tomllib.loads((workspace / 'Cargo.toml').read_text())['workspace']['members']
    require(isinstance(value, list) and bool(value) and len(value) == len(set(value)), 'invalid workspace members')
    for name in value:
        require(isinstance(name, str) and safe_relative(name) and not any(c in name for c in '*?['), 'unsafe workspace member')
        require((workspace / name / 'Cargo.toml').is_file(), 'workspace member manifest missing')
    return value


def project(source: Path, docker: str, destination: Path) -> None:
    require(safe_relative(docker), 'unsafe Docker manifest path')
    destination.mkdir()
    shutil.copy2(source / docker / 'workspace.Cargo.toml', destination / 'Cargo.toml')
    shutil.copy2(source / docker / 'Cargo.lock', destination / 'Cargo.lock')
    declared = tomllib.loads((destination / 'Cargo.toml').read_text())['workspace']['members']
    require(isinstance(declared, list) and bool(declared), 'Docker workspace members missing')
    for name in declared:
        require(isinstance(name, str) and safe_relative(name) and not any(c in name for c in '*?['), 'unsafe Docker member')
        require((source / name / 'Cargo.toml').is_file(), 'Docker member manifest missing')
        shutil.copytree(source / name, destination / name)
    members(destination)


def validate_graph(data: dict, workspace: Path) -> dict:
    require(isinstance(data.get('resolve'), dict), 'full dependency resolve graph is required')
    packages = {item['id']: item for item in data['packages']}
    nodes = {item['id']: item for item in data['resolve']['nodes']}
    require(bool(nodes), 'dependency graph is empty')
    observed_members = set()
    for member in data['workspace_members']:
        manifest = Path(packages[member]['manifest_path']).resolve()
        observed_members.add(manifest.parent.relative_to(workspace.resolve()).as_posix())
    require(observed_members == set(members(workspace)), 'metadata workspace membership differs from projection')
    for package in packages.values():
        if package.get('source') is None:
            require(Path(package['manifest_path']).resolve().is_relative_to(workspace.resolve()), 'external path dependency')
    lock = tomllib.loads((workspace / 'Cargo.lock').read_text())['package']
    selected = [item for item in lock if item['name'] == 'rustls']
    require(len(selected) == 1 and selected[0]['version'] == '0.23.45' and selected[0]['checksum'] == RUSTLS_CHECKSUM, 'unexpected rustls lock identity')
    rustls = [item for item in packages.values() if item['name'] == 'rustls']
    require(len(rustls) == 1 and rustls[0]['version'] == '0.23.45', 'unexpected rustls graph identity')
    identity = rustls[0]['id']
    require(identity in nodes, 'rustls is absent from resolved graph')
    features = nodes[identity]['features']
    require('ring' in features and not any('aws' in item for item in features), 'TLS provider feature drift')
    webpki_lock = [item for item in lock if item['name'] == 'rustls-webpki']
    webpki_graph = [item for item in packages.values() if item['name'] == 'rustls-webpki']
    require(len(webpki_lock) == len(webpki_graph) == 1 and webpki_lock[0]['version'] == webpki_graph[0]['version'], 'webpki graph/lock drift')
    aws_active = any(package['name'].startswith('aws-lc') for key, package in packages.items() if key in nodes)
    require(not aws_active, 'unexpected active aws-lc provider')
    return {
        'rustls_webpki_version': webpki_graph[0]['version'],
        'rustls_version': rustls[0]['version'],
        'rustls_features': features,
        'rustls_direct_dependents': sorted(packages[key]['name'] for key, node in nodes.items() if identity in node['dependencies']),
        'workspace_members': sorted(observed_members),
        'resolved_nodes': len(nodes),
        'aws_lc_active': aws_active,
    }


def qualify(workspace: Path, output: Path, label: str, observe) -> dict:
    lock = workspace / 'Cargo.lock'
    before = digest(lock)
    manifest_hash = digest(workspace / 'Cargo.toml')
    original = Path.cwd()
    try:
        os.chdir(workspace)
        identity = observe()
    finally:
        os.chdir(original)
    graphs = {}
    for mode, extra in [('default', []), ('all-features', ['--all-features'])]:
        raw = command('cargo', 'metadata', '--locked', '--format-version', '1', *extra, cwd=workspace)
        require(digest(lock) == before, 'Cargo.lock changed during metadata')
        data = json.loads(raw)
        graphs[mode] = validate_graph(data, workspace)
        (output / f'{label}-{mode}.json').write_bytes(raw)
    return {'compiler': identity, 'manifest_sha256': manifest_hash, 'lock_sha256': before, 'graphs': graphs}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--source-tree', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    verify_source(ROOT, args.source_sha, args.source_tree)
    policy = json.loads((ROOT / 'docs/security/rust-toolchain-surfaces-v1.json').read_text())
    require(os.environ.get('RUST_TOOLCHAIN') == policy['expected_rust_toolchain'], 'compiler policy version mismatch')
    require(os.environ.get('RUST_RELEASE_COMMIT') == policy['expected_rust_release_commit'], 'compiler policy commit mismatch')
    observe = runpy.run_path(str(ROOT / 'scripts/record-executed-rust-toolchain.py'))['observe']
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    result = {'source_sha': args.source_sha, 'source_tree': args.source_tree, 'graphs': {}, 'production_authorization': 'not_granted'}
    with tempfile.TemporaryDirectory(prefix='cex-locked-graphs-', dir=os.environ.get('RUNNER_TEMP')) as temporary:
        scratch = Path(temporary)
        source = scratch / 'root'
        source.mkdir()
        snapshot(ROOT, args.source_sha, source)
        result['graphs']['root'] = qualify(source, output, 'root', observe)
        for label, docker in DOCKER.items():
            workspace = scratch / label
            project(source, docker, workspace)
            result['graphs'][label] = qualify(workspace, output, label, observe)
    verify_source(ROOT, args.source_sha, args.source_tree)
    (output / 'locked-build-graphs.json').write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')


if __name__ == '__main__':
    main()

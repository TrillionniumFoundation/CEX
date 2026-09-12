#!/usr/bin/env python3
"""Export an exact tracked-source handoff, never runtime or release approval.

Only Git blobs from HEAD enter the archive. No working-tree credentials, ignored
files, Git history, environment values, or CI logs are collected. The export is a
source input for independent handoff/reproduction, not successful test evidence.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import tarfile
import tempfile
import tomllib
import unittest

MAX_FILES = 20000
MAX_FILE = 32 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
ROOT = Path(__file__).resolve().parents[1]


class ExportError(RuntimeError):
    pass


def require(value: bool, reason: str) -> None:
    if not value:
        raise ExportError(reason)


def git(root: Path, *args: str) -> bytes:
    result = subprocess.run(['git', *args], cwd=root, stdin=subprocess.DEVNULL,
                            capture_output=True, timeout=60, check=False)
    require(result.returncode == 0, 'git_read_failed')
    require(len(result.stdout) <= MAX_TOTAL, 'git_output_too_large')
    return result.stdout


def unique(pairs: list) -> dict:
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate_json_key')
        result[key] = value
    return result


def canonical(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2,
                       allow_nan=False) + '\n').encode('utf-8')


def safe_name(name: str) -> None:
    path = PurePosixPath(name)
    require(bool(name) and not path.is_absolute() and '..' not in path.parts
            and '.' not in path.parts and '\\' not in name
            and path.as_posix() == name and path.parts[0] != '.git'
            and not any(ord(c) < 32 or ord(c) == 127 for c in name), 'unsafe_git_path')


def snapshot(root: Path) -> tuple[str, str, list[dict], dict[str, bytes]]:
    source = git(root, 'rev-parse', 'HEAD').decode().strip()
    tree = git(root, 'rev-parse', 'HEAD^{tree}').decode().strip()
    require(len(source) == 40 and len(tree) == 40, 'unsupported_object_identity')
    require(not git(root, 'status', '--porcelain=v1', '--untracked-files=no'), 'dirty_source')
    raw = git(root, 'ls-tree', '-rz', '--full-tree', 'HEAD')
    rows = raw.rstrip(b'\0').split(b'\0') if raw else []
    require(0 < len(rows) <= MAX_FILES, 'source_count_out_of_range')
    entries, blobs, total = [], {}, 0
    for row in rows:
        header, path_bytes = row.split(b'\t', 1)
        mode, kind, object_id = header.decode('ascii').split()
        name = path_bytes.decode('utf-8')
        safe_name(name)
        require(mode in ('100644', '100755') and kind == 'blob', 'nonregular_tracked_source')
        # Query size before reading the object, including for duplicated blobs.
        size = int(git(root, 'cat-file', '-s', object_id).strip())
        total += size
        require(0 <= size <= MAX_FILE and total <= MAX_TOTAL, 'source_bytes_out_of_range')
        data = git(root, 'cat-file', 'blob', object_id)
        require(len(data) == size, 'git_blob_size_changed')
        expected = hashlib.sha1(b'blob ' + str(size).encode() + b'\0' + data).hexdigest()
        require(expected == object_id, 'git_blob_identity_mismatch')
        require(name not in blobs, 'duplicate_source_path')
        blobs[name] = data
        entries.append({'path': name, 'mode': mode, 'git_blob': object_id,
                        'bytes': size, 'sha256': hashlib.sha256(data).hexdigest()})
    require(git(root, 'rev-parse', 'HEAD').decode().strip() == source
            and not git(root, 'status', '--porcelain=v1', '--untracked-files=no'), 'source_changed')
    return source, tree, entries, blobs


def module_index(blobs: dict[str, bytes]) -> list[dict]:
    cargo = tomllib.loads(blobs['Cargo.toml'].decode('utf-8'))
    catalog = json.loads(blobs['docs/module-catalog-v1.json'], object_pairs_hook=unique)
    members = cargo['workspace']['members']
    require(isinstance(members, list) and members and len(members) == len(set(members)), 'invalid_members')
    require(all(isinstance(x, str) and not any(c in x for c in '*?[') for x in members), 'unsupported_member_glob')
    listed = catalog['modules']
    require(len(listed) == len(members) and {m['workspace_member'] for m in listed} == set(members), 'catalog_member_mismatch')
    index = []
    packages = set()
    for item in listed:
        member, document = item['workspace_member'], item['documentation']
        safe_name(member); safe_name(document)
        manifest = tomllib.loads(blobs[member + '/Cargo.toml'].decode('utf-8'))
        package = manifest['package']['name']
        require(package == item['package'] and package not in packages, 'package_identity_mismatch')
        require(document in blobs and bool(blobs[document]), 'module_document_missing')
        require(bool(item['owner']) and bool(item['source_entrypoints']), 'module_owner_or_entrypoints_missing')
        for point in item['source_entrypoints']:
            safe_name(point)
            require(point in blobs, 'module_source_missing')
        packages.add(package)
        owned = [p for p in sorted(blobs) if p.startswith(member + '/')]
        index.append({'package': package, 'member': member, 'owner_role': item['owner'],
                      'documentation': document, 'source_entrypoints': item['source_entrypoints'],
                      'tracked_files': owned,
                      'sql_files': [p for p in owned if p.endswith('.sql')],
                      'verification': item['verification'], 'independent_handoff': 'not_claimed'})
    return index


def archive(entries: list[dict], blobs: dict[str, bytes]) -> bytes:
    stream = io.BytesIO()
    with gzip.GzipFile(fileobj=stream, mode='wb', mtime=0, filename='') as zipped:
        with tarfile.open(fileobj=zipped, mode='w|', format=tarfile.PAX_FORMAT) as out:
            for entry in sorted(entries, key=lambda e: e['path']):
                info = tarfile.TarInfo(entry['path'])
                info.size = entry['bytes']
                info.mode = 0o755 if entry['mode'] == '100755' else 0o644
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ''
                out.addfile(info, io.BytesIO(blobs[entry['path']]))
    return stream.getvalue()


def export(root: Path, output: Path) -> dict:
    require(root.resolve() == root, 'noncanonical_repository_root')
    require(output.is_absolute() and not output.exists() and not output.is_symlink(), 'new_absolute_output_required')
    require(output.parent.is_dir() and output.parent.resolve() == output.parent, 'unsafe_output_parent')
    source, tree, entries, blobs = snapshot(root)
    modules = module_index(blobs)
    tar = archive(entries, blobs)
    report = {'schema': 'cex.development-source-input.v1', 'status': 'exported',
              'source_sha': source, 'source_tree': tree, 'module_count': len(modules),
              'modules': modules, 'files': entries, 'file_count': len(entries),
              'archive_sha256': hashlib.sha256(tar).hexdigest(),
              'production_authorization': 'not_granted',
              'repository_qualification': 'not_claimed', 'runtime_tests_executed_by_exporter': False,
              'scope': 'tracked-source-input-only; no history, credentials or execution evidence'}
    stage = Path(tempfile.mkdtemp(prefix='.cex-source-input-', dir=output.parent))
    try:
        (stage / 'source.tar.gz').write_bytes(tar)
        (stage / 'manifest.json').write_bytes(canonical(report))
        lines = ['# Exact-source developer handoff input', '',
                 'This archive is source input, not proof of successful execution or independent acceptance.',
                 f'Source: `{source}`; tree: `{tree}`.',
                 'Production authorization: `not_granted`.', '',
                 'The archive contains HEAD tracked blobs only. Git history, runtime secrets, ignored files and CI logs are excluded.',
                 'Use the file manifest to verify every extracted byte and executable mode before reproduction.', '',
                 '| Package | Owner role | Module contract | Tracked files |', '|---|---|---|---:|']
        for module in modules:
            lines.append(f"| `{module['package']}` | `{module['owner_role']}` | `{module['documentation']}` | {len(module['tracked_files'])} |")
        lines.extend(['', 'Actual Cargo metadata and all required tests remain separate checks.',
                      'A source file list is not a field-level API specification or an independent handoff signoff.', ''])
        (stage / 'README.md').write_text('\n'.join(lines), encoding='utf-8')
        require(git(root, 'rev-parse', 'HEAD').decode().strip() == source
                and not git(root, 'status', '--porcelain=v1', '--untracked-files=no'), 'source_changed')
        require(not output.exists(), 'output_appeared_during_export')
        stage.rename(output)
    finally:
        if stage.exists():
            shutil.rmtree(stage)
    return {k: report[k] for k in ('schema', 'status', 'source_sha', 'source_tree', 'module_count',
                                  'file_count', 'archive_sha256', 'repository_qualification', 'production_authorization')}


class Tests(unittest.TestCase):
    def workspace(self, root: Path) -> None:
        files = {'Cargo.toml': '[workspace]\nmembers=["crates/example"]\n',
                 'crates/example/Cargo.toml': '[package]\nname="example"\nversion="0.1.0"\n',
                 'crates/example/src/lib.rs': 'pub fn example() {}\n',
                 'docs/modules/example.md': '# Example\n',
                 'docs/module-catalog-v1.json': json.dumps({'modules': [{
                     'workspace_member': 'crates/example', 'package': 'example', 'owner': 'test-owner',
                     'documentation': 'docs/modules/example.md',
                     'source_entrypoints': ['crates/example/src/lib.rs'], 'verification': ['test-only command']} ]})}
        for name, content in files.items():
            path = root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(content)
        git(root, 'init', '-q')
        git(root, 'add', '.')
        git(root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'synthetic source fixture')

    def test_complete_export_and_excludes_untracked(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d).resolve(); self.workspace(root)
            (root / 'runtime-secret.env').write_text('must-not-appear')
            result = export(root, root / 'output')
            manifest = json.loads((root / 'output/manifest.json').read_bytes())
            self.assertEqual(result['module_count'], 1)
            self.assertNotIn('runtime-secret.env', [e['path'] for e in manifest['files']])
            with tarfile.open(root / 'output/source.tar.gz') as tar:
                self.assertEqual(set(tar.getnames()), {e['path'] for e in manifest['files']})
                for entry in manifest['files']:
                    data = tar.extractfile(entry['path']).read()
                    self.assertEqual(hashlib.sha256(data).hexdigest(), entry['sha256'])
            export(root, root / 'second')
            self.assertEqual((root/'output/source.tar.gz').read_bytes(), (root/'second/source.tar.gz').read_bytes())

    def test_dirty_missing_and_existing_output_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d).resolve(); self.workspace(root)
            with self.assertRaises(ExportError): export(root, root)
            (root / 'crates/example/src/lib.rs').unlink()
            with self.assertRaises(ExportError): export(root, root / 'output')
            self.assertFalse((root / 'output').exists())

    def test_path_rejections(self):
        for value in ('../x', '/tmp/x', '.git/config', 'a\\b', 'a/../b', 'a\nb', 'a//b'):
            with self.subTest(value=value), self.assertRaises(ExportError): safe_name(value)

    def test_duplicate_json_rejected(self):
        with self.assertRaises(ExportError): json.loads('{"a":1,"a":2}', object_pairs_hook=unique)

    def test_catalog_mismatch_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d).resolve(); self.workspace(root)
            _, _, _, blobs = snapshot(root)
            value = json.loads(blobs['docs/module-catalog-v1.json'])
            value['modules'][0]['package'] = 'wrong'
            blobs['docs/module-catalog-v1.json'] = canonical(value)
            with self.assertRaises(ExportError): module_index(blobs)

    @unittest.skipUnless(os.name == 'posix', 'Git symlink regression requires POSIX')
    def test_tracked_symlink_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d).resolve(); self.workspace(root)
            (root/'link').symlink_to('/etc/passwd'); git(root, 'add', 'link')
            git(root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'synthetic symlink')
            with self.assertRaises(ExportError): snapshot(root)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests))
        return 0 if result.wasSuccessful() else 1
    try:
        require(args.output is not None, 'output_required')
        result = export(ROOT.resolve(), args.output.absolute())
    except Exception as error:
        result = {'schema': 'cex.development-source-input.v1', 'status': 'failed',
                  'problem': str(error) if isinstance(error, ExportError) else 'source_export_unavailable',
                  'production_authorization': 'not_granted'}
    print(json.dumps(result, sort_keys=True))
    return 0 if result['status'] == 'exported' else 1


if __name__ == '__main__':
    raise SystemExit(main())

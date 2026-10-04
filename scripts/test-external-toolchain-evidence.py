#!/usr/bin/env python3
"""Synthetic fail-closed regressions; never compiler or hosted qualification proof."""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('identity', ROOT / 'scripts/record-executed-rust-toolchain.py')
assert SPEC and SPEC.loader
IDENTITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IDENTITY)
VERSION = '1.99.0'
COMMIT = 'b940084d7eb6a299eb4bfeb8e34901bc051e7ac4'
WORKFLOW = (ROOT / '.github/workflows/world-settlement-external-evidence.yml').read_text()


def run_block(name: str) -> str:
    step = WORKFLOW.split('      - name: ' + name + '\n', 1)[1].split('      - name:', 1)[0]
    return textwrap.dedent(step.split('        run: |\n', 1)[1])


class CompilerIdentityTests(unittest.TestCase):
    def nested_probe(self, override: str | None, reported: str = VERSION, commit: str = COMMIT):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            nested = root / 'world' / 'trillionnium'
            nested.mkdir(parents=True)
            (nested / 'rust-toolchain.toml').write_text('[toolchain]\nchannel = "1.98.1"\n')
            binaries = root / 'bin'
            binaries.mkdir()
            # Simulate rustup precedence: without the explicit override the nested
            # pinned toolchain wins over the host default. No Rust is executed.
            rustc = binaries / 'rustc'
            rustc.write_text('#!/bin/sh\n' + f'version=${{RUSTUP_TOOLCHAIN:+{reported}}}\n' +
                            'version=${version:-1.98.1}\n' +
                            f'printf "rustc %s (fixture)\\nrelease: %s\\ncommit-hash: {commit}\\n" "$version" "$version"\n')
            (binaries / 'rustup').write_text('#!/bin/sh\nprintf "%s-x86_64-unknown-linux-gnu (fixture)\\n" "${RUSTUP_TOOLCHAIN:-1.98.1}"\n')
            (binaries / 'cargo').write_text('#!/bin/sh\nprintf "cargo 1.99.0 (fixture)\\n"\n')
            for binary in binaries.iterdir():
                binary.chmod(0o755)
            env = {**os.environ, 'PATH': str(binaries) + os.pathsep + os.environ['PATH'],
                   'RUST_TOOLCHAIN': VERSION, 'RUST_RELEASE_COMMIT': COMMIT}
            env.pop('RUSTUP_TOOLCHAIN', None)
            if override is not None:
                env['RUSTUP_TOOLCHAIN'] = override
            output = root / 'identity.json'
            result = subprocess.run([os.sys.executable, str(ROOT / 'scripts/record-executed-rust-toolchain.py'), str(output)],
                                    cwd=nested, env=env, capture_output=True, text=True, timeout=5)
            return result.returncode, json.loads(output.read_text()) if output.exists() else None

    def test_older_nested_pin_without_override_is_rejected(self):
        self.assertEqual(self.nested_probe(None), (1, None))

    def test_explicit_override_is_observed_in_nested_directory(self):
        code, identity = self.nested_probe(VERSION)
        self.assertEqual(code, 0)
        self.assertEqual(identity['rust_toolchain'], VERSION)
        self.assertEqual(identity['rust_release_commit'], COMMIT)

    def test_wrong_executed_release_is_rejected(self):
        self.assertEqual(self.nested_probe(VERSION, reported='1.98.1'), (1, None))

    def test_wrong_full_release_commit_is_rejected(self):
        self.assertEqual(self.nested_probe(VERSION, commit='0' * 40), (1, None))

    def test_workflow_overrides_both_nested_builds_and_records_observations(self):
        self.assertIn("  RUSTUP_TOOLCHAIN: '1.99.0'", WORKFLOW)
        self.assertIn('for directory in cex-owner world/trillionnium; do', WORKFLOW)
        self.assertIn("'rust_toolchain': cex_identity['rust_toolchain']", WORKFLOW)
        self.assertIn("'rust_release_commit': cex_identity['rust_release_commit']", WORKFLOW)
        self.assertNotIn("'rust_toolchain': '1.99.0'", WORKFLOW)


class ImmutableLockTests(unittest.TestCase):
    def run_preflight(self, missing=False, modified=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for repo, path in [('cex-owner', 'Cargo.lock'), ('world', 'trillionnium/Cargo.lock')]:
                target = root / repo
                target.mkdir()
                subprocess.run(['git', 'init', '-q', str(target)], check=True)
                tracked = target / path
                tracked.parent.mkdir(parents=True, exist_ok=True)
                (target / 'README').write_text('synthetic fixture\n')
                if not (missing and repo == 'cex-owner'):
                    tracked.write_text('version = 4\n')
                subprocess.run(['git', '-C', str(target), 'add', '.'], check=True)
                subprocess.run(['git', '-C', str(target), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                                '-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'fixture'], check=True)
                if modified and repo == 'world':
                    tracked.write_text('version = 3\n')
            result = subprocess.run(['bash', '-c', run_block('Require unchanged committed dependency locks')],
                                    cwd=root, capture_output=True, text=True, timeout=5)
            return result.returncode, (root / 'cex-owner/Cargo.lock').exists()

    def test_missing_committed_lock_fails_without_generating_it(self):
        code, exists = self.run_preflight(missing=True)
        self.assertNotEqual(code, 0)
        self.assertFalse(exists)

    def test_modified_lock_is_rejected(self):
        self.assertNotEqual(self.run_preflight(modified=True)[0], 0)

    def test_unchanged_committed_locks_pass(self):
        self.assertEqual(self.run_preflight()[0], 0)

    def test_qualification_does_not_generate_locks_or_format_source(self):
        self.assertNotIn('cargo generate-lockfile', WORKFLOW)
        self.assertNotIn('cargo fmt --all\n', WORKFLOW)
        self.assertIn('cargo fmt --all -- --check', WORKFLOW)
        self.assertIn('git -C cex-owner diff --exit-code HEAD', WORKFLOW)


if __name__ == '__main__':
    unittest.main(verbosity=2)

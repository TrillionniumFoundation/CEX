#!/usr/bin/env python3
"""Synthetic source/projection guards; no Cargo execution or qualification credit."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('graphs', Path(__file__).with_name('check-locked-build-graphs.py'))
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


class GraphTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.workspace = self.root / 'workspace'
        (self.workspace / 'member').mkdir(parents=True)
        (self.workspace / 'Cargo.toml').write_text('[workspace]\nmembers=["member"]\n')
        (self.workspace / 'member/Cargo.toml').write_text('[package]\nname="member"\nversion="0.1.0"\n')
        (self.workspace / 'Cargo.lock').write_text('[[package]]\nname="rustls"\nversion="0.23.45"\nchecksum="' + MOD.RUSTLS_CHECKSUM + '"\n[[package]]\nname="rustls-webpki"\nversion="0.103.14"\n')
        self.data = {
            'workspace_members': ['member'],
            'packages': [
                {'id': 'member', 'name': 'member', 'version': '0.1.0', 'source': None, 'manifest_path': str(self.workspace / 'member/Cargo.toml')},
                {'id': 'rustls', 'name': 'rustls', 'version': '0.23.45', 'source': 'registry'},
                {'id': 'webpki', 'name': 'rustls-webpki', 'version': '0.103.14', 'source': 'registry'},
            ],
            'resolve': {'nodes': [
                {'id': 'member', 'dependencies': ['rustls'], 'features': []},
                {'id': 'rustls', 'dependencies': ['webpki'], 'features': ['ring', 'std']},
                {'id': 'webpki', 'dependencies': [], 'features': ['ring']},
            ]},
        }
        self.output = self.root / 'output'
        self.output.mkdir()

    def test_full_graph_accepts_expected_ring_identity(self):
        result = MOD.validate_graph(self.data, self.workspace)
        self.assertFalse(result['aws_lc_active'])
        self.assertEqual(result['rustls_direct_dependents'], ['member'])

    def test_no_deps_metadata_is_not_a_complete_graph(self):
        self.data['resolve'] = None
        with self.assertRaisesRegex(ValueError, 'full dependency'):
            MOD.validate_graph(self.data, self.workspace)

    def test_wrong_rustls_version_is_rejected(self):
        self.data['packages'][1]['version'] = '0.23.43'
        with self.assertRaisesRegex(ValueError, 'rustls graph'):
            MOD.validate_graph(self.data, self.workspace)

    def test_provider_feature_change_is_rejected(self):
        self.data['resolve']['nodes'][1]['features'].append('aws_lc_rs')
        with self.assertRaisesRegex(ValueError, 'provider feature'):
            MOD.validate_graph(self.data, self.workspace)

    def test_external_package_path_is_rejected(self):
        self.data['packages'].append({'id':'outside','name':'outside','source':None,'manifest_path':'/outside/Cargo.toml'})
        with self.assertRaisesRegex(ValueError, 'external path'):
            MOD.validate_graph(self.data, self.workspace)

    def test_default_and_all_feature_commands_are_locked_and_complete(self):
        with patch.object(MOD, 'command', return_value=json.dumps(self.data).encode()) as run:
            result = MOD.qualify(self.workspace, self.output, 'fixture', lambda: {'rust_toolchain': '1.99.0', 'fixture_only': True})
        self.assertEqual(set(result['graphs']), {'default', 'all-features'})
        self.assertEqual(run.call_count, 2)
        for call in run.call_args_list:
            self.assertIn('--locked', call.args)
            self.assertNotIn('--no-deps', call.args)
        self.assertIn('--all-features', run.call_args_list[1].args)

    def test_changed_lock_is_rejected(self):
        def changed(*args, **kwargs):
            (self.workspace / 'Cargo.lock').write_text('changed')
            return json.dumps(self.data).encode()
        with patch.object(MOD, 'command', side_effect=changed):
            with self.assertRaisesRegex(ValueError, 'Cargo.lock changed'):
                MOD.qualify(self.workspace, self.output, 'fixture', lambda: {})

    def test_incorrect_compiler_prevents_metadata_execution(self):
        def wrong():
            raise ValueError('incorrect compiler fixture')
        with patch.object(MOD, 'command') as run:
            with self.assertRaisesRegex(ValueError, 'incorrect compiler'):
                MOD.qualify(self.workspace, self.output, 'fixture', wrong)
            run.assert_not_called()

    def test_actual_docker_manifest_controls_projection(self):
        docker = self.workspace / 'docker'
        docker.mkdir()
        (docker / 'workspace.Cargo.toml').write_text('[workspace]\nmembers=["member"]\n')
        (docker / 'Cargo.lock').write_bytes((self.workspace / 'Cargo.lock').read_bytes())
        dest = self.root / 'projected'
        MOD.project(self.workspace, 'docker', dest)
        self.assertEqual((dest / 'Cargo.toml').read_bytes(), (docker / 'workspace.Cargo.toml').read_bytes())
        self.assertFalse((dest / 'docker').exists())

    def test_malformed_projection_cannot_escape_source(self):
        docker = self.workspace / 'docker'
        docker.mkdir()
        (docker / 'workspace.Cargo.toml').write_text('[workspace]\nmembers=["../outside"]\n')
        (docker / 'Cargo.lock').write_text('version=4\n')
        with self.assertRaisesRegex(ValueError, 'unsafe Docker member'):
            MOD.project(self.workspace, 'docker', self.root / 'projected')

    def test_source_sha_tree_and_dirty_source_are_rejected(self):
        def git(*args):
            return subprocess.check_output(['git', '-C', str(self.workspace), *args], stderr=subprocess.DEVNULL).decode().strip()
        git('init', '-q')
        git('add', '.')
        git('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','-c','core.hooksPath=/dev/null','commit','-qm','fixture')
        sha, tree = git('rev-parse','HEAD'), git('rev-parse','HEAD^{tree}')
        MOD.verify_source(self.workspace, sha, tree)
        with self.assertRaisesRegex(ValueError, 'source SHA mismatch'):
            MOD.verify_source(self.workspace, '0'*40, tree)
        with self.assertRaisesRegex(ValueError, 'source tree mismatch'):
            MOD.verify_source(self.workspace, sha, '0'*40)
        (self.workspace / 'Cargo.lock').write_text('changed')
        with self.assertRaises(subprocess.CalledProcessError):
            MOD.verify_source(self.workspace, sha, tree)


if __name__ == '__main__':
    unittest.main(verbosity=2)

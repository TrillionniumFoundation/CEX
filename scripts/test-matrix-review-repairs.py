#!/usr/bin/env python3
"""Local source-contract and shell negative tests; not Rust/SQL execution proof."""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('matrix_wiring', ROOT / 'scripts/check-matrix-runtime-wiring.py')
assert SPEC and SPEC.loader
WIRING = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WIRING)


class SourceContractTests(unittest.TestCase):
    def test_ordered_transaction_operations(self):
        WIRING.require_order('async fn work() { begin(); enqueue(); finish(); commit(); }',
                             'work', ['begin()', 'enqueue()', 'finish()', 'commit()'])

    def test_out_of_order_operations_rejected(self):
        with self.assertRaises(AssertionError):
            WIRING.require_order('async fn work() { begin(); commit(); enqueue(); }',
                                 'work', ['begin()', 'enqueue()', 'commit()'])

    def test_another_function_cannot_satisfy_transaction(self):
        with self.assertRaises(AssertionError):
            WIRING.require_order('async fn work() { begin(); }\nfn other() { commit(); }',
                                 'work', ['begin()', 'commit()'])

    def test_missing_function_rejected(self):
        with self.assertRaises(AssertionError):
            WIRING.function_source('// work is planned, not implemented', 'work')

    def test_no_weaker_literal_presence_rules(self):
        with self.assertRaises(AssertionError):
            WIRING.require('', 'missing', ['cex_matrix_advance_cursor_v1'])
        with self.assertRaises(AssertionError):
            WIRING.forbid('File::create()', 'cursor', ['File::create'])

    def test_catalog_command_alignment(self):
        catalog = json.loads((ROOT / 'docs/module-catalog-v1.json').read_text())
        self.assertEqual(len(catalog['modules']), 18)
        for name in ['execution-service', 'matrix-bot-poller', 'matrix-bot-relay']:
            module = next(m for m in catalog['modules'] if m['package'] == name)
            commands = [c for c in module['verification'] if c.startswith('cargo')]
            self.assertEqual(len(commands), 2)
            self.assertTrue(all('--locked' in command for command in commands))

    def test_adapter_catalog_references_new_contracts(self):
        catalog = json.loads((ROOT / 'docs/module-catalog-v1.json').read_text())
        module = next(m for m in catalog['modules'] if m['package'] == 'matrix-entry-adapter')
        text = (ROOT / module['documentation']).read_text()
        for source in module['source_entrypoints']:
            self.assertIn('`' + source + '`', text)
        for command in module['verification']:
            self.assertIn(command, text)

    def test_profile_validation_precedes_runtime(self):
        text = (ROOT / 'services/matrix-entry-adapter/src/main.rs').read_text()
        main = WIRING.function_source(text, 'main')
        self.assertLess(main.index('prepare_runtime_profile()'), main.index('tokio::runtime::Builder'))
        self.assertNotIn('#[tokio::main]', text)
        self.assertNotIn('unsafe', text)

    def test_migration_preserves_inbox_and_distinguishes_observations(self):
        sql = (ROOT / 'services/matrix-entry-adapter/migrations/0002_source_observation_replay.sql').read_text()
        self.assertIn('unique nulls not distinct', sql)
        self.assertIn('existing_hash is distinct from p_source_event_sha256', sql)
        self.assertNotIn('existing_cursor', sql)
        self.assertNotIn('update public.matrix_transport_inbox', sql.lower())
        self.assertNotIn('security definer', sql.lower())
        self.assertTrue(sql.startswith('begin;'))
        self.assertTrue(sql.rstrip().endswith('commit;'))


class ShellNegativeTests(unittest.TestCase):
    def run_wrapper(self, url, consent):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / 'scripts'
            scripts.mkdir()
            bin_dir = root / 'bin'
            bin_dir.mkdir()
            wrapper = scripts / 'check-matrix-source-observation-postgres.sh'
            wrapper.write_text((ROOT / 'scripts' / wrapper.name).read_text())
            marker = root / 'side-effect'
            (bin_dir / 'psql').write_text('#!/bin/sh\ntouch "$TEST_SIDE_EFFECT"\nexit 0\n')
            (bin_dir / 'psql').chmod(0o755)
            (scripts / 'check-matrix-transport-postgres.sh').write_text(
                '#!/bin/sh\ntouch "$TEST_SIDE_EFFECT"\nexit 0\n')
            env = dict(os.environ, MATRIX_TEST_DATABASE_URL=url,
                       MATRIX_TEST_ALLOW_SCHEMA_RESET=consent, TEST_SIDE_EFFECT=str(marker),
                       PATH=str(bin_dir) + os.pathsep + os.environ['PATH'])
            result = subprocess.run(['bash', str(wrapper)], env=env, capture_output=True, text=True, timeout=10)
            return result, marker.exists()

    def test_missing_reset_consent_has_no_database_side_effect(self):
        result, mutated = self.run_wrapper('postgres://user:secret@localhost/testdb', '0')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(mutated)
        self.assertNotIn('secret', result.stderr)

    def test_invalid_url_rejected_before_legacy_script(self):
        result, mutated = self.run_wrapper('https://user:secret@localhost/testdb', '1')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(mutated)
        self.assertNotIn('secret', result.stderr)

    def test_invalid_port_is_not_executed_as_shell(self):
        result, mutated = self.run_wrapper('postgres://u:secret@localhost:notaport/testdb', '1')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(mutated)
        self.assertNotIn('secret', result.stderr)


if __name__ == '__main__':
    unittest.main(verbosity=2)

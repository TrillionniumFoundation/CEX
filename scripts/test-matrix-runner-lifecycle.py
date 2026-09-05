#!/usr/bin/env python3
"""Real POSIX child-lifecycle tests and input-profile tests; no PostgreSQL server.

SQL fixtures validate the driver's pre-client policy, not PostgreSQL semantics.
Native child fixtures use the current Python executable, not a fake SQL pass.
"""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('matrix_lifecycle_runner', ROOT / 'scripts/matrix_postgres_regression.py')
assert SPEC and SPEC.loader
R = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(R)


class SourceProfileTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in R.MIGRATIONS:
            self.write('services/matrix-entry-adapter/migrations/' + name, 'begin; select 1; commit;\n')
        self.write(R.BASELINE, "DO $test$ begin perform 'matrix_source_event_identity_collision'; end; $test$;\n")
        for name in R.REGRESSIONS:
            self.write(name, 'begin; select 1; rollback;\n')
        self.write(R.RUNNER_SOURCE, Path(R.__file__).read_text())
        for name in R.ENTRYPOINTS:
            self.write(name, R.wrapper_source())

    def write(self, name, text):
        target = self.root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def test_plain_sql_profile_keeps_all_stages(self):
        stages, hashes = R.input_stages(self.root)
        self.assertEqual(len(stages), 2 * len(R.MIGRATIONS) + 1 + len(R.REGRESSIONS))
        self.assertIn(R.BASELINE, hashes)

    def test_inline_control_commands_fail_before_client_selection(self):
        for command in [r'\connect production', r'\set ON_ERROR_STOP off', r'\! command',
                        r'\gexec', r'\i file.sql', r'\copy table TO file']:
            with self.subTest(command=command):
                self.write(R.REGRESSIONS[0], 'select 1; ' + command + '\n')
                with patch.object(R.shutil, 'which', side_effect=AssertionError('client selected')):
                    with self.assertRaisesRegex(R.RegressionError, 'backslashes'):
                        R.execute(self.root, {'MATRIX_TEST_ALLOW_SCHEMA_RESET': '1',
                                  'MATRIX_TEST_DATABASE_URL': 'postgres://u:p@localhost/matrix_lifecycle_ci'})

    def test_backslash_data_and_comments_are_explicitly_outside_profile(self):
        for sql in [r"select E'\n';", r"select '\connect fixture';", r'-- \! fixture',
                    r'/* \connect fixture */', r"select $tag$\gexec$tag$;", r'select "\name";']:
            with self.subTest(sql=sql), self.assertRaises(R.RegressionError):
                R.validate_sql_source(sql)

    def test_nul_cannot_hide_later_input(self):
        with self.assertRaises(R.RegressionError):
            R.validate_sql_source('select 1;\x00select 2;')

    def test_migration_input_cannot_escape_the_same_policy(self):
        self.write('services/matrix-entry-adapter/migrations/' + R.MIGRATIONS[-1],
                   'begin; select 1; \\set ON_ERROR_STOP off\ncommit;\n')
        with self.assertRaises(R.RegressionError):
            R.input_stages(self.root)

    def test_baseline_cannot_escape_the_same_policy(self):
        original = (self.root / R.BASELINE).read_text()
        self.write(R.BASELINE, 'select 1; \\connect other\n' + original)
        with self.assertRaises(R.RegressionError):
            R.input_stages(self.root)

    def test_temp_namespace_guard_is_identity_based_not_name_pattern(self):
        self.assertIn('n.oid <> pg_catalog.pg_my_temp_schema()', R.FOREIGN_TABLES_SQL)
        self.assertIn('not pg_catalog.pg_is_other_temp_schema(n.oid)', R.FOREIGN_TABLES_SQL)
        self.assertNotIn('not like', R.FOREIGN_TABLES_SQL)
        self.assertIn(R.FOREIGN_TABLES_SQL.rstrip().rstrip(';'), R.RESET_SQL)


class NativeProcessTests(unittest.TestCase):
    def run_child(self, program, limit=3):
        return R.bounded_client([sys.executable, '-S', '-c', program], input='', env=dict(os.environ), timeout=limit)

    def test_native_exit_code_and_output_are_retained(self):
        result = self.run_child("import sys;print('native-fixture');sys.exit(7)")
        self.assertEqual(result.returncode, 7)
        self.assertEqual(result.stdout, 'native-fixture\n')
        self.assertEqual(result.stderr, '')

    def test_invalid_timeout_never_starts_a_process(self):
        for value in [0, -1, float('nan'), float('inf'), True, '10', R.MAX_RUN_SECONDS + 1, 10**1000]:
            with self.subTest(value=value), patch.object(R.subprocess, 'Popen', side_effect=AssertionError('started')):
                with self.assertRaisesRegex(R.RegressionError, 'timeout_invalid'):
                    self.run_child('pass', value)

    def test_missing_waitable_group_custody_is_a_preclient_failure(self):
        with patch.object(R.os, 'name', 'nt'), patch.object(R.subprocess, 'Popen', side_effect=AssertionError('started')):
            with self.assertRaisesRegex(R.RegressionError, 'posix_process_custody_required'):
                self.run_child('pass')

    def descendant_case(self, ending, timeout=3, expected_error=None):
        with tempfile.TemporaryDirectory() as directory:
            ready, late = Path(directory) / 'ready', Path(directory) / 'late'
            child = (f'import pathlib,time;pathlib.Path({str(ready)!r}).write_text("ready");'
                     f'time.sleep(1.5);pathlib.Path({str(late)!r}).write_text("escaped")')
            program = (f'import subprocess,sys,time,pathlib;subprocess.Popen([sys.executable,"-S","-c",{child!r}]);'
                       f'deadline=time.monotonic()+2\n'
                       f'while not pathlib.Path({str(ready)!r}).exists():\n'
                       ' if time.monotonic()>deadline:raise RuntimeError("fixture not ready")\n'
                       ' time.sleep(.01)\n' + ending)
            if expected_error:
                with self.assertRaisesRegex(R.RegressionError, expected_error):
                    self.run_child(program, timeout)
            else:
                result = self.run_child(program, timeout)
                self.assertEqual(result.returncode, 0 if ending.endswith('(0)') else 9)
            self.assertTrue(ready.exists(), 'child must start before its leader exits')
            time.sleep(1.65)
            self.assertFalse(late.exists(), 'descendant performed work after the supervisor returned')

    def test_successful_leader_cannot_leave_a_late_writer(self):
        self.descendant_case('sys.exit(0)')

    def test_failed_leader_cannot_leave_a_late_writer(self):
        self.descendant_case('sys.exit(9)')

    def test_timeout_stops_the_started_descendant_too(self):
        self.descendant_case('time.sleep(30)', timeout=1, expected_error='client_timeout')

    def test_output_limit_stops_descendants_even_after_leader_exit(self):
        self.descendant_case("sys.stdout.write('x'*2097152);sys.exit(0)", expected_error='output_limit')

    def test_leader_remains_waitable_until_group_is_stopped(self):
        kill = os.killpg
        observed = []
        def check_then_kill(pid, sig):
            # This raises ChildProcessError if the direct child was already reaped.
            os.waitid(os.P_PID, pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
            observed.append(pid)
            return kill(pid, sig)
        with patch.object(R.os, 'killpg', side_effect=check_then_kill):
            result = self.run_child('pass')
        self.assertEqual(result.returncode, 0)
        self.assertEqual(len(observed), 1)

    def test_poll_cannot_reap_the_leader_ahead_of_group_cleanup(self):
        with patch.object(R.subprocess.Popen, 'poll', side_effect=AssertionError('early reap')):
            self.assertEqual(self.run_child('pass').returncode, 0)

    def test_unrelated_parent_descriptors_are_not_inherited(self):
        read_fd, write_fd = os.pipe()
        try:
            os.set_inheritable(write_fd, True)
            program = (f'import os\ntry:os.fstat({write_fd})\n'
                       'except OSError:print("closed")\nelse:print("inherited")')
            self.assertEqual(self.run_child(program).stdout, 'closed\n')
        finally:
            os.close(read_fd)
            os.close(write_fd)


if __name__ == '__main__':
    unittest.main(verbosity=2)

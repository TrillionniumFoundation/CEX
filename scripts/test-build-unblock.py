#!/usr/bin/env python3
"""Regression tests for direct lock edges and compiler selection, not Rust CI."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import os
import shlex
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'scripts/fixtures/matrix-lock'
SPEC = importlib.util.spec_from_file_location('lock_coherence', ROOT / 'scripts/check-matrix-lock-coherence.py')
assert SPEC and SPEC.loader
LOCK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LOCK)


class LockEdgesTests(unittest.TestCase):
    def setUp(self):
        self.before = (FIXTURES / 'before.toml').read_text()
        self.after = (FIXTURES / 'after.toml').read_text()
        self.manifests = {name: (ROOT / 'apps' / name / 'Cargo.toml').read_text() for name in LOCK.PACKAGES}

    def test_remote_before_fragment_reproduces_actual_drift(self):
        findings = LOCK.check(self.before, self.manifests)
        self.assertEqual(len(findings), 2)
        for item in findings:
            self.assertEqual(item['missing'], ['anyhow', 'sha2', 'shared-config', 'sqlx'])
        self.assertEqual(findings[1]['unexpected'], ['chrono'])

    def test_proposed_edges_match_both_real_manifests(self):
        self.assertEqual(LOCK.check(self.after, self.manifests), [])

    def test_only_two_dependency_arrays_change(self):
        before, after = tomllib.loads(self.before), tomllib.loads(self.after)
        for old, new in zip(before['package'], after['package']):
            old.pop('dependencies'); new.pop('dependencies')
            self.assertEqual(old, new)
        self.assertEqual(self.after.count('"sha2 0.10.9"'), 2)

    def test_missing_dev_dependency_is_not_silently_ignored(self):
        self.assertEqual(LOCK.check(self.after.replace(' "tower",\n', ''), self.manifests)[0]['missing'], ['tower'])

    def test_duplicate_or_absent_workspace_entry_is_rejected(self):
        for text in [self.after + self.after, '[[package]]\nname="other"\nversion="1"\n']:
            with self.assertRaises(ValueError): LOCK.check(text, self.manifests)

    def test_duplicate_lock_reference_is_rejected(self):
        with self.assertRaises(ValueError):
            LOCK.check(self.after.replace(' "anyhow",', ' "anyhow",\n "anyhow",'), self.manifests)

    def test_manifest_package_identity_is_checked(self):
        self.manifests['matrix-bot-relay'] = self.manifests['matrix-bot-relay'].replace('name = "matrix-bot-relay"','name = "other"')
        with self.assertRaises(ValueError): LOCK.check(self.after, self.manifests)

    def test_target_build_and_renamed_dependencies_are_included(self):
        manifest = {'dependencies': {'alias': {'package': 'actual', 'version': '1'}},
                    'target': {'cfg(unix)': {'build-dependencies': {'build-only': '1'}}},
                    'dev-dependencies': {'test-only': '1'}}
        self.assertEqual(LOCK.direct_dependencies(manifest), {'actual', 'build-only', 'test-only'})

    def test_new_direct_dependency_reopens_the_preflight(self):
        self.manifests['matrix-bot-poller'] += '\n[build-dependencies]\nnew_build_dep = "1"\n'
        self.assertEqual(LOCK.check(self.after, self.manifests)[0]['missing'], ['new_build_dep'])


class CompilerSelectionTests(unittest.TestCase):
    def test_root_toolchain_is_the_corrected_release(self):
        toolchain = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']
        self.assertEqual(toolchain['channel'], '1.98.1')
        self.assertEqual(set(toolchain['components']), {'rustfmt', 'clippy'})

    def test_known_workflows_select_corrected_rust(self):
        for filename, expected in [('matrix-review-repair-regression.yml', 4), ('rust-service-gate.yml', 3)]:
            workflow = (ROOT / '.github/workflows' / filename).read_text()
            self.assertEqual(workflow.count('toolchain: 1.98.1'), expected)
        trnm = (ROOT / '.github/workflows/trnm-economy-settlement.yml').read_text()
        self.assertIn('rustup toolchain install 1.98.1', trnm)
        self.assertIn('rustup default 1.98.1', trnm)
        self.assertIn("TRNM_RUST_TOOLCHAIN: '1.98.1'", trnm)
        self.assertNotIn('1.98.0', trnm)

    def test_container_bootstrap_must_install_and_select_fixed_compiler(self):
        script = (ROOT / 'scripts/run-isolated-matrix-tests.sh').read_text()
        self.assertIn('FROM rust:1.98.0-bookworm', script)
        self.assertIn('rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy', script)
        self.assertIn('rustup default 1.98.1', script)
        self.assertIn('rustup toolchain uninstall 1.98.0', script)
        self.assertIn('ENV RUST_VERSION=1.98.1', script)

    def run_versions(self, rust_output: str, rust_exit: int):
        script = (ROOT / 'scripts/run-isolated-matrix-tests.sh').read_text()
        # shlex follows the actual outer bash quoting; do not duplicate the code
        # under test in this fixture.
        outer = script.split('"$IMAGE" bash -c ', 1)[1].split(' > "$OUT/container.log"', 1)[0]
        body = shlex.split(outer)[0]
        versions = next(line.strip() for line in body.splitlines() if line.strip().startswith('run versions '))
        command = shlex.split(versions)[2:]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            (path / 'rustc').write_text(f'#!/bin/sh\nprintf "%s\\n" "{rust_output}"\nexit {rust_exit}\n')
            for name in ['cargo','rustfmt','psql']:
                (path / name).write_text('#!/bin/sh\nexit 0\n')
            for file in path.iterdir(): file.chmod(0o755)
            env = {**os.environ, 'PATH': str(path) + os.pathsep + os.environ['PATH']}
            return subprocess.run(command, env=env, capture_output=True, text=True, timeout=5)

    def test_original_compiler_is_rejected_even_when_other_tools_pass(self):
        self.assertNotEqual(self.run_versions('rustc 1.98.0 (fixture)', 0).returncode, 0)

    def test_missing_compiler_cannot_be_masked_by_successful_psql(self):
        self.assertNotEqual(self.run_versions('', 127).returncode, 0)

    def test_compiler_nonzero_cannot_be_hidden_by_a_valid_version_string(self):
        self.assertNotEqual(self.run_versions('rustc 1.98.1 (fixture)', 1).returncode, 0)

    def test_correct_version_fixture_runs_the_version_probe(self):
        self.assertEqual(self.run_versions('rustc 1.98.1 (fixture)', 0).returncode, 0)


class AuthoritativeWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.rust = (ROOT / '.github/workflows/rust-service-gate.yml').read_text()
        self.execution = (ROOT / '.github/workflows/p0-execution-settlement-gate.yml').read_text()

    def matrix_sequence(self):
        import textwrap
        section = self.rust.split('      - name: Complete Matrix package and current-schema/operator regression\n', 1)[1]
        section = section.split('\n      - name:', 1)[0]
        return textwrap.dedent(section.split('        run: |\n', 1)[1])

    def execute_matrix_sequence(self, failure='none'):
        # Execute the workflow's actual shell sequence, with explicit fake
        # compiler/database clients. This tests failure propagation only.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); tools = root / 'bin'; tools.mkdir()
            scripts = root / 'scripts'; scripts.mkdir()
            trace = root / 'trace'
            for tool in ('python3', 'cargo', 'git'):
                body = '#!/bin/sh\n'
                body += ('name="$1"\n' if tool == 'cargo' else f'name="{tool}"\n')
                body += 'printf "%s\\n" "$name" >> "$TRACE"\n[ "$FAIL" != "$name" ] || exit 71\n'
                path = tools / tool; path.write_text(body); path.chmod(0o755)
            (scripts / 'check-matrix-source-observation-postgres.sh').write_text(
                '#!/bin/sh\nprintf "%s\\n" database >> "$TRACE"\n[ "$FAIL" != database ] || exit 71\n')
            (scripts / 'check-matrix-operator-postgres.sh').write_text(
                '#!/bin/sh\nprintf "%s\\n" operator >> "$TRACE"\n[ "$FAIL" != operator ] || exit 71\n')
            env = {**os.environ, 'PATH': str(tools) + os.pathsep + os.environ['PATH'],
                   'TRACE': str(trace), 'FAIL': failure}
            result = subprocess.run(['bash', '-c', self.matrix_sequence()], cwd=root, env=env,
                                    capture_output=True, text=True, timeout=5)
            return result.returncode, trace.read_text().splitlines() if trace.exists() else []

    def test_matrix_is_in_existing_authoritative_job(self):
        job = self.rust.split('  hepta-postgres-integration:\n', 1)[1]
        for name in ('Complete Matrix package and current-schema/operator regression',
                     'Strict Hepta PostgreSQL package gate', 'Upload Hepta PostgreSQL evidence'):
            self.assertIn(name, job)
        self.assertNotIn('continue-on-error:', job)
        self.assertNotIn('if:', job)
        self.assertIn('cargo test --locked -p matrix-entry-adapter -p matrix-bot-poller -p matrix-bot-relay --all-targets', job)
        self.assertIn('cargo clippy --locked -p matrix-entry-adapter -p matrix-bot-poller -p matrix-bot-relay --all-targets -- -D warnings', job)

    def test_matrix_database_is_separate_and_explicitly_disposable(self):
        self.assertIn("-c 'CREATE DATABASE matrix_review_ci'", self.rust)
        self.assertIn('MATRIX_TEST_DATABASE_URL: postgres://cex:cex_ci_password@127.0.0.1:5432/matrix_review_ci', self.rust)
        self.assertIn("MATRIX_TEST_ALLOW_SCHEMA_RESET: '1'", self.rust)
        self.assertIn('HEPTA_TEST_DATABASE_URL: postgres://cex:cex_ci_password@127.0.0.1:5432/hepta_ci', self.rust)
        self.assertNotIn('DROP DATABASE', self.rust)

    def test_matrix_report_is_retained_with_sha_and_attempt(self):
        self.assertIn('--evidence run/matrix-transport-postgres.json', self.rust)
        self.assertIn('cex-matrix-postgres-${{ github.sha }}-attempt-${{ github.run_attempt }}', self.rust)
        self.assertIn('path: |', self.rust)
        self.assertIn('run/matrix-transport-postgres.json', self.rust)
        self.assertIn('run/matrix-operator-postgres.json', self.rust)
        self.assertIn('if-no-files-found: error', self.rust)

    def test_format_failure_does_not_reach_tests_or_database(self):
        code, trace = self.execute_matrix_sequence('fmt')
        self.assertEqual(code, 71)
        self.assertEqual(trace, ['python3', 'fmt'])

    def test_test_failure_does_not_reach_lint_or_database(self):
        code, trace = self.execute_matrix_sequence('test')
        self.assertEqual(code, 71)
        self.assertEqual(trace, ['python3', 'fmt', 'test'])

    def test_database_failure_cannot_be_hidden_by_final_git_check(self):
        code, trace = self.execute_matrix_sequence('database')
        self.assertEqual(code, 71)
        self.assertEqual(trace, ['python3', 'fmt', 'test', 'clippy', 'database'])

    def test_success_fixture_executes_all_steps_in_order(self):
        code, trace = self.execute_matrix_sequence()
        self.assertEqual(code, 0)
        self.assertEqual(trace, ['python3', 'fmt', 'test', 'clippy', 'database', 'operator', 'git'])

    def test_execution_authority_retains_and_deepens_complete_coverage(self):
        for command in ('cargo test --locked -p execution-service --all-targets --no-fail-fast',
                        'cargo clippy --locked -p execution-service --all-targets -- -D warnings',
                        'cargo check --locked --workspace --all-targets',
                        'python3 scripts/check-execution-lifecycle-coverage.py',
                        'python3 scripts/check-execution-default-state-boundary.py',
                        'python3 scripts/check-execution-ledger-settlement.py',
                        'python3 scripts/check-provider-success-evidence.py',
                        'python3 scripts/check-external-agent-runtime-boundary.py',
                        'bash scripts/check-p0-migrations-postgres.sh',
                        'bash scripts/check-invocation-ledger-contract-postgres.sh',
                        'bash scripts/check-invocation-ledger-terminal-postgres.sh',
                        'bash scripts/check-execution-settlement-commands-postgres.sh',
                        'bash scripts/check-provider-reconciliation-postgres.sh'):
            self.assertIn(command, self.execution)
        self.assertIn('toolchain: 1.98.1', self.execution)
        self.assertIn('components: rustfmt, clippy', self.execution)
        self.assertNotIn('continue-on-error:', self.execution)
        self.assertNotIn('contents: write', self.execution)
        self.assertNotIn('git push', self.execution)
        self.assertNotIn('legacy-local-provider-dispatch', self.execution)


if __name__ == '__main__':
    unittest.main(verbosity=2)

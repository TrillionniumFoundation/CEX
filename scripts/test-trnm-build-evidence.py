#!/usr/bin/env python3
"""Real filesystem/Git packet tests with synthetic ELF bytes, not a Rust build."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import trnm_build_evidence as E


class BuildPacketTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.parent = Path(self.temp.name)
        self.root = self.parent / 'checkout'
        self.output = self.parent / 'artifacts'
        self.root.mkdir(); self.output.mkdir()
        self.write('.gitignore', 'target/\n__pycache__/\n')
        self.write('Cargo.lock', '# fixture lock\nversion = 4\n')
        self.write(E.SOURCE_FILES['settlement_v1.sql'], 'begin; select 1; commit;\n')
        status = {'schema': 'trnm_cex_settlement_runtime_status_v1', 'owner_repository': E.REPOSITORY,
                  'status': 'implemented_pending_exact_commit_ci', 'release_effect': 'none',
                  'trusted_settlement': False, 'public_online': False, 'public_player_market': False,
                  'verified_commit': None}
        self.write(E.SOURCE_FILES['source-status.json'], json.dumps(status))
        binary = bytearray(128)
        binary[:7] = b'\x7fELF\x02\x01\x01'
        binary[16:20] = b'\x03\x00\x3e\x00'
        self.write(E.BINARY, bytes(binary))
        self.git('init', '-q')
        self.git('add', '.')
        self.commit()
        self.env = {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': E.REPOSITORY,
                    'EXPECTED_HEAD_SHA': self.git('rev-parse', 'HEAD').strip(),
                    'GITHUB_EVENT_NAME': 'push', 'GITHUB_RUN_ID': '123', 'GITHUB_RUN_ATTEMPT': '2',
                    'TRNM_STATIC_RESULT': 'success',
                    'TRNM_RUST_TOOLCHAIN': '1.98.0', 'TRNM_POSTGRES_IMAGE': 'postgres:16.4-alpine',
                    **{'TRNM_' + key.upper() + '_OUTCOME': 'success' for key in E.STEP_KEYS}}

    def write(self, relative, data):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data.encode() if isinstance(data, str) else data)
        return path

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True, stderr=subprocess.PIPE)

    def commit(self):
        self.git('-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid',
                 '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')

    def collect(self, **env):
        return E.collect(self.root, self.output, {**self.env, **env})

    def assert_no_packet(self):
        self.assertEqual(list(self.output.iterdir()), [])

    def test_packet_binds_exact_source_and_closed_file_set(self):
        dest = self.collect()
        record = E.verify_packet(dest)
        self.assertEqual(set(p.name for p in dest.iterdir()), E.PACKET_FILES)
        self.assertEqual(record['commit'], self.env['EXPECTED_HEAD_SHA'])
        self.assertEqual(record['tree'], self.git('rev-parse', 'HEAD^{tree}').strip())
        self.assertEqual(record['workflow_run_attempt'], 2)
        self.assertEqual(record['production_authorization'], 'not_granted')
        self.assertEqual((dest / 'Cargo.lock').read_bytes(), (self.root / 'Cargo.lock').read_bytes())
        self.assertFalse((self.root / 'evidence').exists())
        self.assertEqual(self.git('status', '--porcelain'), '')

    def test_attempt_packets_are_distinct_and_do_not_overwrite(self):
        one, two = self.collect(), self.collect(GITHUB_RUN_ATTEMPT='3')
        self.assertNotEqual(one, two)
        E.verify_packet(one); E.verify_packet(two)

    def test_preexisting_stale_output_is_not_included_or_deleted(self):
        stale = self.output / 'old-evidence.json'; stale.write_text('historical')
        dest = self.collect()
        self.assertNotIn(stale.name, E.PACKET_FILES)
        self.assertEqual(stale.read_text(), 'historical')
        self.assertNotEqual(dest, self.output)

    def test_packet_is_private_and_binary_remains_executable(self):
        dest = self.collect()
        self.assertEqual(dest.stat().st_mode & 0o777, 0o700)
        self.assertEqual((dest / 'Cargo.lock').stat().st_mode & 0o777, 0o600)
        self.assertEqual((dest / 'trnm-economy-service').stat().st_mode & 0o777, 0o700)

    def test_wrong_repository_or_unsupported_event_fails(self):
        for change in [{'GITHUB_REPOSITORY': 'other/repo'}, {'GITHUB_ACTIONS': 'false'},
                       {'GITHUB_EVENT_NAME': 'pull_request_target'}]:
            with self.assertRaises(E.EvidenceError): self.collect(**change)
        self.assert_no_packet()

    def test_noncanonical_run_and_attempt_are_rejected(self):
        for key in ['GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT']:
            for value in ['', '0', '-1', '01', '1.0', 'true', '1\n', '9' * 30]:
                with self.subTest(key=key, value=value), self.assertRaises(E.EvidenceError):
                    self.collect(**{key: value})
        self.assert_no_packet()

    def test_wrong_or_malformed_head_never_collects(self):
        for head in ['0' * 40, 'bad', 'A' * 40, self.env['EXPECTED_HEAD_SHA'] + '\n']:
            with self.assertRaises(E.EvidenceError): self.collect(EXPECTED_HEAD_SHA=head)
        self.assert_no_packet()

    def test_every_step_requires_outcome_success_not_skipped_or_failure(self):
        for key in E.STEP_KEYS:
            for value in ['failure', 'skipped', 'cancelled', '', 'true']:
                with self.subTest(key=key, value=value), self.assertRaises(E.EvidenceError):
                    self.collect(**{'TRNM_' + key.upper() + '_OUTCOME': value})
        self.assert_no_packet()

    def test_static_job_must_actually_succeed(self):
        for outcome in ['skipped', 'failure', 'cancelled', '']:
            with self.assertRaises(E.EvidenceError): self.collect(TRNM_STATIC_RESULT=outcome)
        self.assert_no_packet()

    def test_dirty_lock_is_rejected_without_refresh(self):
        changed = self.write('Cargo.lock', 'locally regenerated dependency set')
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assertEqual(changed.read_text(), 'locally regenerated dependency set')
        self.assert_no_packet()

    def test_staged_source_change_is_rejected(self):
        self.write(E.SOURCE_FILES['settlement_v1.sql'], 'changed')
        self.git('add', '.')
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_untracked_nonignored_source_is_rejected(self):
        self.write('src/untracked.rs', 'fn unexpected() {}')
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_index_flags_cannot_hide_dirty_source(self):
        for flag, undo in [('--assume-unchanged', '--no-assume-unchanged'), ('--skip-worktree', '--no-skip-worktree')]:
            self.git('update-index', flag, 'Cargo.lock')
            with self.assertRaises(E.EvidenceError): self.collect()
            self.git('update-index', undo, 'Cargo.lock')
        self.assert_no_packet()

    def test_build_environment_declaration_is_required(self):
        with self.assertRaises(E.EvidenceError): self.collect(TRNM_RUST_TOOLCHAIN='')
        with self.assertRaises(E.EvidenceError): self.collect(TRNM_POSTGRES_IMAGE='postgres:latest')
        self.assert_no_packet()

    def test_missing_binary_does_not_publish(self):
        (self.root / E.BINARY).unlink()
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_non_elf_and_wrong_architecture_rejected(self):
        original = (self.root / E.BINARY).read_bytes()
        wrong = bytearray(original); wrong[18:20] = b'\xb7\x00'
        for data in [b'not a binary', original[:8], bytes(wrong)]:
            self.write(E.BINARY, data)
            with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_symlink_binary_and_parent_are_rejected(self):
        binary = self.root / E.BINARY
        target = self.parent / 'outside'; target.write_bytes(binary.read_bytes())
        binary.unlink(); binary.symlink_to(target)
        with self.assertRaises(E.EvidenceError): self.collect()
        binary.unlink(); binary.parent.rename(binary.parent.with_name('moved'))
        binary.parent.symlink_to(binary.parent.with_name('moved'), target_is_directory=True)
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_hardlinked_binary_is_rejected(self):
        os.link(self.root / E.BINARY, self.parent / 'linked-binary')
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_fifo_is_rejected_without_waiting(self):
        binary = self.root / E.BINARY; binary.unlink(); os.mkfifo(binary)
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_oversized_input_rejected(self):
        with patch.object(E, 'MAX_BINARY', 64), self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_source_status_cannot_claim_production(self):
        status = self.root / E.SOURCE_FILES['source-status.json']
        payload = json.loads(status.read_text()); payload['trusted_settlement'] = True
        status.write_text(json.dumps(payload)); self.git('add', '.'); self.commit()
        self.env['EXPECTED_HEAD_SHA'] = self.git('rev-parse', 'HEAD').strip()
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()

    def test_output_cannot_be_inside_checkout_or_through_symlink(self):
        for destination in [self.root, self.root / 'target']:
            with self.assertRaises(E.EvidenceError): E.collect(self.root, destination, self.env)
        link = self.parent / 'output-link'; link.symlink_to(self.output, target_is_directory=True)
        with self.assertRaises(E.EvidenceError): E.collect(self.root, link, self.env)
        self.assert_no_packet()

    def test_source_mutation_during_collection_removes_partial_packet(self):
        original = E.verify_packet
        def mutate(directory):
            answer = original(directory)
            self.write('Cargo.lock', 'changed while collecting')
            return answer
        with patch.object(E, 'verify_packet', side_effect=mutate), self.assertRaises(E.EvidenceError):
            self.collect()
        self.assert_no_packet()

    def test_binary_mutation_during_collection_removes_partial_packet(self):
        original = E.verify_packet
        def mutate(directory):
            answer = original(directory)
            with (self.root / E.BINARY).open('ab') as stream: stream.write(b'changed')
            return answer
        with patch.object(E, 'verify_packet', side_effect=mutate), self.assertRaises(E.EvidenceError):
            self.collect()
        self.assert_no_packet()

    def test_payload_tampering_is_detected(self):
        dest = self.collect(); (dest / 'Cargo.lock').write_text('tampered')
        with self.assertRaises(E.EvidenceError): E.verify_packet(dest)

    def test_extra_stale_files_and_missing_files_are_detected(self):
        dest = self.collect(); extra = dest / 'old-success.json'; extra.write_text('{}')
        with self.assertRaises(E.EvidenceError): E.verify_packet(dest)
        extra.unlink(); (dest / 'source-status.json').unlink()
        with self.assertRaises(E.EvidenceError): E.verify_packet(dest)

    def test_packet_symlink_is_rejected(self):
        dest = self.collect(); lock = dest / 'Cargo.lock'; lock.unlink(); lock.symlink_to(self.root / 'Cargo.lock')
        with self.assertRaises(E.EvidenceError): E.verify_packet(dest)

    def test_manifest_commitment_tampering_is_detected(self):
        dest = self.collect(); path = dest / 'manifest.json'; value = json.loads(path.read_text())
        value['commit'] = '0' * 40; path.write_text(json.dumps(value))
        with self.assertRaises(E.EvidenceError): E.verify_packet(dest)

    def test_checksum_list_is_exact_not_subset(self):
        dest = self.collect(); (dest / 'SHA256SUMS').write_text('')
        with self.assertRaises(E.EvidenceError): E.verify_packet(dest)

    def test_duplicate_json_keys_and_nonfinite_values_are_rejected(self):
        for data in [b'{"a":1,"a":2}', b'{"value":NaN}']:
            with self.assertRaises(E.EvidenceError): E.parse_json(data)

    def test_cli_failure_does_not_echo_private_path_or_payload(self):
        script = Path(E.__file__)
        env = dict(os.environ, **self.env)
        result = subprocess.run([sys.executable, str(script), '--root', str(self.root),
                                 '--output-parent', str(self.parent / 'PRIVATE_SECRET_MISSING')],
                                env=env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('PRIVATE_SECRET', result.stderr + result.stdout)


class WorkflowContractTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parents[1]
        self.workflow = (self.root / '.github/workflows/trnm-economy-settlement.yml').read_text()

    def test_obsolete_source_writers_are_removed(self):
        for name in ['seq53-matrix-lock-and-gate.yml', 'seq53-semantic-catalog.yml']:
            path = self.root / '.github/workflows' / name
            self.assertFalse(path.exists() or path.is_symlink())

    def test_static_dependency_and_outcomes_are_explicit(self):
        self.assertIn('needs: static-contracts', self.workflow)
        self.assertIn('TRNM_STATIC_RESULT: ${{ needs.static-contracts.result }}', self.workflow)
        for key in E.STEP_KEYS:
            self.assertIn('id: ' + key, self.workflow)
            self.assertIn('TRNM_' + key.upper() + '_OUTCOME: ${{ steps.' + key + '.outcome }}', self.workflow)
        self.assertNotIn('continue-on-error:', self.workflow)

    def test_committed_lock_and_original_tests_are_preserved(self):
        self.assertNotIn('cargo generate-lockfile', self.workflow)
        self.assertNotIn('cargo update', self.workflow)
        for command in ['cargo metadata --locked --no-deps --format-version 1',
                        'cargo fmt -p trnm-economy-service -- --check',
                        'cargo test -p trnm-economy-service --all-targets --locked',
                        'cargo clippy -p trnm-economy-service --all-targets --locked -- -D warnings',
                        'cargo build -p trnm-economy-service --release --locked',
                        'scripts/check-trnm-economy-settlement-contract.py',
                        'scripts/test-trnm-economy-settlement-status-negative.py']:
            self.assertIn(command, self.workflow)
        self.assertIn("TRNM_REQUIRE_CEX_SETTLEMENT_DATABASE_TEST: '1'", self.workflow)

    def test_upload_is_closed_packet_not_preexisting_repository_evidence(self):
        self.assertIn('--output-parent "$RUNNER_TEMP"', self.workflow)
        self.assertIn('path: ${{ steps.packet.outputs.directory }}/', self.workflow)
        self.assertIn('-attempt-${{ github.run_attempt }}', self.workflow)
        self.assertNotIn('path: evidence/', self.workflow)
        self.assertNotIn('contents: write', self.workflow)
        self.assertNotIn('git push', self.workflow)


import sys

if __name__ == '__main__':
    unittest.main(verbosity=2)

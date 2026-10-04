#!/usr/bin/env python3
"""Run the complete strict-wiring test suite on named or detached checkouts.

The unchanged historical cases are retained as a byte-bound test module. The
synthetic fixture metadata and current producer schema are adapted: detached HEAD has no live branch,
so its in-memory fake-run context uses an explicit self-test namespace. Real
commit/tree/lock/migration values, all validators and every negative case remain
unchanged. This entry point neither creates a Git ref nor emits release evidence.
"""
from __future__ import annotations

import hashlib
import copy
import subprocess
import json
from pathlib import Path
import re
import sys
from types import ModuleType
from typing import Any
import unittest

from evidence_safe_io import read_regular_nofollow

CASES_PATH = Path(__file__).absolute().with_name('strict_release_wiring_cases.py')
CASES_BLOB = '6864a7845bd858e18b4cb39011e1d6e9dbef6ece'
CASES_BYTES = 69644


def fixture_metadata(metadata: dict[str, str]) -> dict[str, str]:
    """Adapt only a test branch label, never the input or actual source identity."""
    result = dict(metadata)
    commit = result.get('commit_sha')
    branch = result.get('branch')
    if not isinstance(commit, str) or re.fullmatch(r'[0-9a-f]{40}', commit) is None:
        raise ValueError('self_test_source_sha_invalid')
    if not isinstance(branch, str):
        raise ValueError('self_test_branch_invalid')
    if not branch:
        result['branch'] = 'self-test/detached-' + commit
    return result


def load_cases() -> ModuleType:
    # Use the existing cross-platform no-follow boundary, including on Windows.
    data = read_regular_nofollow(CASES_PATH, maximum=CASES_BYTES)
    if len(data) != CASES_BYTES:
        raise ValueError('strict_wiring_cases_size_changed')
    identity = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    if identity != CASES_BLOB:
        raise ValueError('strict_wiring_cases_identity_changed')
    module = ModuleType('cex_strict_release_wiring_cases')
    module.__file__ = str(CASES_PATH)
    sys.modules[module.__name__] = module
    exec(compile(data, str(CASES_PATH), 'exec', dont_inherit=True), module.__dict__)
    original = module.checkout_metadata

    def checkout_metadata(contract: Any) -> dict[str, str]:
        return fixture_metadata(original(contract))

    # Dependency injection is confined to synthetic fixture construction.
    # No production collector, validator or subprocess behavior is replaced.
    module.checkout_metadata = checkout_metadata
    historical_local_fixture = module.local_fixture

    def local_fixture(name: str, contract: Any, source: dict[str, str]) -> dict[str, Any]:
        payload = historical_local_fixture(name, contract, source)
        if name == 'candidate-hygiene':
            payload['bootstrap'] = {
                'status': 'ok', 'self_test_problems': [], 'git_preflight_problems': [],
                'trust_executed_before_core': True, 'core_executed': True,
            }
        return payload

    module.local_fixture = local_fixture
    return module


class FixtureMetadataTests(unittest.TestCase):
    def test_named_branch_and_source_preserved(self) -> None:
        original = {'branch': 'candidate/named', 'commit_sha': 'a' * 40,
                    'tree_sha': 'b' * 40, 'cargo_lock_sha256': 'unchanged'}
        observed = fixture_metadata(original)
        self.assertEqual(observed, original)
        self.assertIsNot(observed, original)

    def test_detached_label_is_explicit_and_nonmutating(self) -> None:
        original = {'branch': '', 'commit_sha': 'a' * 40, 'tree_sha': 'b' * 40}
        observed = fixture_metadata(original)
        self.assertEqual(observed['branch'], 'self-test/detached-' + 'a' * 40)
        self.assertEqual(original['branch'], '')
        self.assertEqual(observed['tree_sha'], original['tree_sha'])
        self.assertNotEqual(observed['branch'], fixture_metadata(
            {'branch': '', 'commit_sha': 'c' * 40})['branch'])

    def test_bad_commit_or_missing_branch_is_rejected(self) -> None:
        for value in (None, '', 'HEAD', 'a' * 39, 'A' * 40, 'a' * 40 + '\n'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                fixture_metadata({'branch': '', 'commit_sha': value})
        for value in (None, False, 1):
            with self.subTest(value=value), self.assertRaises(ValueError):
                fixture_metadata({'branch': value, 'commit_sha': 'a' * 40})


class HostedRunnerGroupTests(unittest.TestCase):
    def test_builtin_group_preserves_complete_execution_contract(self) -> None:
        cases = load_cases()
        contract = cases.load_contract_module()
        execution = cases.load_execution_module()
        manifest = cases.valid_manifest(contract)
        context = cases.valid_context(contract, manifest)
        source = manifest['source']
        payload = cases.execution_fixture(contract, execution, context, source)
        gate = payload['gates']['p0-migration-gate']
        job = gate['jobs'][0]

        def validate(changes: dict[str, Any]) -> None:
            candidate = copy.deepcopy(payload)
            changed_gate = candidate['gates']['p0-migration-gate']
            changed_gate['jobs'][0].update(changes)
            changed_job = changed_gate['jobs'][0]
            changed_job['record_sha256'] = execution.canonical_digest(
                {key: value for key, value in changed_job.items() if key != 'record_sha256'})
            changed_gate['jobs_sha256'] = execution.canonical_digest(changed_gate['jobs'])
            contract.validate_execution_payload(candidate, context, source)

        validate({})
        validate({'runner_group_id': 0, 'runner_group_name': 'GitHub Actions'})
        validate({'runner_group_id': 123, 'runner_group_name': 'Governed fixture pool'})
        invalid = [
            {'runner_group_id': False, 'runner_group_name': 'GitHub Actions'},
            {'runner_group_id': True, 'runner_group_name': 'GitHub Actions'},
            {'runner_group_id': -1, 'runner_group_name': 'GitHub Actions'},
            {'runner_group_id': '0', 'runner_group_name': 'GitHub Actions'},
            {'runner_group_id': 0.0, 'runner_group_name': 'GitHub Actions'},
            {'runner_group_id': 0, 'runner_group_name': None},
            {'runner_group_id': 0, 'runner_group_name': 'Other pool'},
            {'runner_group_id': None, 'runner_group_name': 'GitHub Actions'},
            {'labels': [*job['labels'], 'self-hosted']},
            {'labels': [*job['labels'], 'SELF-HOSTED']},
            {'labels': [*job['labels'], 'Self-Hosted']},
            {'runner_id': 0}, {'runner_name': ''}, {'labels': []},
            {'status': 'queued'}, {'steps': []},
        ]
        for changes in invalid:
            with self.subTest(changes=changes), self.assertRaises(contract.ContractError):
                validate({'runner_group_id': 0, 'runner_group_name': 'GitHub Actions', **changes})


class CurrentHygieneContractTests(unittest.TestCase):
    def test_current_v3_producer_and_closed_bootstrap_contract(self) -> None:
        cases = load_cases()
        contract = cases.load_contract_module()
        root = Path(__file__).resolve().parents[1]
        result = subprocess.run(
            [sys.executable, str(root / 'scripts/check-p0-release-candidate-hygiene.py')],
            cwd=root, check=True, capture_output=True, text=True,
        )
        payload = json.loads(result.stdout)
        self.assertEqual(payload['schema'], 'cex.p0-release-candidate-hygiene.v3')
        source = {key: payload[key] for key in ('commit_sha', 'tree_sha')}
        contract.validate_local_payload('candidate-hygiene', payload, source, repository_root=root)
        original = copy.deepcopy(payload)
        invalid = [True, False, None, [], {},
                   {**payload['bootstrap'], 'extra': 'forbidden'}]
        for field in payload['bootstrap']:
            invalid.append({key: value for key, value in payload['bootstrap'].items() if key != field})
        for field in ('self_test_problems', 'git_preflight_problems'):
            for value in (None, False, {}, '', ['failed']):
                invalid.append({**payload['bootstrap'], field: value})
        for field in ('trust_executed_before_core', 'core_executed'):
            for value in (False, 1, 'true', None):
                invalid.append({**payload['bootstrap'], field: value})
        for value in ('failed', True, None):
            invalid.append({**payload['bootstrap'], 'status': value})
        for bootstrap in invalid:
            with self.subTest(bootstrap=bootstrap), self.assertRaises(contract.ContractError):
                contract.validate_local_payload('candidate-hygiene',
                    {**payload, 'bootstrap': bootstrap}, source, repository_root=root)
        missing = {key: value for key, value in payload.items() if key != 'bootstrap'}
        for forged in (missing, {**payload, 'schema': 'cex.p0-release-candidate-hygiene.v2'}):
            with self.assertRaises(contract.ContractError):
                contract.validate_local_payload('candidate-hygiene', forged, source, repository_root=root)
        self.assertEqual(payload, original)


def main() -> int:
    try:
        result = unittest.TextTestRunner(stream=sys.stderr, verbosity=1).run(
            unittest.TestSuite([
                unittest.defaultTestLoader.loadTestsFromTestCase(FixtureMetadataTests),
                unittest.defaultTestLoader.loadTestsFromTestCase(CurrentHygieneContractTests),
                unittest.defaultTestLoader.loadTestsFromTestCase(HostedRunnerGroupTests)]))
        if not result.wasSuccessful():
            raise ValueError('strict_wiring_fixture_tests_failed')
        return int(load_cases().main())
    except Exception:
        print(json.dumps({'schema': 'cex.strict-release-evidence-wiring-check.v2',
                          'status': 'failed', 'canonical_evidence_count': 13,
                          'payload_only_attestations': ['repository-governance.json',
                                                        'hosted-run-execution.json'],
                          'problems': ['strict_wiring_cases_or_fixture_unavailable']}, sort_keys=True))
        return 1


if __name__ == '__main__':
    raise SystemExit(main())

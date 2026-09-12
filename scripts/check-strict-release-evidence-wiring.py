#!/usr/bin/env python3
"""Run the complete strict-wiring test suite on named or detached checkouts.

The unchanged historical cases are retained as a byte-bound test module. Only the
synthetic fixture metadata provider is adapted: detached HEAD has no live branch,
so its in-memory fake-run context uses an explicit self-test namespace. Real
commit/tree/lock/migration values, all validators and every negative case remain
unchanged. This entry point neither creates a Git ref nor emits release evidence.
"""
from __future__ import annotations

import hashlib
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


def main() -> int:
    try:
        result = unittest.TextTestRunner(stream=sys.stderr, verbosity=1).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(FixtureMetadataTests))
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

#!/usr/bin/env python3
"""Qualify a Python host's path/descriptor identity semantics on a scratch file.

This is a real local-filesystem observation, not a production custody or approval
claim. No repository source is loaded, no credentials are read, and the existing
hygiene reader and all of its rejection conditions remain independently required.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import stat
import sys
import tempfile
import unittest
from types import SimpleNamespace

FIELDS = ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns')


def identity(metadata: os.stat_result) -> dict[str, int]:
    return {name: int(getattr(metadata, name)) for name in FIELDS}


def consistent(snapshots: list[dict[str, int]]) -> bool:
    return (len(snapshots) == 4 and all(set(item) == set(FIELDS) for item in snapshots)
            and all(item == snapshots[0] for item in snapshots[1:])
            and snapshots[0]['st_ino'] != 0)


def observe() -> dict:
    payload = b'cex-python-file-identity-fixture\n'
    with tempfile.TemporaryDirectory(prefix='cex-python-identity-') as directory:
        path = Path(directory) / 'regular.py'
        path.write_bytes(payload)
        before = path.lstat()
        flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_CLOEXEC', 0)
        flags |= getattr(os, 'O_NOFOLLOW', 0)
        descriptor = os.open(path, flags)
        try:
            opened = os.fstat(descriptor)
            data = os.read(descriptor, len(payload) + 1)
            after_read = os.fstat(descriptor)
        finally:
            os.close(descriptor)
        final = path.lstat()
        stats = (before, opened, after_read, final)
        snapshots = [identity(item) for item in stats]
        regular = all(stat.S_ISREG(item.st_mode) and item.st_nlink == 1 for item in stats)
        return {
            'status': 'ok' if regular and data == payload and consistent(snapshots) else 'failed',
            'snapshots': dict(zip(('before_open', 'descriptor_open', 'descriptor_after_read',
                                  'after_close'), snapshots)),
            'single_link_regular': regular,
            'scratch_bytes_match': data == payload,
        }


class IdentityTests(unittest.TestCase):
    def test_actual_scratch_read(self):
        self.assertEqual(observe()['status'], 'ok')

    def test_every_identity_field_is_required(self):
        sample = {name: index + 1 for index, name in enumerate(FIELDS)}
        self.assertTrue(consistent([sample.copy() for _ in range(4)]))
        for name in FIELDS:
            values = [sample.copy() for _ in range(4)]
            values[1][name] += 1
            self.assertFalse(consistent(values), name)
            values = [sample.copy() for _ in range(4)]
            del values[2][name]
            self.assertFalse(consistent(values), name)

    def test_missing_or_zero_identity_is_rejected(self):
        self.assertFalse(consistent([]))
        sample = {name: 0 for name in FIELDS}
        self.assertFalse(consistent([sample.copy() for _ in range(4)]))

    def test_integer_precision_is_retained(self):
        sample = SimpleNamespace(**{name: 2**100 + i for i, name in enumerate(FIELDS)})
        self.assertEqual(identity(sample)['st_ino'], 2**100 + 1)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--python-version', help='require an exact CPython patch version')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(IdentityTests))
        return 0 if result.wasSuccessful() else 1
    result = observe()
    actual = platform.python_version()
    if args.python_version and (actual != args.python_version or platform.python_implementation() != 'CPython'):
        result['status'] = 'failed'
        result['version_mismatch'] = True
    result.update(schema='cex.python-file-identity-observation.v1', python=actual,
                  implementation=platform.python_implementation(), platform=sys.platform,
                  expected_python=args.python_version, scope='scratch-file-host-conformance-only',
                  production_authorization='not_granted')
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result['status'] == 'ok' else 1


if __name__ == '__main__':
    raise SystemExit(main())

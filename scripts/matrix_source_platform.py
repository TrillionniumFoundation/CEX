"""Portable source checks; POSIX CLI custody and database tests stay on Linux.

Windows does not expose the Git executable bit or a POSIX uid. The source gate
uses a byte-verified index mode and exercises actual pure parsing/SQL construction
there. It never substitutes those observations for POSIX custody, TLS, subprocess
or real database qualification. Runtime implementation bytes are not modified.
"""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from types import SimpleNamespace
from typing import Any

from evidence_safe_io import read_regular_nofollow


def decode_index_mode(raw: bytes, relative: str, data: bytes) -> int:
    if not raw.endswith(b'\0') or raw.count(b'\0') != 1:
        raise ValueError('source_index_entry_ambiguous')
    header, name = raw[:-1].split(b'\t', 1)
    mode, identity, stage = header.decode('ascii').split()
    if (name.decode('utf-8') != relative or mode not in ('100644', '100755')
            or stage != '0' or re.fullmatch(r'[0-9a-f]{40}', identity) is None):
        raise ValueError('source_index_entry_invalid')
    actual = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    if actual != identity:
        raise ValueError('source_index_bytes_differ')
    return int(mode, 8)


def source_mode(root: Path, relative: str, observed_mode: int) -> int:
    if os.name != 'nt':
        return observed_mode
    path = Path(relative)
    if path.is_absolute() or '..' in path.parts or '\\' in relative:
        raise ValueError('source_mode_path_invalid')
    result = subprocess.run(['git', 'ls-files', '--stage', '-z', '--', relative],
                            cwd=root, stdin=subprocess.DEVNULL, capture_output=True,
                            timeout=10, check=False)
    if result.returncode != 0 or len(result.stdout) > 4096:
        raise ValueError('source_index_unavailable')
    data = read_regular_nofollow(root / relative, maximum=2_000_000)
    return decode_index_mode(result.stdout, relative, data)


def probe_environment() -> dict[str, str]:
    env = {'PATH': '', 'PYTHONPATH': '', 'PYTHONDONTWRITEBYTECODE': '1'}
    if os.name == 'nt':
        root = os.environ.get('SystemRoot', '')
        if not root or not Path(root).is_absolute() or not Path(root).is_dir():
            raise ValueError('windows_system_root_unavailable')
        # Python on Windows requires the OS root, not arbitrary inherited PG,
        # service, loader, PYTHONHOME, credentials or shell configuration.
        env['SystemRoot'] = root
    return env


def load(root: Path, filename: str) -> Any:
    path = root / 'scripts' / filename
    raw = read_regular_nofollow(path, maximum=2_000_000)
    spec = importlib.util.spec_from_loader('cex_portable_' + filename.replace('-', '_'), loader=None)
    if spec is None:
        raise ValueError('portable_module_unavailable')
    module = importlib.util.module_from_spec(spec)
    module.__file__ = str(path)
    sys.modules[spec.name] = module
    exec(compile(raw, str(path), 'exec', dont_inherit=True), module.__dict__)
    return module


def portable_self_test(root: Path, contract: str = 'v3') -> list[str]:
    """Run real pure functions with synthetic envelopes; never launch psql."""
    try:
        if contract not in ('v2', 'v3'):
            raise ValueError('portable_contract_unknown')
        canonical = load(root, 'reconcile-matrix-adapter-result.py')
        v3 = canonical.load_v3()
        core = v3.load_core()
        if contract == 'v3':
            v3.patch_core(core)
        fixture = load(root, 'test-matrix-cli-postgres.py')
        expected, _, envelope = fixture.fixture()
        original = copy.deepcopy(envelope)
        forwarded = core.parse_lookup_response(fixture.wire(envelope), expected)
        if forwarded != envelope['forwarded']:
            raise ValueError('portable_result_drift')
        for label, value in fixture.mutants(envelope):
            if contract == 'v2' and not label.startswith('outer-'):
                continue  # Legacy parsing is not silently upgraded to v3.
            try:
                core.parse_lookup_response(fixture.wire(value), expected)
            except (core.ReconciliationError, v3.V3BindingError):
                continue
            raise ValueError('portable_mutation_accepted:' + label)
        try:
            core.parse_lookup_response(b'{"accepted":true,"accepted":false}', expected)
        except core.ReconciliationError:
            pass
        else:
            raise ValueError('portable_duplicate_json_accepted')
        repeated = copy.deepcopy(envelope)
        repeated['generated_at'] = '2026-09-12T00:00:01Z'
        if core.parse_lookup_response(fixture.wire(repeated), expected) != forwarded:
            raise ValueError('portable_observation_changed_result')
        args = SimpleNamespace(**expected, candidate_sha='a' * 40)
        sql = core.build_sql(args, forwarded, 'sha256:' + 'b' * 64,
                             '2026-09-12T00:00:00Z', expected['request_fingerprint'])
        function = 'public.cex_matrix_reconcile_adapter_result_' + contract + '('
        other = 'public.cex_matrix_reconcile_adapter_result_' + ('v2' if contract == 'v3' else 'v3') + '('
        if sql.count(function) != 1 or other in sql or envelope != original:
            raise ValueError('portable_sql_or_fixture_identity_drift')
        implementation = core._IMPLEMENTATION
        environment = implementation.psql_environment({})
        if any(key in environment for key in ('PGSERVICE', 'PGHOSTADDR', 'LD_PRELOAD', 'DATABASE_URL')):
            raise ValueError('portable_closed_environment_drift')
        # This exercises pure construction only, not actual remote TLS/custody.
        if implementation.lookup_url('https://adapter.example', False) != 'https://adapter.example/v1/matrix/results/lookup':
            raise ValueError('portable_lookup_url_drift')
        return []
    except Exception:
        return ['Matrix portable parsing/index contract self-test failed']


def main() -> int:
    import unittest

    class IndexModeTests(unittest.TestCase):
        def test_modes_and_byte_binding(self):
            data = b'test-only\n'
            identity = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
            for mode in ('100644', '100755'):
                raw = f'{mode} {identity} 0\tscripts/example.py\0'.encode()
                self.assertEqual(decode_index_mode(raw, 'scripts/example.py', data), int(mode, 8))
                for bad in (raw + raw, raw.replace(b' 0\t', b' 1\t'), raw.replace(mode.encode(), b'120000')):
                    with self.assertRaises(ValueError):
                        decode_index_mode(bad, 'scripts/example.py', data)
                with self.assertRaises(ValueError):
                    decode_index_mode(raw, 'scripts/example.py', data + b'changed')
                with self.assertRaises(ValueError):
                    decode_index_mode(raw, 'scripts/other.py', data)

    result = unittest.TextTestRunner(stream=sys.stderr).run(unittest.defaultTestLoader.loadTestsFromTestCase(IndexModeTests))
    failures = portable_self_test(Path(__file__).resolve().parents[1], 'v2') + portable_self_test(Path(__file__).resolve().parents[1], 'v3')
    print(json.dumps({'schema': 'cex.matrix-portable-source-self-test.v1',
                      'status': 'ok' if result.wasSuccessful() and not failures else 'failed',
                      'problems': failures, 'posix_custody_tested': False,
                      'database_tested': False, 'production_authorization': 'not_granted'}, sort_keys=True))
    return 0 if result.wasSuccessful() and not failures else 1


if __name__ == '__main__':
    raise SystemExit(main())

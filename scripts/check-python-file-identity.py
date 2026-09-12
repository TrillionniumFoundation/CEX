#!/usr/bin/env python3
"""Qualify a Python host's path/descriptor identity semantics on a scratch file.

This is a real local-filesystem observation, not a production custody or approval
claim. No repository source is loaded, no credentials are read, and the existing
hygiene reader and all of its rejection conditions remain independently required.
"""
from __future__ import annotations

import argparse
import ast
import json
import os
from pathlib import Path
import platform
import stat
import sys
import tempfile
import unittest
from types import SimpleNamespace

def _windows_stamp_records(basic, standard, file_id) -> tuple[int, ...]:
    """Decode native records; creation and metadata-change times are distinct."""
    attributes = int(basic.FileAttributes)
    identifier = int.from_bytes(bytes(file_id.FileId), 'little')
    if (attributes & (0x400 | 0x10) or standard.Directory or standard.DeletePending
            or int(standard.NumberOfLinks) != 1 or int(standard.EndOfFile) < 0
            or not identifier or not int(file_id.VolumeSerialNumber)):
        raise OSError('windows_stamp_not_single_regular_file')
    times = (int(basic.LastWriteTime), int(basic.ChangeTime), int(basic.CreationTime))
    if any(value <= 0 for value in times):
        raise OSError('windows_stamp_time_unavailable')
    return (int(file_id.VolumeSerialNumber), identifier, int(standard.EndOfFile),
            *times, attributes, int(standard.NumberOfLinks))


def _windows_stamp(*, path=None, descriptor=None) -> tuple[int, ...]:
    """Read a no-follow path or existing CRT handle using one native API family.

    Kept inline in the two trust bootstraps: no repository module is imported
    before workflow trust. The conformance test checks their exact AST equality.
    No ctime fallback is allowed when native metadata is unavailable.
    """
    if os.name != 'nt' or (path is None) == (descriptor is None):
        raise OSError('windows_stamp_invalid_request')
    import ctypes
    from ctypes import wintypes
    import msvcrt

    class Basic(ctypes.Structure):
        _fields_ = [('CreationTime', ctypes.c_longlong),
                    ('LastAccessTime', ctypes.c_longlong),
                    ('LastWriteTime', ctypes.c_longlong),
                    ('ChangeTime', ctypes.c_longlong),
                    ('FileAttributes', ctypes.c_uint32)]

    class Standard(ctypes.Structure):
        _fields_ = [('AllocationSize', ctypes.c_longlong),
                    ('EndOfFile', ctypes.c_longlong),
                    ('NumberOfLinks', ctypes.c_uint32),
                    ('DeletePending', ctypes.c_ubyte), ('Directory', ctypes.c_ubyte)]

    class FileId(ctypes.Structure):
        _fields_ = [('VolumeSerialNumber', ctypes.c_ulonglong),
                    ('FileId', ctypes.c_ubyte * 16)]

    if (ctypes.sizeof(Basic), ctypes.sizeof(Standard), ctypes.sizeof(FileId)) != (40, 24, 24):
        raise OSError('windows_stamp_abi_mismatch')
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    create = kernel.CreateFileW
    create.argtypes = (wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                       wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE)
    create.restype = wintypes.HANDLE
    query = kernel.GetFileInformationByHandleEx
    query.argtypes = (wintypes.HANDLE, ctypes.c_int, wintypes.LPVOID, wintypes.DWORD)
    query.restype = wintypes.BOOL
    close = kernel.CloseHandle
    close.argtypes = (wintypes.HANDLE,)
    close.restype = wintypes.BOOL
    file_type = kernel.GetFileType
    file_type.argtypes = (wintypes.HANDLE,)
    file_type.restype = wintypes.DWORD

    owned = path is not None
    # FILE_READ_ATTRIBUTES, FILE_SHARE_READ, OPEN_EXISTING,
    # FILE_FLAG_OPEN_REPARSE_POINT. Never create, truncate, write or follow links.
    handle = (create(os.fspath(path), 0x80, 0x1, None, 3, 0x00200000, None)
              if owned else msvcrt.get_osfhandle(descriptor))
    if handle in (None, -1, ctypes.c_void_p(-1).value):
        raise OSError('windows_stamp_open_failed')
    try:
        if file_type(handle) != 1:  # FILE_TYPE_DISK
            raise OSError('windows_stamp_not_disk_file')
        snapshots = []
        for _ in range(2):
            basic, standard, identity = Basic(), Standard(), FileId()
            for kind, record in ((0, basic), (1, standard), (18, identity)):
                if not query(handle, kind, ctypes.byref(record), ctypes.sizeof(record)):
                    raise OSError('windows_stamp_query_failed')
            snapshots.append(_windows_stamp_records(basic, standard, identity))
        if snapshots[0] != snapshots[1]:
            raise OSError('windows_stamp_changed_during_query')
        return snapshots[0]
    finally:
        if owned and not close(handle):
            raise OSError('windows_stamp_close_failed')


def _windows_stamp_matches_metadata(stamp: tuple[int, ...], metadata: os.stat_result) -> None:
    """Bind the native record to Python's unambiguous fields, retaining read limits."""
    epoch = 116444736000000000  # FILETIME ticks at 1970-01-01 UTC.
    expected = (int(metadata.st_dev), int(metadata.st_ino), int(metadata.st_size),
                int(metadata.st_mtime_ns), int(metadata.st_birthtime_ns))
    observed = (stamp[0], stamp[1], stamp[2],
                (stamp[3] - epoch) * 100, (stamp[5] - epoch) * 100)
    if observed != expected:
        raise OSError('windows_stamp_python_metadata_mismatch')


FIELDS = ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns')
NATIVE_FIELDS = ('volume_serial', 'file_id', 'size', 'last_write_100ns', 'change_100ns',
                 'creation_100ns', 'attributes', 'links')


def identity(metadata: os.stat_result) -> dict[str, int]:
    return {name: int(getattr(metadata, name)) for name in FIELDS}


def consistent(snapshots: list[dict[str, int]], *, native: bool = False) -> bool:
    fields = NATIVE_FIELDS if native else FIELDS
    return (len(snapshots) == 4 and all(set(item) == set(fields) for item in snapshots)
            and all(item == snapshots[0] for item in snapshots[1:])
            and snapshots[0]['file_id' if native else 'st_ino'] != 0)


def observe() -> dict:
    payload = b'cex-python-file-identity-fixture\n'
    with tempfile.TemporaryDirectory(prefix='cex-python-identity-') as directory:
        path = Path(directory) / 'regular.py'
        path.write_bytes(payload)
        before = path.lstat()
        native_values = []
        def capture(metadata, *, path=None, descriptor=None):
            if os.name == "nt":
                stamp = _windows_stamp(path=path, descriptor=descriptor)
                _windows_stamp_matches_metadata(stamp, metadata)
                native_values.append(dict(zip(NATIVE_FIELDS, stamp)))
        capture(before, path=path)
        flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_CLOEXEC', 0)
        flags |= getattr(os, 'O_NOFOLLOW', 0)
        descriptor = os.open(path, flags)
        try:
            opened = os.fstat(descriptor)
            capture(opened, descriptor=descriptor)
            data = os.read(descriptor, len(payload) + 1)
            after_read = os.fstat(descriptor)
            capture(after_read, descriptor=descriptor)
        finally:
            os.close(descriptor)
        final = path.lstat()
        capture(final, path=path)
        stats = (before, opened, after_read, final)
        snapshots = [identity(item) for item in stats]
        regular = all(stat.S_ISREG(item.st_mode) and item.st_nlink == 1 for item in stats)
        coherent = consistent(native_values, native=True) if os.name == 'nt' else consistent(snapshots)
        return {
            'status': 'ok' if regular and data == payload and coherent else 'failed',
            'identity_source': 'windows-native-basic-standard-id' if os.name == 'nt' else 'posix-stat',
            'native_snapshots': native_values,
            'python_ctime_fields_agree': consistent(snapshots),
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



    def native_records(self):
        basic = SimpleNamespace(FileAttributes=0x20, LastWriteTime=116444736000000003,
                                ChangeTime=116444736000000004, CreationTime=116444736000000001)
        standard = SimpleNamespace(Directory=0, DeletePending=0, NumberOfLinks=1, EndOfFile=33)
        identifier = SimpleNamespace(FileId=(2**100 + 5).to_bytes(16, 'little'), VolumeSerialNumber=99)
        return basic, standard, identifier

    def test_native_records_preserve_change_creation_and_128_bit_id(self):
        stamp = _windows_stamp_records(*self.native_records())
        self.assertEqual(stamp[1], 2**100 + 5)
        self.assertNotEqual(stamp[4], stamp[5])
        self.assertEqual(stamp[3:6], (116444736000000003, 116444736000000004, 116444736000000001))

    def test_native_unsafe_or_unavailable_records_are_rejected(self):
        cases = [(0, 'FileAttributes', 0x400), (0, 'FileAttributes', 0x10),
                 (0, 'ChangeTime', 0), (0, 'CreationTime', 0), (0, 'LastWriteTime', 0),
                 (1, 'Directory', 1), (1, 'DeletePending', 1), (1, 'NumberOfLinks', 2),
                 (1, 'NumberOfLinks', 0), (1, 'EndOfFile', -1),
                 (2, 'FileId', bytes(16)), (2, 'VolumeSerialNumber', 0)]
        for index, key, value in cases:
            records = self.native_records()
            setattr(records[index], key, value)
            with self.subTest(field=key), self.assertRaises(OSError):
                _windows_stamp_records(*records)

    def test_native_metadata_binding_rejects_every_mismatch(self):
        stamp = _windows_stamp_records(*self.native_records())
        fields = dict(st_dev=99, st_ino=2**100 + 5, st_size=33,
                      st_mtime_ns=300, st_birthtime_ns=100)
        _windows_stamp_matches_metadata(stamp, SimpleNamespace(**fields))
        for key in fields:
            changed = dict(fields); changed[key] += 1
            with self.subTest(field=key), self.assertRaises(OSError):
                _windows_stamp_matches_metadata(stamp, SimpleNamespace(**changed))

    def test_native_snapshot_changes_are_all_rejected(self):
        sample = dict(zip(NATIVE_FIELDS, _windows_stamp_records(*self.native_records())))
        self.assertTrue(consistent([dict(sample) for _ in range(4)], native=True))
        for key in NATIVE_FIELDS:
            values = [dict(sample) for _ in range(4)]
            values[2][key] += 1
            self.assertFalse(consistent(values, native=True), key)

    def test_native_bootstrap_copies_are_identical(self):
        root = Path(__file__).resolve().parents[1]
        paths = [Path(__file__), root / 'scripts/check-p0-release-candidate-hygiene.py',
                 root / 'scripts/check-workflow-trust.py']
        names = {'_windows_stamp', '_windows_stamp_records', '_windows_stamp_matches_metadata'}
        definitions = []
        for path in paths:
            tree = ast.parse(path.read_text(encoding='utf-8'))
            functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
            self.assertEqual({node.name for node in functions}, names)
            self.assertEqual(len(functions), len(names))
            definitions.append({node.name: ast.dump(node, include_attributes=False) for node in functions})
        self.assertTrue(all(item == definitions[0] for item in definitions[1:]))

    def test_no_fallback_on_invalid_native_request(self):
        with self.assertRaises(OSError):
            _windows_stamp()
        with self.assertRaises(OSError):
            _windows_stamp(path='unused', descriptor=0)


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

#!/usr/bin/env python3
"""Byte-bound cross-platform adapter for the workflow trust implementation.

The reviewed implementation is retained verbatim in
``check-workflow-trust-impl.py``. This adapter applies exact, counted source
corrections before execution so the no-symlink self-tests preserve identical
fail-closed semantics on POSIX and Windows:

* the descriptor sentinel uses the same canonical action target as production;
* local descriptor and path-component mocks patch the concrete ``Path`` type;
* the immutable-script fallback mock patches the concrete type; and
* that fallback installs a real method via ``new=`` instead of an unbound
  ``MagicMock`` ``side_effect``; and
* literal glob and closed non-executable YAML leaf parsing plus hostile tests.

The latter does not admit flow action/event mappings, arbitrary YAML aliases,
dynamic action references, or mutable action versions. The frozen implementation
identity and every original negative test remain required.
"""

from __future__ import annotations

import hashlib
import os
import stat
import sys
from pathlib import Path

_THIS_FILE = Path(__file__)
if not _THIS_FILE.is_absolute():
    _THIS_FILE = Path.cwd() / _THIS_FILE
_SCRIPT_DIR = _THIS_FILE.parent
_IMPL_PATH = _SCRIPT_DIR / "check-workflow-trust-impl.py"
_EXPECTED_IMPL_GIT_BLOB = "a159b71f42083365b4c9c7d41966745d406c54fc"
_EXPECTED_IMPL_SIZE = 40825
_SOURCE_REWRITES: tuple[tuple[bytes, bytes, str], ...] = (
    (
        b'            descriptor_path = action_dir / "action.yml"\n',
        b'            descriptor_path = action_dir.resolve() / "action.yml"\n',
        "canonical local descriptor sentinel",
    ),
    (
        (
            b"            with mock.patch.object(\n"
            b"                Path,\n"
            b'                "is_symlink",\n'
            b"                new=lambda candidate: candidate == descriptor_path,\n"
            b"            ):\n"
        ),
        (
            b"            with mock.patch.object(\n"
            b"                type(descriptor_path),\n"
            b'                "is_symlink",\n'
            b"                new=lambda candidate: candidate == descriptor_path,\n"
            b"            ):\n"
        ),
        "local descriptor concrete-path mock",
    ),
    (
        (
            b"            with mock.patch.object(\n"
            b"                Path,\n"
            b'                "is_symlink",\n'
            b"                new=lambda candidate: candidate == action_dir,\n"
            b"            ):\n"
        ),
        (
            b"            with mock.patch.object(\n"
            b"                type(action_dir),\n"
            b'                "is_symlink",\n'
            b"                new=lambda candidate: candidate == action_dir,\n"
            b"            ):\n"
        ),
        "local path-component concrete-path mock",
    ),
    (
        (
            b"            with mock.patch.object(\n"
            b"                Path,\n"
            b'                "is_symlink",\n'
            b"                side_effect=lambda candidate: candidate == symlink_path,\n"
            b"            ):\n"
        ),
        (
            b"            with mock.patch.object(\n"
            b"                type(symlink_path),\n"
            b'                "is_symlink",\n'
            b"                new=lambda candidate: candidate == symlink_path,\n"
            b"            ):\n"
        ),
        "immutable-script concrete-path mock",
    ),
)
# The frozen implementation remains unchanged. These bounded corrections repair
# plain-scalar classification and admit only non-executable, single-line leaves.
# Missing, repeated or already changed correction sites fail before execution.
_LITERAL_LEAF_SOURCE = r'''def literal_leaf_collection(line: str) -> bool:
    """Recognize only closed, non-executable one-line leaf collections.

    This is not a general flow-YAML parser. Empty permissions denies all access.
    The only admitted lists hold simple literal branch names, runner labels or
    port bindings. No map entry, quote escape, interpolation, alias, nested
    collection, continuation, trailing token or uses/event key can enter here.
    Unknown flow shapes remain fail-closed and action/event checks still inspect
    the complete surrounding document.
    """
    text = strip_yaml_comment(line).strip()
    if text == "permissions: {}":
        return True
    match = re.fullmatch(r"(branches|runs-on|ports):[ \t]*\[([^\r\n]*)\]", text)
    if match is None:
        return False
    key, body = match.groups()
    values = body.split(",")
    if not 1 <= len(values) <= 32:
        return False
    decoded: list[str] = []
    for raw in values:
        value = raw.strip()
        if value.startswith(("'", '\"')):
            if len(value) < 2 or value[-1] != value[0]:
                return False
            value = value[1:-1]
        elif key == "ports":
            # Require a quoted port mapping, avoiding implicit YAML typing.
            return False
        if re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.:/-]{0,255}", value) is None:
            return False
        if value.lower() in {"null", "true", "false", "yes", "no", "on", "off"}:
            return False
        if key == "ports" and re.fullmatch(r"[0-9]{1,5}:[0-9]{1,5}", value) is None:
            return False
        decoded.append(value)
    return len(decoded) == len(set(decoded))


'''.encode("utf-8")
_LITERAL_LEAF_TESTS = r'''    # Literal glob suffixes are plain scalar content, not YAML aliases.
    # Safe leaf collections must never hide an action or a forbidden event.
    for leaf in (
        "paths:\n  - services/example/**\n  - scripts/*matrix*\n",
        "with:\n  path: run/evidence/**\n",
        "permissions: {}\n",
        "branches: [main]\n",
        "runs-on: [self-hosted, linux, x64]\n",
        "ports: ['5432:5432']\n",
    ):
        before = len(PROBLEMS)
        found = uses_entries(leaf + f"steps:\n  - uses: {pinned}\n", "<self-test-leaf>")
        observed = PROBLEMS[before:]
        del PROBLEMS[before:]
        if observed or [value for _, value in found] != [pinned]:
            PROBLEMS.append("workflow trust plain-scalar/leaf collection regression failed")
        found = uses_entries(leaf + "steps:\n  - uses: actions/checkout@v4\n", "<self-test-leaf-mutable>")
        if not found or not all(action_pin_problem(value, require_local_exists=False)[0] for _, value in found):
            PROBLEMS.append("workflow trust leaf form hid mutable action")
    for hostile in (
        "permissions: {contents: write}",
        "permissions: {} , uses: actions/checkout@v4",
        "runs-on: [linux, {uses: actions/checkout@v4}]",
        "runs-on: [linux, *runner]",
        "runs-on: [linux, &runner x64]",
        "runs-on: [linux, !!str x64]",
        "runs-on: [linux, ${{ matrix.runner }}]",
        "runs-on: [linux, 'x64\\nuses: actions/checkout@v4']",
        "runs-on: [linux, 'x64',]",
        "branches: [main] uses: actions/checkout@v4",
        "branches: [main, [pull_request]]",
        "on: [push, pull_request]",
        "uses: [actions/checkout@v4]",
        "anything: {}",
        "paths: *paths",
        "- &step",
    ):
        if not parser_ambiguities(hostile):
            PROBLEMS.append("workflow trust unsafe leaf/alias regression failed")

'''.encode("utf-8")
_SOURCE_REWRITES += (
    (b'r"(?<![A-Za-z0-9_$])[&*][A-Za-z0-9_.-]*(?=$|[\\s,}\\]])"', b'r"(?:^|[ \\t\\[{,])[&*][A-Za-z0-9_.-]*(?=$|[\\s,}\\]])"',
     "alias indicator must begin a YAML token, not a glob suffix"),
    (b'    if FLOW_DELIMITER_RE.search(masked):\n', b'    if FLOW_DELIMITER_RE.search(masked) and not literal_leaf_collection(line):\n',
     "closed non-executable literal leaf collections"),
    (b"def parser_ambiguities(", _LITERAL_LEAF_SOURCE + b"def parser_ambiguities(",
     "literal leaf grammar"),
    (b'    problem, _ = action_pin_problem("./../escape", require_local_exists=False)\n',
     _LITERAL_LEAF_TESTS + b'    problem, _ = action_pin_problem("./../escape", require_local_exists=False)\n',
     "literal leaf positive and hostile regressions"),
)

_ORIGINAL_MODULE_NAME = __name__


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


def _file_stamp(metadata: os.stat_result, *, path=None, descriptor=None) -> tuple[int, ...]:
    if os.name != "nt":
        return _identity(metadata)
    try:
        stamp = _windows_stamp(path=path, descriptor=descriptor)
        _windows_stamp_matches_metadata(stamp, metadata)
        return stamp
    except (OSError, ValueError, ImportError, AttributeError) as error:
        raise SystemExit("cannot obtain native Windows file identity: " + str(error)) from error


def _identity(metadata: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        int(metadata.st_dev),
        int(metadata.st_ino),
        int(metadata.st_size),
        int(metadata.st_mtime_ns),
        int(metadata.st_ctime_ns),
    )


def _read_stable_regular(path: Path) -> bytes:
    """Read one regular file without accepting a symlink or path substitution."""

    absolute = path if path.is_absolute() else Path.cwd() / path
    if ".." in absolute.parts:
        raise SystemExit("workflow-trust implementation path contains parent traversal")
    parents = tuple(reversed(absolute.parents))
    for parent in parents:
        if parent.is_symlink():
            raise SystemExit(
                f"workflow-trust implementation parent is a symlink: {parent}"
            )
    try:
        before = absolute.lstat()
    except OSError as error:
        raise SystemExit(f"cannot inspect workflow-trust implementation: {error}") from error
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        raise SystemExit("workflow-trust implementation is not a single-linked regular file")
    if before.st_size != _EXPECTED_IMPL_SIZE:
        raise SystemExit("workflow-trust implementation size differs from the reviewed candidate")

    before_stamp = _file_stamp(before, path=absolute)

    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_BINARY", 0)
    if getattr(os, "O_NOFOLLOW", None) is not None:
        flags |= int(os.O_NOFOLLOW)
    try:
        descriptor = os.open(absolute, flags)
    except OSError as error:
        raise SystemExit(f"cannot open workflow-trust implementation: {error}") from error
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode) or opened.st_nlink != 1:
            raise SystemExit(
                "opened workflow-trust implementation is not a single-linked regular file"
            )
        opened_stamp = _file_stamp(opened, descriptor=descriptor)
        if opened_stamp != before_stamp:
            raise SystemExit("workflow-trust implementation changed during open")
        chunks: list[bytes] = []
        remaining = opened.st_size
        while remaining:
            chunk = os.read(descriptor, min(1024 * 1024, remaining))
            if not chunk:
                raise SystemExit("workflow-trust implementation shortened while reading")
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1):
            raise SystemExit("workflow-trust implementation grew while reading")
        after = os.fstat(descriptor)
        if _file_stamp(after, descriptor=descriptor) != opened_stamp:
            raise SystemExit("workflow-trust implementation changed while reading")
    finally:
        os.close(descriptor)

    try:
        final = absolute.lstat()
    except OSError as error:
        raise SystemExit(
            f"cannot re-inspect workflow-trust implementation: {error}"
        ) from error
    if _file_stamp(final, path=absolute) != before_stamp or not stat.S_ISREG(final.st_mode):
        raise SystemExit("workflow-trust implementation path changed while reading")
    for parent in parents:
        if parent.is_symlink():
            raise SystemExit(
                f"workflow-trust implementation parent became a symlink: {parent}"
            )
    return b"".join(chunks)


def _git_blob_sha(payload: bytes) -> str:
    header = f"blob {len(payload)}\0".encode("ascii")
    return hashlib.sha1(header + payload).hexdigest()


_SOURCE = _read_stable_regular(_IMPL_PATH)
if _git_blob_sha(_SOURCE) != _EXPECTED_IMPL_GIT_BLOB:
    raise SystemExit("workflow-trust implementation blob differs from the reviewed candidate")
for broken, fixed, label in _SOURCE_REWRITES:
    if _SOURCE.count(broken) != 1:
        raise SystemExit(f"workflow-trust correction point is missing or ambiguous: {label}")
    if fixed in _SOURCE:
        raise SystemExit(f"workflow-trust correction is already present: {label}")
    _SOURCE = _SOURCE.replace(broken, fixed, 1)

globals()["__name__"] = f"{_ORIGINAL_MODULE_NAME}.__impl__"
try:
    exec(compile(_SOURCE, str(_IMPL_PATH), "exec", dont_inherit=True), globals())
finally:
    globals()["__name__"] = _ORIGINAL_MODULE_NAME

if _ORIGINAL_MODULE_NAME == "__main__":
    raise SystemExit(main())

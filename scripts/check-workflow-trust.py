#!/usr/bin/env python3
"""Byte-bound cross-platform adapter for the workflow trust implementation.

The reviewed implementation is retained verbatim in
``check-workflow-trust-impl.py``. This adapter applies three exact source
corrections before execution so the no-symlink self-tests preserve identical
fail-closed semantics on POSIX and Windows:

* local descriptor and path-component mocks patch the concrete ``Path`` type;
* the immutable-script fallback mock patches the concrete type; and
* that fallback installs a real method via ``new=`` instead of an unbound
  ``MagicMock`` ``side_effect``.
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
_ORIGINAL_MODULE_NAME = __name__


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
        if _identity(opened) != _identity(before):
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
        if _identity(after) != _identity(opened):
            raise SystemExit("workflow-trust implementation changed while reading")
    finally:
        os.close(descriptor)

    try:
        final = absolute.lstat()
    except OSError as error:
        raise SystemExit(
            f"cannot re-inspect workflow-trust implementation: {error}"
        ) from error
    if _identity(final) != _identity(before) or not stat.S_ISREG(final.st_mode):
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

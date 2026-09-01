#!/usr/bin/env python3
"""Cross-platform symlink-safe file helpers for release-evidence boundaries.

Evidence paths are usually created inside a workflow workspace, but the
workspace is still writable by every preceding step. ``Path.write_text`` and
``Path.read_text`` follow a replaced symlink, so release scripts use these
helpers for JSON files which form the candidate boundary.

On POSIX, directory descriptors are walked with ``O_NOFOLLOW`` and final files
are opened relative to stable parents. On Windows, every parent directory is
held open without delete sharing and final objects are opened with
``FILE_FLAG_OPEN_REPARSE_POINT``; reparse points, directories masquerading as
files, and multiply-linked files are rejected before bytes are consumed or
committed.
"""

from __future__ import annotations

import hashlib
import json
import os
import stat
import tempfile
from pathlib import Path
from typing import Any, Iterable


class SafeIOError(RuntimeError):
    """Raised when an evidence path is not a safe regular-file boundary."""


def nofollow_supported() -> bool:
    """Whether this host has one of the supported no-follow implementations."""

    if os.name == "posix":
        return all(
            getattr(os, flag, None) is not None
            for flag in ("O_NOFOLLOW", "O_DIRECTORY")
        )
    return os.name == "nt" and _WINDOWS_API_AVAILABLE


def absolute_no_parent(path: Path) -> Path:
    """Return a lexical absolute path without following any symlink.

    ``Path.resolve`` is deliberately not used here: resolving before checking
    would turn a caller-supplied symlink into its target and erase the very
    condition the boundary is meant to reject.
    """

    if os.name == "nt" and path.drive and not path.root:
        raise SafeIOError(f"drive-relative evidence paths are forbidden: {path}")
    candidate = path if path.is_absolute() else Path.cwd() / path
    if ".." in candidate.parts:
        raise SafeIOError(f"parent traversal is forbidden: {path}")
    return candidate


def _basename(path: Path) -> tuple[Path, str]:
    absolute = absolute_no_parent(path)
    name = absolute.name
    if not name or name in {".", ".."} or "/" in name or "\\" in name:
        raise SafeIOError(f"invalid evidence target basename: {path}")
    return absolute, name


def _identity(metadata: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        int(metadata.st_dev),
        int(metadata.st_ino),
        int(metadata.st_size),
        int(metadata.st_mtime_ns),
        int(metadata.st_ctime_ns),
    )


def _require_regular(metadata: os.stat_result, label: str) -> None:
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
        raise SafeIOError(f"{label} must be one single-linked regular file")


def _required_flag(name: str) -> int:
    value = getattr(os, name, None)
    if value is None:
        raise SafeIOError(f"{name} is required for symlink-safe evidence I/O")
    return int(value)


def _directory_flags() -> int:
    return (
        os.O_RDONLY
        | _required_flag("O_DIRECTORY")
        | getattr(os, "O_CLOEXEC", 0)
        | _required_flag("O_NOFOLLOW")
    )


def _target_flags(*, create: bool) -> int:
    flags = os.O_WRONLY | getattr(os, "O_CLOEXEC", 0) | _required_flag("O_NOFOLLOW")
    flags |= getattr(os, "O_NONBLOCK", 0)
    if create:
        flags |= os.O_CREAT | os.O_EXCL
    return flags


def open_directory_nofollow(
    path: Path, *, create: bool = False
) -> tuple[Path, int]:
    """Open every component of ``path`` as a real POSIX directory.

    Higher-level public helpers below use an equivalent retained-handle chain
    on Windows.
    """

    if os.name != "posix":
        raise SafeIOError(
            "open_directory_nofollow descriptor API is POSIX-only; "
            "use the cross-platform evidence helpers"
        )

    absolute = absolute_no_parent(path)
    flags = _directory_flags()
    descriptor = os.open("/", flags)
    try:
        for component in absolute.parts[1:]:
            if component in {"", "."}:
                continue
            try:
                next_descriptor = os.open(component, flags, dir_fd=descriptor)
            except FileNotFoundError:
                if not create:
                    raise SafeIOError(f"evidence parent directory is missing: {absolute}")
                try:
                    os.mkdir(component, 0o700, dir_fd=descriptor)
                except FileExistsError:
                    pass
                try:
                    next_descriptor = os.open(component, flags, dir_fd=descriptor)
                except OSError as error:
                    raise SafeIOError(
                        "cannot open evidence directory without following "
                        f"symlinks: {absolute}"
                    ) from error
            except OSError as error:
                raise SafeIOError(
                    "cannot open evidence directory without following "
                    f"symlinks: {absolute}"
                ) from error
            try:
                metadata = os.fstat(next_descriptor)
                if not stat.S_ISDIR(metadata.st_mode):
                    raise SafeIOError(
                        f"evidence path component is not a directory: {absolute}"
                    )
            except BaseException:
                os.close(next_descriptor)
                raise
            os.close(descriptor)
            descriptor = next_descriptor
        return absolute, descriptor
    except BaseException:
        os.close(descriptor)
        raise


_WINDOWS_API_AVAILABLE = False
if os.name == "nt":  # pragma: no cover - exercised by the hosted Windows lane
    try:
        import ctypes
        from ctypes import wintypes

        class _BY_HANDLE_FILE_INFORMATION(ctypes.Structure):
            _fields_ = [
                ("dwFileAttributes", wintypes.DWORD),
                ("ftCreationTime", wintypes.FILETIME),
                ("ftLastAccessTime", wintypes.FILETIME),
                ("ftLastWriteTime", wintypes.FILETIME),
                ("dwVolumeSerialNumber", wintypes.DWORD),
                ("nFileSizeHigh", wintypes.DWORD),
                ("nFileSizeLow", wintypes.DWORD),
                ("nNumberOfLinks", wintypes.DWORD),
                ("nFileIndexHigh", wintypes.DWORD),
                ("nFileIndexLow", wintypes.DWORD),
            ]

        _KERNEL32 = ctypes.WinDLL("kernel32", use_last_error=True)
        _CREATE_FILE = _KERNEL32.CreateFileW
        _CREATE_FILE.argtypes = (
            wintypes.LPCWSTR,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.LPVOID,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.HANDLE,
        )
        _CREATE_FILE.restype = wintypes.HANDLE

        _GET_FILE_INFO = _KERNEL32.GetFileInformationByHandle
        _GET_FILE_INFO.argtypes = (
            wintypes.HANDLE,
            ctypes.POINTER(_BY_HANDLE_FILE_INFORMATION),
        )
        _GET_FILE_INFO.restype = wintypes.BOOL

        _READ_FILE = _KERNEL32.ReadFile
        _READ_FILE.argtypes = (
            wintypes.HANDLE,
            wintypes.LPVOID,
            wintypes.DWORD,
            ctypes.POINTER(wintypes.DWORD),
            wintypes.LPVOID,
        )
        _READ_FILE.restype = wintypes.BOOL

        _WRITE_FILE = _KERNEL32.WriteFile
        _WRITE_FILE.argtypes = (
            wintypes.HANDLE,
            wintypes.LPCVOID,
            wintypes.DWORD,
            ctypes.POINTER(wintypes.DWORD),
            wintypes.LPVOID,
        )
        _WRITE_FILE.restype = wintypes.BOOL

        _SET_FILE_POINTER_EX = _KERNEL32.SetFilePointerEx
        _SET_FILE_POINTER_EX.argtypes = (
            wintypes.HANDLE,
            ctypes.c_longlong,
            ctypes.POINTER(ctypes.c_longlong),
            wintypes.DWORD,
        )
        _SET_FILE_POINTER_EX.restype = wintypes.BOOL

        _SET_END_OF_FILE = _KERNEL32.SetEndOfFile
        _SET_END_OF_FILE.argtypes = (wintypes.HANDLE,)
        _SET_END_OF_FILE.restype = wintypes.BOOL

        _FLUSH_FILE_BUFFERS = _KERNEL32.FlushFileBuffers
        _FLUSH_FILE_BUFFERS.argtypes = (wintypes.HANDLE,)
        _FLUSH_FILE_BUFFERS.restype = wintypes.BOOL

        _CLOSE_HANDLE = _KERNEL32.CloseHandle
        _CLOSE_HANDLE.argtypes = (wintypes.HANDLE,)
        _CLOSE_HANDLE.restype = wintypes.BOOL

        _INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value
        _GENERIC_READ = 0x80000000
        _GENERIC_WRITE = 0x40000000
        _FILE_READ_ATTRIBUTES = 0x00000080
        _FILE_SHARE_READ = 0x00000001
        _FILE_SHARE_WRITE = 0x00000002
        _CREATE_NEW = 1
        _OPEN_EXISTING = 3
        _FILE_ATTRIBUTE_DIRECTORY = 0x00000010
        _FILE_ATTRIBUTE_REPARSE_POINT = 0x00000400
        _FILE_FLAG_BACKUP_SEMANTICS = 0x02000000
        _FILE_FLAG_OPEN_REPARSE_POINT = 0x00200000
        _FILE_BEGIN = 0
        _ERROR_FILE_NOT_FOUND = 2
        _ERROR_PATH_NOT_FOUND = 3
        _ERROR_FILE_EXISTS = 80
        _ERROR_ALREADY_EXISTS = 183
        _WINDOWS_API_AVAILABLE = True
    except (AttributeError, OSError, ImportError):
        _WINDOWS_API_AVAILABLE = False


def _win_error(path: Path, operation: str) -> OSError:
    if os.name != "nt":
        return OSError(f"{operation} is available only on Windows: {path}")
    code = ctypes.get_last_error()
    return ctypes.WinError(code, f"{operation}: {path}")


def _win_close(handle: int) -> None:
    if os.name == "nt" and handle not in (None, _INVALID_HANDLE_VALUE):
        _CLOSE_HANDLE(handle)


def _win_open(
    path: Path,
    *,
    access: int,
    share: int,
    disposition: int,
    allow_directory: bool,
    missing_ok: bool = False,
) -> int | None:
    if not _WINDOWS_API_AVAILABLE:
        raise SafeIOError("Win32 reparse-point-safe evidence I/O is unavailable")
    flags = _FILE_FLAG_OPEN_REPARSE_POINT
    if allow_directory:
        flags |= _FILE_FLAG_BACKUP_SEMANTICS
    ctypes.set_last_error(0)
    handle = _CREATE_FILE(
        str(path), access, share, None, disposition, flags, None
    )
    if handle == _INVALID_HANDLE_VALUE:
        code = ctypes.get_last_error()
        if missing_ok and code in {_ERROR_FILE_NOT_FOUND, _ERROR_PATH_NOT_FOUND}:
            return None
        raise _win_error(path, "cannot open evidence path without following reparse points")
    value = handle if isinstance(handle, int) else handle.value
    if value is None:
        raise SafeIOError(f"Win32 returned a null evidence handle: {path}")
    return int(value)


def _win_info(handle: int, path: Path) -> Any:
    info = _BY_HANDLE_FILE_INFORMATION()
    ctypes.set_last_error(0)
    if not _GET_FILE_INFO(handle, ctypes.byref(info)):
        raise _win_error(path, "cannot inspect opened evidence handle")
    return info


def _win_filetime(value: Any) -> int:
    return (int(value.dwHighDateTime) << 32) | int(value.dwLowDateTime)


def _win_size(info: Any) -> int:
    return (int(info.nFileSizeHigh) << 32) | int(info.nFileSizeLow)


def _win_stable_identity(info: Any) -> tuple[int, int, int, int, int, int]:
    return (
        int(info.dwVolumeSerialNumber),
        int(info.nFileIndexHigh),
        int(info.nFileIndexLow),
        int(info.nNumberOfLinks),
        _win_size(info),
        _win_filetime(info.ftLastWriteTime),
    )


def _win_object_identity(info: Any) -> tuple[int, int, int, int]:
    return (
        int(info.dwVolumeSerialNumber),
        int(info.nFileIndexHigh),
        int(info.nFileIndexLow),
        int(info.nNumberOfLinks),
    )


def _win_require_not_reparse(info: Any, label: str) -> None:
    if int(info.dwFileAttributes) & _FILE_ATTRIBUTE_REPARSE_POINT:
        raise SafeIOError(f"{label} must not be a reparse point")


def _win_require_directory(info: Any, label: str) -> None:
    _win_require_not_reparse(info, label)
    if not int(info.dwFileAttributes) & _FILE_ATTRIBUTE_DIRECTORY:
        raise SafeIOError(f"{label} must be a real directory")


def _win_require_regular(info: Any, label: str) -> None:
    _win_require_not_reparse(info, label)
    if int(info.dwFileAttributes) & _FILE_ATTRIBUTE_DIRECTORY:
        raise SafeIOError(f"{label} must be one single-linked regular file")
    if int(info.nNumberOfLinks) != 1:
        raise SafeIOError(f"{label} must be one single-linked regular file")


def _win_directory_paths(path: Path) -> list[Path]:
    absolute = absolute_no_parent(path)
    anchor = absolute.anchor
    if not anchor:
        raise SafeIOError(f"Windows evidence path has no absolute anchor: {absolute}")
    current = Path(anchor)
    paths = [current]
    for component in absolute.parts[1:]:
        if component in {"", "."}:
            continue
        current = current / component
        paths.append(current)
    return paths


def _win_open_directory_chain(path: Path, *, create: bool) -> tuple[Path, list[int]]:
    absolute = absolute_no_parent(path)
    handles: list[int] = []
    try:
        for current in _win_directory_paths(absolute):
            handle = _win_open(
                current,
                access=_FILE_READ_ATTRIBUTES,
                share=_FILE_SHARE_READ | _FILE_SHARE_WRITE,
                disposition=_OPEN_EXISTING,
                allow_directory=True,
                missing_ok=True,
            )
            if handle is None:
                if not create:
                    raise SafeIOError(
                        f"evidence parent directory is missing: {absolute}"
                    )
                try:
                    current.mkdir()
                except FileExistsError:
                    pass
                except OSError as error:
                    raise SafeIOError(
                        f"cannot create evidence directory safely: {current}: {error}"
                    ) from error
                handle = _win_open(
                    current,
                    access=_FILE_READ_ATTRIBUTES,
                    share=_FILE_SHARE_READ | _FILE_SHARE_WRITE,
                    disposition=_OPEN_EXISTING,
                    allow_directory=True,
                )
                assert handle is not None
            info = _win_info(handle, current)
            try:
                _win_require_directory(info, f"evidence path component {current}")
            except BaseException:
                _win_close(handle)
                raise
            handles.append(handle)
        return absolute, handles
    except BaseException:
        for handle in reversed(handles):
            _win_close(handle)
        raise


def _win_close_chain(handles: Iterable[int]) -> None:
    for handle in reversed(tuple(handles)):
        _win_close(handle)


def _win_check_target(
    path: Path,
    *,
    label: str,
    kind: str,
    allow_missing: bool,
    create_parent: bool,
) -> bool:
    absolute, _name = _basename(path)
    try:
        _parent, handles = _win_open_directory_chain(
            absolute.parent, create=create_parent
        )
    except SafeIOError:
        if allow_missing and not create_parent:
            return False
        raise
    try:
        handle = _win_open(
            absolute,
            access=_FILE_READ_ATTRIBUTES,
            share=_FILE_SHARE_READ | _FILE_SHARE_WRITE,
            disposition=_OPEN_EXISTING,
            allow_directory=True,
            missing_ok=True,
        )
        if handle is None:
            if allow_missing:
                return False
            raise SafeIOError(f"{label} is missing: {absolute}")
        try:
            info = _win_info(handle, absolute)
            if kind == "directory":
                _win_require_directory(info, f"{label} {absolute}")
            elif kind == "file":
                _win_require_regular(info, f"{label} {absolute}")
            else:
                raise SafeIOError(f"unsupported evidence target kind: {kind}")
            return True
        finally:
            _win_close(handle)
    finally:
        _win_close_chain(handles)


def _win_validate_directory_tree(root: Path) -> None:
    absolute, handles = _win_open_directory_chain(root, create=False)

    def walk(directory: Path) -> None:
        try:
            entries = list(os.scandir(directory))
        except OSError as error:
            raise SafeIOError(
                f"cannot scan evidence directory {directory}: {error}"
            ) from error
        for entry in entries:
            child = directory / entry.name
            label = child.relative_to(absolute).as_posix()
            handle = _win_open(
                child,
                access=_FILE_READ_ATTRIBUTES,
                share=_FILE_SHARE_READ | _FILE_SHARE_WRITE,
                disposition=_OPEN_EXISTING,
                allow_directory=True,
            )
            assert handle is not None
            try:
                info = _win_info(handle, child)
                _win_require_not_reparse(info, f"evidence path {label}")
                if int(info.dwFileAttributes) & _FILE_ATTRIBUTE_DIRECTORY:
                    walk(child)
                else:
                    _win_require_regular(info, f"evidence path {label}")
            finally:
                _win_close(handle)

    try:
        walk(absolute)
    finally:
        _win_close_chain(handles)


def _win_read_regular(path: Path, *, maximum: int) -> bytes:
    absolute, _name = _basename(path)
    _parent, handles = _win_open_directory_chain(absolute.parent, create=False)
    handle: int | None = None
    try:
        handle = _win_open(
            absolute,
            access=_GENERIC_READ | _FILE_READ_ATTRIBUTES,
            share=_FILE_SHARE_READ,
            disposition=_OPEN_EXISTING,
            allow_directory=True,
        )
        assert handle is not None
        before = _win_info(handle, absolute)
        _win_require_regular(before, f"evidence file {absolute}")
        size = _win_size(before)
        if size < 0 or size > maximum:
            raise SafeIOError(f"evidence file exceeds read boundary: {absolute}")
        chunks: list[bytes] = []
        remaining = size
        while remaining:
            width = min(1024 * 1024, remaining)
            buffer = ctypes.create_string_buffer(width)
            read = wintypes.DWORD(0)
            ctypes.set_last_error(0)
            if not _READ_FILE(handle, buffer, width, ctypes.byref(read), None):
                raise _win_error(absolute, "cannot read regular evidence file")
            if read.value == 0:
                raise SafeIOError(f"evidence file shortened while reading: {absolute}")
            chunks.append(buffer.raw[: read.value])
            remaining -= int(read.value)
        extra_buffer = ctypes.create_string_buffer(1)
        extra_read = wintypes.DWORD(0)
        ctypes.set_last_error(0)
        if not _READ_FILE(
            handle, extra_buffer, 1, ctypes.byref(extra_read), None
        ):
            raise _win_error(absolute, "cannot verify evidence file length")
        if extra_read.value:
            raise SafeIOError(f"evidence file grew while reading: {absolute}")
        after = _win_info(handle, absolute)
        _win_require_regular(after, f"evidence file {absolute}")
        if _win_stable_identity(before) != _win_stable_identity(after):
            raise SafeIOError(f"evidence file changed while reading: {absolute}")
        return b"".join(chunks)
    finally:
        if handle is not None:
            _win_close(handle)
        _win_close_chain(handles)


def _win_write_bytes(path: Path, payload: bytes) -> None:
    absolute, _name = _basename(path)
    _parent, handles = _win_open_directory_chain(absolute.parent, create=True)
    handle: int | None = None
    try:
        try:
            handle = _win_open(
                absolute,
                access=_GENERIC_WRITE | _FILE_READ_ATTRIBUTES,
                share=0,
                disposition=_CREATE_NEW,
                allow_directory=True,
            )
        except OSError as error:
            code = getattr(error, "winerror", None)
            if code not in {_ERROR_FILE_EXISTS, _ERROR_ALREADY_EXISTS}:
                raise SafeIOError(
                    "cannot create JSON output without following reparse points: "
                    f"{absolute}: {error}"
                ) from error
            handle = _win_open(
                absolute,
                access=_GENERIC_WRITE | _FILE_READ_ATTRIBUTES,
                share=0,
                disposition=_OPEN_EXISTING,
                allow_directory=True,
            )
        assert handle is not None
        before = _win_info(handle, absolute)
        _win_require_regular(before, f"JSON output {absolute}")
        object_identity = _win_object_identity(before)

        ctypes.set_last_error(0)
        if not _SET_FILE_POINTER_EX(handle, 0, None, _FILE_BEGIN):
            raise _win_error(absolute, "cannot seek JSON output")
        ctypes.set_last_error(0)
        if not _SET_END_OF_FILE(handle):
            raise _win_error(absolute, "cannot truncate JSON output")

        view = memoryview(payload)
        while view:
            chunk = bytes(view[: 1024 * 1024])
            buffer = ctypes.create_string_buffer(chunk, len(chunk))
            written = wintypes.DWORD(0)
            ctypes.set_last_error(0)
            if not _WRITE_FILE(
                handle, buffer, len(chunk), ctypes.byref(written), None
            ):
                raise _win_error(absolute, "cannot write JSON output")
            if written.value <= 0:
                raise SafeIOError(f"short write for JSON output: {absolute}")
            view = view[int(written.value) :]

        ctypes.set_last_error(0)
        if not _FLUSH_FILE_BUFFERS(handle):
            raise _win_error(absolute, "cannot flush JSON output")
        after = _win_info(handle, absolute)
        _win_require_regular(after, f"JSON output {absolute}")
        if _win_object_identity(after) != object_identity:
            raise SafeIOError(f"JSON output changed during safe write: {absolute}")
        if _win_size(after) != len(payload):
            raise SafeIOError(f"short write for JSON output: {absolute}")
    except OSError as error:
        raise SafeIOError(str(error)) from error
    finally:
        if handle is not None:
            _win_close(handle)
        _win_close_chain(handles)


def check_target(
    path: Path,
    *,
    label: str,
    kind: str = "file",
    allow_missing: bool = True,
    create_parent: bool = False,
) -> bool:
    """Check an existing target without following its final link."""

    if os.name == "nt":
        return _win_check_target(
            path,
            label=label,
            kind=kind,
            allow_missing=allow_missing,
            create_parent=create_parent,
        )

    absolute, name = _basename(path)
    try:
        _parent, parent_descriptor = open_directory_nofollow(
            absolute.parent, create=create_parent
        )
    except SafeIOError:
        if allow_missing:
            if create_parent:
                raise
            return False
        raise
    try:
        try:
            metadata = os.stat(name, dir_fd=parent_descriptor, follow_symlinks=False)
        except FileNotFoundError:
            if allow_missing:
                return False
            raise SafeIOError(f"{label} is missing: {absolute}")
        except OSError as error:
            raise SafeIOError(f"cannot inspect {label}: {error}") from error
        if kind == "directory":
            if not stat.S_ISDIR(metadata.st_mode):
                raise SafeIOError(f"{label} must be a real directory: {absolute}")
        elif kind == "file":
            _require_regular(metadata, label)
        else:
            raise SafeIOError(f"unsupported evidence target kind: {kind}")
        return True
    finally:
        os.close(parent_descriptor)


def prepare_collect_targets(
    evidence_dir: Path,
    context_path: Path,
    *,
    input_files: Iterable[Path] = (),
    output_files: Iterable[Path] = (),
    output_directories: Iterable[Path] = (),
) -> tuple[Path, Path]:
    """Validate all collect inputs/outputs before any producer writes."""

    if os.name == "nt":
        evidence_absolute, handles = _win_open_directory_chain(
            evidence_dir, create=True
        )
        _win_close_chain(handles)
    else:
        evidence_absolute, evidence_descriptor = open_directory_nofollow(
            evidence_dir, create=True
        )
        os.close(evidence_descriptor)
    validate_directory_tree(evidence_absolute)

    check_target(
        context_path,
        label="release context target",
        kind="file",
        allow_missing=True,
        create_parent=True,
    )
    for item in input_files:
        check_target(
            item,
            label="collect input",
            kind="file",
            allow_missing=False,
            create_parent=False,
        )
    for item in output_files:
        check_target(
            item,
            label="collect output target",
            kind="file",
            allow_missing=True,
            create_parent=True,
        )
    for item in output_directories:
        check_target(
            item,
            label="collect output directory",
            kind="directory",
            allow_missing=True,
            create_parent=True,
        )
    return evidence_absolute, absolute_no_parent(context_path)


def validate_directory_tree(root: Path) -> None:
    """Reject links and non-regular entries below an evidence root."""

    if os.name == "nt":
        _win_validate_directory_tree(root)
        return

    absolute, descriptor = open_directory_nofollow(root, create=False)

    def walk(directory_descriptor: int, relative: str) -> None:
        try:
            entries = list(os.scandir(directory_descriptor))
        except OSError as error:
            raise SafeIOError(
                f"cannot scan evidence directory {absolute}: {error}"
            ) from error
        for entry in entries:
            entry_label = f"{relative}/{entry.name}" if relative else entry.name
            try:
                metadata = entry.stat(follow_symlinks=False)
            except OSError as error:
                raise SafeIOError(
                    f"cannot inspect evidence path {entry_label}: {error}"
                ) from error
            if stat.S_ISLNK(metadata.st_mode):
                raise SafeIOError(f"evidence path is a symlink: {entry_label}")
            if stat.S_ISDIR(metadata.st_mode):
                try:
                    child = os.open(
                        entry.name, _directory_flags(), dir_fd=directory_descriptor
                    )
                except OSError as error:
                    raise SafeIOError(
                        "cannot open evidence directory without following "
                        f"symlinks: {entry_label}"
                    ) from error
                try:
                    child_metadata = os.fstat(child)
                    if not stat.S_ISDIR(child_metadata.st_mode):
                        raise SafeIOError(
                            f"evidence path is not a directory: {entry_label}"
                        )
                    walk(child, entry_label)
                finally:
                    os.close(child)
            elif stat.S_ISREG(metadata.st_mode):
                _require_regular(metadata, f"evidence path {entry_label}")
            else:
                raise SafeIOError(
                    "evidence path is not a regular file or directory: "
                    f"{entry_label}"
                )

    try:
        walk(descriptor, "")
    finally:
        os.close(descriptor)


def read_regular_nofollow(path: Path, *, maximum: int = 64 * 1024 * 1024) -> bytes:
    """Read one stable, single-linked regular file through a no-follow handle."""

    if os.name == "nt":
        return _win_read_regular(path, maximum=maximum)

    absolute, name = _basename(path)
    _parent, parent_descriptor = open_directory_nofollow(absolute.parent, create=False)
    try:
        try:
            descriptor = os.open(
                name,
                os.O_RDONLY
                | getattr(os, "O_CLOEXEC", 0)
                | _required_flag("O_NOFOLLOW")
                | getattr(os, "O_NONBLOCK", 0),
                dir_fd=parent_descriptor,
            )
        except OSError as error:
            raise SafeIOError(
                f"cannot open regular evidence file {absolute}: {error}"
            ) from error
        try:
            before = os.fstat(descriptor)
            _require_regular(before, f"evidence file {absolute}")
            if before.st_size < 0 or before.st_size > maximum:
                raise SafeIOError(f"evidence file exceeds read boundary: {absolute}")
            chunks: list[bytes] = []
            remaining = before.st_size
            while remaining:
                chunk = os.read(descriptor, min(1024 * 1024, remaining))
                if not chunk:
                    raise SafeIOError(
                        f"evidence file shortened while reading: {absolute}"
                    )
                chunks.append(chunk)
                remaining -= len(chunk)
            if os.read(descriptor, 1):
                raise SafeIOError(f"evidence file grew while reading: {absolute}")
            after = os.fstat(descriptor)
            _require_regular(after, f"evidence file {absolute}")
            if _identity(before) != _identity(after):
                raise SafeIOError(f"evidence file changed while reading: {absolute}")
            return b"".join(chunks)
        finally:
            os.close(descriptor)
    finally:
        os.close(parent_descriptor)


def read_json_nofollow(path: Path, *, label: str = "evidence JSON") -> Any:
    try:
        raw = read_regular_nofollow(path)
        return json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SafeIOError(f"{label} is not valid UTF-8 JSON: {error}") from error


def sha256_file_nofollow(path: Path) -> str:
    digest = hashlib.sha256()
    digest.update(read_regular_nofollow(path, maximum=2**63 - 1))
    return "sha256:" + digest.hexdigest()


def write_json_nofollow(path: Path, value: Any) -> None:
    """Write canonical pretty JSON to a stable regular-file handle."""

    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    if os.name == "nt":
        _win_write_bytes(path, payload)
        return

    absolute, name = _basename(path)
    _parent, parent_descriptor = open_directory_nofollow(
        absolute.parent, create=True
    )
    descriptor: int | None = None
    try:
        try:
            before = os.stat(name, dir_fd=parent_descriptor, follow_symlinks=False)
        except FileNotFoundError:
            before = None
        except OSError as error:
            raise SafeIOError(f"cannot inspect JSON output {absolute}: {error}") from error

        if before is not None:
            _require_regular(before, f"JSON output {absolute}")
        try:
            descriptor = os.open(
                name,
                _target_flags(create=before is None),
                0o600,
                dir_fd=parent_descriptor,
            )
        except OSError as error:
            raise SafeIOError(
                "cannot open JSON output without following symlinks: "
                f"{absolute}: {error}"
            ) from error

        after_open = os.fstat(descriptor)
        _require_regular(after_open, f"JSON output {absolute}")
        if before is not None and _identity(before) != _identity(after_open):
            raise SafeIOError(f"JSON output changed during safe open: {absolute}")
        os.fchmod(descriptor, 0o600)
        os.ftruncate(descriptor, 0)
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise SafeIOError(f"short write for JSON output: {absolute}")
            view = view[written:]
        os.fsync(descriptor)
    finally:
        if descriptor is not None:
            os.close(descriptor)
        os.close(parent_descriptor)


def self_test() -> list[str]:
    """Deterministic link/non-regular output regression fixtures."""

    if not nofollow_supported():
        return ["no supported symlink-safe evidence I/O implementation"]

    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="cex-safe-json-") as directory:
        root = Path(directory)
        evidence = root / "evidence"
        context = root / "context.json"
        prepare_collect_targets(
            evidence, context, output_files=(evidence / "out.json",)
        )
        output = evidence / "out.json"
        write_json_nofollow(output, {"ok": True})
        if read_json_nofollow(output).get("ok") is not True:
            failures.append("safe JSON writer did not round-trip a regular file")

        hardlink_target = root / "hardlink-target.json"
        hardlink_target.write_text("sentinel\n", encoding="utf-8")
        output.unlink()
        try:
            os.link(hardlink_target, output)
        except OSError:
            pass
        else:
            try:
                write_json_nofollow(output, {"overwritten": True})
            except SafeIOError:
                pass
            else:
                failures.append("safe JSON writer accepted a multiply-linked target")
            if hardlink_target.read_text(encoding="utf-8") != "sentinel\n":
                failures.append("hardlink negative fixture modified its peer")
            output.unlink()

        outside = root / "outside.json"
        outside.write_text("sentinel\n", encoding="utf-8")
        try:
            output.symlink_to(outside)
        except (OSError, NotImplementedError):
            symlink_supported = False
        else:
            symlink_supported = True
        if symlink_supported:
            try:
                write_json_nofollow(output, {"overwritten": True})
            except SafeIOError:
                pass
            else:
                failures.append("safe JSON writer followed a symlink target")
            if outside.read_text(encoding="utf-8") != "sentinel\n":
                failures.append("symlink negative fixture modified its outside target")
            output.unlink()

        output.mkdir()
        try:
            check_target(output, label="self-test output", allow_missing=False)
        except SafeIOError:
            pass
        else:
            failures.append("collect target check accepted a directory output target")
        output.rmdir()

        linked_parent = root / "linked-parent"
        try:
            linked_parent.symlink_to(evidence, target_is_directory=True)
        except (OSError, NotImplementedError):
            parent_symlink_supported = False
        else:
            parent_symlink_supported = True
        if parent_symlink_supported:
            try:
                write_json_nofollow(linked_parent / "nested.json", {"bad": True})
            except SafeIOError:
                pass
            else:
                failures.append("safe JSON writer followed a symlink parent")
    return failures


def safe_io_self_test() -> list[str]:
    return self_test()

#!/usr/bin/env python3
"""Small POSIX-safe file helpers for release-evidence JSON boundaries.

Evidence paths are usually created inside a workflow workspace, but the
workspace is still writable by every preceding step.  ``Path.write_text`` and
``Path.read_text`` follow a replaced symlink, so the release scripts use these
helpers for the JSON files which form the candidate boundary.  Directory file
descriptors are walked with ``O_NOFOLLOW`` and every opened target is checked
with ``fstat`` before bytes are read or written.
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
    """Whether this host exposes the POSIX descriptor primitives we require."""

    return os.name == "posix" and all(
        getattr(os, flag, None) is not None for flag in ("O_NOFOLLOW", "O_DIRECTORY")
    )


def absolute_no_parent(path: Path) -> Path:
    """Return a lexical absolute path without following any symlink.

    ``Path.resolve`` is deliberately not used here: resolving before checking
    would turn a caller-supplied symlink into its target and erase the very
    condition the boundary is meant to reject.
    """

    candidate = path if path.is_absolute() else Path.cwd() / path
    if ".." in candidate.parts:
        raise SafeIOError(f"parent traversal is forbidden: {path}")
    return candidate


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
    # O_NONBLOCK prevents a hostile FIFO/device target from blocking the
    # collector before its descriptor can be checked with fstat.
    flags |= getattr(os, "O_NONBLOCK", 0)
    if create:
        flags |= os.O_CREAT | os.O_EXCL
    return flags


def _basename(path: Path) -> tuple[Path, str]:
    absolute = absolute_no_parent(path)
    name = absolute.name
    if not name or name in {".", ".."} or "/" in name:
        raise SafeIOError(f"invalid evidence target basename: {path}")
    return absolute, name


def open_directory_nofollow(
    path: Path, *, create: bool = False
) -> tuple[Path, int]:
    """Open every component of ``path`` as a real directory.

    Missing components are created only when ``create`` is true, and each
    newly-created component is immediately reopened with ``O_NOFOLLOW``.
    Returning the descriptor lets callers perform the final operation relative
    to a stable parent, so a later path swap cannot redirect the write.
    """

    absolute = absolute_no_parent(path)
    flags = _directory_flags()
    descriptor = os.open("/", flags)
    try:
        for component in absolute.parts[1:]:
            if component in {"", "."}:
                continue
            try:
                next_descriptor = os.open(
                    component, flags, dir_fd=descriptor
                )
            except FileNotFoundError:
                if not create:
                    raise SafeIOError(f"evidence parent directory is missing: {absolute}")
                try:
                    os.mkdir(component, 0o700, dir_fd=descriptor)
                except FileExistsError:
                    # A concurrent creator is acceptable only if the reopen
                    # below proves it is a real directory, not a symlink.
                    pass
                try:
                    next_descriptor = os.open(
                        component, flags, dir_fd=descriptor
                    )
                except OSError as error:
                    raise SafeIOError(
                        f"cannot open evidence directory without following symlinks: {absolute}"
                    ) from error
            except OSError as error:
                raise SafeIOError(
                    f"cannot open evidence directory without following symlinks: {absolute}"
                ) from error
            try:
                metadata = os.fstat(next_descriptor)
                if not stat.S_ISDIR(metadata.st_mode):
                    raise SafeIOError(f"evidence path component is not a directory: {absolute}")
            except BaseException:
                os.close(next_descriptor)
                raise
            os.close(descriptor)
            descriptor = next_descriptor
        return absolute, descriptor
    except BaseException:
        os.close(descriptor)
        raise


def _identity(metadata: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
    )


def _require_regular(metadata: os.stat_result, label: str) -> None:
    # A single link prevents an attacker from making an evidence write mutate
    # an unrelated inode through a hardlink.  It also gives the caller a stable
    # regular-file boundary rather than merely a path which happens to resolve.
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
        raise SafeIOError(f"{label} must be one single-linked regular file")


def check_target(
    path: Path,
    *,
    label: str,
    kind: str = "file",
    allow_missing: bool = True,
    create_parent: bool = False,
) -> bool:
    """Check an existing target without following its final symlink.

    ``kind`` is ``file`` or ``directory``.  The check is intentionally
    conservative: an existing non-regular output target (including a FIFO,
    socket, device, directory, symlink, or multiply-linked file) is rejected.
    ``write_json_nofollow`` repeats the check on the opened descriptor to close
    the check/use race.
    """

    absolute, name = _basename(path)
    try:
        _parent, parent_descriptor = open_directory_nofollow(
            absolute.parent, create=create_parent
        )
    except SafeIOError:
        if allow_missing:
            # A missing parent is safe only when the caller explicitly asked
            # for it to be created later; otherwise report the path failure.
            if create_parent:
                raise
            return False
        raise
    try:
        try:
            metadata = os.stat(
                name, dir_fd=parent_descriptor, follow_symlinks=False
            )
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
    """Validate all collect inputs/outputs before any producer writes.

    The evidence root is created through the no-follow dirfd walker when
    absent.  Existing entries are traversed without following symlinks, and
    every named input/output target is checked before the collector starts.
    """

    evidence_absolute, evidence_descriptor = open_directory_nofollow(
        evidence_dir, create=True
    )
    os.close(evidence_descriptor)
    validate_directory_tree(evidence_absolute)

    # Context and output parents may be missing on a fresh workspace; create
    # them through the same no-follow walk so Path.mkdir cannot follow a link.
    check_target(
        context_path,
        label="release context target",
        kind="file",
        allow_missing=True,
        create_parent=True,
    )
    for path in input_files:
        check_target(
            path,
            label="collect input",
            kind="file",
            allow_missing=False,
            create_parent=False,
        )
    for path in output_files:
        check_target(
            path,
            label="collect output target",
            kind="file",
            allow_missing=True,
            create_parent=True,
        )
    for path in output_directories:
        check_target(
            path,
            label="collect output directory",
            kind="directory",
            allow_missing=True,
            create_parent=True,
        )
    return evidence_absolute, absolute_no_parent(context_path)


def validate_directory_tree(root: Path) -> None:
    """Reject symlinks and non-regular entries below an evidence root."""

    absolute, descriptor = open_directory_nofollow(root, create=False)

    def walk(directory_descriptor: int, relative: str) -> None:
        try:
            entries = list(os.scandir(directory_descriptor))
        except OSError as error:
            raise SafeIOError(f"cannot scan evidence directory {absolute}: {error}") from error
        for entry in entries:
            entry_label = f"{relative}/{entry.name}" if relative else entry.name
            try:
                metadata = entry.stat(follow_symlinks=False)
            except OSError as error:
                raise SafeIOError(f"cannot inspect evidence path {entry_label}: {error}") from error
            if stat.S_ISLNK(metadata.st_mode):
                raise SafeIOError(f"evidence path is a symlink: {entry_label}")
            if stat.S_ISDIR(metadata.st_mode):
                try:
                    child = os.open(
                        entry.name, _directory_flags(), dir_fd=directory_descriptor
                    )
                except OSError as error:
                    raise SafeIOError(
                        f"cannot open evidence directory without following symlinks: {entry_label}"
                    ) from error
                try:
                    child_metadata = os.fstat(child)
                    if not stat.S_ISDIR(child_metadata.st_mode):
                        raise SafeIOError(f"evidence path is not a directory: {entry_label}")
                    walk(child, entry_label)
                finally:
                    os.close(child)
            elif stat.S_ISREG(metadata.st_mode):
                _require_regular(metadata, f"evidence path {entry_label}")
            else:
                raise SafeIOError(f"evidence path is not a regular file or directory: {entry_label}")

    try:
        walk(descriptor, "")
    finally:
        os.close(descriptor)


def read_regular_nofollow(path: Path, *, maximum: int = 64 * 1024 * 1024) -> bytes:
    """Read one stable, single-linked regular file through a no-follow fd."""

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
            raise SafeIOError(f"cannot open regular evidence file {absolute}: {error}") from error
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
                    raise SafeIOError(f"evidence file shortened while reading: {absolute}")
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
    # Reading through the stable descriptor also keeps hashes from silently
    # following a symlink introduced between a path check and a digest pass.
    digest.update(read_regular_nofollow(path, maximum=2**63 - 1))
    return "sha256:" + digest.hexdigest()


def write_json_nofollow(path: Path, value: Any) -> None:
    """Write canonical pretty JSON to a stable regular-file descriptor."""

    absolute, name = _basename(path)
    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
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
                f"cannot open JSON output without following symlinks: {absolute}: {error}"
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
    """Minimal deterministic symlink/non-regular output regression fixtures."""

    # The strict evidence writer intentionally relies on POSIX descriptor
    # primitives (O_NOFOLLOW/O_DIRECTORY).  The release payload is collected
    # on the Linux runner; the Windows service-local gate still imports the
    # core collector for its run-snapshot self-test, so do not turn an
    # unavailable POSIX primitive into a false Windows product failure.
    if not nofollow_supported():
        return []

    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="cex-safe-json-") as directory:
        root = Path(directory)
        evidence = root / "evidence"
        context = root / "context.json"
        prepare_collect_targets(evidence, context, output_files=(evidence / "out.json",))
        output = evidence / "out.json"
        write_json_nofollow(output, {"ok": True})
        if read_json_nofollow(output).get("ok") is not True:
            failures.append("safe JSON writer did not round-trip a regular file")

        outside = root / "outside.json"
        outside.write_text("sentinel\n", encoding="utf-8")
        output.unlink()
        output.symlink_to(outside)
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
        linked_parent.symlink_to(evidence, target_is_directory=True)
        try:
            write_json_nofollow(linked_parent / "nested.json", {"bad": True})
        except SafeIOError:
            pass
        else:
            failures.append("safe JSON writer followed a symlink parent")
    return failures


# Public alias used by the collectors' import-time self-test hook.  Keep the
# implementation returning a list so callers can report all negative-fixture
# failures in one deterministic diagnostic.
def safe_io_self_test() -> list[str]:
    return self_test()

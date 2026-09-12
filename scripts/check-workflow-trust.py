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

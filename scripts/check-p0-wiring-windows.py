#!/usr/bin/env python3
"""Windows adapter for the byte-bound P0 static wiring checker.

Python's direct executable lookup selects ``System32\\bash.exe`` (the WSL
launcher) before PATH on current Windows runners, even though the workflow's
interactive ``bash`` command resolves to Git for Windows. The authoritative
checker deliberately exercises shell fixtures in-process, so this adapter
loads its reviewed bytes and redirects only argv[0] == ``bash`` to the Git for
Windows binary. Every other subprocess invocation and all checker semantics
remain unchanged.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))
from evidence_safe_io import read_regular_nofollow  # noqa: E402

_IMPL_PATH = _SCRIPT_DIR / "check-p0-wiring.py"
_EXPECTED_IMPL_GIT_BLOB = "9e32ee9423c69c88e9c36812511c57ec72c2960b"
_ORIGINAL_MODULE_NAME = __name__


def _consume_adapter_arguments() -> None:
    expected = ["--implementation", "scripts/check-p0-wiring.py"]
    if sys.argv[1:] != expected:
        raise SystemExit(
            "usage: check-p0-wiring-windows.py "
            "--implementation scripts/check-p0-wiring.py"
        )
    sys.argv[:] = [sys.argv[0]]


def _git_blob_sha(payload: bytes) -> str:
    header = f"blob {len(payload)}\0".encode("ascii")
    return hashlib.sha1(header + payload).hexdigest()


def _git_bash() -> Path:
    """Resolve Git for Windows Bash without accepting the WSL launcher."""

    candidates: list[Path] = []
    git = shutil.which("git")
    if git:
        git_path = Path(git).resolve()
        for parent in git_path.parents:
            candidates.extend(
                (
                    parent / "bin" / "bash.exe",
                    parent / "usr" / "bin" / "bash.exe",
                )
            )
    for variable in ("ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"):
        value = os.environ.get(variable)
        if value:
            root = Path(value) / "Git"
            candidates.extend(
                (root / "bin" / "bash.exe", root / "usr" / "bin" / "bash.exe")
            )

    seen: set[str] = set()
    for candidate in candidates:
        key = os.path.normcase(os.fspath(candidate))
        if key in seen:
            continue
        seen.add(key)
        if not candidate.is_file() or candidate.is_symlink():
            continue
        normalized = os.path.normcase(os.fspath(candidate.resolve()))
        system32 = os.path.normcase(
            os.fspath(Path(os.environ.get("SystemRoot", r"C:\\Windows")) / "System32")
        )
        try:
            if os.path.commonpath((normalized, system32)) == system32:
                continue
        except ValueError:
            pass
        return candidate
    raise SystemExit("Git for Windows bash.exe was not found; refusing WSL fallback")


def _redirect_bash(
    git_bash: Path,
    original_run: Any,
    args: Any,
    *positional: Any,
    **keywords: Any,
) -> Any:
    rewritten = args
    if isinstance(args, (list, tuple)) and args:
        executable = os.fspath(args[0])
        if Path(executable).name.casefold() in {"bash", "bash.exe"}:
            rewritten_items = [os.fspath(git_bash), *args[1:]]
            rewritten = (
                tuple(rewritten_items) if isinstance(args, tuple) else rewritten_items
            )
    return original_run(rewritten, *positional, **keywords)


_consume_adapter_arguments()
_SOURCE = read_regular_nofollow(_IMPL_PATH)
if _git_blob_sha(_SOURCE) != _EXPECTED_IMPL_GIT_BLOB:
    raise SystemExit("P0 wiring implementation blob differs from the reviewed candidate")

if os.name == "nt":
    _BASH = _git_bash()
else:
    resolved = shutil.which("bash")
    if not resolved:
        raise SystemExit("bash is required by the P0 wiring checker")
    _BASH = Path(resolved)

_ORIGINAL_RUN = subprocess.run
subprocess.run = lambda args, *positional, **keywords: _redirect_bash(  # type: ignore[assignment]
    _BASH,
    _ORIGINAL_RUN,
    args,
    *positional,
    **keywords,
)

globals()["__name__"] = f"{_ORIGINAL_MODULE_NAME}.__impl__"
try:
    exec(compile(_SOURCE, str(_IMPL_PATH), "exec", dont_inherit=True), globals())
finally:
    globals()["__name__"] = _ORIGINAL_MODULE_NAME

try:
    if _ORIGINAL_MODULE_NAME == "__main__":
        raise SystemExit(main())
finally:
    subprocess.run = _ORIGINAL_RUN

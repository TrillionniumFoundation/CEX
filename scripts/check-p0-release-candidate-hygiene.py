#!/usr/bin/env python3
"""Trust-first candidate hygiene bootstrap and single-trigger authority check.

No repository-local module is imported before workflow trust succeeds. Git
index/worktree state is checked before child execution, then trust and core are
read through stable regular-file descriptors and executed from those exact
bytes, with trust always preceding core.
"""

from __future__ import annotations

import argparse
import json
import os
import stat
import subprocess
import sys
import tempfile
import types
from pathlib import Path
from typing import Any


class BootstrapError(RuntimeError):
    """Raised when a repository script is not a safe file boundary."""


_THIS_FILE = Path(__file__)
if not _THIS_FILE.is_absolute():
    _THIS_FILE = Path.cwd() / _THIS_FILE
_SCRIPT_DIR = _THIS_FILE.parent
ROOT = _SCRIPT_DIR.parent
CORE = ROOT / "scripts/check-p0-release-candidate-hygiene-core.py"
TRUST = ROOT / "scripts/check-workflow-trust.py"
SAFE_IO = ROOT / "scripts/evidence_safe_io.py"
TRIGGER_PATH = "docs/release-evidence/p0-candidate-trigger.json"
SECONDARY_FREEZE_PATTERNS = (
    "docs/release-evidence/*qualification*freeze*",
    "docs/release-evidence/.*qualification*freeze*",
)
_ALLOWED_GIT_MODES = {b"100644", b"100755"}
_MAX_SCRIPT_BYTES = 64 * 1024 * 1024
_CHILD_LAUNCHER = r'''
from __future__ import annotations
import sys
source = sys.stdin.buffer.read()
path = sys.argv[1]
scope = {
    "__name__": "__main__",
    "__file__": path,
    "__package__": None,
    "__cached__": None,
}
exec(compile(source, path, "exec", dont_inherit=True), scope, scope)
'''


def _identity(metadata: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        int(metadata.st_dev),
        int(metadata.st_ino),
        int(metadata.st_size),
        int(metadata.st_mtime_ns),
        int(metadata.st_ctime_ns),
    )


def _link_like(path: Path) -> bool:
    try:
        if path.is_symlink():
            return True
        is_junction = getattr(os.path, "isjunction", None)
        return bool(is_junction and is_junction(path))
    except OSError as error:
        raise BootstrapError(f"cannot inspect path boundary {path}: {error}") from error


def _lexical_absolute(path: Path) -> Path:
    candidate = path if path.is_absolute() else Path.cwd() / path
    if ".." in candidate.parts:
        raise BootstrapError(f"parent traversal is forbidden: {path}")
    return candidate


def _check_parent_boundaries(path: Path) -> None:
    for parent in reversed(path.parents):
        if _link_like(parent):
            raise BootstrapError(f"script parent is a symlink or junction: {parent}")


def _require_single_regular(metadata: os.stat_result, label: str) -> None:
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
        raise BootstrapError(f"{label} must be one single-linked regular file")


def read_stable_regular(path: Path, *, maximum: int = _MAX_SCRIPT_BYTES) -> bytes:
    """Read one lexical path without accepting links or identity substitution."""

    absolute = _lexical_absolute(path)
    _check_parent_boundaries(absolute)
    try:
        before = absolute.lstat()
    except OSError as error:
        raise BootstrapError(f"cannot inspect repository script {absolute}: {error}") from error
    _require_single_regular(before, f"repository script {absolute}")
    if before.st_size < 0 or before.st_size > maximum:
        raise BootstrapError(f"repository script exceeds read boundary: {absolute}")

    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_BINARY", 0)
    nofollow = getattr(os, "O_NOFOLLOW", None)
    if nofollow is not None:
        flags |= int(nofollow)
    flags |= getattr(os, "O_NONBLOCK", 0)
    try:
        descriptor = os.open(absolute, flags)
    except OSError as error:
        raise BootstrapError(
            f"cannot open repository script without following links: {absolute}: {error}"
        ) from error
    try:
        opened = os.fstat(descriptor)
        _require_single_regular(opened, f"opened repository script {absolute}")
        if _identity(opened) != _identity(before):
            raise BootstrapError(f"repository script changed during open: {absolute}")
        chunks: list[bytes] = []
        remaining = opened.st_size
        while remaining:
            chunk = os.read(descriptor, min(1024 * 1024, remaining))
            if not chunk:
                raise BootstrapError(f"repository script shortened while reading: {absolute}")
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1):
            raise BootstrapError(f"repository script grew while reading: {absolute}")
        after = os.fstat(descriptor)
        _require_single_regular(after, f"opened repository script {absolute}")
        if _identity(after) != _identity(opened):
            raise BootstrapError(f"repository script changed while reading: {absolute}")
    finally:
        os.close(descriptor)

    try:
        final = absolute.lstat()
    except OSError as error:
        raise BootstrapError(f"cannot re-inspect repository script {absolute}: {error}") from error
    _require_single_regular(final, f"repository script {absolute}")
    if _identity(final) != _identity(before):
        raise BootstrapError(f"repository script path changed while reading: {absolute}")
    _check_parent_boundaries(absolute)
    return b"".join(chunks)


def _git_bytes(*arguments: str) -> subprocess.CompletedProcess[bytes]:
    try:
        return subprocess.run(
            ["git", "-C", str(ROOT), *arguments],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as error:
        raise BootstrapError(f"cannot execute Git preflight: {error}") from error


def git_preflight_problems() -> list[str]:
    """Reject non-regular index entries or any dirty/untracked candidate path."""

    problems: list[str] = []
    top = _git_bytes("rev-parse", "--show-toplevel")
    if top.returncode != 0:
        return [
            "cannot resolve candidate Git worktree: "
            + top.stderr.decode("utf-8", "replace").strip()
        ]
    try:
        git_root = Path(top.stdout.decode("utf-8").strip())
        if not os.path.samefile(ROOT, git_root):
            problems.append("candidate hygiene root is not the Git worktree root")
    except (OSError, ValueError) as error:
        problems.append(f"cannot bind candidate hygiene root to Git worktree: {error}")

    staged = _git_bytes("ls-files", "-s", "-z")
    if staged.returncode != 0:
        problems.append(
            "cannot enumerate exact Git index: "
            + staged.stderr.decode("utf-8", "replace").strip()
        )
    else:
        for record in staged.stdout.split(b"\0"):
            if not record:
                continue
            metadata, separator, raw_path = record.partition(b"\t")
            fields = metadata.split()
            label = (
                raw_path.decode("utf-8", "backslashreplace")
                if separator
                else "<malformed>"
            )
            if len(fields) != 3 or not separator:
                problems.append(f"malformed Git index entry: {label}")
                continue
            mode, object_id, stage = fields
            if mode not in _ALLOWED_GIT_MODES:
                problems.append(
                    "tracked entry is not a regular file: "
                    f"{label} (mode={mode.decode('ascii', 'replace')})"
                )
            if stage != b"0":
                problems.append(
                    "tracked entry has unresolved index stage: "
                    f"{label} (stage={stage.decode('ascii', 'replace')})"
                )
            if len(object_id) not in {40, 64} or any(
                byte not in b"0123456789abcdef" for byte in object_id
            ):
                problems.append(f"tracked entry has invalid object identity: {label}")

    status = _git_bytes("status", "--porcelain=v1", "-z", "--untracked-files=all")
    if status.returncode != 0:
        problems.append(
            "cannot inspect candidate worktree state: "
            + status.stderr.decode("utf-8", "replace").strip()
        )
    elif status.stdout:
        entries = [
            item.decode("utf-8", "backslashreplace")
            for item in status.stdout.split(b"\0")
            if item
        ]
        problems.append("candidate worktree is not clean: " + "; ".join(entries[:20]))
    return problems


def run_json(path: Path) -> tuple[int, dict[str, Any] | None, str]:
    """Execute stable bytes in an isolated child and parse its sole JSON output."""

    source = read_stable_regular(path)
    completed = subprocess.run(
        [sys.executable, "-I", "-c", _CHILD_LAUNCHER, str(path)],
        cwd=ROOT,
        input=source,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    raw = completed.stdout.decode("utf-8", "replace").strip()
    try:
        decoded = json.loads(raw)
    except json.JSONDecodeError:
        payload: dict[str, Any] | None = None
    else:
        payload = decoded if isinstance(decoded, dict) else None
    return completed.returncode, payload, raw


def payload_problems(
    label: str,
    returncode: int,
    payload: dict[str, Any] | None,
    raw: str,
) -> list[str]:
    if payload is None:
        return [f"{label} did not emit one JSON object: {raw or '<empty>'}"]
    value = payload.get("problems")
    result = [str(item) for item in value] if isinstance(value, list) else []
    if payload.get("ok") is False and not result:
        result.append(f"{label} reported ok=false without diagnostics")
    status = payload.get("status")
    if status is not None and status != "ok" and not result:
        result.append(f"{label} reported status={status!r}")
    if returncode != 0 and not result:
        result.append(f"{label} failed with exit code {returncode}")
    return result


def secondary_freeze_markers() -> list[str]:
    markers: set[str] = set()
    for pattern in SECONDARY_FREEZE_PATTERNS:
        markers.update(
            path.relative_to(ROOT).as_posix()
            for path in ROOT.glob(pattern)
            if path.name != Path(TRIGGER_PATH).name
        )
    return sorted(markers)


def write_result_nofollow(path: Path, result: dict[str, Any]) -> None:
    """Load the reviewed safe-I/O helper only after workflow trust passes."""

    source = read_stable_regular(SAFE_IO)
    module = types.ModuleType("_cex_hygiene_safe_io")
    module.__file__ = str(SAFE_IO)
    module.__package__ = ""
    exec(compile(source, str(SAFE_IO), "exec", dont_inherit=True), module.__dict__)
    try:
        module.write_json_nofollow(path, result)
    except Exception as error:
        raise BootstrapError(str(error)) from error


def bootstrap_self_test() -> list[str]:
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="cex-hygiene-bootstrap-") as directory:
        root = Path(directory)
        regular = root / "regular.py"
        regular.write_bytes(b"print('ok')\n")
        try:
            if read_stable_regular(regular) != b"print('ok')\n":
                failures.append("stable regular-file read changed bytes")
        except BootstrapError as error:
            failures.append(f"stable regular-file read failed: {error}")

        hardlink = root / "hardlink.py"
        try:
            os.link(regular, hardlink)
        except OSError:
            pass
        else:
            try:
                read_stable_regular(regular)
            except BootstrapError:
                pass
            else:
                failures.append("multiply-linked script did not fail closed")
            hardlink.unlink()

        link = root / "link.py"
        try:
            link.symlink_to(regular)
        except (OSError, NotImplementedError):
            pass
        else:
            try:
                read_stable_regular(link)
            except BootstrapError:
                pass
            else:
                failures.append("script symlink did not fail closed")
    return failures


def render_result(
    *,
    core: dict[str, Any] | None,
    trust: dict[str, Any] | None,
    problems: list[str],
    bootstrap_problems: list[str],
    preflight_problems: list[str],
    core_executed: bool,
) -> dict[str, Any]:
    trigger = ROOT / TRIGGER_PATH
    secondary_markers = secondary_freeze_markers() if not preflight_problems else []
    for marker in secondary_markers:
        problems.append(
            "non-authoritative qualification freeze marker remains: "
            f"{marker}; use {TRIGGER_PATH} only"
        )
    if not preflight_problems and (trigger.is_symlink() or not trigger.is_file()):
        problems.append(f"missing sole candidate freeze authority: {TRIGGER_PATH}")

    result: dict[str, Any] = dict(core or {})
    result["schema"] = "cex.p0-release-candidate-hygiene.v3"
    result["status"] = "failed" if problems else "ok"
    result["ok"] = not problems
    result["problems"] = problems
    result["bootstrap"] = {
        "status": (
            "ok"
            if not bootstrap_problems and not preflight_problems
            else "failed"
        ),
        "self_test_problems": bootstrap_problems,
        "git_preflight_problems": preflight_problems,
        "trust_executed_before_core": True,
        "core_executed": core_executed,
    }
    result["workflow_trust"] = {
        "status": trust.get("status") if trust else "not_executed",
        "workflow_count": trust.get("workflow_count") if trust else None,
        "local_action_descriptor_count": (
            trust.get("local_action_descriptor_count") if trust else None
        ),
    }
    result["candidate_trigger_authority"] = {
        "path": TRIGGER_PATH,
        "sole_authority": (
            not preflight_problems
            and trigger.is_file()
            and not trigger.is_symlink()
            and not secondary_markers
        ),
        "secondary_freeze_markers": secondary_markers,
    }
    if result.get("commit_sha") is None and trust:
        result["commit_sha"] = trust.get("commit_sha")
    if result.get("tree_sha") is None and trust:
        result["tree_sha"] = trust.get("tree_sha")
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        help="optional no-follow JSON output path for workflow evidence",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run only the repository-independent bootstrap regressions",
    )
    args = parser.parse_args()

    bootstrap_problems = bootstrap_self_test()
    if args.self_test:
        result = {
            "schema": "cex.p0-hygiene-bootstrap-self-test.v1",
            "status": "failed" if bootstrap_problems else "ok",
            "ok": not bootstrap_problems,
            "problems": bootstrap_problems,
        }
        print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
        return 1 if bootstrap_problems else 0

    preflight_problems = git_preflight_problems()
    problems = [*bootstrap_problems, *preflight_problems]
    trust: dict[str, Any] | None = None
    core: dict[str, Any] | None = None
    core_executed = False
    trust_passed = False

    if not problems:
        try:
            trust_code, trust, trust_raw = run_json(TRUST)
        except BootstrapError as error:
            problems.append(f"cannot execute workflow trust check safely: {error}")
        else:
            trust_problems = payload_problems(
                "workflow trust check", trust_code, trust, trust_raw
            )
            problems.extend(trust_problems)
            trust_passed = not trust_problems
            if trust_passed:
                try:
                    core_code, core, core_raw = run_json(CORE)
                    core_executed = True
                except BootstrapError as error:
                    problems.append(f"cannot execute candidate hygiene core safely: {error}")
                else:
                    problems.extend(
                        payload_problems(
                            "candidate hygiene core", core_code, core, core_raw
                        )
                    )
            else:
                problems.append(
                    "candidate hygiene core was not executed because workflow trust did not pass"
                )

    result = render_result(
        core=core,
        trust=trust,
        problems=problems,
        bootstrap_problems=bootstrap_problems,
        preflight_problems=preflight_problems,
        core_executed=core_executed,
    )
    if args.output is not None:
        if not trust_passed:
            print(
                "refusing to load repository output helpers before workflow trust passes",
                file=sys.stderr,
            )
            print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
            return 1
        output = args.output if args.output.is_absolute() else Path.cwd() / args.output
        try:
            write_result_nofollow(output, result)
        except BootstrapError as error:
            print(f"cannot safely write candidate hygiene evidence: {error}", file=sys.stderr)
            return 1
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if result["problems"] else 0


if __name__ == "__main__":
    raise SystemExit(main())

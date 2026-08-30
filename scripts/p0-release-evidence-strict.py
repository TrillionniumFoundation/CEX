#!/usr/bin/env python3
"""Strict wrapper around the v12 P0 evidence collector and manifest generator.

The evidence core remains the implementation source for SBOM and provenance;
the hosted-gate checker is the sole latest-run selector and its frozen result
is injected into the core collection pass.  This wrapper adds the final
fail-closed controls
that must not be optional: exact-attempt job execution verification, exact-tree
binding of governance evidence, and first-class manifest entries for both.
Hosted job attestation is derived from the verifier's frozen context; it never
performs a second latest-run selection.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import subprocess
import sys
import types
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))
from evidence_safe_io import (  # noqa: E402
    SafeIOError,
    nofollow_supported,
    prepare_collect_targets,
    read_json_nofollow,
    read_regular_nofollow,
    sha256_file_nofollow,
    validate_directory_tree,
    write_json_nofollow,
)

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/p0-release-evidence-core.py"
VERIFY_EXECUTION = ROOT / "scripts/verify-hosted-run-execution.py"
LOCAL_BINDER = ROOT / "scripts/bind-p0-local-evidence.py"
HOSTED_CHECKER = ROOT / "scripts/check-hosted-gate-execution.py"
# The hosted checker is the sole latest-run selector; the core consumes its
# frozen result through the in-process collection seam below.
LATEST_RUN_POLICY = "latest_authoritative_run_is_binding"
HOSTED_GATE_SELECTION_SCHEMA = "cex.hosted-gate-selection-binding.v1"
BASELINE_VALIDATOR = ROOT / "scripts/check-release-baseline-manifest.py"
STRICT_CONTRACT = ROOT / "scripts/check-release-evidence-contract.py"
PAYLOAD_ONLY_ATTESTATIONS = (
    "repository-governance.json",
    "hosted-run-execution.json",
)
ATTESTATION_EVIDENCE = {
    "local-evidence-binding": "local-evidence-binding.json",
    "hosted-gate-execution": "hosted-gate-execution.json",
}
CANONICAL_EVIDENCE_ORDER = (
    "hosted:p0-migration-gate",
    "hosted:rust-service-gate",
    "hosted:p0-gateway-exact-reserve-gate",
    "hosted:p0-execution-settlement-gate",
    "hosted:p0-provider-reconciliation-gate",
    "candidate-hygiene",
    "repository-integrity",
    "hepta-postgres-integration",
    "migration-and-lifecycle-matrix",
    "exact-ledger-soak",
    "backup-restore",
    "local-evidence-binding",
    "hosted-gate-execution",
)
FORBIDDEN_SPLIT_BRAIN_EVIDENCE = {
    "repository-governance",
    "hosted-run-execution",
}
PAYLOAD_LOCK_SCHEMA = "cex.p0-release-evidence-payload-lock.v1"
PAYLOAD_LOCK_FIELDS = {
    "schema",
    "repository",
    "branch",
    "commit_sha",
    "tree_sha",
    "workflow_run_id",
    "workflow_run_attempt",
    "payload_name",
    "generated_at",
    "files",
    "files_sha256",
    "frozen_at",
}
_PAYLOAD_CONTRACT: Any | None = None


def load_core_module() -> Any:
    """Load the evidence core for one in-process frozen collection pass."""

    module_name = "cex_p0_release_evidence_core_frozen"
    sys.modules.pop(module_name, None)
    try:
        # Compile and execute the exact bytes returned by the no-follow read.
        # Loading through ``spec_from_file_location`` would open CORE a second
        # time and leave a check/use window in which a replaced path could
        # change the selector implementation after validation.
        source = read_regular_nofollow(CORE)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error
    module = types.ModuleType(module_name)
    module.__file__ = str(CORE)
    module.__package__ = ""
    sys.modules[module_name] = module
    try:
        code = compile(source, str(CORE), "exec", dont_inherit=True)
        exec(code, module.__dict__)
    except (SyntaxError, TypeError, ValueError) as error:
        sys.modules.pop(module_name, None)
        raise SystemExit(f"cannot load immutable P0 release-evidence core: {error}") from error
    return module


def require_positive_int(value: Any, label: str) -> int:
    if type(value) is not int or value < 1:
        raise SystemExit(f"{label} must be a strict positive integer")
    return value


def frozen_runs_from_attestation(
    path: Path,
    *,
    repository: str,
    branch: str,
    sha: str,
    tree: str,
) -> dict[str, dict[str, Any]]:
    """Extract the exact run set selected by the hosted checker.

    The normal hosted checker is the sole latest-run selector.  Its attestation
    is treated as untrusted input until every run identity and gate map is
    checked, then injected into the core collector in-process.
    """

    payload = read_json(path, "hosted gate execution selection")
    if not isinstance(payload, dict):
        raise SystemExit("hosted gate execution selection must be an object")
    if payload.get("schema") != "cex.hosted-gate-execution.v1":
        raise SystemExit("hosted gate execution selection schema is invalid")
    if payload.get("status") != "ok" or payload.get("ok") is not True:
        raise SystemExit("hosted gate execution selection is not successful")
    for field, expected in (
        ("repository", repository),
        ("branch", branch),
        ("commit_sha", sha),
        ("tree_sha", tree),
        ("selection_policy", LATEST_RUN_POLICY),
    ):
        if payload.get(field) != expected:
            raise SystemExit(f"hosted gate execution selection {field} differs from candidate")

    gates = payload.get("gates")
    if not isinstance(gates, dict):
        raise SystemExit("hosted gate execution selection lacks a gate map")
    expected_paths = set(payload_contract().HOSTED_WORKFLOW_PATHS.values())
    if set(gates) != expected_paths:
        raise SystemExit(
            "hosted gate execution selection workflow set mismatch: "
            f"expected={sorted(expected_paths)!r} actual={sorted(gates)!r}"
        )

    selected: dict[str, dict[str, Any]] = {}
    for gate_name, workflow_path in payload_contract().HOSTED_WORKFLOW_PATHS.items():
        summary = gates.get(workflow_path)
        if not isinstance(summary, dict):
            raise SystemExit(f"hosted gate execution selection is invalid: {workflow_path}")
        run_id = require_positive_int(summary.get("run_id"), f"{workflow_path}.run_id")
        run_attempt = require_positive_int(
            summary.get("run_attempt"), f"{workflow_path}.run_attempt"
        )
        for field, expected in (
            ("selection_policy", LATEST_RUN_POLICY),
            ("event", None),
            ("head_branch", branch),
            ("head_sha", sha),
            ("status", "completed"),
            ("conclusion", "success"),
        ):
            if field == "event":
                if summary.get(field) not in {"push", "workflow_dispatch"}:
                    raise SystemExit(f"{workflow_path}.event is not authoritative")
            elif summary.get(field) != expected:
                raise SystemExit(f"{workflow_path}.{field} is not bound to candidate")
        jobs = summary.get("jobs")
        if not isinstance(jobs, list) or not jobs or any(not isinstance(job, dict) for job in jobs):
            raise SystemExit(f"{workflow_path} lacks validated non-empty jobs")
        selected[gate_name] = {
            "id": run_id,
            "run_attempt": run_attempt,
            "path": workflow_path,
            "event": summary.get("event"),
            "head_branch": summary.get("head_branch"),
            "head_sha": summary.get("head_sha"),
            "status": summary.get("status"),
            "conclusion": summary.get("conclusion"),
            "created_at": summary.get("created_at"),
            "updated_at": summary.get("updated_at"),
            "html_url": f"https://github.com/{repository}/actions/runs/{run_id}",
        }
    return selected


def sha256_file(path: Path) -> str:
    try:
        return sha256_file_nofollow(path)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error


def rooted(path: Path) -> Path:
    return path if path.is_absolute() else ROOT / path


def read_json(path: Path, label: str = "JSON") -> Any:
    try:
        return read_json_nofollow(path, label=label)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error


def payload_contract() -> Any:
    """Load the contract helpers used for the shared payload allow-list."""

    global _PAYLOAD_CONTRACT
    if _PAYLOAD_CONTRACT is None:
        try:
            source = read_regular_nofollow(STRICT_CONTRACT)
        except SafeIOError as error:
            raise SystemExit(f"cannot load strict payload contract: {error}") from error
        module_name = "cex_release_evidence_payload_contract"
        sys.modules.pop(module_name, None)
        module = types.ModuleType(module_name)
        module.__file__ = str(STRICT_CONTRACT)
        module.__package__ = ""
        sys.modules[module_name] = module
        try:
            code = compile(source, str(STRICT_CONTRACT), "exec", dont_inherit=True)
            exec(code, module.__dict__)
        except (SyntaxError, TypeError, ValueError) as error:
            sys.modules.pop(module_name, None)
            raise SystemExit(f"cannot load strict payload contract: {error}") from error
        _PAYLOAD_CONTRACT = module
    return _PAYLOAD_CONTRACT


def canonical_payload_files() -> set[str]:
    return set(payload_contract().CANONICAL_PAYLOAD_FILES)


def canonical_files_digest(files: dict[str, str]) -> str:
    encoded = json.dumps(files, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def payload_file_map(evidence_dir: Path, *, require_index: bool) -> dict[str, str]:
    """Hash only the canonical payload namespace and reject all other paths."""

    if evidence_dir.is_symlink():
        raise SystemExit("strict evidence directory must not be a symlink")
    root = evidence_dir.absolute()
    if not root.is_dir() or root.is_symlink():
        raise SystemExit("strict evidence directory is not a real directory")
    try:
        validate_directory_tree(root)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error
    contract = payload_contract()
    canonical = canonical_payload_files()
    expected = canonical if require_index else canonical - {"payload-index.json"}
    canonical_directories = set(contract.CANONICAL_PAYLOAD_DIRECTORIES)
    actual: set[str] = set()
    for path in root.rglob("*"):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            raise SystemExit(f"strict evidence path is a symlink: {relative}")
        if path.is_dir():
            if relative not in canonical_directories:
                raise SystemExit(f"strict evidence directory is not canonical: {relative}")
            continue
        if not path.is_file() or relative not in canonical:
            raise SystemExit(f"strict evidence path is not canonical: {relative}")
        try:
            contract.reject_secret_like_payload(
                relative,
                read_regular_nofollow(path),
                f"$evidence.files.{relative}",
            )
        except (contract.ContractError, SafeIOError) as error:
            raise SystemExit(str(error)) from error
        # The core collector writes an initial index before the strict wrapper
        # adds governance/execution attestations.  Refresh intentionally
        # replaces that index; do not count the stale copy as an extra file.
        if relative == "payload-index.json" and not require_index:
            continue
        actual.add(relative)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        raise SystemExit(
            "strict evidence payload file set is not canonical "
            f"(missing={missing!r}, extra={extra!r})"
        )
    return {relative: sha256_file(root / relative) for relative in sorted(actual)}


def validate_payload_lock(
    evidence_dir: Path, context: dict[str, Any], lock_path: Path
) -> dict[str, str]:
    """Verify the pre-upload immutable snapshot against the current checkout."""

    if evidence_dir.is_symlink():
        raise SystemExit("strict evidence directory must not be a symlink")
    if lock_path.is_symlink():
        raise SystemExit("strict payload lock must not be a symlink")
    evidence_root = evidence_dir.absolute()
    lock_path = lock_path.absolute()
    if lock_path == evidence_root or evidence_root in lock_path.parents:
        raise SystemExit("strict payload lock must be outside evidence directory")
    if lock_path.is_symlink() or not lock_path.is_file():
        raise SystemExit(f"strict payload lock is missing or symlinked: {lock_path}")
    lock = read_json(lock_path, "strict payload lock")
    if not isinstance(lock, dict):
        raise SystemExit("strict payload lock must be an object")
    unknown = sorted(set(lock) - PAYLOAD_LOCK_FIELDS)
    if unknown:
        raise SystemExit(f"strict payload lock contains unknown fields: {unknown}")
    required = PAYLOAD_LOCK_FIELDS
    missing = sorted(required - set(lock))
    if missing:
        raise SystemExit(f"strict payload lock is missing fields: {missing}")
    if lock.get("schema") != PAYLOAD_LOCK_SCHEMA:
        raise SystemExit("strict payload lock schema is invalid")
    for field in (
        "repository",
        "branch",
        "commit_sha",
        "tree_sha",
        "workflow_run_id",
        "workflow_run_attempt",
        "payload_name",
        "generated_at",
    ):
        if lock.get(field) != context.get(field):
            raise SystemExit(f"strict payload lock {field} differs from context")
    contract = payload_contract()
    files = lock.get("files")
    try:
        contract.validate_payload_file_names(files, "$payload_lock.files")
    except contract.ContractError as error:
        raise SystemExit(str(error)) from error
    try:
        contract.utc_timestamp(lock.get("frozen_at"), "$payload_lock.frozen_at")
    except contract.ContractError as error:
        raise SystemExit(str(error)) from error
    if lock.get("files_sha256") != canonical_files_digest(files):
        raise SystemExit("strict payload lock aggregate digest is invalid")
    current = payload_file_map(evidence_root, require_index=True)
    if current != files:
        raise SystemExit("strict evidence payload changed after freeze/upload")
    context_files = context.get("files")
    if context_files != files:
        raise SystemExit("strict payload lock files differ from context")
    # Freeze is a process boundary as well as a digest boundary.  A writable
    # indexed file would allow a later step to diverge from the bytes uploaded
    # by actions/upload-artifact, so fail closed if permissions were relaxed.
    if evidence_root.stat().st_mode & 0o222:
        raise SystemExit("strict evidence payload directory is writable after freeze")
    for path in evidence_root.rglob("*"):
        if path.is_symlink():
            raise SystemExit(f"strict evidence path became a symlink: {path}")
        if path.is_dir() and path.stat().st_mode & 0o222:
            raise SystemExit(f"strict evidence payload directory is writable after freeze: {path}")
        if path.is_file() and path.stat().st_mode & 0o222:
            raise SystemExit(f"strict evidence payload is writable after freeze: {path}")
    return files


def freeze_payload(evidence_dir: Path, context_path: Path, lock_path: Path) -> None:
    """Create a digest-bound lock and make the uploaded payload read-only."""

    if not nofollow_supported():
        raise SystemExit(
            "strict evidence payload freezing requires POSIX O_NOFOLLOW/O_DIRECTORY"
        )

    if evidence_dir.is_symlink():
        raise SystemExit("strict evidence directory must not be a symlink")
    if lock_path.is_symlink():
        raise SystemExit("strict payload lock path must not be a symlink")
    evidence_dir = evidence_dir.absolute()
    context_path = context_path.absolute()
    lock_path = lock_path.absolute()
    if evidence_dir == lock_path or evidence_dir in lock_path.parents:
        raise SystemExit("strict payload lock must be outside evidence directory")
    context = read_json(context_path, "strict release context")
    if not isinstance(context, dict):
        raise SystemExit("strict release context must be an object")
    files = payload_file_map(evidence_dir, require_index=True)
    try:
        payload_contract().validate_payload_file_names(context.get("files"), "$context.files")
    except payload_contract().ContractError as error:
        raise SystemExit(str(error)) from error
    if context.get("files") != files:
        raise SystemExit("strict release context does not match payload bytes before freeze")
    lock = {
        "schema": PAYLOAD_LOCK_SCHEMA,
        **{
            field: context.get(field)
            for field in (
                "repository",
                "branch",
                "commit_sha",
                "tree_sha",
                "workflow_run_id",
                "workflow_run_attempt",
                "payload_name",
                "generated_at",
            )
        },
        "files": files,
        "files_sha256": canonical_files_digest(files),
        "frozen_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }
    write_json(lock_path, lock)
    # Refuse symlinks before changing permissions, then remove write bits from
    # every file and directory so the upload and manifest steps see one stable
    # byte set.  The lock itself is frozen too.
    for path in sorted(evidence_dir.rglob("*"), key=lambda item: len(item.parts), reverse=True):
        if path.is_symlink():
            raise SystemExit(f"strict evidence path is a symlink: {path}")
        mode = path.stat().st_mode
        path.chmod(mode & ~0o222)
    # The root directory itself must be immutable too.  Otherwise a later
    # step (or an untrusted tool invoked by the workflow) can replace a file
    # after the lock is written but before upload-artifact snapshots it.
    evidence_dir.chmod(evidence_dir.stat().st_mode & ~0o222)
    lock_path.chmod(lock_path.stat().st_mode & ~0o222)
    verify_payload_lock(evidence_dir, context, lock_path)


def verify_payload_lock(evidence_dir: Path, context: dict[str, Any], lock_path: Path) -> dict[str, str]:
    return validate_payload_lock(evidence_dir, context, lock_path)


def require_clean_checkout(context: dict[str, Any]) -> None:
    """Re-check the exact checkout immediately before final qualification."""

    try:
        commit = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        tree = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD^{tree}"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        tracked_dirty = subprocess.run(
            ["git", "-C", str(ROOT), "diff", "--quiet"],
            check=False,
        ).returncode
        staged_dirty = subprocess.run(
            ["git", "-C", str(ROOT), "diff", "--cached", "--quiet"],
            check=False,
        ).returncode
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"cannot verify final checkout identity: {error}") from error
    if commit != context.get("commit_sha") or tree != context.get("tree_sha"):
        raise SystemExit("final checkout commit/tree differs from release context")
    if tracked_dirty != 0 or staged_dirty != 0:
        raise SystemExit("tracked worktree changes exist during strict manifest qualification")


def run_core(arguments: list[str]) -> None:
    subprocess.run([sys.executable, str(CORE), *arguments], cwd=ROOT, check=True)


def run_core_collect_frozen(
    arguments: list[str],
    *,
    frozen_runs: dict[str, dict[str, Any]],
    repository: str,
    branch: str,
    sha: str,
) -> None:
    """Run core collection with exactly one injected hosted-run snapshot.

    The hosted checker has already selected and job-validated the latest
    authoritative runs.  Calling the core as a subprocess would let it query
    GitHub again and select a different snapshot, so load it in-process and
    replace only its selector for this one invocation.  The original function
    is restored even when collection fails.
    """

    expected_gates = set(payload_contract().HOSTED_WORKFLOW_PATHS)
    if set(frozen_runs) != expected_gates:
        raise SystemExit("frozen hosted-run set does not match authoritative gates")
    core = load_core_module()
    parser_factory = getattr(core, "build_parser", None)
    original_selector = getattr(core, "collect_gate_runs", None)
    if not callable(parser_factory) or not callable(original_selector):
        raise SystemExit("immutable core lacks its parser or hosted-run selector")
    parsed = parser_factory().parse_args(arguments)
    if getattr(parsed, "command", None) != "collect":
        raise SystemExit("frozen core execution is permitted only for collect")

    calls = 0
    expected_branch = branch

    def frozen_selector(
        selected_repository: str,
        selected_sha: str,
        _token: str,
        attempts: int,
        interval_seconds: int,
        *,
        branch: str,
    ) -> dict[str, dict[str, Any]]:
        nonlocal calls
        calls += 1
        if calls != 1:
            raise SystemExit("immutable core attempted to select hosted runs more than once")
        if selected_repository != repository:
            raise SystemExit("core selector repository differs from candidate")
        if selected_sha != sha:
            raise SystemExit("core selector commit differs from candidate")
        if branch != parsed.branch or branch != expected_branch:
            raise SystemExit("core selector branch differs from candidate")
        if type(attempts) is not int or attempts < 1:
            raise SystemExit("core selector polling attempts are invalid")
        if type(interval_seconds) is not int or interval_seconds < 1:
            raise SystemExit("core selector polling interval is invalid")
        return copy.deepcopy(frozen_runs)

    core.collect_gate_runs = frozen_selector
    try:
        result = int(parsed.function(parsed))
    finally:
        core.collect_gate_runs = original_selector
    if calls != 1:
        raise SystemExit(
            f"immutable core consumed the frozen hosted-run snapshot {calls} times"
        )
    if result != 0:
        raise SystemExit(f"frozen core collection failed with status {result}")


def validate_manifest(path: Path, context_path: Path, evidence_dir: Path) -> None:
    subprocess.run(
        [sys.executable, str(BASELINE_VALIDATOR), str(path)],
        cwd=ROOT,
        check=True,
    )
    # The baseline checker enforces the public shape.  The strict checker also
    # binds every URI/digest to the final collector context and re-hashes the
    # uploaded payload files, so a swapped run or post-upload mutation cannot
    # qualify merely because it still has a valid-looking URI.
    subprocess.run(
        [
            sys.executable,
            str(STRICT_CONTRACT),
            str(path),
            "--context",
            str(context_path),
            "--evidence-dir",
            str(evidence_dir),
        ],
        cwd=ROOT,
        check=True,
    )


def write_json(path: Path, value: Any) -> None:
    try:
        write_json_nofollow(path, value)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error


def refresh_payload_index(evidence_dir: Path, context_path: Path) -> None:
    """Re-index files added after the core collector wrote its context.

    Governance and exact-job attestations are deliberately collected after the
    core payload.  Re-running the collector would perform another mutable API
    selection and could mix two different run snapshots.  Instead, update the
    local payload index and context atomically from the already verified files.
    """

    if not nofollow_supported():
        raise SystemExit(
            "strict evidence payload re-indexing requires POSIX O_NOFOLLOW/O_DIRECTORY"
        )

    context = read_json(context_path, "strict release context")
    if not isinstance(context, dict):
        raise SystemExit("strict release context must be an object")
    files = payload_file_map(evidence_dir, require_index=False)

    payload_index = {
        "schema": "cex.p0-release-evidence-payload.v1",
        "repository": context.get("repository"),
        "branch": context.get("branch"),
        "commit_sha": context.get("commit_sha"),
        "tree_sha": context.get("tree_sha"),
        "workflow_run_id": context.get("workflow_run_id"),
        "workflow_run_attempt": context.get("workflow_run_attempt"),
        "payload_name": context.get("payload_name"),
        "generated_at": context.get("generated_at"),
        "files": files,
    }
    payload_index_path = evidence_dir / "payload-index.json"
    write_json(payload_index_path, payload_index)
    files["payload-index.json"] = sha256_file(payload_index_path)
    # Re-scan after writing the index so an unexpected file, symlink or
    # secret-like payload cannot be hidden by a self-consistent context.
    files = payload_file_map(evidence_dir, require_index=True)
    context["files"] = files
    write_json(context_path, context)


def bind_hosted_gate_selection(evidence_dir: Path, context_path: Path) -> None:
    """Bind the selected run IDs and attestation digest into the context.

    ``check-hosted-gate-execution.py`` consumes the core's frozen context and
    emits a job-level attestation.  Keep an explicit, machine-readable binding
    between that attestation and the run IDs in the context so later freshness
    checks cannot accidentally validate an unconnected run set.
    """

    context = read_json(context_path, "strict release context")
    hosted_path = evidence_dir / ATTESTATION_EVIDENCE["hosted-gate-execution"]
    hosted = read_json(hosted_path, "hosted gate execution attestation")
    if not isinstance(context, dict) or not isinstance(hosted, dict):
        raise SystemExit("hosted selection binding inputs must be JSON objects")
    if hosted.get("schema") != "cex.hosted-gate-execution.v1":
        raise SystemExit("hosted gate execution attestation schema is invalid")
    if hosted.get("status") != "ok" or hosted.get("ok") is not True:
        raise SystemExit("hosted gate execution attestation is not successful")
    for field in ("repository", "branch", "commit_sha", "tree_sha"):
        if hosted.get(field) != context.get(field):
            raise SystemExit(f"hosted gate execution {field} differs from context")
    if hosted.get("selection_policy") != LATEST_RUN_POLICY:
        raise SystemExit("hosted gate execution selection policy is invalid")

    context_gates = context.get("hosted_gates")
    attested_gates = hosted.get("gates")
    if not isinstance(context_gates, dict) or not isinstance(attested_gates, dict):
        raise SystemExit("hosted selection binding requires context and attestation gate maps")
    expected_paths: set[str] = set()
    for gate_name, record in context_gates.items():
        if not isinstance(gate_name, str) or not isinstance(record, dict):
            raise SystemExit(f"release context hosted gate is invalid: {gate_name!r}")
        workflow_path = record.get("workflow_path")
        if not isinstance(workflow_path, str) or not workflow_path:
            raise SystemExit(f"release context hosted gate lacks workflow path: {gate_name}")
        expected_paths.add(workflow_path)
        summary = attested_gates.get(workflow_path)
        if not isinstance(summary, dict):
            raise SystemExit(f"hosted gate execution attestation lacks workflow: {workflow_path}")
        for field in (
            "run_id",
            "run_attempt",
            "event",
            "head_branch",
            "head_sha",
            "status",
            "conclusion",
            "created_at",
            "updated_at",
        ):
            if summary.get(field) != record.get(field):
                raise SystemExit(
                    f"hosted gate execution {workflow_path}.{field} differs from context"
                )
        jobs = summary.get("jobs")
        if not isinstance(jobs, list) or not jobs:
            raise SystemExit(f"hosted gate execution attestation has no jobs: {workflow_path}")
    if set(attested_gates) != expected_paths:
        raise SystemExit("hosted gate execution workflow set differs from context")

    files = context.get("files")
    if not isinstance(files, dict):
        raise SystemExit("release context files map is missing")
    attestation_relative = ATTESTATION_EVIDENCE["hosted-gate-execution"]
    attestation_digest = sha256_file(hosted_path)
    if files.get(attestation_relative) != attestation_digest:
        raise SystemExit("hosted gate execution digest is not bound to context files")
    selected_run_ids: dict[str, int] = {}
    for gate_name, record in context_gates.items():
        run_id = record.get("run_id")
        if type(run_id) is not int or run_id < 1:
            raise SystemExit(f"release context hosted gate run id is invalid: {gate_name}")
        selected_run_ids[gate_name] = run_id
    context["hosted_gate_selection"] = {
        "schema": HOSTED_GATE_SELECTION_SCHEMA,
        "policy": LATEST_RUN_POLICY,
        "source": attestation_relative,
        "sha256": attestation_digest,
        "selected_run_ids": selected_run_ids,
    }
    write_json(context_path, context)


def collect(args: argparse.Namespace) -> int:
    repo_root = rooted(args.repo_root)
    evidence_dir = rooted(args.evidence_dir)
    context_path = rooted(args.context)
    # Establish a no-follow boundary before any producer is allowed to write.
    # The core and the three attestation producers all operate in this same
    # directory; checking it once up front catches symlink/FIFO substitutions
    # while the later safe hashes close the remaining check/use window.
    try:
        prepare_collect_targets(
            evidence_dir,
            context_path,
            output_files=(
                context_path,
                evidence_dir / "repository-governance.json",
                evidence_dir / "hosted-run-execution.json",
                evidence_dir / ATTESTATION_EVIDENCE["local-evidence-binding"],
                evidence_dir / ATTESTATION_EVIDENCE["hosted-gate-execution"],
            ),
            output_directories=(evidence_dir / "hosted-gates",),
        )
    except SafeIOError as error:
        raise SystemExit(str(error)) from error
    forwarded = [
        "collect",
        "--repo-root", str(repo_root),
        "--evidence-dir", str(evidence_dir),
        "--context", str(context_path),
        "--repository", args.repository,
        "--branch", args.branch,
        "--sha", args.sha,
        "--tree", args.tree,
        "--run-id", str(args.run_id),
        "--run-attempt", str(args.run_attempt),
        "--server-url", args.server_url,
    ]

    # Select and job-validate the latest authoritative run set exactly once.
    # The attestation is then injected into the core in-process; invoking the
    # core as a subprocess here would reopen a second mutable GitHub selection
    # window and could mix two different run snapshots.
    selection_path = evidence_dir / ATTESTATION_EVIDENCE["hosted-gate-execution"]
    hosted_selection_command = [
        sys.executable,
        str(HOSTED_CHECKER),
        "--repository", args.repository,
        "--branch", args.branch,
        "--sha", args.sha,
        "--tree", args.tree,
        "--output", str(selection_path),
    ]
    subprocess.run(
        hosted_selection_command,
        cwd=ROOT,
        check=True,
        env=os.environ.copy(),
    )
    frozen_runs = frozen_runs_from_attestation(
        selection_path,
        repository=args.repository,
        branch=args.branch,
        sha=args.sha,
        tree=args.tree,
    )
    run_core_collect_frozen(
        forwarded,
        frozen_runs=frozen_runs,
        repository=args.repository,
        branch=args.branch,
        sha=args.sha,
    )

    governance_path = evidence_dir / "repository-governance.json"
    governance = read_json(governance_path, "repository governance evidence")
    if not isinstance(governance, dict) or governance.get("ok") is not True:
        raise SystemExit("repository governance evidence is not successful")
    if governance.get("repository") != args.repository:
        raise SystemExit("repository governance evidence is bound to another repository")
    if governance.get("candidate_branch") != args.branch:
        raise SystemExit("repository governance evidence is bound to another branch")
    if governance.get("commit_sha") != args.sha:
        raise SystemExit("repository governance evidence is bound to another commit")
    if governance.get("candidate_commit_matches_branch") is not True:
        raise SystemExit("repository governance did not confirm the candidate branch commit")
    existing_tree = governance.get("tree_sha")
    if not isinstance(existing_tree, str) or not existing_tree:
        raise SystemExit("repository governance evidence lacks an exact tree")
    if existing_tree != args.tree:
        raise SystemExit("repository governance evidence is bound to another tree")
    # Re-emit through the no-follow writer so the bytes subsequently indexed
    # by the context are the exact semantically checked object.
    write_json(governance_path, governance)

    execution_path = evidence_dir / "hosted-run-execution.json"
    subprocess.run(
        [
            sys.executable,
            str(VERIFY_EXECUTION),
            "--context", str(context_path),
            "--output", str(execution_path),
        ],
        cwd=ROOT,
        check=True,
        env=os.environ.copy(),
    )

    binder_command = [
        sys.executable,
        str(LOCAL_BINDER),
        "--evidence-dir", str(evidence_dir),
        "--repository", args.repository,
        "--branch", args.branch,
        "--sha", args.sha,
        "--tree", args.tree,
        "--run-id", str(args.run_id),
        "--run-attempt", str(args.run_attempt),
        "--output", str(evidence_dir / ATTESTATION_EVIDENCE["local-evidence-binding"]),
    ]
    subprocess.run(binder_command, cwd=ROOT, check=True, env=os.environ.copy())

    hosted_command = [
        sys.executable,
        str(HOSTED_CHECKER),
        "--repository", args.repository,
        "--branch", args.branch,
        "--sha", args.sha,
        "--tree", args.tree,
        "--context", str(context_path),
        "--execution", str(execution_path),
        "--output", str(evidence_dir / ATTESTATION_EVIDENCE["hosted-gate-execution"]),
    ]
    subprocess.run(hosted_command, cwd=ROOT, check=True, env=os.environ.copy())

    refresh_payload_index(evidence_dir, context_path)
    context = read_json(context_path, "strict release context")
    files = context.get("files") if isinstance(context, dict) else None
    if not isinstance(files, dict):
        raise SystemExit("strict release context lacks a files index")
    payload_only: dict[str, str] = {}
    for relative in PAYLOAD_ONLY_ATTESTATIONS:
        path = evidence_dir / relative
        expected = sha256_file(path)
        if files.get(relative) != expected:
            raise SystemExit(f"strict release context did not index {relative}")
        payload_only[relative] = expected
    attestations: dict[str, dict[str, str]] = {}
    for name, relative in ATTESTATION_EVIDENCE.items():
        path = evidence_dir / relative
        expected = sha256_file(path)
        if files.get(relative) != expected:
            raise SystemExit(f"strict release context did not index {relative}")
        attestations[name] = {"path": relative, "sha256": expected}
    context["payload_only_attestations"] = payload_only
    context["attestations"] = attestations
    # Persist the two canonical binding maps before the selection helper
    # re-reads the context.  Without this write, the helper would preserve
    # only its new selection field and silently discard both maps.
    write_json(context_path, context)
    bind_hosted_gate_selection(evidence_dir, context_path)
    # bind_hosted_gate_selection re-reads and writes the context after adding
    # the selection binding; do not overwrite that final object with the
    # pre-binding local variable.
    print(context_path)
    return 0


def freeze(args: argparse.Namespace) -> int:
    evidence_dir = rooted(args.evidence_dir)
    context_path = rooted(args.context)
    lock_path = rooted(args.payload_lock)
    if not context_path.is_file():
        raise SystemExit(f"strict release context is missing: {context_path}")
    if not evidence_dir.is_dir():
        raise SystemExit(f"strict release evidence directory is missing: {evidence_dir}")
    freeze_payload(evidence_dir, context_path, lock_path)
    print(lock_path)
    return 0


def manifest(args: argparse.Namespace) -> int:
    context_path = rooted(args.context)
    output_path = rooted(args.output)
    evidence_dir = rooted(args.evidence_dir)
    payload_lock = rooted(args.payload_lock)
    if not context_path.is_file():
        raise SystemExit(f"strict release context is missing: {context_path}")
    if not evidence_dir.is_dir():
        raise SystemExit(f"strict release evidence directory is missing: {evidence_dir}")
    payload_digest = args.payload_digest.strip().lower()
    if not payload_digest.startswith("sha256:"):
        payload_digest = "sha256:" + payload_digest
    if len(payload_digest) != len("sha256:") + 64 or any(
        character not in "0123456789abcdef" for character in payload_digest[7:]
    ) or payload_digest == "sha256:" + "0" * 64:
        raise SystemExit("payload digest must be a non-placeholder SHA-256")
    context = read_json(context_path, "strict release context")
    if not isinstance(context, dict):
        raise SystemExit("strict release context must be an object")
    # The lock was created before upload-artifact and freezes the exact file
    # set/hash map that was uploaded.  Validate it before accepting the action's
    # artifact digest or generating a manifest; no post-upload collector pass
    # may rewrite an evidence file.
    validate_payload_lock(evidence_dir, context, payload_lock)
    # The artifact digest is only known after upload-artifact returns.  Bind it
    # into the collector context before any strict validation so a caller
    # cannot replace the payload with another artifact while keeping the same
    # release/run identity.
    context["payload_digest"] = payload_digest
    write_json(context_path, context)
    require_clean_checkout(context)
    forwarded = [
        "manifest",
        "--context", str(context_path),
        "--output", str(output_path),
        "--release-id", args.release_id,
        "--payload-name", args.payload_name,
        "--payload-digest", args.payload_digest,
        "--frozen-hosted-context",
    ]
    # Use the core manifest builder directly.  The older wrapper appends the
    # superseded local-evidence-binding/hosted-gate-execution pair; strict v12
    # has one canonical thirteen-entry vocabulary instead.
    run_core(forwarded)

    context = read_json(context_path, "strict release context")
    manifest_value = read_json(output_path, "generated strict manifest")
    if not isinstance(context, dict) or not isinstance(manifest_value, dict):
        raise SystemExit("strict manifest inputs must be JSON objects")
    files = context.get("files")
    evidence = manifest_value.get("evidence")
    if not isinstance(files, dict) or not isinstance(evidence, list):
        raise SystemExit("strict manifest inputs lack files/evidence")

    names = [
        item.get("name")
        for item in evidence
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    ]
    if names != list(CANONICAL_EVIDENCE_ORDER[:11]):
        raise SystemExit(
            "generated core manifest is not the expected eleven-record prefix"
        )
    if FORBIDDEN_SPLIT_BRAIN_EVIDENCE.intersection(names):
        raise SystemExit("payload-only attestations leaked into core manifest evidence")
    attestations = context.get("attestations")
    if not isinstance(attestations, dict) or set(attestations) != set(ATTESTATION_EVIDENCE):
        raise SystemExit("strict release context lacks the exact canonical attestation set")
    for name, relative in ATTESTATION_EVIDENCE.items():
        digest = files.get(relative)
        if not isinstance(digest, str):
            raise SystemExit(f"strict release context lacks digest for {relative}")
        attestation = attestations.get(name)
        if not isinstance(attestation, dict) or attestation.get("path") != relative:
            raise SystemExit(f"strict release context attestation path is invalid: {name}")
        if attestation.get("sha256") != digest:
            raise SystemExit(f"strict release context attestation digest is invalid: {name}")
        evidence.append(
            {
                "name": name,
                "status": "pass",
                "uri": f"artifact://{args.payload_name}/{relative}",
                "sha256": digest,
                "waiver": None,
            }
        )
    if [item.get("name") for item in evidence if isinstance(item, dict)] != list(CANONICAL_EVIDENCE_ORDER):
        raise SystemExit("strict manifest evidence is not the canonical ordered thirteen-record contract")
    write_json(output_path, manifest_value)
    validate_manifest(output_path, context_path, evidence_dir)
    print(output_path)
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    collect_parser = subparsers.add_parser("collect")
    collect_parser.add_argument("--repo-root", type=Path, default=ROOT)
    collect_parser.add_argument("--evidence-dir", required=True, type=Path)
    collect_parser.add_argument("--context", required=True, type=Path)
    collect_parser.add_argument("--repository", required=True)
    collect_parser.add_argument("--branch", required=True)
    collect_parser.add_argument("--sha", required=True)
    collect_parser.add_argument("--tree", required=True)
    collect_parser.add_argument("--run-id", required=True, type=int)
    collect_parser.add_argument("--run-attempt", required=True, type=int)
    collect_parser.add_argument("--server-url", required=True)
    collect_parser.set_defaults(function=collect)

    manifest_parser = subparsers.add_parser("manifest")
    manifest_parser.add_argument("--context", required=True, type=Path)
    manifest_parser.add_argument("--output", required=True, type=Path)
    manifest_parser.add_argument("--release-id", required=True)
    manifest_parser.add_argument("--payload-name", required=True)
    manifest_parser.add_argument("--payload-digest", required=True)
    manifest_parser.add_argument(
        "--evidence-dir",
        type=Path,
        required=True,
        help="payload directory to re-hash while binding the manifest",
    )
    manifest_parser.add_argument(
        "--payload-lock",
        type=Path,
        required=True,
        help="pre-upload payload lock created by the freeze command",
    )
    manifest_parser.set_defaults(function=manifest)
    freeze_parser = subparsers.add_parser("freeze")
    freeze_parser.add_argument("--context", required=True, type=Path)
    freeze_parser.add_argument("--evidence-dir", required=True, type=Path)
    freeze_parser.add_argument(
        "--payload-lock",
        required=True,
        type=Path,
        help="lock path outside the uploaded evidence directory",
    )
    freeze_parser.set_defaults(function=freeze)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    return int(args.function(args))


if __name__ == "__main__":
    raise SystemExit(main())

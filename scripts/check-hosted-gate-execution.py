#!/usr/bin/env python3
"""Creation-identity adapter for the canonical hosted-gate implementation.

The exact implementation is retained in ``check-hosted-gate-execution-impl.py``
and loaded through a no-follow read. This adapter changes only the authority
ordering seam: latest_authoritative_run_is_binding, no real runner was
allocated, build_frozen_attestation, --context and --execution must be supplied
together, and --execution disables run re-selection remain enforced by the
canonical implementation.
"""

from __future__ import annotations

import sys
from pathlib import Path

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from evidence_safe_io import SafeIOError, read_regular_nofollow  # noqa: E402
from p0_authoritative_runs import (  # noqa: E402
    authoritative_run_sort_key,
    self_test as authoritative_order_self_test,
)

_IMPL_PATH = _SCRIPT_DIR / "check-hosted-gate-execution-impl.py"
_ORIGINAL_MODULE_NAME = __name__
try:
    _SOURCE = read_regular_nofollow(_IMPL_PATH)
except SafeIOError as error:
    raise SystemExit(f"cannot load hosted-gate implementation safely: {error}") from error

globals()["__name__"] = f"{_ORIGINAL_MODULE_NAME}.__impl__"
try:
    exec(compile(_SOURCE, str(_IMPL_PATH), "exec", dont_inherit=True), globals())
finally:
    globals()["__name__"] = _ORIGINAL_MODULE_NAME


def run_sort_key(run):
    """Delegate all latest-run decisions to immutable creation identity."""

    return authoritative_run_sort_key(run)


_IMPLEMENTATION_SELF_TEST = self_test


def self_test():
    """Run canonical checks plus the shared late-finish inversion regression."""

    failures = list(_IMPLEMENTATION_SELF_TEST())
    failures.extend(authoritative_order_self_test())

    branch = "fix/authoritative-order"
    sha = "a" * 40
    runs = []
    for index, workflow_path in enumerate(REQUIRED_GATES, start=1):
        runs.append(
            {
                "id": 1000 + index,
                "run_number": 100 + index,
                "run_attempt": 1,
                "path": workflow_path,
                "head_sha": sha,
                "head_branch": branch,
                "event": "push",
                "status": "completed",
                "conclusion": "success",
                "created_at": f"2026-08-30T00:00:{index:02d}Z",
                "updated_at": f"2026-08-30T23:00:{index:02d}Z",
            }
        )

    first_path = next(iter(REQUIRED_GATES))
    older = next(item for item in runs if item["path"] == first_path)
    newer_failure = {
        **older,
        "id": 9001,
        "run_number": 901,
        "created_at": "2026-08-30T01:00:00Z",
        "updated_at": "2026-08-30T01:00:01Z",
        "conclusion": "failure",
    }
    selected, pending, terminal = latest_run_states(
        [*runs, newer_failure], branch, sha
    )
    if first_path in selected or pending or not terminal:
        failures.append("older late success masked a newer failed run")

    newer_active = {
        **newer_failure,
        "id": 9002,
        "run_number": 902,
        "created_at": "2026-08-30T02:00:00Z",
        "updated_at": "2026-08-30T02:00:01Z",
        "status": "in_progress",
        "conclusion": None,
    }
    selected, pending, terminal = latest_run_states(
        [*runs, newer_active], branch, sha
    )
    if first_path in selected or not pending or terminal:
        failures.append("older late success masked a newer active run")

    newer_success = {
        **newer_failure,
        "id": 9003,
        "run_number": 903,
        "created_at": "2026-08-30T03:00:00Z",
        "updated_at": "2026-08-30T03:00:01Z",
        "conclusion": "success",
    }
    selected, pending, terminal = latest_run_states(
        [*runs, newer_success], branch, sha
    )
    if pending or terminal or selected.get(first_path, {}).get("id") != 9003:
        failures.append("newest successful creation identity was not selected")
    return failures


if _ORIGINAL_MODULE_NAME == "__main__":
    raise SystemExit(main())

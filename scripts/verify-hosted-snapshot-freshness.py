#!/usr/bin/env python3
"""Creation-order regression adapter for hosted snapshot freshness.

The canonical implementation remains in
``verify-hosted-snapshot-freshness-impl.py`` and is loaded with
read_regular_nofollow. Static contract markers retained here:
cex.hosted-gate-selection-binding.v1, latest_authoritative_run_is_binding,
latest_run_states, paged_collection, newer-success, newer-rerun-attempt,
read_json_nofollow, read_regular_nofollow, workflow_revalidation_count.
"""

from __future__ import annotations

import sys
from pathlib import Path

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from evidence_safe_io import SafeIOError, read_regular_nofollow  # noqa: E402
from p0_authoritative_runs import self_test as authoritative_order_self_test  # noqa: E402

_IMPL_PATH = _SCRIPT_DIR / "verify-hosted-snapshot-freshness-impl.py"
_ORIGINAL_MODULE_NAME = __name__
try:
    _SOURCE = read_regular_nofollow(_IMPL_PATH)
except SafeIOError as error:
    raise SystemExit(f"cannot load snapshot freshness implementation safely: {error}") from error

globals()["__name__"] = f"{_ORIGINAL_MODULE_NAME}.__impl__"
try:
    exec(compile(_SOURCE, str(_IMPL_PATH), "exec", dont_inherit=True), globals())
finally:
    globals()["__name__"] = _ORIGINAL_MODULE_NAME

_IMPLEMENTATION_SELF_TEST = self_test


def self_test():
    """Run canonical freshness checks and explicit late-finish inversion cases."""

    failures = list(_IMPLEMENTATION_SELF_TEST())
    failures.extend(authoritative_order_self_test())
    try:
        selector = load_selector_module()
        branch = "candidate/creation-order"
        sha = "a" * 40
        runs = []
        for index, workflow_path in enumerate(WORKFLOW_PATHS.values(), start=1):
            runs.append(
                {
                    "id": 2000 + index,
                    "run_number": 200 + index,
                    "run_attempt": 1,
                    "path": workflow_path,
                    "head_branch": branch,
                    "head_sha": sha,
                    "event": "push",
                    "status": "completed",
                    "conclusion": "success",
                    "created_at": f"2026-08-30T00:00:{index:02d}Z",
                    "updated_at": f"2026-08-30T23:00:{index:02d}Z",
                }
            )
        first_path = next(iter(WORKFLOW_PATHS.values()))
        older = next(item for item in runs if item["path"] == first_path)
        newer_failure = {
            **older,
            "id": 9901,
            "run_number": 991,
            "created_at": "2026-08-30T01:00:00Z",
            "updated_at": "2026-08-30T01:00:01Z",
            "conclusion": "failure",
        }
        selected, pending, terminal = selector.latest_run_states(
            [*runs, newer_failure], branch, sha
        )
        if first_path in selected or pending or not terminal:
            failures.append("freshness accepted older late success over newer failure")

        newer_active = {
            **newer_failure,
            "id": 9902,
            "run_number": 992,
            "created_at": "2026-08-30T02:00:00Z",
            "updated_at": "2026-08-30T02:00:01Z",
            "status": "in_progress",
            "conclusion": None,
        }
        selected, pending, terminal = selector.latest_run_states(
            [*runs, newer_active], branch, sha
        )
        if first_path in selected or not pending or terminal:
            failures.append("freshness accepted older late success over newer active run")
    except Exception as error:
        failures.append(f"creation-order freshness regression crashed: {error}")
    return failures


if _ORIGINAL_MODULE_NAME == "__main__":
    raise SystemExit(main())

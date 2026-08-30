#!/usr/bin/env python3
"""Creation-identity adapter for the immutable P0 evidence core.

The canonical implementation is retained byte-for-byte in
``p0-release-evidence-core-impl.py`` and loaded through a no-follow read. The
adapter changes only ``_run_sort_key`` so direct/legacy callers cannot use
``updated_at`` to let an older late-finishing workflow outrank a newer run.
All existing repeated ``revalidate_gate_runs`` and frozen-context controls stay
inside the canonical implementation.
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

_IMPL_PATH = _SCRIPT_DIR / "p0-release-evidence-core-impl.py"
_ORIGINAL_MODULE_NAME = __name__
try:
    _SOURCE = read_regular_nofollow(_IMPL_PATH)
except SafeIOError as error:
    raise SystemExit(f"cannot load P0 evidence core safely: {error}") from error

globals()["__name__"] = f"{_ORIGINAL_MODULE_NAME}.__impl__"
try:
    exec(compile(_SOURCE, str(_IMPL_PATH), "exec", dont_inherit=True), globals())
finally:
    globals()["__name__"] = _ORIGINAL_MODULE_NAME


def _run_sort_key(run):
    """Delegate all latest-run decisions to immutable creation identity."""

    return authoritative_run_sort_key(run)


_IMPLEMENTATION_SELF_TEST = self_test


def self_test():
    failures = list(_IMPLEMENTATION_SELF_TEST())
    failures.extend(authoritative_order_self_test())
    return failures


if _ORIGINAL_MODULE_NAME == "__main__":
    raise SystemExit(main())

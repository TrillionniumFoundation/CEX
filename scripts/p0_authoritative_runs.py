#!/usr/bin/env python3
"""Shared fail-closed ordering for authoritative GitHub Actions runs.

A workflow run becomes authoritative when GitHub creates the run, not when the
run later finishes or receives another metadata update. Completion metadata
such as ``updated_at`` is therefore evidence carried by the selected run, but
it is never allowed to decide which run is newest.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any


def _non_negative_int(value: Any) -> int:
    """Return a defensive non-negative integer for secondary tie-breakers."""

    try:
        parsed = int(value or 0)
    except (TypeError, ValueError):
        return 0
    return parsed if parsed >= 0 else 0


def _created_at(value: Any) -> datetime:
    """Parse one GitHub UTC timestamp and fail closed when it is malformed."""

    if not isinstance(value, str) or not value.strip():
        raise ValueError("authoritative workflow run lacks created_at")
    raw = value.strip()
    try:
        parsed = datetime.fromisoformat(raw[:-1] + "+00:00" if raw.endswith("Z") else raw)
    except ValueError as error:
        raise ValueError("authoritative workflow run has invalid created_at") from error
    if parsed.tzinfo is None:
        raise ValueError("authoritative workflow run created_at is not timezone-aware")
    return parsed.astimezone(timezone.utc)


def authoritative_run_sort_key(run: dict[str, Any]) -> tuple[datetime, int, int, int]:
    """Order runs by immutable creation identity, never by completion time.

    ``created_at`` is the primary authority. ``run_number`` and ``id`` break
    the rare same-timestamp tie between distinct runs, while ``run_attempt``
    orders reruns of the same run. ``updated_at`` is deliberately absent.
    """

    return (
        _created_at(run.get("created_at")),
        _non_negative_int(run.get("run_number")),
        _non_negative_int(run.get("id")),
        _non_negative_int(run.get("run_attempt")),
    )


def self_test() -> list[str]:
    """Exercise the late-finish inversion and rerun tie-breakers offline."""

    failures: list[str] = []
    older = {
        "id": 100,
        "run_number": 40,
        "run_attempt": 1,
        "created_at": "2026-08-30T00:00:00Z",
        "updated_at": "2026-08-30T23:59:59Z",
    }
    newer = {
        "id": 101,
        "run_number": 41,
        "run_attempt": 1,
        "created_at": "2026-08-30T00:01:00Z",
        "updated_at": "2026-08-30T00:01:01Z",
    }
    if authoritative_run_sort_key(older) >= authoritative_run_sort_key(newer):
        failures.append("an older late-finishing run outranked a newer run")

    mutated = dict(older)
    mutated["updated_at"] = "2099-12-31T23:59:59Z"
    if authoritative_run_sort_key(mutated) != authoritative_run_sort_key(older):
        failures.append("updated_at changed authoritative run ordering")

    rerun = dict(newer)
    rerun["run_attempt"] = 2
    rerun["updated_at"] = "2026-08-30T00:10:00Z"
    if authoritative_run_sort_key(rerun) <= authoritative_run_sort_key(newer):
        failures.append("a newer attempt of the same run did not outrank its prior attempt")

    malformed = dict(newer)
    malformed["created_at"] = "not-a-timestamp"
    try:
        authoritative_run_sort_key(malformed)
    except ValueError:
        pass
    else:
        failures.append("malformed created_at did not fail closed")
    return failures

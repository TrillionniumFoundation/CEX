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


def _positive_int(name: str, value: Any) -> int:
    """Parse one immutable positive integer identity or fail closed."""

    if isinstance(value, bool):
        raise ValueError(f"authoritative workflow run has invalid {name}")
    if isinstance(value, int):
        parsed = value
    elif isinstance(value, str) and value.strip():
        try:
            parsed = int(value.strip(), 10)
        except ValueError as error:
            raise ValueError(
                f"authoritative workflow run has invalid {name}"
            ) from error
    else:
        raise ValueError(f"authoritative workflow run lacks {name}")
    if parsed <= 0:
        raise ValueError(f"authoritative workflow run has non-positive {name}")
    return parsed


def _optional_positive_int(name: str, value: Any) -> int:
    """Parse an optional legacy tie-breaker without accepting malformed data.

    GitHub REST workflow-run payloads include ``run_number``. A few immutable
    offline fixtures created before that field became part of the shared
    authority contract omit it, so absence remains a neutral final tie-breaker.
    A present value is still required to be a real positive integer. Distinct
    same-timestamp runs are ordered first by their globally unique immutable
    run ``id``, preventing a missing legacy run number from making an older run
    outrank a newer one.
    """

    if value is None or value == "":
        return 0
    return _positive_int(name, value)


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

    ``created_at`` is the primary authority. The positive globally unique run
    ``id`` breaks same-timestamp ties between distinct runs, ``run_attempt``
    orders reruns of the same run, and the positive optional ``run_number`` is
    retained only as a final consistency tie-breaker for legacy offline
    fixtures. ``updated_at`` is deliberately absent.
    """

    return (
        _created_at(run.get("created_at")),
        _positive_int("id", run.get("id")),
        _positive_int("run_attempt", run.get("run_attempt")),
        _optional_positive_int("run_number", run.get("run_number")),
    )


def self_test() -> list[str]:
    """Exercise late-finish, malformed-identity, and same-second inversions."""

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

    same_second_older = {
        **older,
        "id": 200,
        "run_number": 999,
        "created_at": "2026-08-30T01:00:00Z",
    }
    same_second_newer_without_number = {
        **newer,
        "id": 201,
        "run_number": None,
        "created_at": "2026-08-30T01:00:00Z",
    }
    if authoritative_run_sort_key(same_second_newer_without_number) <= authoritative_run_sort_key(
        same_second_older
    ):
        failures.append("a newer same-second run id was masked by a legacy run number")

    malformed_cases = (
        ("created_at", {**newer, "created_at": "not-a-timestamp"}),
        ("id", {**newer, "id": "not-an-id"}),
        ("run_attempt", {**newer, "run_attempt": 0}),
        ("run_number", {**newer, "run_number": -1}),
    )
    for label, malformed in malformed_cases:
        try:
            authoritative_run_sort_key(malformed)
        except ValueError:
            pass
        else:
            failures.append(f"malformed {label} did not fail closed")
    return failures

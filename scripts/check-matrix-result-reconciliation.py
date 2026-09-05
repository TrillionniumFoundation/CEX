#!/usr/bin/env python3
"""Fail closed unless Matrix result reconciliation is read-only and principal-bound."""
from __future__ import annotations

import json
from pathlib import Path
import stat
import sys

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    "consumer_main": "services/consumer-entry-api/src/main.rs",
    "consumer_lookup": "services/consumer-entry-api/src/matrix_result_lookup.rs",
    "replay_snapshot": "services/consumer-entry-api/src/replay_store_snapshot.rs",
    "adapter_facade": "services/matrix-entry-adapter/src/lib.rs",
    "adapter_reconciliation": "services/matrix-entry-adapter/src/result_reconciliation.rs",
    "relay_response": "apps/matrix-bot-relay/src/response_contract.rs",
}


def read_regular(relative: str) -> str:
    path = ROOT / relative
    try:
        metadata = path.lstat()
    except OSError as error:
        raise RuntimeError(f"required source unavailable: {relative}: {error}") from None
    if not stat.S_ISREG(metadata.st_mode) or path.is_symlink() or metadata.st_size > 2_000_000:
        raise RuntimeError(f"source is not a bounded regular file: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise RuntimeError(f"source is not readable UTF-8: {relative}: {error}") from None


def require(source: str, tokens: tuple[str, ...], label: str) -> list[str]:
    return [f"{label}: missing {token!r}" for token in tokens if token not in source]


def forbid(source: str, tokens: tuple[str, ...], label: str) -> list[str]:
    return [f"{label}: forbidden {token!r}" for token in tokens if token in source]


def main() -> int:
    try:
        sources = {name: read_regular(path) for name, path in FILES.items()}
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 1

    failures: list[str] = []
    failures += require(
        sources["consumer_main"],
        (
            "mod matrix_result_lookup;",
            ".merge(matrix_result_lookup::router(state.config().clone()))",
        ),
        "consumer main",
    )
    failures += require(
        sources["consumer_lookup"],
        (
            'const LOOKUP_PATH: &str = "/v1/matrix/messages/result";',
            'const LOOKUP_SOURCE_KIND: &str = "matrix_result_lookup";',
            'format!("matrix-event:{}", request.event_id)',
            '#[path = "replay_store_snapshot.rs"]',
            "read_stable_regular_file(path, MAX_REPLAY_STORE_BYTES)",
            "x-cex-user-session",
            "x-cex-user-session-signature",
            "lookup_request_fingerprint(request)",
            'source.get("event_id")',
            'source.get("matrix_user_id")',
            'source.get("room_id")',
            '"production_authorization": "not_granted"',
        ),
        "consumer lookup",
    )
    failures += forbid(
        sources["consumer_lookup"],
        (
            "reqwest::",
            '"/v1/invocations"',
            "forward_to_cex_task",
            "create_matrix_message_task",
            ".post(url)",
            "File::open",
            "std::fs::symlink_metadata",
            ".read_to_end",
        ),
        "consumer lookup must be read-only and use the stable snapshot boundary",
    )
    failures += require(
        sources["replay_snapshot"],
        (
            "pub(super) fn read_stable_regular_file",
            "fs::symlink_metadata(path)",
            "File::open(path)",
            "handle_before != expected",
            "handle_after != expected || path_after != expected",
            "metadata.nlink() != 1",
            "FILE_ATTRIBUTE_REPARSE_POINT",
            ".take(limit)",
            "rejects_symbolic_and_hard_linked_inputs",
        ),
        "replay-store stable snapshot",
    )

    failures += require(
        sources["adapter_facade"],
        (
            "mod result_reconciliation;",
            "result_reconciliation::router(state.inner.config())",
            ".merge(reconciliation)",
        ),
        "adapter facade",
    )
    failures += require(
        sources["adapter_reconciliation"],
        (
            'const RECONCILIATION_PATH: &str = "/v1/matrix/results/lookup";',
            'const CONSUMER_LOOKUP_PATH: &str = "/v1/matrix/messages/result";',
            'const LOOKUP_SOURCE_KIND: &str = "matrix_result_lookup";',
            "x-entry-token",
            "x-cex-user-session",
            "x-cex-user-session-signature",
            "sign_lookup_assertion",
            "validate_lookup_response",
            '"action": "task_result_reconciled"',
            '"source": "consumer_entry_durable_replay"',
            '"read_only": true',
            '"production_authorization": "not_granted"',
        ),
        "adapter reconciliation",
    )
    failures += forbid(
        sources["adapter_reconciliation"],
        (
            '"/v1/matrix/messages";',
            '"/v1/invocations"',
            "forward_to_consumer_entry",
        ),
        "adapter reconciliation must call only lookup",
    )

    failures += require(
        sources["relay_response"],
        (
            'if action == "duplicate_event"',
            'return Err("adapter_duplicate_outcome_unknown")',
            "byte.is_ascii_lowercase()",
        ),
        "relay response contract",
    )
    failures += forbid(
        sources["relay_response"],
        ('"task_result_reconciled" => Err',),
        "relay reconciliation response",
    )

    if failures:
        print("Matrix result reconciliation contract failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "schema": "cex.matrix.result-reconciliation-source-check.v1",
                "status": "ok",
                "consumer_lookup": "/v1/matrix/messages/result",
                "adapter_lookup": "/v1/matrix/results/lookup",
                "read_only": True,
                "principal_bound": True,
                "stable_replay_snapshot": True,
                "production_authorization": "not_granted",
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

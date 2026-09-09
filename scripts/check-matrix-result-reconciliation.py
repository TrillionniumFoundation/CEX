#!/usr/bin/env python3
"""Fail closed unless Matrix result reconciliation is principal-bound and executable."""
from __future__ import annotations

import json
from pathlib import Path
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    "consumer_main": "services/consumer-entry-api/src/main.rs",
    "consumer_lookup": "services/consumer-entry-api/src/matrix_result_lookup.rs",
    "replay_snapshot": "services/consumer-entry-api/src/replay_store_snapshot.rs",
    "adapter_facade": "services/matrix-entry-adapter/src/lib.rs",
    "adapter_reconciliation": "services/matrix-entry-adapter/src/result_reconciliation.rs",
    "relay_response": "apps/matrix-bot-relay/src/response_contract.rs",
    "operator_hardening": (
        "services/matrix-entry-adapter/operator-migrations/"
        "0003_adapter_result_evidence_binding.sql"
    ),
    "operator_runtime": (
        "services/matrix-entry-adapter/operator-migrations/"
        "0004_adapter_result_runtime_reconciliation.sql"
    ),
    "operator_runner": "scripts/matrix_operator_postgres_regression.py",
    "operator_regression": "scripts/test-matrix-result-evidence-hardening-postgres.sql",
    "runtime_regression": "scripts/test-matrix-result-runtime-reconciliation-postgres.sql",
    "runtime_reconciler": "scripts/reconcile-matrix-adapter-result.py",
    "traceability": "docs/traceability/sequence54-matrix-result-reconciliation-v1.json",
    "design": "docs/matrix-result-reconciliation-v1.md",
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


def runtime_self_test() -> list[str]:
    path = ROOT / FILES["runtime_reconciler"]
    try:
        result = subprocess.run(
            [sys.executable, str(path), "--self-test"],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return ["runtime reconciler: self-test could not execute"]
    if result.returncode != 0:
        return ["runtime reconciler: self-test failed"]
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        return ["runtime reconciler: self-test output is not JSON"]
    if (
        payload.get("schema") != "cex.matrix.adapter-result-reconciler.v1"
        or payload.get("status") != "ok"
        or payload.get("self_test") is not True
        or payload.get("production_authorization") != "not_granted"
    ):
        return ["runtime reconciler: self-test output contract mismatch"]
    return []


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

    failures += require(
        sources["operator_hardening"],
        (
            "create or replace function public.cex_matrix_reconcile_adapter_result_v1(",
            "matrix_adapter_result_identity_scope_mismatch",
            "matrix_adapter_result_raw_task_mismatch",
            "not p_evidence_context ?& array[",
            "p_evidence_context - array[",
            "::timestamptz",
            "not isfinite(observed_at)",
            "to cex_matrix_reconciler_runtime;",
        ),
        "operator reconciliation hardening",
    )
    failures += require(
        sources["operator_runtime"],
        (
            "0003. The previous replacement",
            "result_payload,",
            "p_result_payload,",
            "existing.result_payload is distinct from p_result_payload",
            "delivery_row.last_error_code like 'adapter_response_unknown_%'",
            "adapter_unverified_oversized_response",
            "adapter_duplicate_outcome_unknown",
            "relay_internal_unknown_outcome",
            "matrix_adapter_delivery_reconciliation_race",
            "owner to cex_matrix_api_owner;",
            "to cex_matrix_reconciler_runtime;",
        ),
        "runtime reconciliation migration",
    )
    failures += forbid(
        sources["operator_runtime"].lower(),
        ("grant all", " to public;"),
        "runtime reconciliation least privilege",
    )

    failures += require(
        sources["operator_runner"],
        (
            '"0003_adapter_result_evidence_binding.sql"',
            '"0004_adapter_result_runtime_reconciliation.sql"',
            '"scripts/test-matrix-result-evidence-hardening-postgres.sql"',
            '"scripts/test-matrix-result-runtime-reconciliation-postgres.sql"',
            "for pass_number in (1, 2):",
            "base.validate_sql_source(sql)",
            "base.bounded_client",
            '"production_authorization": "not_granted"',
        ),
        "operator PostgreSQL runner",
    )
    failures += require(
        sources["operator_regression"],
        (
            "matrix_result_identity_scope_mismatch_not_rejected",
            "matrix_result_raw_task_mismatch_not_rejected",
            "matrix_result_extra_evidence_key_not_rejected",
            "matrix_result_invalid_observed_at_not_rejected",
            "matrix_result_infinite_observed_at_not_rejected",
        ),
        "operator hostile PostgreSQL regression",
    )
    failures += require(
        sources["runtime_regression"],
        (
            "adapter_response_unknown_network",
            "matrix_runtime_reconciliation_payload_not_persisted",
            "matrix_runtime_reconciliation_replay_wrote_history",
            "matrix_runtime_reconciliation_collision_not_rejected",
        ),
        "runtime PostgreSQL reconciliation regression",
    )

    failures += require(
        sources["runtime_reconciler"],
        (
            'SCHEMA = "cex.matrix.adapter-result-reconciler.v1"',
            'LOOKUP_PATH = "/v1/matrix/results/lookup"',
            "class NoRedirect(HTTPRedirectHandler)",
            "unique_object",
            "lookup_response_identity_mismatch",
            "lookup_forwarded_scope_mismatch",
            "MATRIX_ENTRY_INGRESS_TOKEN",
            "MATRIX_RECONCILIATION_DATABASE_URL",
            "environment.pop(\"DATABASE_URL\", None)",
            "cex_matrix_reconcile_adapter_result_v1",
            "production_authorization",
            "--self-test",
        ),
        "runtime reconciliation command",
    )
    failures += forbid(
        sources["runtime_reconciler"],
        (
            "shell=True",
            "requests.",
            "verify=False",
            "allow_redirects=True",
            "--password",
        ),
        "runtime reconciliation command safety",
    )
    failures += require(
        sources["traceability"],
        (
            '"schema": "cex.sequence54-matrix-result-reconciliation-traceability.v1"',
            '"candidate_sequence": 54',
            '"operator_migration_head": "services/matrix-entry-adapter/operator-migrations/0004_adapter_result_runtime_reconciliation.sql"',
            '"runtime_command": "scripts/reconcile-matrix-adapter-result.py"',
            '"id": "MRR-1"',
            '"id": "MRR-6"',
            '"production_authorization": "not_granted"',
        ),
        "Matrix result reconciliation traceability",
    )
    failures += require(
        sources["design"],
        (
            "Migration 0004 fixes a concrete runtime defect",
            "scripts/reconcile-matrix-adapter-result.py",
            "result_payload",
            "all_plan_gaps_closed=false",
            "production_authorization=not_granted",
        ),
        "Matrix result reconciliation design contract",
    )
    failures += runtime_self_test()

    if failures:
        print("Matrix result reconciliation contract failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "schema": "cex.matrix.result-reconciliation-source-check.v2",
                "status": "ok",
                "consumer_lookup": "/v1/matrix/messages/result",
                "adapter_lookup": "/v1/matrix/results/lookup",
                "read_only_lookup": True,
                "principal_bound": True,
                "stable_replay_snapshot": True,
                "operator_evidence_bound": True,
                "runtime_reconciler_present": True,
                "runtime_reconciler_self_test": True,
                "runtime_payload_persistence_fix_present": True,
                "operator_postgres_runner_present": True,
                "problems": [],
                "checker_max_grant_production_authorization": False,
                "production_authorization": "not_granted",
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Fail closed when Sequence 54 Matrix v3 authority or gate wiring drifts."""
from __future__ import annotations

import json
from pathlib import Path
import stat
import sys
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []

AUTHORITY = "docs/development-doc-authority-v1.json"
TRIGGER = "docs/release-evidence/p0-candidate-trigger.json"
INTEGRATION = "docs/traceability/v12-sequence54-integration-v1.json"
TRACEABILITY = "docs/traceability/sequence54-matrix-result-reconciliation-v3.json"
REVIEW_PACKET = "docs/traceability/sequence54-matrix-security-v3-review-packet.json"
PLAN = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-SEQUENCE54-INTEGRATION.md"
DESIGN = "docs/matrix-result-reconciliation-v3.md"
COMPONENT_STATUS = "docs/status/component-status-v1.md"
P0_WORKFLOW = ".github/workflows/p0-sequence54-integration.yml"
MATRIX_WORKFLOW = ".github/workflows/matrix-review-repair-regression.yml"
CHECKER = "scripts/check-sequence54-matrix-v3-admission.py"
MIGRATION = (
    "services/matrix-entry-adapter/operator-migrations/"
    "0006_adapter_result_embedded_delivery_binding.sql"
)
OPERATOR_HEAD = "0006_adapter_result_embedded_delivery_binding.sql"
RUNTIME_ENTRYPOINT = "cex_matrix_reconcile_adapter_result_v3"


def problem(message: str) -> None:
    PROBLEMS.append(message)


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON member: {key}")
        result[key] = value
    return result


def read_text(relative: str, *, max_bytes: int = 2_000_000) -> str:
    path = ROOT / relative
    try:
        metadata = path.lstat()
    except OSError as error:
        problem(f"required source unavailable: {relative}: {error}")
        return ""
    if (
        not stat.S_ISREG(metadata.st_mode)
        or path.is_symlink()
        or metadata.st_size > max_bytes
    ):
        problem(f"source is not a bounded regular file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        problem(f"source is not readable UTF-8: {relative}: {error}")
        return ""


def load_json(relative: str) -> dict[str, Any]:
    raw = read_text(relative, max_bytes=1_000_000)
    if not raw:
        return {}
    try:
        value = json.loads(raw, object_pairs_hook=unique_object)
    except (json.JSONDecodeError, ValueError) as error:
        problem(f"invalid JSON in {relative}: {error}")
        return {}
    if not isinstance(value, dict):
        problem(f"JSON root must be an object: {relative}")
        return {}
    return value


def require_equal(document: dict[str, Any], field: str, expected: Any, label: str) -> None:
    actual = document.get(field)
    if actual != expected:
        problem(f"{label}.{field} drift: expected={expected!r} actual={actual!r}")


def require_markers(relative: str, markers: tuple[str, ...]) -> None:
    text = read_text(relative)
    for marker in markers:
        if marker not in text:
            problem(f"{relative} lacks Matrix v3 marker: {marker}")


def require_repository_file(relative: object, label: str) -> None:
    if not isinstance(relative, str) or not relative:
        problem(f"{label} must be a non-empty repository path")
        return
    path = Path(relative)
    if path.is_absolute() or ".." in path.parts or "\\" in relative:
        problem(f"{label} escapes repository: {relative}")
        return
    absolute = ROOT / path
    try:
        metadata = absolute.lstat()
    except OSError:
        problem(f"{label} references a missing file: {relative}")
        return
    if not stat.S_ISREG(metadata.st_mode) or absolute.is_symlink():
        problem(f"{label} is not a regular repository file: {relative}")


def main() -> int:
    authority = load_json(AUTHORITY)
    require_equal(authority, "candidate_sequence", 54, "authority")
    require_equal(authority, "integration_plan", PLAN, "authority")
    require_equal(authority, "integration_traceability", INTEGRATION, "authority")
    require_equal(
        authority,
        "matrix_result_reconciliation_contract",
        DESIGN,
        "authority",
    )
    require_equal(
        authority,
        "matrix_result_reconciliation_traceability",
        TRACEABILITY,
        "authority",
    )
    require_equal(
        authority,
        "matrix_result_reconciliation_review_packet",
        REVIEW_PACKET,
        "authority",
    )
    require_equal(
        authority,
        "matrix_operator_migration_head",
        OPERATOR_HEAD,
        "authority",
    )
    require_equal(
        authority,
        "matrix_operator_runtime_entrypoint",
        RUNTIME_ENTRYPOINT,
        "authority",
    )
    require_equal(authority, "production_authorization", "not_granted", "authority")

    trigger = load_json(TRIGGER)
    require_equal(trigger, "schema", "cex.p0-candidate-trigger.v1", "trigger")
    require_equal(trigger, "sequence", 54, "trigger")
    require_equal(trigger, "integration_traceability", INTEGRATION, "trigger")
    require_equal(
        trigger,
        "matrix_result_reconciliation_traceability",
        TRACEABILITY,
        "trigger",
    )
    require_equal(
        trigger,
        "matrix_result_reconciliation_review_packet",
        REVIEW_PACKET,
        "trigger",
    )
    require_equal(
        trigger,
        "matrix_operator_migration_head",
        OPERATOR_HEAD,
        "trigger",
    )
    require_equal(
        trigger,
        "matrix_operator_runtime_entrypoint",
        RUNTIME_ENTRYPOINT,
        "trigger",
    )
    require_equal(trigger, "production_authorization", "not_granted", "trigger")
    purpose = trigger.get("purpose")
    if not isinstance(purpose, str) or len(purpose) < 600:
        problem("trigger purpose does not describe the complete Sequence 54 freeze")
    else:
        for marker in (
            "operator migration 0006",
            "Consumer",
            "Adapter",
            "task_id",
            "raw.invocation_id",
            "non-empty successful required jobs",
            "production authorization remains not_granted",
        ):
            if marker not in purpose:
                problem(f"trigger purpose lacks marker: {marker}")

    integration = load_json(INTEGRATION)
    require_equal(integration, "schema", "cex.v12-sequence54-integration.v1", "integration")
    require_equal(integration, "production_authorization", "not_granted", "integration")
    matrix = integration.get("matrix_result_reconciliation")
    if not isinstance(matrix, dict):
        problem("integration.matrix_result_reconciliation must be an object")
        matrix = {}
    require_equal(matrix, "security_contract", DESIGN, "integration.matrix")
    require_equal(matrix, "traceability", TRACEABILITY, "integration.matrix")
    require_equal(matrix, "review_packet", REVIEW_PACKET, "integration.matrix")
    require_equal(matrix, "operator_migration_head", OPERATOR_HEAD, "integration.matrix")
    require_equal(matrix, "runtime_entrypoint", RUNTIME_ENTRYPOINT, "integration.matrix")
    require_equal(matrix, "production_authorization", "not_granted", "integration.matrix")
    verification = integration.get("verification")
    if not isinstance(verification, list) or CHECKER not in verification:
        problem("integration verification does not execute the Matrix v3 admission checker")

    traceability = load_json(TRACEABILITY)
    require_equal(
        traceability,
        "schema",
        "cex.sequence54-matrix-result-reconciliation-traceability.v3",
        "matrix_traceability",
    )
    require_equal(traceability, "status", "active", "matrix_traceability")
    require_equal(traceability, "candidate_sequence", 54, "matrix_traceability")
    require_equal(traceability, "operator_migration_head", MIGRATION, "matrix_traceability")
    require_equal(traceability, "review_packet", REVIEW_PACKET, "matrix_traceability")
    require_equal(
        traceability,
        "production_authorization",
        "not_granted",
        "matrix_traceability",
    )
    requirements = traceability.get("requirements")
    ids = {
        item.get("id")
        for item in requirements
        if isinstance(requirements, list) and isinstance(item, dict)
    }
    if ids != {f"MRR3-{index}" for index in range(1, 8)}:
        problem("Matrix v3 traceability requirements are incomplete")

    packet = load_json(REVIEW_PACKET)
    require_equal(
        packet,
        "schema",
        "cex.sequence54.matrix-security-v3-review-packet.v1",
        "review_packet",
    )
    require_equal(packet, "source_mapping_complete", True, "review_packet")
    require_equal(packet, "source_execution_verified", False, "review_packet")
    require_equal(packet, "all_plan_gaps_closed", False, "review_packet")
    packet_authority = packet.get("authority")
    if not isinstance(packet_authority, dict):
        problem("review_packet.authority must be an object")
        packet_authority = {}
    require_equal(packet_authority, "repository", "TrillionniumFoundation/CEX", "review_packet.authority")
    require_equal(packet_authority, "pull_request", 53, "review_packet.authority")
    require_equal(packet_authority, "production_authorization", "not_granted", "review_packet.authority")
    require_equal(packet_authority, "merge_authorized", False, "review_packet.authority")

    for path, label in (
        (PLAN, "plan"),
        (DESIGN, "design"),
        (TRACEABILITY, "traceability"),
        (REVIEW_PACKET, "review_packet"),
        (MIGRATION, "operator_migration"),
        ("services/consumer-entry-api/src/matrix_result_response_binding.rs", "consumer_response_guard"),
        ("services/matrix-entry-adapter/src/reconciliation_response_binding.rs", "adapter_response_guard"),
        ("scripts/reconcile-matrix-adapter-result-v3.py", "operator_command"),
        ("scripts/test-matrix-result-task-invocation-binding-postgres.sql", "task_invocation_regression"),
    ):
        require_repository_file(path, label)

    require_markers(
        PLAN,
        (
            "## 4. Matrix response-loss causal binding",
            OPERATOR_HEAD,
            "task_id == raw.invocation_id",
            "production_authorization=not_granted",
        ),
    )
    require_markers(
        COMPONENT_STATUS,
        (
            f"Matrix operator head: `{OPERATOR_HEAD}`",
            "Consumer and Adapter success-response guards",
            "task_id == raw.invocation_id",
            "Production authorization: `not_granted`",
        ),
    )
    require_markers(
        P0_WORKFLOW,
        (
            "python3 scripts/check-sequence54-integration.py",
            f"python3 {CHECKER}",
            "cargo fmt --all -- --check",
            "cargo test --workspace --all-targets --locked",
            "cargo clippy --workspace --all-targets --locked -- -D warnings",
        ),
    )
    require_markers(
        MATRIX_WORKFLOW,
        (
            f"python3 {CHECKER}",
            "python3 scripts/check-matrix-result-reconciliation-security-v3.py",
            "python3 scripts/check-matrix-result-reconciliation-traceability-v3.py",
            "cargo test --locked -p matrix-entry-adapter --all-targets",
            "cargo test --locked -p consumer-entry-api --all-targets",
            "python3 scripts/matrix_operator_postgres_regression.py",
        ),
    )

    result = {
        "schema": "cex.sequence54.matrix-v3-admission-source-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "candidate_sequence": 54,
        "matrix_operator_migration_head": OPERATOR_HEAD,
        "matrix_operator_runtime_entrypoint": RUNTIME_ENTRYPOINT,
        "authority_converged": not PROBLEMS,
        "source_execution_verified": False,
        "checker_may_grant_production_authorization": False,
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

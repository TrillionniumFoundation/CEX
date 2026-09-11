#!/usr/bin/env python3
"""Validate the complete Sequence 54 Matrix reconciliation v3 traceability graph."""
from __future__ import annotations

import json
from pathlib import Path
import stat
import sys
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
TRACE = ROOT / "docs/traceability/sequence54-matrix-result-reconciliation-v3.json"
EXPECTED_IDS = {f"MRR3-{index}" for index in range(1, 8)}
EXPECTED_RUNTIME_PATHS = {
    "runtime_command": "scripts/reconcile-matrix-adapter-result.py",
    "runtime_implementation": "scripts/reconcile-matrix-adapter-result-v3.py",
    "historical_v2_loader": "scripts/reconcile-matrix-adapter-result-v2-core.py",
    "historical_v2_implementation": "scripts/reconcile-matrix-adapter-result-v2-internal.py",
}
EXPECTED_EXTERNAL = {
    "nonempty_exact_sha_hosted_execution",
    "disposable_postgresql_16_operator_chain",
    "real_matrix_response_loss_rehearsal",
    "least_privilege_runtime_identity_readback",
    "protected_main_ruleset_and_negative_probes",
    "two_fresh_eligible_final_head_approvals",
    "production_secret_custody",
    "cross_repository_component_tuple_acceptance",
    "final_human_go_no_go",
}


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON member")
        result[key] = value
    return result


def read_trace() -> dict[str, Any]:
    metadata = TRACE.lstat()
    if not stat.S_ISREG(metadata.st_mode) or TRACE.is_symlink() or metadata.st_size > 512_000:
        raise ValueError("traceability source is not a bounded regular file")
    value = json.loads(TRACE.read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    if not isinstance(value, dict):
        raise ValueError("traceability root is not an object")
    return value


def repository_file(value: object, label: str) -> tuple[str, int]:
    if not isinstance(value, str) or not value:
        raise ValueError(f"{label} is not a repository path")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value:
        raise ValueError(f"{label} escapes the repository")
    absolute = ROOT / path
    metadata = absolute.lstat()
    if not stat.S_ISREG(metadata.st_mode) or absolute.is_symlink():
        raise ValueError(f"{label} is not a regular repository file")
    return value, metadata.st_mode


def main() -> int:
    problems: list[str] = []
    try:
        value = read_trace()
        if value.get("schema") != "cex.sequence54-matrix-result-reconciliation-traceability.v3":
            problems.append("invalid traceability schema")
        if value.get("status") != "active" or value.get("candidate_sequence") != 54:
            problems.append("invalid traceability status or sequence")
        if value.get("production_authorization") != "not_granted":
            problems.append("traceability may not grant production authorization")

        for field, expected in EXPECTED_RUNTIME_PATHS.items():
            if value.get(field) != expected:
                problems.append(f"{field} does not identify the canonical cutover path")

        singleton_paths = (
            "supersedes_source_contract",
            *EXPECTED_RUNTIME_PATHS,
            "postgres_runner",
            "operator_migration_head",
            "hosted_gate",
            "review_packet",
        )
        modes: dict[str, int] = {}
        for field in singleton_paths:
            try:
                _, mode = repository_file(value.get(field), field)
                modes[field] = mode
            except (OSError, ValueError) as error:
                problems.append(str(error))
        for field in ("historical_v2_loader", "historical_v2_implementation"):
            if modes.get(field, 0) & 0o111:
                problems.append(f"{field} must not be executable")
        if modes.get("runtime_command", 0) & 0o111 == 0:
            problems.append("runtime_command must remain executable")
        if modes.get("runtime_implementation", 0) & 0o111 == 0:
            problems.append("runtime_implementation must remain executable")

        for field in ("design_contracts", "source_checkers"):
            entries = value.get(field)
            if not isinstance(entries, list) or not entries:
                problems.append(f"{field} must be a non-empty array")
                continue
            if len(entries) != len(set(entries)):
                problems.append(f"{field} contains duplicates")
            for index, path in enumerate(entries):
                try:
                    repository_file(path, f"{field}[{index}]")
                except (OSError, ValueError) as error:
                    problems.append(str(error))

        requirements = value.get("requirements")
        seen_ids: set[str] = set()
        if not isinstance(requirements, list):
            problems.append("requirements must be an array")
            requirements = []
        for index, item in enumerate(requirements):
            label = f"requirements[{index}]"
            if not isinstance(item, dict):
                problems.append(f"{label} must be an object")
                continue
            if set(item) != {"id", "requirement", "implementation", "verification"}:
                problems.append(f"{label} has an invalid field set")
            requirement_id = item.get("id")
            if not isinstance(requirement_id, str) or requirement_id in seen_ids:
                problems.append(f"{label}.id is invalid or duplicated")
            else:
                seen_ids.add(requirement_id)
            requirement = item.get("requirement")
            if not isinstance(requirement, str) or len(requirement.strip()) < 80:
                problems.append(f"{label}.requirement is too shallow")
            for field in ("implementation", "verification"):
                entries = item.get(field)
                if not isinstance(entries, list) or not entries:
                    problems.append(f"{label}.{field} must be a non-empty array")
                    continue
                if len(entries) != len(set(entries)):
                    problems.append(f"{label}.{field} contains duplicates")
                for path_index, path in enumerate(entries):
                    try:
                        repository_file(path, f"{label}.{field}[{path_index}]")
                    except (OSError, ValueError) as error:
                        problems.append(str(error))
        if seen_ids != EXPECTED_IDS:
            problems.append("traceability requirement IDs are incomplete")

        operator_requirement = next(
            (item for item in requirements if isinstance(item, dict) and item.get("id") == "MRR3-4"),
            {},
        )
        operator_paths = set(operator_requirement.get("implementation", []))
        if operator_paths != set(EXPECTED_RUNTIME_PATHS.values()):
            problems.append("MRR3-4 does not cover the complete canonical-to-historical command chain")

        external = value.get("external_evidence_required")
        if not isinstance(external, list) or set(external) != EXPECTED_EXTERNAL:
            problems.append("external evidence denominator is incomplete")
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        problems.append(str(error))

    result = {
        "schema": "cex.matrix.result-reconciliation-traceability-check.v2",
        "status": "failed" if problems else "ok",
        "requirements": 7,
        "runtime_chain_files": len(EXPECTED_RUNTIME_PATHS),
        "external_evidence_classes": len(EXPECTED_EXTERNAL),
        "problems": problems,
        "checker_may_grant_production_authorization": False,
        "production_authorization": "not_granted",
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Run all manifest validators and guard the active v12 schema contract."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-release-baseline-manifest-core.py"
STRICT = ROOT / "scripts/check-release-baseline-manifest-contract.py"
SCHEMA = ROOT / "docs/schemas/cex-release-baseline-manifest-v1.schema.json"
ACTIVE_MIGRATION_HEAD = "0087_add_term_exchange_receipt_event_history.sql"
EXPECTED_EVIDENCE = (
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
EXTERNAL_GATES = (
    "X1: production-like backup and restore rehearsal against representative data volume and the real storage topology",
    "X2: deployment/cutover and rollback rehearsal with real service identities, network policy and secret custody",
    "X3: real provider reconciliation artifacts for success, definite non-execution and indeterminate outcome",
    "X4: credential issuance, rotation, revocation and break-glass custody review",
    "X5: sustained production-like soak/endurance run with queue age, retry, reconciliation, parity and Audit delivery SLOs",
    "X6: independent security, operations and financial-control review",
    "X7: legal, commercial or provider approvals where the production integration requires them",
    "X8: final human go/no-go decision bound to the immutable release candidate",
)


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


def print_output(completed: subprocess.CompletedProcess[str]) -> None:
    if completed.stdout:
        print(completed.stdout, end="" if completed.stdout.endswith("\n") else "\n")


def read_object(path: Path, label: str) -> tuple[dict[str, Any] | None, list[str]]:
    try:
        value: Any = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return None, [f"cannot read {label}: {error}"]
    if not isinstance(value, dict):
        return None, [f"{label} root must be an object"]
    return value, []


def validate_active_head(path: Path) -> list[str]:
    data, problems = read_object(path, "manifest for active-head validation")
    if data is None:
        return problems
    database = data.get("database")
    if not isinstance(database, dict):
        problems.append("manifest database must be an object")
    elif database.get("migration_head") != ACTIVE_MIGRATION_HEAD:
        problems.append(
            "manifest database.migration_head must equal the active v12 head "
            f"{ACTIVE_MIGRATION_HEAD!r}"
        )
    return problems


def nested(value: Any, *keys: str) -> Any:
    current = value
    for key in keys:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current


def validate_schema_contract(path: Path = SCHEMA) -> list[str]:
    schema, problems = read_object(path, "release manifest JSON Schema")
    if schema is None:
        return problems

    migration_head = nested(
        schema,
        "properties",
        "database",
        "properties",
        "migration_head",
        "const",
    )
    if migration_head != ACTIVE_MIGRATION_HEAD:
        problems.append("JSON Schema migration head is not the active v12 head")

    statuses = nested(
        schema,
        "$defs",
        "evidenceItem",
        "properties",
        "status",
        "enum",
    )
    if statuses != ["pending", "pass", "fail"]:
        problems.append("JSON Schema evidence status must exclude waived")
    waiver_type = nested(
        schema,
        "$defs",
        "evidenceItem",
        "properties",
        "waiver",
        "type",
    )
    if waiver_type != "null":
        problems.append("JSON Schema waiver must be null-only")

    external_prefix = nested(
        schema,
        "properties",
        "external_gates",
        "properties",
        "items",
        "prefixItems",
    )
    external_values = (
        [item.get("const") for item in external_prefix if isinstance(item, dict)]
        if isinstance(external_prefix, list)
        else []
    )
    if external_values != list(EXTERNAL_GATES):
        problems.append("JSON Schema external gates are not canonical X1-X8")

    all_of = schema.get("allOf")
    candidate_then: dict[str, Any] | None = None
    if isinstance(all_of, list):
        for item in all_of:
            if not isinstance(item, dict):
                continue
            if nested(item, "if", "properties", "status", "const") == "candidate":
                candidate_then = item.get("then") if isinstance(item.get("then"), dict) else None
                break
    prefix = nested(candidate_then, "properties", "evidence", "prefixItems")
    evidence_values: list[Any] = []
    if isinstance(prefix, list):
        for item in prefix:
            all_parts = item.get("allOf") if isinstance(item, dict) else None
            contract = (
                all_parts[1]
                if isinstance(all_parts, list)
                and len(all_parts) > 1
                and isinstance(all_parts[1], dict)
                else {}
            )
            evidence_values.append(nested(contract, "properties", "name", "const"))
    if evidence_values != list(EXPECTED_EVIDENCE):
        problems.append("JSON Schema candidate evidence set/order is not canonical")
    return problems


def self_test() -> list[str]:
    import tempfile

    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="cex-active-head-") as directory:
        path = Path(directory) / "manifest.json"
        path.write_text(
            json.dumps({"database": {"migration_head": ACTIVE_MIGRATION_HEAD}}),
            encoding="utf-8",
        )
        if validate_active_head(path):
            failures.append("valid active migration head was rejected")
        path.write_text(
            json.dumps({"database": {"migration_head": "0086_stale.sql"}}),
            encoding="utf-8",
        )
        if not validate_active_head(path):
            failures.append("stale migration head was accepted")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--allow-template", action="store_true")
    args = parser.parse_args()

    problems = [f"validator self-test failed: {item}" for item in self_test()]
    problems.extend(validate_schema_contract())
    if problems:
        print(
            json.dumps(
                {
                    "schema": "cex.active-v12-manifest-guard.v1",
                    "status": "failed",
                    "problems": problems,
                },
                indent=2,
                ensure_ascii=False,
                sort_keys=True,
            )
        )
        return 1

    arguments = [str(args.manifest)]
    if args.allow_template:
        arguments.append("--allow-template")

    core = run([sys.executable, str(CORE), *arguments])
    print_output(core)
    if core.returncode != 0:
        return core.returncode

    strict = run([sys.executable, str(STRICT), *arguments])
    print_output(strict)
    if strict.returncode != 0:
        return strict.returncode

    problems = validate_active_head(args.manifest)
    result = {
        "schema": "cex.active-v12-manifest-guard.v1",
        "status": "failed" if problems else "ok",
        "manifest": str(args.manifest),
        "expected_migration_head": ACTIVE_MIGRATION_HEAD,
        "schema_contract": "ok",
        "problems": problems,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())

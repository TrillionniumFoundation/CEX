#!/usr/bin/env python3
"""Run all manifest validators and guard the active v12 schema contract."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-release-baseline-manifest-core.py"
STRICT = ROOT / "scripts/check-release-baseline-manifest-contract.py"
SCHEMA = ROOT / "docs/schemas/cex-release-baseline-manifest-v1.schema.json"
ACTIVE_MIGRATION_HEAD = "0087_add_term_exchange_receipt_event_history.sql"
CANONICAL_BRANCH_PATTERN = (
    r"^(?!/)(?!.*//)(?!.*\.\.)(?!.*(?:^|/)\.(?:/|$))"
    r"(?!.*(?:^|/)\.\.(?:/|$))(?!.*@\{)(?!refs/)(?!HEAD$)(?!.*/$)"
    r"[A-Za-z0-9._/-]+$"
)
HOSTED_EVIDENCE_URI_PATTERN = (
    r"^gh://TrillionniumFoundation/CEX/actions/runs/"
    r"[1-9][0-9]*/attempts/[1-9][0-9]*$"
)
PAYLOAD_ARTIFACT_NAME_PATTERN = (
    r"^cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*$"
)
PAYLOAD_ARTIFACT_URI_PATTERN = (
    r"^gh://TrillionniumFoundation/CEX/actions/runs/"
    r"[1-9][0-9]*/attempts/[1-9][0-9]*/artifacts/"
    r"cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*$"
)
SBOM_URI_PATTERN = (
    r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/"
    r"sbom\.spdx\.json$"
)
PROVENANCE_URI_PATTERN = (
    r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/"
    r"provenance\.intoto\.json$"
)
LOCAL_EVIDENCE_URI_PATTERNS = {
    "candidate-hygiene": r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/candidate-hygiene\.json$",
    "repository-integrity": r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/repository-integrity\.json$",
    "hepta-postgres-integration": r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/hepta-postgres-integration\.json$",
    "migration-and-lifecycle-matrix": r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/database-lifecycle\.json$",
    "exact-ledger-soak": r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/exact-ledger-soak\.json$",
    "backup-restore": r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/backup-restore\.json$",
    "repository-governance": r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/repository-governance\.json$",
    "hosted-run-execution": r"^artifact://cex-p0-evidence-[0-9a-f]{40}-attempt-[1-9][0-9]*/hosted-run-execution\.json$",
}
ACTIVE_STATUSES = ("draft", "candidate")
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
    "repository-governance",
    "hosted-run-execution",
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
REQUIRED_ROOT_FIELDS = {
    "schema",
    "status",
    "project_id",
    "release_id",
    "generated_at",
    "qualification_scope",
    "production_ready",
    "production_authorization",
    "source",
    "dependencies",
    "database",
    "build",
    "evidence",
    "approvals",
    "external_gates",
    "revocation",
}


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
    if data.get("status") not in ACTIVE_STATUSES:
        problems.append(
            "active v12 manifest status must be draft or candidate; "
            "released/revoked lifecycle states are not implemented"
        )
    if "revocation" not in data:
        problems.append("manifest revocation field is required")
    elif data.get("revocation") is not None:
        problems.append("active v12 manifest revocation must be null")
    return problems


def nested(value: Any, *keys: str) -> Any:
    current = value
    for key in keys:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current


def validate_schema_object(
    schema: dict[str, Any], path: tuple[str, ...], required: set[str], problems: list[str]
) -> None:
    value = nested(schema, *path)
    label = ".".join(path) or "$"
    if not isinstance(value, dict):
        problems.append(f"JSON Schema object is missing: {label}")
        return
    if value.get("additionalProperties") is not False:
        problems.append(f"JSON Schema {label}.additionalProperties must be false")
    actual = value.get("required")
    if not isinstance(actual, list) or set(actual) != required or len(actual) != len(required):
        problems.append(f"JSON Schema {label}.required fields are not exact")


def conditional(schema: dict[str, Any], status: str) -> dict[str, Any] | None:
    all_of = schema.get("allOf")
    if not isinstance(all_of, list):
        return None
    for item in all_of:
        if not isinstance(item, dict):
            continue
        if nested(item, "if", "properties", "status", "const") == status:
            then = item.get("then")
            return then if isinstance(then, dict) else None
    return None


def evidence_names_from_condition(condition: dict[str, Any] | None) -> list[Any]:
    prefix = nested(condition, "properties", "evidence", "prefixItems")
    values: list[Any] = []
    if not isinstance(prefix, list):
        return values
    for item in prefix:
        all_parts = item.get("allOf") if isinstance(item, dict) else None
        contract = (
            all_parts[1]
            if isinstance(all_parts, list)
            and len(all_parts) > 1
            and isinstance(all_parts[1], dict)
            else {}
        )
        values.append(nested(contract, "properties", "name", "const"))
    return values


def validate_schema_contract(path: Path = SCHEMA) -> list[str]:
    schema, problems = read_object(path, "release manifest JSON Schema")
    if schema is None:
        return problems

    validate_schema_object(schema, (), REQUIRED_ROOT_FIELDS, problems)
    for object_path, required in (
        (("properties", "source"), {"repository", "branch", "commit_sha", "tree_sha"}),
        (("properties", "dependencies"), {"cargo_lock_sha256"}),
        (("properties", "database"), {"migration_head", "migration_sha256", "migration_chain_sha256"}),
        (("properties", "build"), {"workflow_run_id", "artifacts", "images", "sbom", "provenance"}),
        (("properties", "external_gates"), {"status", "items"}),
        (("$defs", "digestedArtifact"), {"name", "uri", "sha256"}),
        (("$defs", "evidenceItem"), {"name", "status", "uri", "sha256", "waiver"}),
        (("$defs", "approval"), {"role", "actor", "decision", "decided_at", "scope"}),
    ):
        validate_schema_object(schema, object_path, required, problems)

    status_values = nested(schema, "properties", "status", "enum")
    if status_values != list(ACTIVE_STATUSES):
        problems.append(
            "JSON Schema status must contain only the implemented draft/candidate states"
        )
    if nested(schema, "properties", "revocation", "type") != "null":
        problems.append("JSON Schema revocation must be null in active v12")

    branch_pattern = nested(
        schema, "properties", "source", "properties", "branch", "pattern"
    )
    if branch_pattern != CANONICAL_BRANCH_PATTERN:
        problems.append("JSON Schema source.branch pattern is not the canonical branch contract")

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

    for status in ACTIVE_STATUSES:
        condition = conditional(schema, status)
        if condition is None:
            problems.append(f"JSON Schema lacks the {status} lifecycle condition")
            continue
        names = evidence_names_from_condition(condition)
        if names != list(EXPECTED_EVIDENCE):
            problems.append(
                f"JSON Schema {status} evidence set/order is not canonical"
            )
        min_items = nested(condition, "properties", "evidence", "minItems")
        max_items = nested(condition, "properties", "evidence", "maxItems")
        trailing = nested(condition, "properties", "evidence", "items")
        if min_items != len(EXPECTED_EVIDENCE) or max_items != len(
            EXPECTED_EVIDENCE
        ):
            problems.append(
                f"JSON Schema {status} evidence cardinality is not exact"
            )
        if trailing is not False:
            problems.append(
                f"JSON Schema {status} evidence permits trailing entries"
            )
        if status == "candidate":
            artifacts = nested(
                condition,
                "properties",
                "build",
                "properties",
                "artifacts",
            )
            artifact_prefix = (
                artifacts.get("prefixItems")
                if isinstance(artifacts, dict)
                else None
            )
            if not isinstance(artifact_prefix, list) or not artifact_prefix:
                problems.append(
                    "JSON Schema candidate payload artifact contract is missing"
                )
            else:
                artifact_item = artifact_prefix[0]
                artifact_parts = (
                    artifact_item.get("allOf")
                    if isinstance(artifact_item, dict)
                    else None
                )
                artifact_contract = (
                    artifact_parts[1]
                    if isinstance(artifact_parts, list)
                    and len(artifact_parts) > 1
                    and isinstance(artifact_parts[1], dict)
                    else artifact_item
                )
                artifact_name_pattern = nested(
                    artifact_contract, "properties", "name", "pattern"
                )
                artifact_uri_pattern = nested(
                    artifact_contract, "properties", "uri", "pattern"
                )
                if artifact_name_pattern != PAYLOAD_ARTIFACT_NAME_PATTERN:
                    problems.append(
                        "JSON Schema candidate payload artifact name pattern is stale"
                    )
                if artifact_uri_pattern != PAYLOAD_ARTIFACT_URI_PATTERN:
                    problems.append(
                        "JSON Schema candidate payload artifact URI pattern is stale"
                    )

            prefix = nested(condition, "properties", "evidence", "prefixItems")
            if not isinstance(prefix, list):
                continue
            for item in prefix:
                parts = item.get("allOf") if isinstance(item, dict) else None
                contract = (
                    parts[1]
                    if isinstance(parts, list)
                    and len(parts) > 1
                    and isinstance(parts[1], dict)
                    else {}
                )
                name = nested(contract, "properties", "name", "const")
                actual_pattern = nested(contract, "properties", "uri", "pattern")
                expected_pattern = (
                    HOSTED_EVIDENCE_URI_PATTERN
                    if isinstance(name, str) and name in EXPECTED_EVIDENCE[:5]
                    else LOCAL_EVIDENCE_URI_PATTERNS.get(name)
                )
                if expected_pattern is None or actual_pattern != expected_pattern:
                    problems.append(
                        f"JSON Schema candidate evidence URI pattern is stale: {name!r}"
                    )

            for field, expected_pattern in (
                (
                    "sbom",
                    SBOM_URI_PATTERN,
                ),
                (
                    "provenance",
                    PROVENANCE_URI_PATTERN,
                ),
            ):
                actual_pattern = nested(
                    condition,
                    "properties",
                    "build",
                    "properties",
                    field,
                    "properties",
                    "uri",
                    "pattern",
                )
                if actual_pattern != expected_pattern:
                    problems.append(f"JSON Schema candidate build.{field}.uri pattern is stale")
    return problems


def self_test() -> list[str]:
    import tempfile

    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="cex-active-head-") as directory:
        path = Path(directory) / "manifest.json"
        path.write_text(
            json.dumps(
                {
                    "status": "candidate",
                    "database": {"migration_head": ACTIVE_MIGRATION_HEAD},
                    "revocation": None,
                }
            ),
            encoding="utf-8",
        )
        if validate_active_head(path):
            failures.append("valid active migration head/status was rejected")

        for label, value in (
            (
                "stale migration",
                {
                    "status": "candidate",
                    "database": {"migration_head": "0086_stale.sql"},
                    "revocation": None,
                },
            ),
            (
                "unimplemented lifecycle",
                {
                    "status": "released",
                    "database": {"migration_head": ACTIVE_MIGRATION_HEAD},
                    "revocation": None,
                },
            ),
            (
                "revocation payload",
                {
                    "status": "candidate",
                    "database": {"migration_head": ACTIVE_MIGRATION_HEAD},
                    "revocation": {"reason": "not active"},
                },
            ),
        ):
            path.write_text(json.dumps(value), encoding="utf-8")
            if not validate_active_head(path):
                failures.append(f"negative self-test accepted {label}")

    for value in (
        "feature/hepta-production-baseline-p0",
        "REPLACE_BRANCH",
        "release.v12-rc_1",
    ):
        if re.fullmatch(CANONICAL_BRANCH_PATTERN, value) is None:
            failures.append(f"canonical branch pattern rejected valid value {value!r}")
    for value in (
        "/leading",
        "trailing/",
        "feature//duplicate",
        "feature/../escape",
        "feature/./dot",
        "refs/heads/main",
        "HEAD",
        "feature/@{bad}",
    ):
        if re.fullmatch(CANONICAL_BRANCH_PATTERN, value) is not None:
            failures.append(f"canonical branch pattern accepted invalid value {value!r}")

    # The schema and both CLI validators must fail closed when a required
    # lifecycle field is silently removed.  ``dict.get`` would otherwise make
    # a missing nullable revocation field indistinguishable from an explicit
    # null.
    with tempfile.TemporaryDirectory(prefix="cex-required-field-") as directory:
        path = Path(directory) / "manifest.json"
        minimal = {
            "status": "candidate",
            "database": {"migration_head": ACTIVE_MIGRATION_HEAD},
            "revocation": None,
        }
        minimal.pop("revocation")
        path.write_text(json.dumps(minimal), encoding="utf-8")
        if not validate_active_head(path):
            failures.append("missing revocation field was accepted")

    uri_patterns = (
        (
            HOSTED_EVIDENCE_URI_PATTERN,
            "gh://TrillionniumFoundation/CEX/actions/runs/123/attempts/2",
            "gh://TrillionniumFoundation/CEX/actions/runs/0/attempts/2",
        ),
        (
            PAYLOAD_ARTIFACT_URI_PATTERN,
            "gh://TrillionniumFoundation/CEX/actions/runs/123/attempts/2/artifacts/cex-p0-evidence-"
            + "a" * 40
            + "-attempt-2",
            "file:///tmp/cex-p0-evidence",
        ),
    )
    for pattern, valid, invalid in uri_patterns:
        if re.fullmatch(pattern, valid) is None:
            failures.append(f"URI pattern rejected valid value {valid!r}")
        if re.fullmatch(pattern, invalid) is not None:
            failures.append(f"URI pattern accepted invalid value {invalid!r}")
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
        "active_statuses": list(ACTIVE_STATUSES),
        "required_evidence_order": list(EXPECTED_EVIDENCE),
        "schema_contract": "ok",
        "problems": problems,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())

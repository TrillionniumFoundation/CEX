#!/usr/bin/env python3
"""Validate the active CEX v12 documentation authority and traceability set."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = "docs/development-doc-authority-v1.json"
BASE_TRACEABILITY_PATH = "docs/traceability/v12-requirements-v1.json"
P0_TRACEABILITY_PATH = "docs/traceability/v12-p0-blockers-v1.json"
MODULE_CATALOG_PATH = "docs/module-catalog-v1.json"
MODULE_STANDARD_PATH = "docs/module-documentation-standard-v1.md"
EXPECTED_PLAN = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md"
EXPECTED_ADDENDUM = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
EXPECTED_MIGRATION_HEAD = "0084_make_provider_reconciliation_replay_terminal_safe.sql"
EXPECTED_BASE_REQUIREMENTS = {
    "V12-A", "V12-B", "V12-C", "V12-D", "V12-E", "V12-F", "V12-G",
    "V12-H", "V12-I", "V12-J", "V12-K",
    "V12-X1", "V12-X2", "V12-X3", "V12-X4", "V12-X5", "V12-X6",
    "V12-X7", "V12-X8",
}
EXPECTED_P0_REQUIREMENTS = {
    "V12-L", "V12-M", "V12-N", "V12-O", "V12-X9", "V12-X10",
}
PROBLEMS: list[str] = []


def read_text(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"missing required file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"required UTF-8 file cannot be decoded: {relative}: {error}")
        return ""


def load_json(relative: str) -> dict[str, Any]:
    raw = read_text(relative)
    if not raw:
        return {}
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        PROBLEMS.append(f"invalid JSON {relative}: {error}")
        return {}
    if not isinstance(value, dict):
        PROBLEMS.append(f"JSON root must be an object: {relative}")
        return {}
    return value


def require_path(value: Any, label: str) -> None:
    if not isinstance(value, str) or not value:
        PROBLEMS.append(f"{label} must be a nonempty repository path")
    elif not (ROOT / value).is_file():
        PROBLEMS.append(f"{label} references missing file: {value}")


def require_markers(relative: str, *markers: str) -> None:
    text = read_text(relative)
    for marker in markers:
        if marker not in text:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")


def run_checker(relative: str, label: str, *arguments: str) -> None:
    result = subprocess.run(
        [sys.executable, str(ROOT / relative), *arguments],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        PROBLEMS.append(f"{label} failed: {result.stdout.strip()}")


def validate_authority() -> dict[str, Any]:
    authority = load_json(AUTHORITY_PATH)
    if authority.get("schema") != "cex.development-doc-authority.v1":
        PROBLEMS.append("development authority schema is invalid")
    if authority.get("status") != "active":
        PROBLEMS.append("development authority must be active")
    if authority.get("active_plan") != EXPECTED_PLAN:
        PROBLEMS.append("development authority does not name the active v12 plan")
    if authority.get("active_addendum") != EXPECTED_ADDENDUM:
        PROBLEMS.append("development authority does not name the active v12 addendum")
    if authority.get("migration_head") != EXPECTED_MIGRATION_HEAD:
        PROBLEMS.append("development authority migration head is stale")
    if authority.get("production_authorization") != "not_granted":
        PROBLEMS.append("development authority must deny production authorization")
    if authority.get("external_gate_policy") != "independent_evidence_required":
        PROBLEMS.append("external gate policy must require independent evidence")

    for key in ("active_plan", "active_addendum", "shared_trigger", "aggregate_release_workflow"):
        require_path(authority.get(key), f"authority.{key}")

    workflows = authority.get("authoritative_workflows")
    if not isinstance(workflows, list) or len(workflows) != 5:
        PROBLEMS.append("authority must name exactly five authoritative workflows")
    else:
        for index, path in enumerate(workflows):
            require_path(path, f"authority.authoritative_workflows[{index}]")

    canonical = authority.get("canonical_documents")
    required_keys = {
        "entrypoint",
        "component_status",
        "traceability",
        "p0_blocker_traceability",
        "module_catalog",
        "module_standard",
        "hepta_state_machines",
        "threat_model",
        "clean_deployment_acceptance",
        "slo_recovery",
        "protocol_compatibility",
        "capability_posture",
        "money_isolation",
        "money_isolation_manifest",
        "project_boundary",
        "world_surface_freeze",
        "world_surface_manifest",
        "repository_governance",
    }
    if not isinstance(canonical, dict) or set(canonical) != required_keys:
        PROBLEMS.append("canonical document set is incomplete or contains unversioned additions")
    else:
        for key, path in canonical.items():
            require_path(path, f"authority.canonical_documents.{key}")
            text = read_text(path) if isinstance(path, str) else ""
            if re.search(r"(?:^|[\s`])/(?:home|data|Users)/", text) or re.search(
                r"\b[A-Za-z]:\\", text
            ):
                PROBLEMS.append(
                    f"canonical document contains a machine-specific absolute path: {path}"
                )
            if isinstance(path, str) and Path(path).name.lower() == "readme.md":
                PROBLEMS.append(
                    "root README must not be part of the normative document authority chain"
                )

    if isinstance(canonical, dict):
        expected_paths = {
            "traceability": BASE_TRACEABILITY_PATH,
            "p0_blocker_traceability": P0_TRACEABILITY_PATH,
            "module_catalog": MODULE_CATALOG_PATH,
            "module_standard": MODULE_STANDARD_PATH,
            "capability_posture": "docs/capability-production-posture-v1.md",
            "money_isolation": "docs/authoritative-money-isolation-v1.md",
            "money_isolation_manifest": "docs/compatibility/authoritative-money-isolation-v1.json",
            "project_boundary": "PROJECT_BOUNDARY.md",
            "world_surface_freeze": "docs/compatibility/world-surface-freeze-v1.md",
            "world_surface_manifest": "docs/compatibility/world-surface-freeze-v1.json",
            "repository_governance": "docs/repository-governance-policy-v1.md",
        }
        for key, expected in expected_paths.items():
            if canonical.get(key) != expected:
                PROBLEMS.append(f"canonical {key} path is stale")

    external = authority.get("external_production_scope")
    if (
        not isinstance(external, dict)
        or external.get("self_certifiable") is not False
        or external.get("status") != "blocked_upstream"
    ):
        PROBLEMS.append(
            "external production scope must remain non-self-certifiable and blocked_upstream"
        )
    elif not {
        "main_branch_ruleset_enforcement",
        "cross_repository_world_authority_transfer",
    }.issubset(set(external.get("includes", []))):
        PROBLEMS.append(
            "external scope must retain Ruleset and World transfer blockers"
        )
    return authority


def validate_requirement_ledger(
    relative: str,
    schema: str,
    expected_requirements: set[str],
    authority: dict[str, Any],
) -> set[str]:
    trace = load_json(relative)
    if trace.get("schema") != schema:
        PROBLEMS.append(f"{relative} schema is invalid")
    if trace.get("status") != "active":
        PROBLEMS.append(f"{relative} status must be active")
    for field, expected in (
        ("active_plan", EXPECTED_PLAN),
        ("active_addendum", EXPECTED_ADDENDUM),
        ("migration_head", EXPECTED_MIGRATION_HEAD),
        ("production_authorization", "not_granted"),
    ):
        if trace.get(field) != expected:
            PROBLEMS.append(f"{relative} {field} is stale")

    requirements = trace.get("requirements")
    if not isinstance(requirements, list):
        PROBLEMS.append(f"{relative} requirements must be a list")
        return set()

    seen: set[str] = set()
    for index, item in enumerate(requirements):
        label = f"{relative}:requirements[{index}]"
        if not isinstance(item, dict):
            PROBLEMS.append(f"{label} must be an object")
            continue
        requirement_id = item.get("id")
        if not isinstance(requirement_id, str) or not requirement_id:
            PROBLEMS.append(f"{label}.id is invalid")
            continue
        if requirement_id in seen:
            PROBLEMS.append(f"duplicate traceability requirement in {relative}: {requirement_id}")
        seen.add(requirement_id)

        classification = item.get("classification")
        if classification not in {"repository_actionable", "external"}:
            PROBLEMS.append(f"{requirement_id} classification is invalid")

        source = item.get("source")
        if not isinstance(source, list) or not source:
            PROBLEMS.append(f"{requirement_id} must name at least one normative source")

        for field in ("source", "implementation", "verification", "gates"):
            paths = item.get(field)
            if not isinstance(paths, list):
                PROBLEMS.append(f"{requirement_id}.{field} must be a list")
                continue
            for path in paths:
                require_path(path, f"{requirement_id}.{field}")

        if classification == "repository_actionable":
            if (
                item.get("self_certifiable") is not True
                or item.get("status") != "required_hosted_exact_sha"
            ):
                PROBLEMS.append(f"{requirement_id} repository evidence policy is invalid")
            for field in ("implementation", "verification", "gates"):
                if not item.get(field):
                    PROBLEMS.append(
                        f"{requirement_id} lacks executable {field} traceability"
                    )
        elif classification == "external":
            if (
                item.get("self_certifiable") is not False
                or item.get("status") != "blocked_upstream"
            ):
                PROBLEMS.append(f"{requirement_id} external evidence policy is invalid")
            if item.get("gates") or item.get("verification") or item.get("implementation"):
                PROBLEMS.append(
                    f"{requirement_id} must not claim repository implementation or a gate can self-certify it"
                )

    if seen != expected_requirements:
        PROBLEMS.append(
            f"{relative} requirement set mismatch: missing="
            + ",".join(sorted(expected_requirements - seen))
            + " extra="
            + ",".join(sorted(seen - expected_requirements))
        )
    if authority and trace.get("active_plan") != authority.get("active_plan"):
        PROBLEMS.append(f"authority and {relative} disagree on active plan")
    return seen


def validate_traceability(authority: dict[str, Any]) -> None:
    base_seen = validate_requirement_ledger(
        BASE_TRACEABILITY_PATH,
        "cex.v12-requirement-traceability.v1",
        EXPECTED_BASE_REQUIREMENTS,
        authority,
    )
    p0 = load_json(P0_TRACEABILITY_PATH)
    if p0.get("base_traceability") != BASE_TRACEABILITY_PATH:
        PROBLEMS.append("P0 blocker traceability does not bind the base ledger")
    p0_seen = validate_requirement_ledger(
        P0_TRACEABILITY_PATH,
        "cex.v12-p0-blocker-traceability.v1",
        EXPECTED_P0_REQUIREMENTS,
        authority,
    )
    overlap = base_seen & p0_seen
    if overlap:
        PROBLEMS.append(
            "base and P0 traceability contain duplicate requirement IDs: "
            + ",".join(sorted(overlap))
        )


def validate_repository_wiring() -> None:
    migrations = sorted((ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql"))
    if not migrations or migrations[-1].name != EXPECTED_MIGRATION_HEAD:
        PROBLEMS.append("repository migration head does not match v12 authority")

    rust_gate = read_text(".github/workflows/rust-service-gate.yml")
    for marker in (
        "# exact-tree: every branch push",
        "repository-integrity:",
        "hepta-postgres-integration:",
        "HEPTA_REQUIRE_POSTGRES_TESTS: '1'",
        "scripts/check-development-docs.py",
        "scripts/check-module-documentation.py",
        "scripts/check-capability-production-posture.py",
        "scripts/check-authoritative-money-isolation.py",
        "scripts/check-project-boundary.py",
        "scripts/check-source-governance.py",
        "scripts/check-repository-integrity.py",
        "scripts/check-hepta-lint-ownership.py",
        "scripts/check-hepta-postgres-integration.sh",
    ):
        if marker not in rust_gate:
            PROBLEMS.append(
                f"rust-service-gate lacks required v12 blocker marker: {marker}"
            )

    release_gate = read_text(".github/workflows/p0-release-candidate-gate.yml")
    for marker in (
        "scripts/check-development-docs.py",
        "scripts/check-repository-integrity.py",
        "HEPTA_REQUIRE_POSTGRES_TESTS: '1'",
        "scripts/check-hepta-postgres-integration.sh --mode recovery-only",
    ):
        if marker not in release_gate:
            PROBLEMS.append(
                f"release candidate gate lacks required addendum marker: {marker}"
            )

    hepta_test = read_text("services/hepta-research-league/tests/postgres_recovery.rs")
    for marker in ("HEPTA_REQUIRE_POSTGRES_TESTS", "strict PostgreSQL integration test"):
        if marker not in hepta_test:
            PROBLEMS.append(
                f"Hepta PostgreSQL recovery test can still silently skip: missing {marker}"
            )

    for relative, label in (
        ("scripts/check-hepta-lint-ownership.py", "Hepta lint ownership contract"),
        ("scripts/check-module-documentation.py", "workspace module documentation contract"),
        ("scripts/check-capability-production-posture.py", "Capability production posture"),
        ("scripts/check-authoritative-money-isolation.py", "authoritative money isolation"),
        ("scripts/check-project-boundary.py", "CEX/World project boundary"),
        ("scripts/check-source-governance.py", "source governance"),
    ):
        run_checker(relative, label)

    require_markers(
        "docs/index.md",
        "Authority order",
        "Production authorization",
        "P0 blocker traceability",
        "Capability production posture",
        "World compatibility freeze",
        "Repository governance",
        "scripts/check-development-docs.py",
    )
    require_markers(
        EXPECTED_ADDENDUM,
        "Block H",
        "Block I",
        "Block J",
        "Block K",
        "Block L",
        "Block M",
        "Block N",
        "Block O",
        "REPOSITORY_CLOSED_CANDIDATE",
        "External production and administration gates remain upstream blockers",
    )
    require_markers(
        MODULE_STANDARD_PATH,
        "Workspace module documentation standard v1",
        "Cargo workspace",
        "Production authorization: `not_granted`",
    )


def main() -> int:
    authority = validate_authority()
    validate_traceability(authority)
    validate_repository_wiring()
    result = {
        "schema": "cex.development-doc-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "active_plan": EXPECTED_PLAN,
        "active_addendum": EXPECTED_ADDENDUM,
        "migration_head": EXPECTED_MIGRATION_HEAD,
        "requirements": len(EXPECTED_BASE_REQUIREMENTS | EXPECTED_P0_REQUIREMENTS),
        "workspace_module_contract": MODULE_CATALOG_PATH,
        "p0_blocker_traceability": P0_TRACEABILITY_PATH,
        "production_authorization": "not_granted",
        "external_blockers": ["V12-X9", "V12-X10"],
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

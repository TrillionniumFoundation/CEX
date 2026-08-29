#!/usr/bin/env python3
"""Validate the active CEX v12 documentation authority and traceability contract."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = "docs/development-doc-authority-v1.json"
TRACEABILITY_PATH = "docs/traceability/v12-requirements-v1.json"
EXPECTED_PLAN = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md"
EXPECTED_ADDENDUM = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
EXPECTED_MIGRATION_HEAD = "0084_make_provider_reconciliation_replay_terminal_safe.sql"
EXPECTED_REQUIREMENTS = {
    "V12-A", "V12-B", "V12-C", "V12-D", "V12-E", "V12-F", "V12-G", "V12-H", "V12-I", "V12-J",
    "V12-X1", "V12-X2", "V12-X3", "V12-X4", "V12-X5", "V12-X6", "V12-X7", "V12-X8",
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
        "entrypoint", "component_status", "traceability", "hepta_state_machines",
        "threat_model", "clean_deployment_acceptance", "slo_recovery", "protocol_compatibility",
    }
    if not isinstance(canonical, dict) or set(canonical) != required_keys:
        PROBLEMS.append("canonical document set is incomplete or contains unversioned additions")
    else:
        for key, path in canonical.items():
            require_path(path, f"authority.canonical_documents.{key}")
            text = read_text(path) if isinstance(path, str) else ""
            if re.search(r"(?:^|[\s`])/(?:home|data|Users)/", text) or re.search(r"\b[A-Za-z]:\\", text):
                PROBLEMS.append(f"canonical document contains a machine-specific absolute path: {path}")
            if isinstance(path, str) and Path(path).name.lower() == "readme.md":
                PROBLEMS.append("root README must not be part of the normative document authority chain")
    external = authority.get("external_production_scope")
    if not isinstance(external, dict) or external.get("self_certifiable") is not False or external.get("status") != "blocked_upstream":
        PROBLEMS.append("external production scope must remain non-self-certifiable and blocked_upstream")
    return authority


def validate_traceability(authority: dict[str, Any]) -> None:
    trace = load_json(TRACEABILITY_PATH)
    if trace.get("schema") != "cex.v12-requirement-traceability.v1":
        PROBLEMS.append("traceability schema is invalid")
    for field, expected in (
        ("active_plan", EXPECTED_PLAN),
        ("active_addendum", EXPECTED_ADDENDUM),
        ("migration_head", EXPECTED_MIGRATION_HEAD),
        ("production_authorization", "not_granted"),
    ):
        if trace.get(field) != expected:
            PROBLEMS.append(f"traceability {field} is stale")
    requirements = trace.get("requirements")
    if not isinstance(requirements, list):
        PROBLEMS.append("traceability requirements must be a list")
        return
    seen: set[str] = set()
    for index, item in enumerate(requirements):
        label = f"requirements[{index}]"
        if not isinstance(item, dict):
            PROBLEMS.append(f"{label} must be an object")
            continue
        requirement_id = item.get("id")
        if not isinstance(requirement_id, str) or not requirement_id:
            PROBLEMS.append(f"{label}.id is invalid")
            continue
        if requirement_id in seen:
            PROBLEMS.append(f"duplicate traceability requirement: {requirement_id}")
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
            if item.get("self_certifiable") is not True or item.get("status") != "required_hosted_exact_sha":
                PROBLEMS.append(f"{requirement_id} repository evidence policy is invalid")
            for field in ("implementation", "verification", "gates"):
                if not item.get(field):
                    PROBLEMS.append(f"{requirement_id} lacks executable {field} traceability")
        elif classification == "external":
            if item.get("self_certifiable") is not False or item.get("status") != "blocked_upstream":
                PROBLEMS.append(f"{requirement_id} external evidence policy is invalid")
            if item.get("gates") or item.get("verification"):
                PROBLEMS.append(f"{requirement_id} must not claim a repository gate can self-certify it")
    if seen != EXPECTED_REQUIREMENTS:
        PROBLEMS.append(
            "traceability requirement set mismatch: missing="
            + ",".join(sorted(EXPECTED_REQUIREMENTS - seen))
            + " extra="
            + ",".join(sorted(seen - EXPECTED_REQUIREMENTS))
        )
    if authority and trace.get("active_plan") != authority.get("active_plan"):
        PROBLEMS.append("authority and traceability disagree on active plan")


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
        "scripts/check-repository-integrity.py",
        "scripts/check-hepta-lint-ownership.py",
        "scripts/check-hepta-postgres-integration.sh",
    ):
        if marker not in rust_gate:
            PROBLEMS.append(f"rust-service-gate lacks required v12 addendum marker: {marker}")
    release_gate = read_text(".github/workflows/p0-release-candidate-gate.yml")
    for marker in (
        "scripts/check-development-docs.py",
        "scripts/check-repository-integrity.py",
        "HEPTA_REQUIRE_POSTGRES_TESTS: '1'",
        "scripts/check-hepta-postgres-integration.sh --mode recovery-only",
    ):
        if marker not in release_gate:
            PROBLEMS.append(f"release candidate gate lacks required addendum marker: {marker}")
    hepta_test = read_text("services/hepta-research-league/tests/postgres_recovery.rs")
    for marker in ("HEPTA_REQUIRE_POSTGRES_TESTS", "strict PostgreSQL integration test"):
        if marker not in hepta_test:
            PROBLEMS.append(f"Hepta PostgreSQL recovery test can still silently skip: missing {marker}")
    lint_check = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-hepta-lint-ownership.py")],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if lint_check.returncode != 0:
        PROBLEMS.append("Hepta lint ownership contract failed: " + lint_check.stdout.strip())
    require_markers(
        "docs/index.md",
        "Authority order",
        "Production authorization",
        "scripts/check-development-docs.py",
    )
    require_markers(
        EXPECTED_ADDENDUM,
        "Block H",
        "Block I",
        "Block J",
        "REPOSITORY_CLOSED_CANDIDATE",
        "External production gates remain upstream blockers",
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
        "requirements": len(EXPECTED_REQUIREMENTS),
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

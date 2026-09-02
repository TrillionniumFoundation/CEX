#!/usr/bin/env python3
"""Validate the active CEX v12 documentation and exact-tree wiring contract."""

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
MODULE_CHECKER = "scripts/check-module-documentation.py"
EXPECTED_PLAN = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md"
EXPECTED_ADDENDUM = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
EXPECTED_MIGRATION_HEAD = "0088_enforce_provider_terminal_evidence_binding.sql"
EXPECTED_REPOSITORY_QUALIFICATION_RESULT = "PENDING_EXACT_SHA_HOSTED_EVIDENCE"
EXPECTED_REPOSITORY_QUALIFICATION_AUTHORITY = "generated_candidate_manifest_only"
SHARED_TRIGGER = "docs/release-evidence/p0-candidate-trigger.json"
EXPECTED_REQUIREMENTS = {
    "V12-A",
    "V12-B",
    "V12-C",
    "V12-D",
    "V12-E",
    "V12-F",
    "V12-G",
    "V12-H",
    "V12-I",
    "V12-J",
    "V12-X1",
    "V12-X2",
    "V12-X3",
    "V12-X4",
    "V12-X5",
    "V12-X6",
    "V12-X7",
    "V12-X8",
}
CANONICAL_DOCUMENT_KEYS = {
    "entrypoint",
    "component_status",
    "traceability",
    "hepta_state_machines",
    "threat_model",
    "clean_deployment_acceptance",
    "slo_recovery",
    "protocol_compatibility",
    "trnm_production_credentials",
}
AUTHORITATIVE_WORKFLOWS = [
    ".github/workflows/p0-migration-gate.yml",
    ".github/workflows/rust-service-gate.yml",
    ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    ".github/workflows/p0-execution-settlement-gate.yml",
    ".github/workflows/p0-provider-reconciliation-gate.yml",
]
AGGREGATE_WORKFLOW = ".github/workflows/p0-release-candidate-gate.yml"
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
        return
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value:
        PROBLEMS.append(f"{label} is not a canonical repository path: {value}")
    elif not (ROOT / value).is_file():
        PROBLEMS.append(f"{label} references missing file: {value}")


def require_markers(relative: str, *markers: str) -> None:
    text = read_text(relative)
    for marker in markers:
        if marker not in text:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")


def validate_authority() -> dict[str, Any]:
    authority = load_json(AUTHORITY_PATH)
    expected = {
        "schema": "cex.development-doc-authority.v1",
        "status": "active",
        "active_plan": EXPECTED_PLAN,
        "active_addendum": EXPECTED_ADDENDUM,
        "migration_head": EXPECTED_MIGRATION_HEAD,
        "shared_trigger": SHARED_TRIGGER,
        "qualification_freeze": SHARED_TRIGGER,
        "production_authorization": "not_granted",
        "repository_qualification_result": EXPECTED_REPOSITORY_QUALIFICATION_RESULT,
        "repository_qualification_authority": EXPECTED_REPOSITORY_QUALIFICATION_AUTHORITY,
        "external_gate_policy": "independent_evidence_required",
    }
    for field, expected_value in expected.items():
        if authority.get(field) != expected_value:
            PROBLEMS.append(f"development authority {field} is stale or invalid")

    for field in (
        "active_plan",
        "active_addendum",
        "shared_trigger",
        "qualification_freeze",
        "aggregate_release_workflow",
    ):
        require_path(authority.get(field), f"authority.{field}")

    workflows = authority.get("authoritative_workflows")
    if workflows != AUTHORITATIVE_WORKFLOWS:
        PROBLEMS.append("authority must name the exact five authoritative workflows in order")
    else:
        for index, path in enumerate(workflows):
            require_path(path, f"authority.authoritative_workflows[{index}]")
    if authority.get("aggregate_release_workflow") != AGGREGATE_WORKFLOW:
        PROBLEMS.append("development authority aggregate release workflow is invalid")

    canonical = authority.get("canonical_documents")
    if not isinstance(canonical, dict) or set(canonical) != CANONICAL_DOCUMENT_KEYS:
        PROBLEMS.append("canonical document set is incomplete or contains incompatible additions")
    else:
        for key, path in canonical.items():
            require_path(path, f"authority.canonical_documents.{key}")
            text = read_text(path) if isinstance(path, str) else ""
            if re.search(r"(?:^|[\s`])/(?:home|data|Users)/", text) or re.search(
                r"\b[A-Za-z]:\\", text
            ):
                PROBLEMS.append(f"canonical document contains a machine-specific path: {path}")
            if isinstance(path, str) and Path(path).name.lower() == "readme.md":
                PROBLEMS.append("root README may not become normative authority")

    external = authority.get("external_production_scope")
    if (
        not isinstance(external, dict)
        or external.get("self_certifiable") is not False
        or external.get("status") != "blocked_upstream"
    ):
        PROBLEMS.append("external production scope must remain non-self-certifiable and blocked")
    return authority


def validate_traceability(authority: dict[str, Any]) -> None:
    trace = load_json(TRACEABILITY_PATH)
    expected = {
        "schema": "cex.v12-requirement-traceability.v1",
        "status": "active",
        "active_plan": EXPECTED_PLAN,
        "active_addendum": EXPECTED_ADDENDUM,
        "migration_head": EXPECTED_MIGRATION_HEAD,
        "production_authorization": "not_granted",
    }
    for field, expected_value in expected.items():
        if trace.get(field) != expected_value:
            PROBLEMS.append(f"traceability {field} is stale or invalid")

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
            if item.get("verification") or item.get("gates"):
                PROBLEMS.append(f"{requirement_id} may not claim repository self-certification")

    if seen != EXPECTED_REQUIREMENTS:
        PROBLEMS.append(
            "traceability requirement set mismatch: missing="
            + ",".join(sorted(EXPECTED_REQUIREMENTS - seen))
            + " extra="
            + ",".join(sorted(seen - EXPECTED_REQUIREMENTS))
        )
    if authority and trace.get("active_plan") != authority.get("active_plan"):
        PROBLEMS.append("authority and traceability disagree on active plan")

    v12_h = next(
        (item for item in requirements if isinstance(item, dict) and item.get("id") == "V12-H"),
        None,
    )
    if not isinstance(v12_h, dict):
        PROBLEMS.append("V12-H traceability entry is missing")
    else:
        required_h_paths = {
            "docs/module-catalog-v1.json",
            "docs/modules/index.md",
            "scripts/check-module-documentation.py",
        }
        observed = {
            *[str(item) for item in v12_h.get("source", [])],
            *[str(item) for item in v12_h.get("implementation", [])],
            *[str(item) for item in v12_h.get("verification", [])],
        }
        if not required_h_paths.issubset(observed):
            PROBLEMS.append("V12-H does not trace the complete workspace module-documentation contract")


def run_checker(relative: str, label: str) -> None:
    completed = subprocess.run(
        [sys.executable, str(ROOT / relative)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if completed.returncode != 0:
        PROBLEMS.append(f"{label} failed: {completed.stdout.strip()}")


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
            PROBLEMS.append(f"rust-service-gate lacks required v12 marker: {marker}")

    release_gate = read_text(AGGREGATE_WORKFLOW)
    for marker in (
        "scripts/check-development-docs.py",
        "scripts/check-repository-integrity.py",
        "HEPTA_REQUIRE_POSTGRES_TESTS: '1'",
        "scripts/check-hepta-postgres-integration.sh --mode recovery-only",
    ):
        if marker not in release_gate:
            PROBLEMS.append(f"release candidate gate lacks required marker: {marker}")

    hepta_test = read_text("services/hepta-research-league/tests/postgres_recovery.rs")
    for marker in ("HEPTA_REQUIRE_POSTGRES_TESTS", "strict PostgreSQL integration test"):
        if marker not in hepta_test:
            PROBLEMS.append(f"Hepta PostgreSQL recovery can silently skip: missing {marker}")

    require_markers(
        "docs/index.md",
        "Authority order",
        "Workspace module contracts",
        "docs/module-catalog-v1.json",
        "docs/modules/index.md",
        "Production authorization",
        "scripts/check-development-docs.py",
    )
    require_markers(
        EXPECTED_ADDENDUM,
        "Block H",
        "Block I",
        "Block J",
        "Block K",
        "complete workspace module documentation",
        "REPOSITORY_CLOSED_CANDIDATE",
        "External production gates remain upstream blockers",
    )
    require_markers(
        "docs/clean-deployment-acceptance-v1.md",
        "scripts/check-module-documentation.py",
        EXPECTED_MIGRATION_HEAD,
    )
    require_markers(
        "docs/trnm-production-credential-contract-v1.md",
        "Status: operational security contract",
        "pairwise distinct",
        "TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH",
        "deploy/trnm-economy/trnm-production.env.example",
        "production authorization",
    )
    require_markers(
        "deploy/trnm-economy/trnm-production.env.example",
        "LEDGER_ADMIN_TOKEN=",
        "TRNM_VALUE_ENTITLEMENT_SIGNING_SECRET=",
        "TRNM_GAME_AUTHORITY_TOKEN=",
        "TRNM_PLAYER_SESSION_SIGNING_SECRET=",
        "CONSUMER_ENTRY_INGRESS_TOKEN=",
        "CONSUMER_ENTRY_SESSION_AUTH_SECRET=",
        "CEX_GATEWAY_API_KEY=",
        "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET=",
    )

    run_checker(MODULE_CHECKER, "workspace module-documentation contract")
    run_checker("scripts/check-hepta-lint-ownership.py", "Hepta lint-ownership contract")


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
        "repository_qualification_result": authority.get("repository_qualification_result"),
        "repository_qualification_authority": authority.get("repository_qualification_authority"),
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

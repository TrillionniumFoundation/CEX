#!/usr/bin/env python3
"""Validate the active CEX v12 documentation, module, and traceability contract."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = "docs/development-doc-authority-v1.json"
TRACEABILITY = "docs/traceability/v12-requirements-v1.json"
MODULE_CATALOG = "docs/module-catalog-v1.json"
PLAN = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md"
ADDENDUM = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
MIGRATION_HEAD = "0087_add_term_exchange_receipt_event_history.sql"
TRIGGER = "docs/release-evidence/p0-candidate-trigger.json"
QUALIFICATION_RESULT = "PENDING_EXACT_SHA_HOSTED_EVIDENCE"
QUALIFICATION_AUTHORITY = "generated_candidate_manifest_only"
CANONICAL_KEYS = {
    "entrypoint",
    "component_status",
    "traceability",
    "module_catalog",
    "module_index",
    "hepta_state_machines",
    "threat_model",
    "clean_deployment_acceptance",
    "slo_recovery",
    "protocol_compatibility",
    "trnm_production_credentials",
}
REQUIREMENTS = {
    "V12-A", "V12-B", "V12-C", "V12-D", "V12-E", "V12-F",
    "V12-G", "V12-H", "V12-I", "V12-J", "V12-K",
    "V12-X1", "V12-X2", "V12-X3", "V12-X4",
    "V12-X5", "V12-X6", "V12-X7", "V12-X8",
}
MODULE_SECTIONS = [
    "## Purpose and non-goals",
    "## Authority and owned state",
    "## Source layout and entry points",
    "## Interfaces and contracts",
    "## Persistence, concurrency, and recovery",
    "## Configuration and secrets",
    "## Security and trust boundaries",
    "## Verification",
    "## Deployment and operations",
    "## Compatibility and change protocol",
]
MODULE_KINDS = {
    "library", "contract-library", "service", "adapter-service", "application"
}
LOGICAL_MODULES = {"shared", "hepta", "trnm"}
MODULE_STATUSES = {
    "active", "repository_candidate", "functional_alpha",
    "supporting_alpha", "contract_qualified",
}
EXTERNAL_COMPONENTS = {
    "nakama",
    "trillionnium-chain",
    "matrix-homeserver",
    "content-addressed-object-store",
    "external-providers-and-agents",
}
AUTHORITATIVE_WORKFLOWS = [
    ".github/workflows/p0-migration-gate.yml",
    ".github/workflows/rust-service-gate.yml",
    ".github/workflows/p0-gateway-exact-reserve-gate.yml",
    ".github/workflows/p0-execution-settlement-gate.yml",
    ".github/workflows/p0-provider-reconciliation-gate.yml",
]
RELEASE_WORKFLOW = ".github/workflows/p0-release-candidate-gate.yml"
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


def load_toml(relative: str) -> dict[str, Any]:
    raw = read_text(relative)
    if not raw:
        return {}
    try:
        value = tomllib.loads(raw)
    except tomllib.TOMLDecodeError as error:
        PROBLEMS.append(f"invalid TOML {relative}: {error}")
        return {}
    return value if isinstance(value, dict) else {}


def require_path(value: object, label: str) -> str | None:
    if not isinstance(value, str) or not value:
        PROBLEMS.append(f"{label} must be a nonempty repository path")
        return None
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value:
        PROBLEMS.append(f"{label} is not a canonical repository path: {value}")
        return None
    if not (ROOT / value).is_file():
        PROBLEMS.append(f"{label} references missing file: {value}")
    return value


def require_markers(relative: str, *markers: str) -> None:
    text = read_text(relative)
    for marker in markers:
        if marker not in text:
            PROBLEMS.append(f"{relative} lacks required marker: {marker}")


def has_machine_absolute_path(text: str) -> bool:
    return bool(
        re.search(r"(?:^|[\s`])/(?:home|data|Users)/", text)
        or re.search(r"\b[A-Za-z]:\\", text)
    )


def validate_authority() -> dict[str, Any]:
    authority = load_json(AUTHORITY)
    expected = {
        "schema": "cex.development-doc-authority.v1",
        "status": "active",
        "active_plan": PLAN,
        "active_addendum": ADDENDUM,
        "migration_head": MIGRATION_HEAD,
        "shared_trigger": TRIGGER,
        "qualification_freeze": TRIGGER,
        "production_authorization": "not_granted",
        "repository_qualification_result": QUALIFICATION_RESULT,
        "repository_qualification_authority": QUALIFICATION_AUTHORITY,
        "external_gate_policy": "independent_evidence_required",
    }
    for field, value in expected.items():
        if authority.get(field) != value:
            PROBLEMS.append(f"authority.{field} must equal {value!r}")

    for field in (
        "active_plan", "active_addendum", "shared_trigger",
        "qualification_freeze", "aggregate_release_workflow",
    ):
        require_path(authority.get(field), f"authority.{field}")

    workflows = authority.get("authoritative_workflows")
    if workflows != AUTHORITATIVE_WORKFLOWS:
        PROBLEMS.append("authority authoritative_workflows is incomplete or reordered")
    else:
        for index, path in enumerate(workflows):
            require_path(path, f"authority.authoritative_workflows[{index}]")
    if authority.get("aggregate_release_workflow") != RELEASE_WORKFLOW:
        PROBLEMS.append("authority aggregate release workflow is invalid")

    canonical = authority.get("canonical_documents")
    if not isinstance(canonical, dict) or set(canonical) != CANONICAL_KEYS:
        PROBLEMS.append("canonical document set is incomplete or contains additions")
    else:
        for key, value in canonical.items():
            path = require_path(value, f"authority.canonical_documents.{key}")
            if path and has_machine_absolute_path(read_text(path)):
                PROBLEMS.append(
                    f"canonical document contains a machine-specific path: {path}"
                )

    policy = authority.get("module_documentation_policy")
    expected_policy = {
        "workspace_source": "Cargo.toml",
        "catalog": MODULE_CATALOG,
        "index": "docs/modules/index.md",
        "coverage": "all_workspace_members",
        "required_sections": MODULE_SECTIONS,
        "exact_tree_bound": True,
    }
    if not isinstance(policy, dict):
        PROBLEMS.append("authority module_documentation_policy is missing")
    else:
        for field, value in expected_policy.items():
            if policy.get(field) != value:
                PROBLEMS.append(
                    f"authority.module_documentation_policy.{field} must equal {value!r}"
                )

    external = authority.get("external_production_scope")
    if (
        not isinstance(external, dict)
        or external.get("self_certifiable") is not False
        or external.get("status") != "blocked_upstream"
    ):
        PROBLEMS.append("external production scope must remain blocked and external")
    return authority


def workspace_members() -> list[str]:
    cargo = load_toml("Cargo.toml")
    workspace = cargo.get("workspace")
    raw = workspace.get("members") if isinstance(workspace, dict) else None
    if not isinstance(raw, list) or not raw:
        PROBLEMS.append("Cargo.toml workspace.members must be a nonempty list")
        return []
    members: list[str] = []
    for index, value in enumerate(raw):
        if not isinstance(value, str) or not value or "\\" in value:
            PROBLEMS.append(f"workspace.members[{index}] is invalid")
            continue
        path = Path(value)
        if path.is_absolute() or ".." in path.parts:
            PROBLEMS.append(f"workspace member escapes repository: {value}")
            continue
        if value in members:
            PROBLEMS.append(f"duplicate workspace member: {value}")
            continue
        require_path(f"{value}/Cargo.toml", f"workspace member {value}")
        members.append(value)
    return members


def validate_module_catalog(authority: dict[str, Any]) -> tuple[int, int]:
    catalog = load_json(MODULE_CATALOG)
    expected = {
        "schema": "cex.module-catalog.v1",
        "status": "active",
        "workspace_source": "Cargo.toml",
        "module_index": "docs/modules/index.md",
        "documentation_contract": ADDENDUM,
        "production_authorization": "not_granted",
        "required_document_sections": MODULE_SECTIONS,
    }
    for field, value in expected.items():
        if catalog.get(field) != value:
            PROBLEMS.append(f"module catalog {field} must equal {value!r}")

    canonical = authority.get("canonical_documents")
    if isinstance(canonical, dict):
        if canonical.get("module_catalog") != MODULE_CATALOG:
            PROBLEMS.append("authority does not bind module catalog")
        if canonical.get("module_index") != catalog.get("module_index"):
            PROBLEMS.append("authority/catalog module index mismatch")

    members = workspace_members()
    member_set = set(members)
    entries = catalog.get("modules")
    if not isinstance(entries, list):
        PROBLEMS.append("module catalog modules must be a list")
        entries = []

    seen_members: set[str] = set()
    seen_packages: set[str] = set()
    seen_docs: set[str] = set()
    index_text = read_text("docs/modules/index.md")
    for index, item in enumerate(entries):
        label = f"module_catalog.modules[{index}]"
        if not isinstance(item, dict):
            PROBLEMS.append(f"{label} must be an object")
            continue
        member = item.get("workspace_member")
        package = item.get("package")
        if not isinstance(member, str) or not member:
            PROBLEMS.append(f"{label}.workspace_member is invalid")
            continue
        if member in seen_members:
            PROBLEMS.append(f"duplicate catalog member: {member}")
        seen_members.add(member)
        if member not in member_set:
            PROBLEMS.append(f"catalog contains non-workspace member: {member}")

        manifest = load_toml(f"{member}/Cargo.toml")
        manifest_package = manifest.get("package")
        manifest_name = (
            manifest_package.get("name")
            if isinstance(manifest_package, dict)
            else None
        )
        if not isinstance(package, str) or package != manifest_name:
            PROBLEMS.append(
                f"{member} package mismatch: catalog={package!r} manifest={manifest_name!r}"
            )
        elif package in seen_packages:
            PROBLEMS.append(f"duplicate catalog package: {package}")
        else:
            seen_packages.add(package)

        if item.get("kind") not in MODULE_KINDS:
            PROBLEMS.append(f"{member} kind is invalid")
        if item.get("logical_module") not in LOGICAL_MODULES:
            PROBLEMS.append(f"{member} logical_module is invalid")
        if not isinstance(item.get("deployable"), bool):
            PROBLEMS.append(f"{member} deployable must be boolean")
        if item.get("status") not in MODULE_STATUSES:
            PROBLEMS.append(f"{member} status is invalid")
        if not isinstance(item.get("owner"), str) or not item["owner"].strip():
            PROBLEMS.append(f"{member} owner is missing")
        authority_text = item.get("authority")
        if not isinstance(authority_text, str) or len(authority_text.strip()) < 20:
            PROBLEMS.append(f"{member} authority boundary is missing")

        document = item.get("documentation")
        if not isinstance(document, str) or not document:
            PROBLEMS.append(f"{member} documentation path is missing")
        else:
            if document in seen_docs:
                PROBLEMS.append(f"module document reused: {document}")
            seen_docs.add(document)
            require_path(document, f"{member}.documentation")
            text = read_text(document)
            for marker in [
                f"Workspace member: `{member}`",
                f"Package: `{package}`",
                "Production authorization: `not_granted`",
                *MODULE_SECTIONS,
            ]:
                if marker not in text:
                    PROBLEMS.append(f"{document} lacks required marker: {marker}")
            if has_machine_absolute_path(text):
                PROBLEMS.append(f"machine-specific path in module document: {document}")
            if member not in index_text or str(package) not in index_text:
                PROBLEMS.append(f"module index lacks {member} / {package}")

        entrypoints = item.get("source_entrypoints")
        if not isinstance(entrypoints, list) or not entrypoints:
            PROBLEMS.append(f"{member} lacks source entry points")
        else:
            local_seen: set[str] = set()
            for entry_index, path in enumerate(entrypoints):
                label = f"{member}.source_entrypoints[{entry_index}]"
                if not isinstance(path, str) or not path:
                    PROBLEMS.append(f"{label} is invalid")
                    continue
                if path in local_seen:
                    PROBLEMS.append(f"{member} repeats source entry point: {path}")
                local_seen.add(path)
                if not path.startswith(member + "/"):
                    PROBLEMS.append(f"{label} escapes member boundary: {path}")
                require_path(path, label)

        verification = item.get("verification")
        if (
            not isinstance(verification, list)
            or not verification
            or any(not isinstance(command, str) or not command.strip()
                   for command in verification)
        ):
            PROBLEMS.append(f"{member} verification commands are incomplete")

    missing = member_set - seen_members
    extra = seen_members - member_set
    if missing or extra or len(entries) != len(members):
        PROBLEMS.append(
            "workspace/catalog mismatch: missing="
            + ",".join(sorted(missing))
            + " extra="
            + ",".join(sorted(extra))
            + f" workspace_count={len(members)} catalog_count={len(entries)}"
        )

    external = catalog.get("external_components")
    external_ids: set[str] = set()
    if not isinstance(external, list):
        PROBLEMS.append("module catalog external_components must be a list")
    else:
        for index, item in enumerate(external):
            label = f"external_components[{index}]"
            if not isinstance(item, dict):
                PROBLEMS.append(f"{label} must be an object")
                continue
            component_id = item.get("id")
            if not isinstance(component_id, str) or not component_id:
                PROBLEMS.append(f"{label}.id is invalid")
                continue
            if component_id in external_ids:
                PROBLEMS.append(f"duplicate external component: {component_id}")
            external_ids.add(component_id)
            if item.get("workspace_member") is not False:
                PROBLEMS.append(f"{component_id} must remain external")
            require_path(item.get("documentation"), f"{component_id}.documentation")
            if item.get("production_evidence") != "external":
                PROBLEMS.append(f"{component_id} production evidence must be external")
    if external_ids != EXTERNAL_COMPONENTS:
        PROBLEMS.append(
            "external component set mismatch: missing="
            + ",".join(sorted(EXTERNAL_COMPONENTS - external_ids))
            + " extra="
            + ",".join(sorted(external_ids - EXTERNAL_COMPONENTS))
        )
    return len(members), len(entries)


def validate_traceability(authority: dict[str, Any]) -> None:
    trace = load_json(TRACEABILITY)
    expected = {
        "schema": "cex.v12-requirement-traceability.v1",
        "status": "active",
        "active_plan": PLAN,
        "active_addendum": ADDENDUM,
        "migration_head": MIGRATION_HEAD,
        "production_authorization": "not_granted",
    }
    for field, value in expected.items():
        if trace.get(field) != value:
            PROBLEMS.append(f"traceability.{field} must equal {value!r}")
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
            PROBLEMS.append(f"duplicate requirement: {requirement_id}")
        seen.add(requirement_id)
        classification = item.get("classification")
        for field in ("source", "implementation", "verification", "gates"):
            paths = item.get(field)
            if not isinstance(paths, list):
                PROBLEMS.append(f"{requirement_id}.{field} must be a list")
                continue
            for path_index, path in enumerate(paths):
                require_path(path, f"{requirement_id}.{field}[{path_index}]")
        if not item.get("source"):
            PROBLEMS.append(f"{requirement_id} lacks a normative source")
        if classification == "repository_actionable":
            if (
                item.get("self_certifiable") is not True
                or item.get("status") != "required_hosted_exact_sha"
            ):
                PROBLEMS.append(f"{requirement_id} repository evidence policy is invalid")
            for field in ("implementation", "verification", "gates"):
                if not item.get(field):
                    PROBLEMS.append(f"{requirement_id} lacks executable {field}")
        elif classification == "external":
            if (
                item.get("self_certifiable") is not False
                or item.get("status") != "blocked_upstream"
                or item.get("implementation")
                or item.get("verification")
                or item.get("gates")
            ):
                PROBLEMS.append(f"{requirement_id} external evidence policy is invalid")
        else:
            PROBLEMS.append(f"{requirement_id} classification is invalid")
    if seen != REQUIREMENTS:
        PROBLEMS.append(
            "requirement set mismatch: missing="
            + ",".join(sorted(REQUIREMENTS - seen))
            + " extra="
            + ",".join(sorted(seen - REQUIREMENTS))
        )
    if authority and trace.get("active_plan") != authority.get("active_plan"):
        PROBLEMS.append("authority and traceability disagree on active plan")


def validate_repository_wiring() -> None:
    migrations = sorted((ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql"))
    if not migrations or migrations[-1].name != MIGRATION_HEAD:
        PROBLEMS.append("repository migration head does not match authority")

    require_markers(
        PLAN,
        "Definition of repository closure",
        "External gates that repository edits cannot self-certify",
        "not production-ready",
        f"Candidate migration head: `{MIGRATION_HEAD}`.",
    )
    require_markers(
        ADDENDUM,
        "Block H",
        "Block I",
        "Block J",
        "Block K",
        "complete workspace module documentation",
        "REPOSITORY_CLOSED_CANDIDATE",
        "External production gates remain upstream blockers",
    )
    require_markers(
        "docs/index.md",
        "Authority order",
        "Workspace module documentation",
        "Production authorization boundary",
        "scripts/check-development-docs.py",
        "module-catalog-v1.json",
    )
    require_markers(
        "readme.md",
        "This README is navigation only",
        "docs/index.md",
        "docs/modules/index.md",
        "Production authorization is not granted",
    )
    require_markers(
        "docs/modules/index.md",
        "Completion rule",
        "Workspace modules",
        "External authoritative components",
        "Nakama",
        "Trillionnium Chain",
    )
    require_markers(
        ".github/CODEOWNERS",
        "Source ownership routing only",
        "not branch-protection",
        "ProfHepta",
        "Franksudoman",
    )

    for pattern in (
        ".github/workflows/seq*-exact-sha-convergence.yml",
        ".github/workflows/seq*-exact-sha-convergence.yaml",
        ".github/workflows/*exact-sha-convergence*.yml",
        ".github/workflows/*exact-sha-convergence*.yaml",
    ):
        for path in ROOT.glob(pattern):
            PROBLEMS.append(
                "temporary exact-SHA convergence workflow remains: "
                + path.relative_to(ROOT).as_posix()
            )

    for workflow in [*AUTHORITATIVE_WORKFLOWS, RELEASE_WORKFLOW]:
        content = read_text(workflow)
        if TRIGGER not in content:
            PROBLEMS.append(f"{workflow} does not listen to shared trigger")
        if "workflow_dispatch:" not in content:
            PROBLEMS.append(f"{workflow} lacks manual recovery dispatch")

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
            PROBLEMS.append(f"rust-service-gate lacks required marker: {marker}")

    release_gate = read_text(RELEASE_WORKFLOW)
    for marker in (
        "scripts/check-development-docs.py",
        "scripts/check-repository-integrity.py",
        "HEPTA_REQUIRE_POSTGRES_TESTS: '1'",
        "scripts/check-hepta-postgres-integration.sh --mode recovery-only",
        "scripts/check-term-exchange-receipt-partial-upgrade-postgres.sh",
        "term-exchange-receipt-partial-upgrade-regression",
        "GITHUB_REF_TYPE",
        "release evidence requires a branch ref",
    ):
        if marker not in release_gate:
            PROBLEMS.append(f"release candidate gate lacks required marker: {marker}")

    require_markers(
        "services/hepta-research-league/tests/postgres_recovery.rs",
        "HEPTA_REQUIRE_POSTGRES_TESTS",
        "strict PostgreSQL integration test",
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

    lint = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-hepta-lint-ownership.py")],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if lint.returncode != 0:
        PROBLEMS.append("Hepta lint ownership contract failed: " + lint.stdout.strip())


def main() -> int:
    authority = validate_authority()
    workspace_count, catalog_count = validate_module_catalog(authority)
    validate_traceability(authority)
    validate_repository_wiring()
    result = {
        "schema": "cex.development-doc-check.v2",
        "status": "failed" if PROBLEMS else "ok",
        "ok": not PROBLEMS,
        "active_plan": PLAN,
        "active_addendum": ADDENDUM,
        "migration_head": MIGRATION_HEAD,
        "requirements": len(REQUIREMENTS),
        "workspace_members": workspace_count,
        "catalog_modules": catalog_count,
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Validate complete, one-to-one technical documentation for the Cargo workspace."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = ROOT / "docs/module-catalog-v1.json"
INDEX_PATH = ROOT / "docs/modules/index.md"
README_PATH = ROOT / "readme.md"
CODEOWNERS_PATH = ROOT / ".github/CODEOWNERS"
ADDENDUM = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
MODULE_INDEX_ROOT = Path("docs/modules")
REQUIRED_SECTIONS = (
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
)
MODULE_KINDS = {
    "library",
    "contract-library",
    "service",
    "adapter-service",
    "application",
}
LOGICAL_MODULES = {"shared", "hepta", "trnm"}
MODULE_STATUSES = {
    "active",
    "repository_candidate",
    "functional_alpha",
    "supporting_alpha",
    "contract_qualified",
}
EXPECTED_EXTERNAL_COMPONENTS = {
    "nakama",
    "trillionnium-chain",
    "matrix-homeserver",
    "content-addressed-object-store",
    "external-providers-and-agents",
}
MACHINE_PATH = re.compile(r"(?:^|[\s`])/(?:home|data|Users)/|\b[A-Za-z]:\\")
PROBLEMS: list[str] = []


def problem(message: str) -> None:
    PROBLEMS.append(message)


def repository_path(value: object, label: str, *, require_file: bool = True) -> Path | None:
    if not isinstance(value, str) or not value:
        problem(f"{label} must be a non-empty repository path")
        return None
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value:
        problem(f"{label} is not a canonical repository path: {value}")
        return None
    absolute = ROOT / path
    if require_file and not absolute.is_file():
        problem(f"{label} references a missing file: {value}")
    return absolute


def read_text(path: Path, label: str) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        problem(f"cannot read {label}: {error}")
        return ""


def load_json(path: Path, label: str) -> dict[str, Any]:
    raw = read_text(path, label)
    if not raw:
        return {}
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        problem(f"invalid JSON in {label}: {error}")
        return {}
    if not isinstance(value, dict):
        problem(f"{label} root must be an object")
        return {}
    return value


def load_toml(path: Path, label: str) -> dict[str, Any]:
    raw = read_text(path, label)
    if not raw:
        return {}
    try:
        value = tomllib.loads(raw)
    except tomllib.TOMLDecodeError as error:
        problem(f"invalid TOML in {label}: {error}")
        return {}
    if not isinstance(value, dict):
        problem(f"{label} root must be a table")
        return {}
    return value


def cargo_members() -> list[str]:
    cargo = load_toml(ROOT / "Cargo.toml", "Cargo.toml")
    workspace = cargo.get("workspace")
    values = workspace.get("members") if isinstance(workspace, dict) else None
    if not isinstance(values, list) or not values:
        problem("Cargo.toml workspace.members must be a non-empty array")
        return []
    members: list[str] = []
    for index, value in enumerate(values):
        if not isinstance(value, str) or not value:
            problem(f"workspace.members[{index}] must be a non-empty string")
            continue
        candidate = Path(value)
        if candidate.is_absolute() or ".." in candidate.parts or "\\" in value:
            problem(f"workspace member escapes repository: {value}")
            continue
        if value in members:
            problem(f"duplicate Cargo workspace member: {value}")
            continue
        if not (ROOT / value / "Cargo.toml").is_file():
            problem(f"workspace member manifest is missing: {value}/Cargo.toml")
        members.append(value)
    return members


def section_body(text: str, marker: str) -> str:
    start = text.find(marker)
    if start < 0:
        return ""
    body_start = start + len(marker)
    next_heading = text.find("\n## ", body_start)
    return text[body_start : next_heading if next_heading >= 0 else len(text)].strip()


def index_link_for_document(document: str) -> str | None:
    path = Path(document)
    try:
        relative = path.relative_to(MODULE_INDEX_ROOT)
    except ValueError:
        problem(
            f"module documentation must live below {MODULE_INDEX_ROOT.as_posix()}: {document}"
        )
        return None
    if not relative.parts or relative.suffix.lower() != ".md":
        problem(f"module documentation must be a Markdown file: {document}")
        return None
    return f"]({relative.as_posix()})"


def validate_document(
    *,
    member: str,
    package: str,
    document: str,
    owner: str,
    index_text: str,
) -> None:
    path = repository_path(document, f"{member}.documentation")
    if path is None or not path.is_file():
        return
    text = read_text(path, document)
    required_markers = (
        f"Workspace member: `{member}`",
        f"Package: `{package}`",
        f"Owner role: `{owner}`",
        "Production authorization: `not_granted`",
        *REQUIRED_SECTIONS,
    )
    for marker in required_markers:
        if marker not in text:
            problem(f"{document} lacks required marker: {marker}")
    for marker in REQUIRED_SECTIONS:
        body = section_body(text, marker)
        if body and len(body) < 80:
            problem(f"{document} section is too shallow: {marker}")
    if MACHINE_PATH.search(text):
        problem(f"{document} contains a machine-specific absolute path")
    lowered = text.lower()
    if "production authorization: `granted`" in lowered or "production-ready: true" in lowered:
        problem(f"{document} improperly claims production authorization")

    expected_link = index_link_for_document(document)
    if (
        member not in index_text
        or package not in index_text
        or expected_link is None
        or expected_link not in index_text
    ):
        problem(
            f"module index does not bind {member}, {package}, and the relative link for {document}"
        )


def validate_catalog() -> tuple[int, int]:
    catalog = load_json(CATALOG_PATH, "module catalog")
    expected_top = {
        "schema": "cex.module-catalog.v1",
        "status": "active",
        "workspace_source": "Cargo.toml",
        "module_index": "docs/modules/index.md",
        "documentation_contract": ADDENDUM,
        "production_authorization": "not_granted",
        "required_document_sections": list(REQUIRED_SECTIONS),
    }
    for field, expected in expected_top.items():
        if catalog.get(field) != expected:
            problem(f"module catalog {field} must equal {expected!r}")

    members = cargo_members()
    member_set = set(members)
    entries = catalog.get("modules")
    if not isinstance(entries, list):
        problem("module catalog modules must be an array")
        entries = []

    index_text = read_text(INDEX_PATH, "module index")
    seen_members: set[str] = set()
    seen_packages: set[str] = set()
    seen_documents: set[str] = set()
    for index, item in enumerate(entries):
        label = f"modules[{index}]"
        if not isinstance(item, dict):
            problem(f"{label} must be an object")
            continue
        member = item.get("workspace_member")
        package = item.get("package")
        owner = item.get("owner")
        document = item.get("documentation")
        if not isinstance(member, str) or not member:
            problem(f"{label}.workspace_member is invalid")
            continue
        if member in seen_members:
            problem(f"duplicate catalog workspace member: {member}")
        seen_members.add(member)
        if member not in member_set:
            problem(f"catalog contains a non-workspace member: {member}")

        manifest = load_toml(ROOT / member / "Cargo.toml", f"{member}/Cargo.toml")
        manifest_package = manifest.get("package")
        manifest_name = manifest_package.get("name") if isinstance(manifest_package, dict) else None
        if not isinstance(package, str) or not package or package != manifest_name:
            problem(
                f"{member} package mismatch: catalog={package!r}, manifest={manifest_name!r}"
            )
        elif package in seen_packages:
            problem(f"duplicate catalog package name: {package}")
        else:
            seen_packages.add(package)

        if item.get("kind") not in MODULE_KINDS:
            problem(f"{member} has an invalid module kind")
        if item.get("logical_module") not in LOGICAL_MODULES:
            problem(f"{member} has an invalid logical_module")
        if not isinstance(item.get("deployable"), bool):
            problem(f"{member} deployable must be boolean")
        if item.get("status") not in MODULE_STATUSES:
            problem(f"{member} has an invalid maturity status")
        if not isinstance(owner, str) or not owner.strip():
            problem(f"{member} owner is missing")
            owner = "<missing>"
        authority = item.get("authority")
        if not isinstance(authority, str) or len(authority.strip()) < 40:
            problem(f"{member} authority boundary is incomplete")

        if not isinstance(document, str) or not document:
            problem(f"{member} documentation path is missing")
        else:
            if document in seen_documents:
                problem(f"module document is reused: {document}")
            seen_documents.add(document)
            validate_document(
                member=member,
                package=str(package),
                document=document,
                owner=owner,
                index_text=index_text,
            )

        entrypoints = item.get("source_entrypoints")
        if not isinstance(entrypoints, list) or not entrypoints:
            problem(f"{member} source_entrypoints must be a non-empty array")
        else:
            local_seen: set[str] = set()
            for entry_index, entry in enumerate(entrypoints):
                entry_label = f"{member}.source_entrypoints[{entry_index}]"
                if not isinstance(entry, str) or not entry:
                    problem(f"{entry_label} is invalid")
                    continue
                if entry in local_seen:
                    problem(f"{member} repeats source entry point: {entry}")
                local_seen.add(entry)
                if not entry.startswith(member + "/"):
                    problem(f"{entry_label} escapes the workspace member: {entry}")
                repository_path(entry, entry_label)

        commands = item.get("verification")
        if (
            not isinstance(commands, list)
            or not commands
            or any(not isinstance(command, str) or not command.strip() for command in commands)
        ):
            problem(f"{member} verification commands are incomplete")

    missing = member_set - seen_members
    extra = seen_members - member_set
    if missing or extra or len(entries) != len(members):
        problem(
            "workspace/catalog mismatch: "
            f"missing={sorted(missing)}, extra={sorted(extra)}, "
            f"workspace_count={len(members)}, catalog_count={len(entries)}"
        )

    external = catalog.get("external_components")
    external_ids: set[str] = set()
    if not isinstance(external, list):
        problem("module catalog external_components must be an array")
        external = []
    for index, item in enumerate(external):
        label = f"external_components[{index}]"
        if not isinstance(item, dict):
            problem(f"{label} must be an object")
            continue
        component_id = item.get("id")
        if not isinstance(component_id, str) or not component_id:
            problem(f"{label}.id is invalid")
            continue
        if component_id in external_ids:
            problem(f"duplicate external component: {component_id}")
        external_ids.add(component_id)

        if item.get("workspace_member") is not False:
            problem(
                f"external component must explicitly set workspace_member=false: {component_id}"
            )
        kind = item.get("kind")
        if not isinstance(kind, str) or not kind.startswith("external_"):
            problem(f"external component kind is invalid: {component_id}")
        authority = item.get("authority")
        if not isinstance(authority, str) or len(authority.strip()) < 40:
            problem(f"external component authority is incomplete: {component_id}")
        repository_path(item.get("documentation"), f"{component_id}.documentation")
        if item.get("production_evidence") != "external":
            problem(f"external component production_evidence is invalid: {component_id}")
        if f"`{component_id}`" not in index_text:
            problem(f"module index lacks external component ID: {component_id}")

    if external_ids != EXPECTED_EXTERNAL_COMPONENTS:
        problem(
            "external component set mismatch: "
            f"missing={sorted(EXPECTED_EXTERNAL_COMPONENTS - external_ids)}, "
            f"extra={sorted(external_ids - EXPECTED_EXTERNAL_COMPONENTS)}"
        )

    return len(members), len(external_ids)


def validate_navigation_and_ownership() -> None:
    readme = read_text(README_PATH, "root readme")
    for marker in (
        "navigation only",
        "docs/index.md",
        "docs/modules/index.md",
        "docs/module-catalog-v1.json",
        "Production authorization is not granted",
    ):
        if marker not in readme:
            problem(f"root readme lacks required navigation marker: {marker}")
    if MACHINE_PATH.search(readme):
        problem("root readme contains a machine-specific absolute path")

    codeowners = read_text(CODEOWNERS_PATH, "CODEOWNERS")
    for marker in (
        "not branch-protection",
        "/docs/module-catalog-v1.json",
        "/.github/workflows/",
        "/migrations/",
    ):
        if marker not in codeowners:
            problem(f"CODEOWNERS lacks required marker: {marker}")

    for forbidden in (
        ".github/workflows/seq44-residual-gap-closure-v3.yml",
        "scripts/seq44_apply_v3.py",
        ".github/workflows/seq44-exact-sha-convergence.yml",
    ):
        if (ROOT / forbidden).exists():
            problem(f"temporary convergence/remediation artifact remains: {forbidden}")


def main() -> int:
    workspace_count, external_count = validate_catalog()
    validate_navigation_and_ownership()
    result = {
        "schema": "cex.module-documentation-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "workspace_member_count": workspace_count,
        "external_component_count": external_count,
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

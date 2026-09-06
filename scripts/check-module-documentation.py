#!/usr/bin/env python3
"""Validate one canonical module contract for every active Cargo workspace member."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = ROOT / "docs/module-catalog-v1.json"
STANDARD_PATH = ROOT / "docs/module-documentation-standard-v1.md"
WORKSPACE_PATH = ROOT / "Cargo.toml"
PROBLEMS: list[str] = []

REQUIRED_HEADINGS = (
    "## Scope",
    "## Non-goals",
    "## Authority and state ownership",
    "## Interfaces",
    "## Data and persistence",
    "## Security and configuration",
    "## Failure and recovery",
    "## Observability",
    "## Verification",
    "## Compatibility and retirement",
)
REQUIRED_ENTRY_FIELDS = (
    "id",
    "path",
    "kind",
    "lifecycle",
    "owner_domain",
    "document",
)
ALLOWED_KINDS = {"crate", "service", "app"}
ALLOWED_LIFECYCLES = {"active", "supporting_alpha", "experimental", "deprecated"}
MACHINE_PATH = re.compile(r"(?:^|[\s`])/(?:home|data|Users)/|\b[A-Za-z]:\\")


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        PROBLEMS.append(f"missing required file: {path.relative_to(ROOT)}")
        return {}
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        PROBLEMS.append(f"invalid JSON/UTF-8 {path.relative_to(ROOT)}: {error}")
        return {}
    if not isinstance(value, dict):
        PROBLEMS.append(f"JSON root must be an object: {path.relative_to(ROOT)}")
        return {}
    return value


def load_workspace_members() -> list[str]:
    try:
        workspace = tomllib.loads(WORKSPACE_PATH.read_text(encoding="utf-8"))
    except FileNotFoundError:
        PROBLEMS.append("missing Cargo.toml")
        return []
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        PROBLEMS.append(f"cannot parse Cargo.toml: {error}")
        return []
    members = workspace.get("workspace", {}).get("members")
    if not isinstance(members, list) or not members:
        PROBLEMS.append("Cargo.toml [workspace].members must be a nonempty list")
        return []
    normalized: list[str] = []
    for index, member in enumerate(members):
        if not isinstance(member, str) or not member.strip():
            PROBLEMS.append(f"workspace member {index} is not a nonempty string")
            continue
        value = Path(member).as_posix().strip("/")
        if value != member:
            PROBLEMS.append(f"workspace member must be normalized repository path: {member!r}")
        normalized.append(value)
    if len(normalized) != len(set(normalized)):
        PROBLEMS.append("Cargo workspace contains duplicate members")
    return normalized


def validate_text_file(relative: str, label: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        PROBLEMS.append(f"{label} references missing file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        PROBLEMS.append(f"{label} is not UTF-8: {relative}: {error}")
        return ""


def validate_entry(entry: Any, index: int, workspace_set: set[str]) -> tuple[str | None, str | None]:
    label = f"modules[{index}]"
    if not isinstance(entry, dict):
        PROBLEMS.append(f"{label} must be an object")
        return None, None

    for field in REQUIRED_ENTRY_FIELDS:
        if field not in entry:
            PROBLEMS.append(f"{label} lacks field {field}")

    module_id = entry.get("id")
    path = entry.get("path")
    document = entry.get("document")
    if not isinstance(module_id, str) or not module_id.strip():
        PROBLEMS.append(f"{label}.id must be a nonempty string")
        module_id = None
    if not isinstance(path, str) or not path.strip():
        PROBLEMS.append(f"{label}.path must be a nonempty string")
        path = None
    elif Path(path).as_posix().strip("/") != path:
        PROBLEMS.append(f"{label}.path must be normalized: {path!r}")
    elif path not in workspace_set:
        PROBLEMS.append(f"{label}.path is not an active Cargo workspace member: {path}")
    elif module_id and module_id != Path(path).name:
        PROBLEMS.append(f"{label}.id {module_id!r} must equal directory name {Path(path).name!r}")

    if entry.get("kind") not in ALLOWED_KINDS:
        PROBLEMS.append(f"{label}.kind must be one of {sorted(ALLOWED_KINDS)}")
    if entry.get("lifecycle") not in ALLOWED_LIFECYCLES:
        PROBLEMS.append(f"{label}.lifecycle must be one of {sorted(ALLOWED_LIFECYCLES)}")
    owner = entry.get("owner_domain")
    if not isinstance(owner, str) or not owner.strip():
        PROBLEMS.append(f"{label}.owner_domain must be a nonempty string")

    if not isinstance(document, str) or not document:
        PROBLEMS.append(f"{label}.document must be a nonempty repository path")
        return path, None
    expected_document = f"{path}/MODULE.md" if path else None
    if expected_document and document != expected_document:
        PROBLEMS.append(
            f"{label}.document must be colocated at {expected_document}, got {document}"
        )
    text = validate_text_file(document, f"{label}.document")
    if text:
        for marker in (
            f"Module path: `{path}`",
            "Status: active module documentation",
            "Production authorization: `not_granted`",
            *REQUIRED_HEADINGS,
        ):
            if marker not in text:
                PROBLEMS.append(f"{document} lacks required marker: {marker}")
        if MACHINE_PATH.search(text):
            PROBLEMS.append(f"{document} contains a machine-specific absolute path")
        if len(text.split()) < 120:
            PROBLEMS.append(f"{document} is too shallow to satisfy the module contract")
    return path, document


def main() -> int:
    workspace_members = load_workspace_members()
    workspace_set = set(workspace_members)
    standard = validate_text_file(
        STANDARD_PATH.relative_to(ROOT).as_posix(), "documentation_standard"
    )
    for marker in (
        "Workspace module documentation standard v1",
        "Cargo workspace",
        "Production authorization: `not_granted`",
        "scripts/check-module-documentation.py",
    ):
        if standard and marker not in standard:
            PROBLEMS.append(f"module documentation standard lacks marker: {marker}")

    catalog = load_json(CATALOG_PATH)
    if catalog.get("schema") != "cex.module-catalog.v1":
        PROBLEMS.append("module catalog schema is invalid")
    if catalog.get("status") != "active":
        PROBLEMS.append("module catalog status must be active")
    if catalog.get("production_authorization") != "not_granted":
        PROBLEMS.append("module catalog must deny production authorization")
    if catalog.get("workspace_manifest") != "Cargo.toml":
        PROBLEMS.append("module catalog workspace_manifest must be Cargo.toml")
    if catalog.get("documentation_standard") != "docs/module-documentation-standard-v1.md":
        PROBLEMS.append("module catalog documentation_standard is stale")

    modules = catalog.get("modules")
    if not isinstance(modules, list):
        PROBLEMS.append("module catalog modules must be a list")
        modules = []

    catalog_paths: list[str] = []
    ids: list[str] = []
    documents: list[str] = []
    for index, entry in enumerate(modules):
        path, document = validate_entry(entry, index, workspace_set)
        if path:
            catalog_paths.append(path)
        if document:
            documents.append(document)
        if isinstance(entry, dict) and isinstance(entry.get("id"), str):
            ids.append(entry["id"])

    if len(catalog_paths) != len(set(catalog_paths)):
        PROBLEMS.append("module catalog contains duplicate paths")
    if len(ids) != len(set(ids)):
        PROBLEMS.append("module catalog contains duplicate ids")
    if len(documents) != len(set(documents)):
        PROBLEMS.append("module catalog contains duplicate documents")

    catalog_set = set(catalog_paths)
    if catalog_set != workspace_set:
        PROBLEMS.append(
            "workspace/catalog mismatch: missing="
            + ",".join(sorted(workspace_set - catalog_set))
            + " extra="
            + ",".join(sorted(catalog_set - workspace_set))
        )
    if catalog_paths != workspace_members:
        PROBLEMS.append("module catalog order must match Cargo workspace member order")

    result = {
        "schema": "cex.module-documentation-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "workspace_members": len(workspace_members),
        "catalog_modules": len(modules),
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Emit an exact-tree integrity record for the active CEX v12 authority set."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))
from evidence_safe_io import (  # noqa: E402
    SafeIOError,
    read_json_nofollow,
    read_regular_nofollow,
    sha256_file_nofollow,
    write_json_nofollow,
)

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = ROOT / "docs/development-doc-authority-v1.json"
MODULE_CATALOG_PATH = ROOT / "docs/module-catalog-v1.json"


def run_git(*arguments: str) -> str:
    return subprocess.run(
        ["git", "-C", str(ROOT), *arguments],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def sha256_file(path: Path) -> str:
    try:
        return sha256_file_nofollow(path)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error


def digest_set(paths: Iterable[Path]) -> str:
    digest = hashlib.sha256()
    unique = {path.relative_to(ROOT).as_posix(): path for path in paths}
    for relative in sorted(unique):
        path = unique[relative]
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        try:
            digest.update(read_regular_nofollow(path, maximum=2**63 - 1))
        except SafeIOError as error:
            raise SystemExit(str(error)) from error
        digest.update(b"\0")
    return "sha256:" + digest.hexdigest()


def load_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = read_json_nofollow(path, label=label)
    except SafeIOError as error:
        raise SystemExit(str(error)) from error
    if not isinstance(value, dict):
        raise SystemExit(f"{label} root must be an object")
    return value


def module_document_paths(catalog: dict[str, Any]) -> tuple[list[Path], list[Path]]:
    modules = catalog.get("modules")
    if not isinstance(modules, list) or not modules:
        raise SystemExit("module catalog contains no modules")
    documents: list[Path] = []
    manifests: list[Path] = []
    seen_members: set[str] = set()
    seen_documents: set[str] = set()
    for index, item in enumerate(modules):
        if not isinstance(item, dict):
            raise SystemExit(f"module catalog entry {index} is not an object")
        member = item.get("workspace_member")
        document = item.get("documentation")
        if not isinstance(member, str) or not member:
            raise SystemExit(f"module catalog entry {index} lacks workspace_member")
        if not isinstance(document, str) or not document:
            raise SystemExit(f"module catalog entry {index} lacks documentation")
        if member in seen_members:
            raise SystemExit(f"duplicate module catalog member: {member}")
        if document in seen_documents:
            raise SystemExit(f"duplicate module catalog document: {document}")
        seen_members.add(member)
        seen_documents.add(document)
        manifest = ROOT / member / "Cargo.toml"
        module_document = ROOT / document
        if not manifest.is_file():
            raise SystemExit(f"module manifest is missing: {manifest.relative_to(ROOT)}")
        if not module_document.is_file():
            raise SystemExit(
                f"module document is missing: {module_document.relative_to(ROOT)}"
            )
        manifests.append(manifest)
        documents.append(module_document)
    return documents, manifests


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    parser.add_argument("--expected-sha")
    parser.add_argument("--expected-tree")
    args = parser.parse_args()

    docs_check = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-development-docs.py")],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if docs_check.returncode != 0:
        print(docs_check.stdout, end="")
        raise SystemExit("development document contract failed")

    if subprocess.run(["git", "-C", str(ROOT), "diff", "--quiet"]).returncode != 0:
        raise SystemExit("tracked worktree changes exist during integrity attestation")
    if subprocess.run(["git", "-C", str(ROOT), "diff", "--cached", "--quiet"]).returncode != 0:
        raise SystemExit("staged changes exist during integrity attestation")

    commit_sha = run_git("rev-parse", "HEAD")
    tree_sha = run_git("rev-parse", "HEAD^{tree}")
    if args.expected_sha and args.expected_sha != commit_sha:
        raise SystemExit("checked-out commit does not match expected SHA")
    if args.expected_tree and args.expected_tree != tree_sha:
        raise SystemExit("checked-out tree does not match expected tree")

    authority = load_json(AUTHORITY_PATH, "development authority")
    catalog = load_json(MODULE_CATALOG_PATH, "module catalog")

    canonical_values = authority.get("canonical_documents")
    if not isinstance(canonical_values, dict):
        raise SystemExit("development authority canonical_documents must be an object")
    canonical = [ROOT / value for value in canonical_values.values()]
    canonical.extend(
        [
            ROOT / authority["active_plan"],
            ROOT / authority["active_addendum"],
            AUTHORITY_PATH,
        ]
    )

    module_documents, module_manifests = module_document_paths(catalog)
    module_paths = [MODULE_CATALOG_PATH, *module_documents, *module_manifests]

    migration_paths = sorted(
        (ROOT / "migrations").glob("[0-9][0-9][0-9][0-9]_*.sql"),
        key=lambda path: path.name,
    )
    if not migration_paths:
        raise SystemExit("no numbered migrations found for integrity attestation")
    workflow_paths = [ROOT / path for path in authority["authoritative_workflows"]]
    workflow_paths.append(ROOT / authority["aggregate_release_workflow"])
    freeze_path = authority.get("qualification_freeze")
    if not isinstance(freeze_path, str) or not (ROOT / freeze_path).is_file():
        raise SystemExit("development authority qualification freeze is missing")

    try:
        documentation_check = json.loads(docs_check.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit(f"development document checker emitted invalid JSON: {error}") from error

    record = {
        "schema": "cex.repository-integrity.v2",
        "status": "ok",
        "ok": True,
        "commit_sha": commit_sha,
        "tree_sha": tree_sha,
        "repository_commit_sha": commit_sha,
        "repository_tree_sha": tree_sha,
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "active_plan": authority["active_plan"],
        "active_addendum": authority["active_addendum"],
        "migration_head": migration_paths[-1].name,
        "production_authorization": "not_granted",
        "workspace_documentation": {
            "schema": catalog.get("schema"),
            "status": catalog.get("status"),
            "workspace_source": catalog.get("workspace_source"),
            "module_count": len(module_documents),
            "external_component_count": len(catalog.get("external_components", [])),
            "coverage": "all_workspace_members",
        },
        "digests": {
            "cargo_lock": sha256_file(ROOT / "Cargo.lock"),
            "migration_chain": digest_set(migration_paths),
            "authoritative_workflows": digest_set(workflow_paths),
            "canonical_documents": digest_set(canonical),
            "module_documentation": digest_set(module_paths),
            "module_catalog": sha256_file(MODULE_CATALOG_PATH),
            "candidate_trigger": sha256_file(ROOT / authority["shared_trigger"]),
            "qualification_freeze": sha256_file(ROOT / freeze_path),
            "root_readme_observed_only": (
                sha256_file(ROOT / "readme.md")
                if (ROOT / "readme.md").is_file()
                else None
            ),
        },
        "documentation_check": documentation_check,
    }
    encoded = json.dumps(record, indent=2, sort_keys=True) + "\n"
    if args.output:
        output = args.output if args.output.is_absolute() else ROOT / args.output
        try:
            write_json_nofollow(output, record)
        except SafeIOError as error:
            raise SystemExit(str(error)) from error
    print(encoded, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Validate CEX release-baseline manifests without third-party Python packages."""

from __future__ import annotations

import argparse
import json
import re
from datetime import datetime
from pathlib import Path
from typing import Any

GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
RELEASE_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,127}$")
MIGRATION_RE = re.compile(r"^[0-9]{4}_[a-z0-9][a-z0-9._-]*\.sql$")
ZERO_GIT_SHA = "0" * 40
ZERO_SHA256 = "sha256:" + "0" * 64
QUALIFICATION_SCOPE = (
    "repository-exact-money-control-plane-plus-hepta-durability-doc-integrity-full-suite-lint-receipt-recovery-and-trnm-production-config-hardening"
)


def reject_unknown(item: dict[str, Any], allowed: set[str], path: str) -> None:
    unknown = sorted(set(item) - allowed)
    require(not unknown, f"{path} contains unknown field(s): {', '.join(unknown)}")


class ValidationError(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def object_at(value: Any, path: str) -> dict[str, Any]:
    require(isinstance(value, dict), f"{path} must be an object")
    return value


def string_at(value: Any, path: str, *, nonempty: bool = True) -> str:
    require(isinstance(value, str), f"{path} must be a string")
    if nonempty:
        require(bool(value.strip()), f"{path} must be non-empty")
    return value


def validate_datetime(value: Any, path: str) -> None:
    raw = string_at(value, path)
    try:
        datetime.fromisoformat(raw.replace("Z", "+00:00"))
    except ValueError as error:
        raise ValidationError(f"{path} must be RFC3339/date-time: {error}") from error


def validate_sha256(value: Any, path: str, allow_placeholder: bool) -> None:
    raw = string_at(value, path)
    require(bool(SHA256_RE.fullmatch(raw)), f"{path} must be canonical sha256:<64 lowercase hex>")
    if not allow_placeholder:
        require(raw != ZERO_SHA256, f"{path} must not be the all-zero placeholder")


def validate_artifact(value: Any, path: str, allow_placeholder: bool) -> None:
    item = object_at(value, path)
    string_at(item.get("name"), f"{path}.name")
    string_at(item.get("uri"), f"{path}.uri")
    validate_sha256(item.get("sha256"), f"{path}.sha256", allow_placeholder)


def validate_manifest(data: Any, allow_template: bool) -> None:
    root = object_at(data, "$" )
    reject_unknown(
        root,
        {
            "schema", "status", "qualification_scope", "production_ready",
            "production_authorization", "project_id", "release_id", "generated_at",
            "source", "dependencies", "database", "build", "evidence", "approvals",
            "external_gates", "revocation",
        },
        "$",
    )
    require(root.get("schema") == "cex.release-baseline-manifest.v1", "$.schema is invalid")
    require(root.get("project_id") == "hepta-control-plane", "$.project_id is invalid")

    qualification_scope = string_at(root.get("qualification_scope"), "$.qualification_scope")
    require(
        qualification_scope == QUALIFICATION_SCOPE,
        "$.qualification_scope is not the active v12 scope",
    )
    require(root.get("production_ready") is False, "$.production_ready must be false")
    require(
        root.get("production_authorization") == "not_granted",
        "$.production_authorization must be not_granted",
    )

    status = string_at(root.get("status"), "$.status")
    require(status in {"draft", "candidate", "released", "revoked"}, "$.status is invalid")
    release_id = string_at(root.get("release_id"), "$.release_id")
    require(bool(RELEASE_RE.fullmatch(release_id)), "$.release_id has invalid shape")
    validate_datetime(root.get("generated_at"), "$.generated_at")

    source = object_at(root.get("source"), "$.source")
    reject_unknown(source, {"repository", "branch", "commit_sha", "tree_sha"}, "$.source")
    require(source.get("repository") == "TrillionniumFoundation/CEX", "$.source.repository is invalid")
    string_at(source.get("branch"), "$.source.branch")
    for field in ("commit_sha", "tree_sha"):
        value = string_at(source.get(field), f"$.source.{field}")
        require(bool(GIT_SHA_RE.fullmatch(value)), f"$.source.{field} must be 40 lowercase hex")
        if not allow_template:
            require(value != ZERO_GIT_SHA, f"$.source.{field} must not be all-zero")

    dependencies = object_at(root.get("dependencies"), "$.dependencies")
    reject_unknown(dependencies, {"cargo_lock_sha256"}, "$.dependencies")
    validate_sha256(
        dependencies.get("cargo_lock_sha256"),
        "$.dependencies.cargo_lock_sha256",
        allow_template,
    )

    database = object_at(root.get("database"), "$.database")
    reject_unknown(
        database,
        {"migration_head", "migration_sha256", "migration_chain_sha256"},
        "$.database",
    )
    migration_head = string_at(database.get("migration_head"), "$.database.migration_head")
    require(bool(MIGRATION_RE.fullmatch(migration_head)), "$.database.migration_head is invalid")
    validate_sha256(database.get("migration_sha256"), "$.database.migration_sha256", allow_template)
    validate_sha256(
        database.get("migration_chain_sha256"),
        "$.database.migration_chain_sha256",
        allow_template,
    )

    build = object_at(root.get("build"), "$.build")
    reject_unknown(
        build,
        {"workflow_run_id", "artifacts", "images", "sbom", "provenance"},
        "$.build",
    )
    workflow_run_id = build.get("workflow_run_id")
    require(workflow_run_id is None or (isinstance(workflow_run_id, int) and workflow_run_id > 0),
            "$.build.workflow_run_id must be null or a positive integer")
    artifacts = build.get("artifacts")
    images = build.get("images")
    require(isinstance(artifacts, list), "$.build.artifacts must be an array")
    require(isinstance(images, list), "$.build.images must be an array")
    for index, artifact in enumerate(artifacts):
        reject_unknown(
            object_at(artifact, f"$.build.artifacts[{index}]"),
            {"name", "uri", "sha256"},
            f"$.build.artifacts[{index}]",
        )
        validate_artifact(artifact, f"$.build.artifacts[{index}]", allow_template)
    for index, image in enumerate(images):
        item = object_at(image, f"$.build.images[{index}]")
        reject_unknown(item, {"name", "digest"}, f"$.build.images[{index}]")
        string_at(item.get("name"), f"$.build.images[{index}].name")
        validate_sha256(item.get("digest"), f"$.build.images[{index}].digest", allow_template)
    for field in ("sbom", "provenance"):
        value = build.get(field)
        if value is not None:
            reject_unknown(
                object_at(value, f"$.build.{field}"),
                {"name", "uri", "sha256"},
                f"$.build.{field}",
            )
            validate_artifact(value, f"$.build.{field}", allow_template)

    external_gates = object_at(root.get("external_gates"), "$.external_gates")
    reject_unknown(external_gates, {"status", "items"}, "$.external_gates")
    require(
        external_gates.get("status") == "independent_approval_required",
        "$.external_gates.status is invalid",
    )
    gate_items = external_gates.get("items")
    require(isinstance(gate_items, list) and len(gate_items) == 8,
            "$.external_gates.items must contain exactly eight external gates")
    for index, item in enumerate(gate_items):
        string_at(item, f"$.external_gates.items[{index}]")

    evidence = root.get("evidence")
    require(isinstance(evidence, list) and evidence, "$.evidence must be a non-empty array")
    evidence_statuses: list[str] = []
    for index, evidence_item in enumerate(evidence):
        item = object_at(evidence_item, f"$.evidence[{index}]")
        reject_unknown(
            item,
            {"name", "status", "uri", "sha256", "waiver"},
            f"$.evidence[{index}]",
        )
        string_at(item.get("name"), f"$.evidence[{index}].name")
        evidence_status = string_at(item.get("status"), f"$.evidence[{index}].status")
        require(evidence_status in {"pending", "pass", "fail", "waived"},
                f"$.evidence[{index}].status is invalid")
        evidence_statuses.append(evidence_status)
        uri = item.get("uri")
        sha256 = item.get("sha256")
        if evidence_status in {"pass", "waived"}:
            string_at(uri, f"$.evidence[{index}].uri")
            validate_sha256(sha256, f"$.evidence[{index}].sha256", allow_template)
        else:
            require(uri is None or isinstance(uri, str), f"$.evidence[{index}].uri is invalid")
            require(sha256 is None or isinstance(sha256, str), f"$.evidence[{index}].sha256 is invalid")
        if evidence_status == "waived":
            string_at(item.get("waiver"), f"$.evidence[{index}].waiver")

    approvals = root.get("approvals")
    require(isinstance(approvals, list), "$.approvals must be an array")
    for index, approval in enumerate(approvals):
        item = object_at(approval, f"$.approvals[{index}]")
        reject_unknown(
            item,
            {"role", "actor", "decision", "decided_at", "scope"},
            f"$.approvals[{index}]",
        )
        string_at(item.get("role"), f"$.approvals[{index}].role")
        string_at(item.get("actor"), f"$.approvals[{index}].actor")
        require(item.get("decision") in {"approve", "reject", "revoke"},
                f"$.approvals[{index}].decision is invalid")
        validate_datetime(item.get("decided_at"), f"$.approvals[{index}].decided_at")
        string_at(item.get("scope"), f"$.approvals[{index}].scope")

    if status in {"candidate", "released"}:
        require(not allow_template, "candidate/released manifests cannot use template mode")
        require(workflow_run_id is not None, "candidate/released manifest requires workflow_run_id")
        require(bool(artifacts), "candidate/released manifest requires build artifacts")
        require(build.get("sbom") is not None, "candidate/released manifest requires SBOM")
        require(build.get("provenance") is not None, "candidate/released manifest requires provenance")
        require(all(value in {"pass", "waived"} for value in evidence_statuses),
                "candidate/released evidence must be pass or waived")
        require(any(item.get("decision") == "approve" for item in approvals),
                "candidate/released manifest requires approval")

    revocation = root.get("revocation")
    if status == "revoked":
        revocation = object_at(revocation, "$.revocation")
        reject_unknown(revocation, {"reason", "revoked_at", "actor"}, "$.revocation")
        string_at(revocation.get("reason"), "$.revocation.reason")
        string_at(revocation.get("actor"), "$.revocation.actor")
        validate_datetime(revocation.get("revoked_at"), "$.revocation.revoked_at")
    else:
        require(revocation is None, "$.revocation must be null unless status=revoked")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--allow-template", action="store_true")
    args = parser.parse_args()

    try:
        data = json.loads(args.manifest.read_text(encoding="utf-8"))
        validate_manifest(data, args.allow_template)
    except (OSError, json.JSONDecodeError, ValidationError) as error:
        print(f"release baseline validation failed: {error}")
        return 1

    print(f"release baseline validation passed: {args.manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

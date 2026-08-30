#!/usr/bin/env python3
"""Apply the strict v12 candidate-evidence contract to a generated manifest."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
ZERO_SHA256 = "sha256:" + "0" * 64
QUALIFICATION_SCOPE = (
    "repository-exact-money-control-plane-plus-hepta-durability-doc-integrity-"
    "full-suite-lint-receipt-recovery-and-trnm-production-config-hardening"
)
APPROVAL_SCOPE = (
    "repository candidate only; not production, financial, security, legal, "
    "or operations approval"
)
EXPECTED_EVIDENCE = {
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
}
EXPECTED_EXTERNAL_GATES = [
    "X1: production-like backup and restore rehearsal against representative data volume and the real storage topology",
    "X2: deployment/cutover and rollback rehearsal with real service identities, network policy and secret custody",
    "X3: real provider reconciliation artifacts for success, definite non-execution and indeterminate outcome",
    "X4: credential issuance, rotation, revocation and break-glass custody review",
    "X5: sustained production-like soak/endurance run with queue age, retry, reconciliation, parity and Audit delivery SLOs",
    "X6: independent security, operations and financial-control review",
    "X7: legal, commercial or provider approvals where the production integration requires them",
    "X8: final human go/no-go decision bound to the immutable release candidate",
]


class ContractError(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ContractError(message)


def object_at(value: Any, path: str) -> dict[str, Any]:
    require(isinstance(value, dict), f"{path} must be an object")
    return value


def nonempty_string(value: Any, path: str) -> str:
    require(isinstance(value, str) and bool(value.strip()), f"{path} must be non-empty")
    return value


def canonical_sha256(value: Any, path: str) -> str:
    raw = nonempty_string(value, path)
    require(bool(SHA256_RE.fullmatch(raw)), f"{path} must be canonical SHA-256")
    require(raw != ZERO_SHA256, f"{path} must not be a placeholder")
    return raw


def immutable_uri(value: Any, path: str, *, hosted: bool) -> str:
    raw = nonempty_string(value, path)
    require(not raw.startswith("file://"), f"{path} must not be a local mutable URI")
    if hosted:
        require(raw.startswith("gh://"), f"{path} must use an immutable gh:// run URI")
        require("/attempts/" in raw, f"{path} must bind an exact run attempt")
    else:
        require(raw.startswith("artifact://"), f"{path} must use artifact://")
    return raw


def validate_manifest(data: Any) -> None:
    root = object_at(data, "$")
    require(root.get("schema") == "cex.release-baseline-manifest.v1", "$.schema is invalid")
    require(root.get("status") == "candidate", "$.status must be candidate")
    require(root.get("qualification_scope") == QUALIFICATION_SCOPE, "$.qualification_scope is stale")
    require(root.get("production_ready") is False, "$.production_ready must be false")
    require(root.get("production_authorization") == "not_granted", "production authorization must remain denied")
    require(root.get("project_id") == "hepta-control-plane", "$.project_id is invalid")

    source = object_at(root.get("source"), "$.source")
    require(source.get("repository") == "TrillionniumFoundation/CEX", "source repository is invalid")
    nonempty_string(source.get("branch"), "$.source.branch")
    commit_sha = nonempty_string(source.get("commit_sha"), "$.source.commit_sha")
    tree_sha = nonempty_string(source.get("tree_sha"), "$.source.tree_sha")
    require(bool(GIT_SHA_RE.fullmatch(commit_sha)), "commit SHA must be 40 lowercase hex")
    require(bool(GIT_SHA_RE.fullmatch(tree_sha)), "tree SHA must be 40 lowercase hex")
    require(commit_sha != "0" * 40 and tree_sha != "0" * 40, "source identity must not be a placeholder")

    dependencies = object_at(root.get("dependencies"), "$.dependencies")
    canonical_sha256(dependencies.get("cargo_lock_sha256"), "$.dependencies.cargo_lock_sha256")
    database = object_at(root.get("database"), "$.database")
    require(database.get("migration_head") == "0087_add_term_exchange_receipt_event_history.sql", "migration head is stale")
    canonical_sha256(database.get("migration_sha256"), "$.database.migration_sha256")
    canonical_sha256(database.get("migration_chain_sha256"), "$.database.migration_chain_sha256")

    build = object_at(root.get("build"), "$.build")
    run_id = build.get("workflow_run_id")
    require(isinstance(run_id, int) and not isinstance(run_id, bool) and run_id > 0, "workflow_run_id is invalid")
    artifacts = build.get("artifacts")
    require(isinstance(artifacts, list) and len(artifacts) == 1, "candidate requires exactly one evidence payload")
    payload = object_at(artifacts[0], "$.build.artifacts[0]")
    payload_name = nonempty_string(payload.get("name"), "$.build.artifacts[0].name")
    require(payload_name.startswith(f"cex-p0-evidence-{commit_sha}-attempt-"), "payload name is not bound to commit/attempt")
    immutable_uri(payload.get("uri"), "$.build.artifacts[0].uri", hosted=True)
    canonical_sha256(payload.get("sha256"), "$.build.artifacts[0].sha256")
    require(build.get("images") == [], "candidate build images must be empty until separately qualified")
    for field in ("sbom", "provenance"):
        artifact = object_at(build.get(field), f"$.build.{field}")
        immutable_uri(artifact.get("uri"), f"$.build.{field}.uri", hosted=False)
        canonical_sha256(artifact.get("sha256"), f"$.build.{field}.sha256")

    evidence = root.get("evidence")
    require(isinstance(evidence, list), "$.evidence must be an array")
    names: list[str] = []
    for index, raw in enumerate(evidence):
        item = object_at(raw, f"$.evidence[{index}]")
        name = nonempty_string(item.get("name"), f"$.evidence[{index}].name")
        names.append(name)
        require(item.get("status") == "pass", f"evidence {name} must be pass; waivers are forbidden")
        require(item.get("waiver") is None, f"evidence {name} must not carry a waiver")
        immutable_uri(item.get("uri"), f"$.evidence[{index}].uri", hosted=name.startswith("hosted:"))
        canonical_sha256(item.get("sha256"), f"$.evidence[{index}].sha256")
    require(len(names) == len(set(names)), "evidence names must be unique")
    require(set(names) == EXPECTED_EVIDENCE, "candidate evidence set is incomplete or contains additions")

    approvals = root.get("approvals")
    require(isinstance(approvals, list) and len(approvals) == 1, "candidate requires exactly one repository automation approval")
    approval = object_at(approvals[0], "$.approvals[0]")
    require(approval.get("role") == "repository-qualification-automation", "approval role is invalid")
    require(approval.get("actor") == "github-actions[bot]", "approval actor is invalid")
    require(approval.get("decision") == "approve", "approval decision is invalid")
    require(approval.get("scope") == APPROVAL_SCOPE, "approval scope exceeds repository qualification")
    nonempty_string(approval.get("decided_at"), "$.approvals[0].decided_at")

    external = object_at(root.get("external_gates"), "$.external_gates")
    require(external.get("status") == "independent_approval_required", "external gate status is invalid")
    require(external.get("items") == EXPECTED_EXTERNAL_GATES, "external X1-X8 gate set/order is invalid")
    require(root.get("revocation") is None, "candidate revocation must be null")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    args = parser.parse_args()
    try:
        value = json.loads(args.manifest.read_text(encoding="utf-8"))
        validate_manifest(value)
    except (OSError, json.JSONDecodeError, ContractError) as error:
        print(f"strict release evidence contract failed: {error}")
        return 1
    print(f"strict release evidence contract passed: {args.manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

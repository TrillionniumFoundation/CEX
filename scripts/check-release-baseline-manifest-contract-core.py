#!/usr/bin/env python3
"""Enforce the exact fail-closed v12 candidate-manifest evidence contract."""

from __future__ import annotations

import argparse
import copy
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

QUALIFICATION_SCOPE = (
    "repository-exact-money-control-plane-plus-hepta-durability-doc-integrity-"
    "full-suite-lint-receipt-recovery-and-trnm-production-config-hardening"
)
REPOSITORY = "TrillionniumFoundation/CEX"
PROJECT_ID = "hepta-control-plane"
ACTIVE_MIGRATION_HEAD = "0088_enforce_provider_terminal_evidence_binding.sql"
ZERO_GIT_SHA = "0" * 40
ZERO_SHA256 = "sha256:" + "0" * 64
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
BRANCH_RE = re.compile(r"^[A-Za-z0-9._/-]+$")
PAYLOAD_NAME_RE = re.compile(
    r"^cex-p0-evidence-(?P<sha>[0-9a-f]{40})-attempt-(?P<attempt>[1-9][0-9]*)$"
)
HOSTED_URI_RE = re.compile(
    r"^gh://TrillionniumFoundation/CEX/actions/runs/"
    r"(?P<run>[1-9][0-9]*)/attempts/(?P<attempt>[1-9][0-9]*)$"
)
APPROVAL_SCOPE = (
    "repository candidate only; not production, financial, security, legal, or operations approval"
)
HOSTED_EVIDENCE = (
    "hosted:p0-migration-gate",
    "hosted:rust-service-gate",
    "hosted:p0-gateway-exact-reserve-gate",
    "hosted:p0-execution-settlement-gate",
    "hosted:p0-provider-reconciliation-gate",
)
LOCAL_EVIDENCE = {
    "candidate-hygiene": "candidate-hygiene.json",
    "repository-integrity": "repository-integrity.json",
    "hepta-postgres-integration": "hepta-postgres-integration.json",
    "migration-and-lifecycle-matrix": "database-lifecycle.json",
    "exact-ledger-soak": "exact-ledger-soak.json",
    "backup-restore": "backup-restore.json",
}
EXPECTED_EVIDENCE = set(HOSTED_EVIDENCE) | set(LOCAL_EVIDENCE)
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


def is_object(value: Any) -> bool:
    return isinstance(value, dict)


def is_strict_positive_int(value: Any) -> bool:
    return type(value) is int and value > 0


def is_nonzero_sha256(value: Any) -> bool:
    return isinstance(value, str) and bool(SHA256_RE.fullmatch(value)) and value != ZERO_SHA256


def is_utc_z(value: Any) -> bool:
    if not isinstance(value, str) or not value.endswith("Z"):
        return False
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        return False
    return parsed.tzinfo is not None and parsed.utcoffset() == timezone.utc.utcoffset(parsed)


def valid_branch(value: Any) -> bool:
    if not isinstance(value, str) or not BRANCH_RE.fullmatch(value):
        return False
    if value.startswith("/") or value.endswith("/") or "//" in value:
        return False
    if ".." in value or "@{" in value:
        return False
    if any(segment in {"", ".", ".."} for segment in value.split("/")):
        return False
    if value.startswith("refs/") or value == "HEAD":
        return False
    return True


def gate_ids(items: Any) -> list[str]:
    if not isinstance(items, list):
        return []
    result: list[str] = []
    for item in items:
        if not isinstance(item, str):
            result.append("")
            continue
        match = re.match(r"^(X[1-8]):", item)
        result.append(match.group(1) if match else "")
    return result


def validate_template(data: dict[str, Any]) -> list[str]:
    problems: list[str] = []
    if data.get("status") != "draft":
        problems.append("template status must be draft")
    if data.get("release_id") != "cex-p0-template":
        problems.append("template release_id must be cex-p0-template")
    if not is_utc_z(data.get("generated_at")):
        problems.append("template generated_at must be an explicit UTC Z timestamp")

    source = data.get("source")
    if not is_object(source):
        problems.append("template source must be an object")
    else:
        if source.get("repository") != REPOSITORY:
            problems.append("template source repository is invalid")
        if source.get("branch") != "REPLACE_BRANCH":
            problems.append("template branch must remain REPLACE_BRANCH")
        if source.get("commit_sha") != ZERO_GIT_SHA or source.get("tree_sha") != ZERO_GIT_SHA:
            problems.append("template commit/tree must remain explicit all-zero placeholders")

    database = data.get("database")
    if not is_object(database):
        problems.append("template database must be an object")
    elif database.get("migration_head") != ACTIVE_MIGRATION_HEAD:
        problems.append(
            "template database.migration_head must equal the active v12 head "
            f"{ACTIVE_MIGRATION_HEAD!r}"
        )

    build = data.get("build")
    if not is_object(build):
        problems.append("template build must be an object")
    else:
        if build.get("workflow_run_id") is not None:
            problems.append("template workflow_run_id must be null")
        if build.get("artifacts") != [] or build.get("images") != []:
            problems.append("template build artifacts/images must be empty")
        if build.get("sbom") is not None or build.get("provenance") is not None:
            problems.append("template SBOM/provenance must be null")

    evidence = data.get("evidence")
    if not isinstance(evidence, list) or not evidence:
        problems.append("template evidence must be a non-empty list")
    else:
        names: list[str] = []
        for index, item in enumerate(evidence):
            if not is_object(item):
                problems.append(f"template evidence[{index}] must be an object")
                continue
            name = item.get("name")
            if not isinstance(name, str) or not name:
                problems.append(f"template evidence[{index}].name is invalid")
            else:
                names.append(name)
            if item.get("status") != "pending":
                problems.append(f"template evidence[{index}] must remain pending")
            if item.get("uri") is not None or item.get("sha256") is not None:
                problems.append(f"template evidence[{index}] must not embed evidence")
            if item.get("waiver") is not None:
                problems.append(f"template evidence[{index}] must not embed a waiver")
        if len(names) != len(set(names)):
            problems.append("template evidence names must be unique")

    external = data.get("external_gates")
    items = external.get("items") if is_object(external) else None
    if gate_ids(items) != [f"X{index}" for index in range(1, 9)]:
        problems.append("template external gates must list X1 through X8 exactly once and in order")
    if data.get("approvals") != []:
        problems.append("template approvals must be empty")
    if data.get("revocation") is not None:
        problems.append("template revocation must be null")
    return problems


def validate_candidate(data: dict[str, Any]) -> list[str]:
    problems: list[str] = []
    if data.get("status") != "candidate":
        problems.append("v12 repository qualification accepts status=candidate only")
    if data.get("qualification_scope") != QUALIFICATION_SCOPE:
        problems.append("candidate qualification_scope is not the active v12 scope")
    if data.get("production_ready") is not False:
        problems.append("candidate production_ready must be false")
    if data.get("production_authorization") != "not_granted":
        problems.append("candidate production_authorization must be not_granted")
    if data.get("project_id") != PROJECT_ID:
        problems.append("candidate project_id is invalid")
    if not is_utc_z(data.get("generated_at")):
        problems.append("candidate generated_at must be an explicit UTC Z timestamp")

    source = data.get("source")
    if not is_object(source):
        problems.append("candidate source must be an object")
        return problems
    repository = source.get("repository")
    branch = source.get("branch")
    commit_sha = source.get("commit_sha")
    tree_sha = source.get("tree_sha")
    if repository != REPOSITORY:
        problems.append("candidate source repository is invalid")
    if not valid_branch(branch):
        problems.append("candidate source branch is not a canonical branch name")
    if not isinstance(commit_sha, str) or not GIT_SHA_RE.fullmatch(commit_sha) or commit_sha == ZERO_GIT_SHA:
        problems.append("candidate commit_sha must be a nonzero lowercase Git SHA")
    if not isinstance(tree_sha, str) or not GIT_SHA_RE.fullmatch(tree_sha) or tree_sha == ZERO_GIT_SHA:
        problems.append("candidate tree_sha must be a nonzero lowercase Git SHA")
    if data.get("release_id") != f"cex-p0-{commit_sha}":
        problems.append("candidate release_id must be bound to source.commit_sha")

    database = data.get("database")
    if not is_object(database):
        problems.append("candidate database must be an object")
    elif database.get("migration_head") != ACTIVE_MIGRATION_HEAD:
        problems.append(
            "candidate database.migration_head must equal the active v12 head "
            f"{ACTIVE_MIGRATION_HEAD!r}"
        )

    for path, value in (
        ("dependencies.cargo_lock_sha256", (data.get("dependencies") or {}).get("cargo_lock_sha256") if is_object(data.get("dependencies")) else None),
        ("database.migration_sha256", (data.get("database") or {}).get("migration_sha256") if is_object(data.get("database")) else None),
        ("database.migration_chain_sha256", (data.get("database") or {}).get("migration_chain_sha256") if is_object(data.get("database")) else None),
    ):
        if not is_nonzero_sha256(value):
            problems.append(f"candidate {path} must be a nonzero SHA-256")

    build = data.get("build")
    if not is_object(build):
        problems.append("candidate build must be an object")
        return problems
    run_id = build.get("workflow_run_id")
    if not is_strict_positive_int(run_id):
        problems.append("candidate workflow_run_id must be a strict positive integer")

    artifacts = build.get("artifacts")
    payload_name: str | None = None
    attempt: int | None = None
    if not isinstance(artifacts, list) or len(artifacts) != 1 or not is_object(artifacts[0]):
        problems.append("candidate build must contain exactly one payload artifact")
    else:
        artifact = artifacts[0]
        payload_name_value = artifact.get("name")
        match = PAYLOAD_NAME_RE.fullmatch(payload_name_value) if isinstance(payload_name_value, str) else None
        if match is None:
            problems.append("candidate payload artifact name is not SHA/attempt bound")
        else:
            payload_name = payload_name_value
            attempt = int(match.group("attempt"))
            if match.group("sha") != commit_sha:
                problems.append("candidate payload artifact name is bound to a different commit")
        expected_uri = (
            f"gh://{REPOSITORY}/actions/runs/{run_id}/attempts/{attempt}/artifacts/{payload_name}"
            if payload_name is not None and attempt is not None and is_strict_positive_int(run_id)
            else None
        )
        if artifact.get("uri") != expected_uri:
            problems.append("candidate payload artifact URI is not bound to run/attempt/name")
        if not is_nonzero_sha256(artifact.get("sha256")):
            problems.append("candidate payload artifact digest must be a nonzero SHA-256")

    if build.get("images") != []:
        problems.append("candidate images must remain empty until image provenance is implemented")

    for field, expected_name in (("sbom", "sbom.spdx.json"), ("provenance", "provenance.intoto.json")):
        item = build.get(field)
        if not is_object(item):
            problems.append(f"candidate build.{field} is required")
            continue
        if item.get("name") != expected_name:
            problems.append(f"candidate build.{field}.name is invalid")
        expected_uri = (
            f"artifact://{payload_name}/{expected_name}" if payload_name is not None else None
        )
        if item.get("uri") != expected_uri:
            problems.append(f"candidate build.{field}.uri is not payload-bound")
        if not is_nonzero_sha256(item.get("sha256")):
            problems.append(f"candidate build.{field}.sha256 must be nonzero")

    evidence = data.get("evidence")
    names: list[str] = []
    items_by_name: dict[str, dict[str, Any]] = {}
    if not isinstance(evidence, list):
        problems.append("candidate evidence must be a list")
    else:
        for index, item in enumerate(evidence):
            if not is_object(item):
                problems.append(f"candidate evidence[{index}] must be an object")
                continue
            name = item.get("name")
            if not isinstance(name, str) or not name:
                problems.append(f"candidate evidence[{index}].name is invalid")
                continue
            names.append(name)
            if name in items_by_name:
                problems.append(f"candidate evidence name is duplicated: {name}")
            items_by_name[name] = item
            if item.get("status") != "pass":
                problems.append(f"candidate evidence {name} must be pass; waivers are forbidden")
            if item.get("waiver") is not None:
                problems.append(f"candidate evidence {name} must not contain a waiver")
            if not is_nonzero_sha256(item.get("sha256")):
                problems.append(f"candidate evidence {name} must have a nonzero SHA-256")

        actual = set(names)
        if actual != EXPECTED_EVIDENCE or len(names) != len(EXPECTED_EVIDENCE):
            problems.append(
                "candidate evidence set mismatch: "
                f"missing={sorted(EXPECTED_EVIDENCE - actual)} "
                f"extra={sorted(actual - EXPECTED_EVIDENCE)}"
            )

        for name in HOSTED_EVIDENCE:
            item = items_by_name.get(name)
            if item is None:
                continue
            uri = item.get("uri")
            if not isinstance(uri, str) or HOSTED_URI_RE.fullmatch(uri) is None:
                problems.append(f"candidate hosted evidence URI is invalid: {name}")

        for name, relative in LOCAL_EVIDENCE.items():
            item = items_by_name.get(name)
            if item is None:
                continue
            expected_uri = (
                f"artifact://{payload_name}/{relative}" if payload_name is not None else None
            )
            if item.get("uri") != expected_uri:
                problems.append(f"candidate local evidence URI is not payload-bound: {name}")

        observed_uris = [
            item.get("uri")
            for item in items_by_name.values()
            if isinstance(item.get("uri"), str)
        ]
        if len(observed_uris) != len(set(observed_uris)):
            problems.append("candidate evidence URIs must be unique")

    approvals = data.get("approvals")
    if not isinstance(approvals, list) or len(approvals) != 1 or not is_object(approvals[0]):
        problems.append("candidate must contain exactly one repository-automation approval")
    else:
        approval = approvals[0]
        expected = {
            "role": "repository-qualification-automation",
            "actor": "github-actions[bot]",
            "decision": "approve",
            "scope": APPROVAL_SCOPE,
        }
        for field, value in expected.items():
            if approval.get(field) != value:
                problems.append(f"candidate approval.{field} is invalid")
        if not is_utc_z(approval.get("decided_at")):
            problems.append("candidate approval.decided_at must be an explicit UTC Z timestamp")

    external = data.get("external_gates")
    if not is_object(external):
        problems.append("candidate external_gates must be an object")
    else:
        if external.get("status") != "independent_approval_required":
            problems.append("candidate external_gates.status is invalid")
        if tuple(external.get("items") or ()) != EXTERNAL_GATES:
            problems.append("candidate external gates must equal the canonical X1-X8 contract")

    if data.get("revocation") is not None:
        problems.append("candidate revocation must be null")
    return problems


def contract_problems(data: Any, allow_template: bool) -> list[str]:
    if not is_object(data):
        return ["manifest root must be an object"]
    return validate_template(data) if allow_template else validate_candidate(data)


def valid_candidate_fixture() -> dict[str, Any]:
    sha = "a" * 40
    tree = "b" * 40
    digest = "sha256:" + "c" * 64
    run_id = 123
    attempt = 2
    payload_name = f"cex-p0-evidence-{sha}-attempt-{attempt}"
    evidence: list[dict[str, Any]] = []
    for index, name in enumerate(HOSTED_EVIDENCE, start=1):
        evidence.append(
            {
                "name": name,
                "status": "pass",
                "uri": f"gh://{REPOSITORY}/actions/runs/{456 + index}/attempts/1",
                "sha256": digest,
                "waiver": None,
            }
        )
    for name, relative in LOCAL_EVIDENCE.items():
        evidence.append(
            {
                "name": name,
                "status": "pass",
                "uri": f"artifact://{payload_name}/{relative}",
                "sha256": digest,
                "waiver": None,
            }
        )
    return {
        "schema": "cex.release-baseline-manifest.v1",
        "status": "candidate",
        "qualification_scope": QUALIFICATION_SCOPE,
        "production_ready": False,
        "production_authorization": "not_granted",
        "project_id": PROJECT_ID,
        "release_id": f"cex-p0-{sha}",
        "generated_at": "2026-08-30T12:00:00Z",
        "source": {
            "repository": REPOSITORY,
            "branch": "fix/evidence-hardening",
            "commit_sha": sha,
            "tree_sha": tree,
        },
        "dependencies": {"cargo_lock_sha256": digest},
        "database": {
            "migration_head": "0088_enforce_provider_terminal_evidence_binding.sql",
            "migration_sha256": digest,
            "migration_chain_sha256": digest,
        },
        "build": {
            "workflow_run_id": run_id,
            "artifacts": [
                {
                    "name": payload_name,
                    "uri": f"gh://{REPOSITORY}/actions/runs/{run_id}/attempts/{attempt}/artifacts/{payload_name}",
                    "sha256": digest,
                }
            ],
            "images": [],
            "sbom": {
                "name": "sbom.spdx.json",
                "uri": f"artifact://{payload_name}/sbom.spdx.json",
                "sha256": digest,
            },
            "provenance": {
                "name": "provenance.intoto.json",
                "uri": f"artifact://{payload_name}/provenance.intoto.json",
                "sha256": digest,
            },
        },
        "evidence": evidence,
        "approvals": [
            {
                "role": "repository-qualification-automation",
                "actor": "github-actions[bot]",
                "decision": "approve",
                "decided_at": "2026-08-30T12:00:01Z",
                "scope": APPROVAL_SCOPE,
            }
        ],
        "external_gates": {
            "status": "independent_approval_required",
            "items": list(EXTERNAL_GATES),
        },
        "revocation": None,
    }


def self_test() -> list[str]:
    failures: list[str] = []
    fixture = valid_candidate_fixture()
    if contract_problems(fixture, False):
        failures.append("valid candidate fixture was rejected")

    mutations: list[tuple[str, Any]] = []

    waived = copy.deepcopy(fixture)
    waived["evidence"][0]["status"] = "waived"
    waived["evidence"][0]["waiver"] = "not allowed"
    mutations.append(("waived evidence", waived))

    duplicate = copy.deepcopy(fixture)
    duplicate["evidence"][-1]["name"] = duplicate["evidence"][0]["name"]
    mutations.append(("duplicate evidence", duplicate))

    bool_run = copy.deepcopy(fixture)
    bool_run["build"]["workflow_run_id"] = True
    mutations.append(("boolean workflow run id", bool_run))

    naive_time = copy.deepcopy(fixture)
    naive_time["generated_at"] = "2026-08-30T12:00:00"
    mutations.append(("naive timestamp", naive_time))

    duplicate_external = copy.deepcopy(fixture)
    duplicate_external["external_gates"]["items"][-1] = duplicate_external["external_gates"]["items"][0]
    mutations.append(("duplicate external gate", duplicate_external))

    local_uri = copy.deepcopy(fixture)
    local_uri["build"]["artifacts"][0]["uri"] = "file:///tmp/evidence"
    mutations.append(("mutable local artifact URI", local_uri))

    broad_approval = copy.deepcopy(fixture)
    broad_approval["approvals"][0]["scope"] = "production approved"
    mutations.append(("overbroad approval", broad_approval))

    duplicate_uri = copy.deepcopy(fixture)
    duplicate_uri["evidence"][1]["uri"] = duplicate_uri["evidence"][0]["uri"]
    mutations.append(("duplicate evidence URI", duplicate_uri))

    stale_migration = copy.deepcopy(fixture)
    stale_migration["database"]["migration_head"] = "0086_add_trnm_native_receipt_evidence.sql"
    mutations.append(("stale migration head", stale_migration))

    for name, mutated in mutations:
        if not contract_problems(mutated, False):
            failures.append(f"negative self-test accepted {name}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--allow-template", action="store_true")
    args = parser.parse_args()

    problems = [f"checker self-test failed: {item}" for item in self_test()]
    try:
        data = json.loads(args.manifest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        problems.append(f"cannot read manifest: {error}")
        data = None
    if data is not None:
        problems.extend(contract_problems(data, args.allow_template))

    result = {
        "schema": "cex.release-baseline-manifest-contract.v1",
        "status": "failed" if problems else "ok",
        "mode": "template" if args.allow_template else "candidate",
        "manifest": str(args.manifest),
        "expected_candidate_evidence": sorted(EXPECTED_EVIDENCE),
        "problems": problems,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())

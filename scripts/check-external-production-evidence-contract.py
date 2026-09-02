#!/usr/bin/env python3
"""Validate the external-production evidence intake contract without self-certifying it."""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "docs/external-production-evidence-contract-v1.md"
TEMPLATE = ROOT / "docs/templates/cex-external-production-evidence-bundle-v1.json"
TRACEABILITY = ROOT / "docs/traceability/v12-requirements-v1.json"
ADDENDUM = ROOT / "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12-IMPLEMENTATION-ADDENDUM.md"
INDEX = ROOT / "docs/index.md"
README = ROOT / "readme.md"
LIVE_SOURCE_ROOT = ROOT / "docs/external-production-evidence"
GATES = tuple(f"V12-X{index}" for index in range(1, 9))
REQUIRED_ROLES = {
    "V12-X1": "independent_operations_recovery_owner",
    "V12-X2": "independent_deployment_operations_owner",
    "V12-X3": "real_provider_reconciliation_owner",
    "V12-X4": "independent_security_custody_owner",
    "V12-X5": "independent_sre_capacity_owner",
    "V12-X6": "independent_security_operations_financial_reviewers",
    "V12-X7": "responsible_legal_commercial_provider_authority",
    "V12-X8": "final_human_release_authority",
}
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
MIGRATION_RE = re.compile(r"^[0-9]{4}_[a-z0-9][a-z0-9._-]*\.sql$")
ALLOWED_URI_SCHEMES = {"https", "s3", "gs", "artifact", "gh", "ipfs"}
ROOT_FIELDS = {
    "schema",
    "status",
    "template",
    "candidate",
    "repository_candidate_manifest",
    "generated_at",
    "retention_policy_id",
    "production_authorization",
    "gates",
    "final_human_decision",
    "revocations",
}
CANDIDATE_FIELDS = {
    "repository",
    "commit_sha",
    "tree_sha",
    "migration_head",
    "artifact_scope",
}
MANIFEST_FIELDS = {"uri", "sha256"}
GATE_FIELDS = {
    "id",
    "classification",
    "self_certifiable",
    "status",
    "required_issuer_role",
    "evidence",
}
EVIDENCE_FIELDS = {
    "uri",
    "sha256",
    "issuer",
    "executed_at",
    "decision",
    "scope",
    "candidate_commit_sha",
    "candidate_tree_sha",
    "waiver",
}
ISSUER_FIELDS = {
    "actor_id",
    "organization",
    "role",
    "independent_of_repository_automation",
}
FINAL_FIELDS = {
    "decision",
    "uri",
    "sha256",
    "decided_at",
    "actor_id",
    "organization",
    "role",
    "scope",
    "candidate_commit_sha",
    "candidate_tree_sha",
}
PROBLEMS: list[str] = []


def problem(message: str) -> None:
    PROBLEMS.append(message)


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


def exact_fields(value: object, expected: set[str], label: str) -> dict[str, Any] | None:
    if not isinstance(value, dict):
        problem(f"{label} must be an object")
        return None
    if set(value) != expected:
        problem(
            f"{label} field set is not canonical: "
            f"missing={sorted(expected - set(value))}, extra={sorted(set(value) - expected)}"
        )
    return value


def utc_timestamp(value: object, label: str) -> None:
    if not isinstance(value, str) or not value.endswith("Z"):
        problem(f"{label} must be a UTC RFC3339 timestamp ending in Z")
        return
    try:
        datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        problem(f"{label} is not a valid RFC3339 timestamp")


def immutable_uri(value: object, label: str) -> None:
    if not isinstance(value, str) or not value or any(ch.isspace() for ch in value):
        problem(f"{label} must be a non-empty immutable URI")
        return
    parsed = urlsplit(value)
    if parsed.scheme not in ALLOWED_URI_SCHEMES:
        problem(f"{label} uses an unsupported or local URI scheme")
    if parsed.username is not None or parsed.password is not None:
        problem(f"{label} contains embedded credentials")
    if parsed.query or parsed.fragment:
        problem(f"{label} may not use mutable/signed query or fragment material")
    if parsed.scheme == "https" and not parsed.netloc:
        problem(f"{label} HTTPS URI lacks a host")
    if parsed.scheme in {"artifact", "gh", "s3", "gs", "ipfs"} and not (
        parsed.netloc or parsed.path.strip("/")
    ):
        problem(f"{label} lacks an immutable object identity")


def sha256(value: object, label: str) -> None:
    if not isinstance(value, str) or not DIGEST_RE.fullmatch(value):
        problem(f"{label} must be sha256:<64 lowercase hex>")


def nonempty(value: object, label: str, minimum: int = 1) -> None:
    if not isinstance(value, str) or len(value.strip()) < minimum:
        problem(f"{label} must be a non-empty string")


def validate_traceability_wiring() -> None:
    trace = load_json(TRACEABILITY, "v12 traceability")
    requirements = trace.get("requirements")
    if not isinstance(requirements, list):
        problem("v12 traceability requirements must be an array")
        return
    v12_h = next(
        (
            item
            for item in requirements
            if isinstance(item, dict) and item.get("id") == "V12-H"
        ),
        None,
    )
    if not isinstance(v12_h, dict):
        problem("V12-H traceability entry is missing")
        return
    expected_paths = {
        "docs/external-production-evidence-contract-v1.md",
        "docs/templates/cex-external-production-evidence-bundle-v1.json",
        "scripts/check-external-production-evidence-contract.py",
        "scripts/check-development-docs.py",
    }
    observed = {
        str(path)
        for field in ("source", "implementation", "verification")
        for path in (v12_h.get(field) if isinstance(v12_h.get(field), list) else [])
    }
    missing = expected_paths - observed
    if missing:
        problem(
            "V12-H traceability does not bind the external evidence intake contract: "
            + ",".join(sorted(missing))
        )


def validate_contract_source() -> None:
    text = read_text(CONTRACT, "external evidence contract")
    for marker in (
        "Status: active operational evidence contract",
        "Production authorization: `not_granted`",
        "does **not** allow repository source",
        "V12-X1",
        "V12-X8",
        "self-certify",
        "--contract-only",
        "--bundle",
        "Automation may validate structure but may not emit",
    ):
        if marker not in text:
            problem(f"external evidence contract lacks required marker: {marker}")

    addendum = read_text(ADDENDUM, "active implementation addendum")
    for marker in (
        "Block L",
        "external production evidence intake",
        "checker_may_grant_production_authorization=false",
    ):
        if marker not in addendum:
            problem(f"active implementation addendum lacks Block L marker: {marker}")

    index = read_text(INDEX, "documentation index")
    for marker in (
        "External production evidence intake",
        "docs/external-production-evidence-contract-v1.md",
        "docs/templates/cex-external-production-evidence-bundle-v1.json",
        "scripts/check-external-production-evidence-contract.py",
    ):
        if marker not in index:
            problem(f"documentation index lacks external evidence marker: {marker}")

    readme = read_text(README, "root readme")
    for marker in (
        "docs/external-production-evidence-contract-v1.md",
        "docs/templates/cex-external-production-evidence-bundle-v1.json",
        "check-external-production-evidence-contract.py --contract-only",
    ):
        if marker not in readme:
            problem(f"root readme lacks external evidence navigation marker: {marker}")

    validate_traceability_wiring()

    if LIVE_SOURCE_ROOT.exists():
        for path in sorted(LIVE_SOURCE_ROOT.rglob("*")):
            if path.is_file():
                problem(
                    "live external evidence must not be committed into the source tree: "
                    + path.relative_to(ROOT).as_posix()
                )


def validate_template() -> None:
    template = load_json(TEMPLATE, "external evidence template")
    if set(template) != ROOT_FIELDS:
        problem("external evidence template root field set is not canonical")
    if template.get("schema") != "cex.external-production-evidence-bundle.v1":
        problem("external evidence template schema is invalid")
    if template.get("status") != "template" or template.get("template") is not True:
        problem("external evidence template must remain a shape-only template")
    if template.get("production_authorization") != "not_granted":
        problem("external evidence template must deny production authorization")

    candidate = exact_fields(template.get("candidate"), CANDIDATE_FIELDS, "template.candidate")
    if candidate is not None:
        if candidate.get("repository") != "TrillionniumFoundation/CEX":
            problem("external evidence template repository is invalid")
        for field in ("commit_sha", "tree_sha", "migration_head", "artifact_scope"):
            if candidate.get(field) is not None:
                problem(f"external evidence template candidate.{field} must be null")

    manifest = exact_fields(
        template.get("repository_candidate_manifest"),
        MANIFEST_FIELDS,
        "template.repository_candidate_manifest",
    )
    if manifest is not None and manifest != {"uri": None, "sha256": None}:
        problem("external evidence template manifest reference must remain empty")

    for field in ("generated_at", "retention_policy_id", "final_human_decision"):
        if template.get(field) is not None:
            problem(f"external evidence template {field} must be null")
    if template.get("revocations") != []:
        problem("external evidence template revocations must be empty")

    gates = template.get("gates")
    if not isinstance(gates, list) or len(gates) != len(GATES):
        problem("external evidence template must contain exactly V12-X1 through V12-X8")
        return
    observed: list[str] = []
    for index, gate_value in enumerate(gates):
        label = f"template.gates[{index}]"
        gate = exact_fields(gate_value, GATE_FIELDS, label)
        if gate is None:
            continue
        gate_id = gate.get("id")
        observed.append(str(gate_id))
        if gate.get("classification") != "external" or gate.get("self_certifiable") is not False:
            problem(f"{label} must remain external and non-self-certifiable")
        if gate.get("status") != "missing" or gate.get("evidence") != []:
            problem(f"{label} template may not claim evidence or closure")
        if gate.get("required_issuer_role") != REQUIRED_ROLES.get(str(gate_id)):
            problem(f"{label} required issuer role is invalid")
    if tuple(observed) != GATES:
        problem("external evidence template gate order/identity is invalid")


def validate_evidence_record(
    record_value: object,
    *,
    label: str,
    commit_sha: str,
    tree_sha: str,
    required_role: str,
) -> str | None:
    record = exact_fields(record_value, EVIDENCE_FIELDS, label)
    if record is None:
        return None
    immutable_uri(record.get("uri"), f"{label}.uri")
    sha256(record.get("sha256"), f"{label}.sha256")
    utc_timestamp(record.get("executed_at"), f"{label}.executed_at")
    decision = record.get("decision")
    if decision not in {"pass", "fail"}:
        problem(f"{label}.decision must be pass or fail")
    nonempty(record.get("scope"), f"{label}.scope", 20)
    if record.get("candidate_commit_sha") != commit_sha:
        problem(f"{label} is bound to another candidate commit")
    if record.get("candidate_tree_sha") != tree_sha:
        problem(f"{label} is bound to another candidate tree")
    if record.get("waiver") is not None:
        problem(f"{label} may not contain a waiver")

    issuer = exact_fields(record.get("issuer"), ISSUER_FIELDS, f"{label}.issuer")
    if issuer is not None:
        nonempty(issuer.get("actor_id"), f"{label}.issuer.actor_id")
        nonempty(issuer.get("organization"), f"{label}.issuer.organization")
        if issuer.get("role") != required_role:
            problem(
                f"{label}.issuer.role must equal the gate's required role {required_role!r}"
            )
        if issuer.get("independent_of_repository_automation") is not True:
            problem(
                f"{label}.issuer must explicitly be independent of repository automation"
            )
    return decision if isinstance(decision, str) else None


def validate_bundle(path: Path) -> bool:
    bundle = load_json(path, "external evidence bundle")
    if set(bundle) != ROOT_FIELDS:
        problem(
            "external evidence bundle root field set is not canonical: "
            f"missing={sorted(ROOT_FIELDS - set(bundle))}, "
            f"extra={sorted(set(bundle) - ROOT_FIELDS)}"
        )
    if bundle.get("schema") != "cex.external-production-evidence-bundle.v1":
        problem("external evidence bundle schema is invalid")
    if bundle.get("status") != "evidence_bundle" or bundle.get("template") is not False:
        problem("real external evidence bundle status/template flags are invalid")
    if bundle.get("production_authorization") != "not_granted":
        problem("structural evidence intake may not itself grant production authorization")

    candidate = exact_fields(bundle.get("candidate"), CANDIDATE_FIELDS, "candidate")
    if candidate is None:
        return False
    if candidate.get("repository") != "TrillionniumFoundation/CEX":
        problem("external evidence bundle repository is invalid")
    commit_sha = candidate.get("commit_sha")
    tree_sha = candidate.get("tree_sha")
    if not isinstance(commit_sha, str) or not SHA_RE.fullmatch(commit_sha):
        problem("external evidence bundle commit SHA is invalid")
        commit_sha = ""
    if not isinstance(tree_sha, str) or not SHA_RE.fullmatch(tree_sha):
        problem("external evidence bundle tree SHA is invalid")
        tree_sha = ""
    migration_head = candidate.get("migration_head")
    if not isinstance(migration_head, str) or not MIGRATION_RE.fullmatch(migration_head):
        problem("external evidence bundle migration head is invalid")
    nonempty(candidate.get("artifact_scope"), "candidate.artifact_scope", 20)

    manifest = exact_fields(
        bundle.get("repository_candidate_manifest"),
        MANIFEST_FIELDS,
        "repository_candidate_manifest",
    )
    if manifest is not None:
        immutable_uri(manifest.get("uri"), "repository_candidate_manifest.uri")
        sha256(manifest.get("sha256"), "repository_candidate_manifest.sha256")
    utc_timestamp(bundle.get("generated_at"), "generated_at")
    nonempty(bundle.get("retention_policy_id"), "retention_policy_id")

    gates = bundle.get("gates")
    if not isinstance(gates, list) or len(gates) != len(GATES):
        problem("external evidence bundle must contain exactly V12-X1 through V12-X8")
        return False
    statuses: dict[str, str] = {}
    observed: list[str] = []
    for index, gate_value in enumerate(gates):
        label = f"gates[{index}]"
        gate = exact_fields(gate_value, GATE_FIELDS, label)
        if gate is None:
            continue
        gate_id = gate.get("id")
        gate_id_text = str(gate_id)
        observed.append(gate_id_text)
        if gate.get("classification") != "external" or gate.get("self_certifiable") is not False:
            problem(f"{label} must remain external and non-self-certifiable")
        required_role = REQUIRED_ROLES.get(gate_id_text)
        if gate.get("required_issuer_role") != required_role:
            problem(f"{label} required issuer role is invalid")
            required_role = str(gate.get("required_issuer_role") or "")

        status = gate.get("status")
        if status not in {"missing", "pass", "fail"}:
            problem(f"{label}.status is invalid")
            status = "missing"
        statuses[gate_id_text] = str(status)

        evidence = gate.get("evidence")
        if not isinstance(evidence, list):
            problem(f"{label}.evidence must be an array")
            evidence = []
        decisions = [
            validate_evidence_record(
                item,
                label=f"{label}.evidence[{record_index}]",
                commit_sha=commit_sha,
                tree_sha=tree_sha,
                required_role=required_role,
            )
            for record_index, item in enumerate(evidence)
        ]
        if status == "missing" and evidence:
            problem(f"{label} missing status cannot carry evidence")
        if status in {"pass", "fail"} and not evidence:
            problem(f"{label} {status} status requires evidence")
        if status == "pass" and any(decision != "pass" for decision in decisions):
            problem(f"{label} pass status contains non-pass evidence")
        if status == "fail" and "fail" not in decisions:
            problem(f"{label} fail status lacks a failing decision")
    if tuple(observed) != GATES:
        problem("external evidence bundle gate order/identity is invalid")

    final = bundle.get("final_human_decision")
    if statuses.get("V12-X8") == "pass":
        if not all(statuses.get(f"V12-X{index}") == "pass" for index in range(1, 8)):
            problem("V12-X8 cannot pass before V12-X1 through V12-X7 pass")
        final_object = exact_fields(final, FINAL_FIELDS, "final_human_decision")
        if final_object is not None:
            if final_object.get("decision") not in {"go", "no-go"}:
                problem("final_human_decision.decision must be go or no-go")
            immutable_uri(final_object.get("uri"), "final_human_decision.uri")
            sha256(final_object.get("sha256"), "final_human_decision.sha256")
            utc_timestamp(final_object.get("decided_at"), "final_human_decision.decided_at")
            nonempty(final_object.get("actor_id"), "final_human_decision.actor_id")
            nonempty(final_object.get("organization"), "final_human_decision.organization")
            if final_object.get("role") != REQUIRED_ROLES["V12-X8"]:
                problem("final_human_decision.role is not the final human release authority")
            nonempty(final_object.get("scope"), "final_human_decision.scope", 20)
            if (
                final_object.get("candidate_commit_sha") != commit_sha
                or final_object.get("candidate_tree_sha") != tree_sha
            ):
                problem("final human decision is bound to another candidate")
    elif final is not None:
        problem("final_human_decision must be null unless V12-X8 is pass")

    revocations = bundle.get("revocations")
    if not isinstance(revocations, list):
        problem("revocations must be an array")
        revocations = []
    if revocations:
        problem("an external evidence bundle with a revocation cannot be structurally eligible")

    final_is_go = (
        isinstance(final, dict)
        and set(final) == FINAL_FIELDS
        and final.get("decision") == "go"
    )
    return (
        not PROBLEMS
        and all(statuses.get(gate_id) == "pass" for gate_id in GATES)
        and final_is_go
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--contract-only", action="store_true")
    group.add_argument("--bundle", type=Path)
    args = parser.parse_args()

    validate_contract_source()
    validate_template()
    structurally_eligible = False
    mode = "contract_only"
    if args.bundle is not None:
        mode = "bundle"
        structurally_eligible = validate_bundle(args.bundle)

    result = {
        "schema": "cex.external-production-evidence-contract-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "mode": mode,
        "gate_ids": list(GATES),
        "structurally_eligible_for_human_decision": structurally_eligible,
        "production_authorization": "not_granted",
        "checker_may_grant_production_authorization": False,
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

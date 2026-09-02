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

    if LIVE_SOURCE_ROOT.exists():
        for path in sorted(LIVE_SOURCE_ROOT.rglob("*")):
            if path.is_file():
                problem(
                    "live external evidence must not be committed into the source tree: "
                    + path.relative_to(ROOT).as_posix()
                )


def validate_template() -> None:
    template = load_json(TEMPLATE, "external evidence template")
    expected_keys = {
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
    if set(template) != expected_keys:
        problem("external evidence template root field set is not canonical")
    if template.get("schema") != "cex.external-production-evidence-bundle.v1":
        problem("external evidence template schema is invalid")
    if template.get("status") != "template" or template.get("template") is not True:
        problem("external evidence template must remain a shape-only template")
    if template.get("production_authorization") != "not_granted":
        problem("external evidence template must deny production authorization")
    candidate = template.get("candidate")
    if not isinstance(candidate, dict) or set(candidate) != {
        "repository", "commit_sha", "tree_sha", "migration_head", "artifact_scope"
    }:
        problem("external evidence template candidate object is invalid")
    else:
        if candidate.get("repository") != "TrillionniumFoundation/CEX":
            problem("external evidence template repository is invalid")
        for field in ("commit_sha", "tree_sha", "migration_head", "artifact_scope"):
            if candidate.get(field) is not None:
                problem(f"external evidence template candidate.{field} must be null")
    manifest = template.get("repository_candidate_manifest")
    if not isinstance(manifest, dict) or manifest != {"uri": None, "sha256": None}:
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
    for index, gate in enumerate(gates):
        label = f"template.gates[{index}]"
        if not isinstance(gate, dict):
            problem(f"{label} must be an object")
            continue
        gate_id = gate.get("id")
        observed.append(str(gate_id))
        if set(gate) != {
            "id", "classification", "self_certifiable", "status",
            "required_issuer_role", "evidence"
        }:
            problem(f"{label} field set is not canonical")
        if gate.get("classification") != "external" or gate.get("self_certifiable") is not False:
            problem(f"{label} must remain external and non-self-certifiable")
        if gate.get("status") != "missing" or gate.get("evidence") != []:
            problem(f"{label} template may not claim evidence or closure")
        if gate.get("required_issuer_role") != REQUIRED_ROLES.get(str(gate_id)):
            problem(f"{label} required issuer role is invalid")
    if tuple(observed) != GATES:
        problem("external evidence template gate order/identity is invalid")


def validate_evidence_record(
    record: object,
    *,
    label: str,
    commit_sha: str,
    tree_sha: str,
) -> str | None:
    if not isinstance(record, dict):
        problem(f"{label} must be an object")
        return None
    expected = {
        "uri", "sha256", "issuer", "executed_at", "decision", "scope",
        "candidate_commit_sha", "candidate_tree_sha", "waiver"
    }
    if set(record) != expected:
        problem(f"{label} field set is not canonical")
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
    issuer = record.get("issuer")
    if not isinstance(issuer, dict) or set(issuer) != {
        "actor_id", "organization", "role", "independent_of_repository_automation"
    }:
        problem(f"{label}.issuer field set is invalid")
    else:
        nonempty(issuer.get("actor_id"), f"{label}.issuer.actor_id")
        nonempty(issuer.get("organization"), f"{label}.issuer.organization")
        nonempty(issuer.get("role"), f"{label}.issuer.role")
        if issuer.get("independent_of_repository_automation") is not True:
            problem(f"{label}.issuer must explicitly be independent of repository automation")
    return decision if isinstance(decision, str) else None


def validate_bundle(path: Path) -> bool:
    bundle = load_json(path, "external evidence bundle")
    if bundle.get("schema") != "cex.external-production-evidence-bundle.v1":
        problem("external evidence bundle schema is invalid")
    if bundle.get("status") != "evidence_bundle" or bundle.get("template") is not False:
        problem("real external evidence bundle status/template flags are invalid")
    if bundle.get("production_authorization") != "not_granted":
        problem("structural evidence intake may not itself grant production authorization")

    candidate = bundle.get("candidate")
    if not isinstance(candidate, dict):
        problem("external evidence bundle candidate must be an object")
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

    manifest = bundle.get("repository_candidate_manifest")
    if not isinstance(manifest, dict):
        problem("repository candidate manifest reference must be an object")
    else:
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
    for index, gate in enumerate(gates):
        label = f"gates[{index}]"
        if not isinstance(gate, dict):
            problem(f"{label} must be an object")
            continue
        gate_id = gate.get("id")
        observed.append(str(gate_id))
        if gate.get("classification") != "external" or gate.get("self_certifiable") is not False:
            problem(f"{label} must remain external and non-self-certifiable")
        if gate.get("required_issuer_role") != REQUIRED_ROLES.get(str(gate_id)):
            problem(f"{label} required issuer role is invalid")
        status = gate.get("status")
        if status not in {"missing", "pass", "fail"}:
            problem(f"{label}.status is invalid")
            status = "missing"
        statuses[str(gate_id)] = str(status)
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
        if not isinstance(final, dict):
            problem("V12-X8 pass requires a final human decision")
        else:
            if final.get("decision") not in {"go", "no-go"}:
                problem("final_human_decision.decision must be go or no-go")
            immutable_uri(final.get("uri"), "final_human_decision.uri")
            sha256(final.get("sha256"), "final_human_decision.sha256")
            utc_timestamp(final.get("decided_at"), "final_human_decision.decided_at")
            nonempty(final.get("actor_id"), "final_human_decision.actor_id")
            nonempty(final.get("organization"), "final_human_decision.organization")
            nonempty(final.get("role"), "final_human_decision.role")
            nonempty(final.get("scope"), "final_human_decision.scope", 20)
            if final.get("candidate_commit_sha") != commit_sha or final.get("candidate_tree_sha") != tree_sha:
                problem("final human decision is bound to another candidate")
    elif final is not None:
        problem("final_human_decision must be null unless V12-X8 is pass")

    revocations = bundle.get("revocations")
    if not isinstance(revocations, list):
        problem("revocations must be an array")
        revocations = []
    if revocations:
        problem("an external evidence bundle with a revocation cannot be structurally eligible")

    return (
        not PROBLEMS
        and all(statuses.get(gate_id) == "pass" for gate_id in GATES)
        and isinstance(final, dict)
        and final.get("decision") == "go"
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

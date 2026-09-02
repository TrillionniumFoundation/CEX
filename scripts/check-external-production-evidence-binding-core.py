#!/usr/bin/env python3
"""Add fail-closed candidate-manifest and temporal binding to external evidence."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "scripts/check-external-production-evidence-contract-core.py"
MANIFEST_CHECKER = ROOT / "scripts/check-release-baseline-manifest.py"
EXPECTED_REPOSITORY = "TrillionniumFoundation/CEX"
EXPECTED_MIGRATION_HEAD = "0088_enforce_provider_terminal_evidence_binding.sql"
EXPECTED_SCOPE = (
    "repository-exact-money-control-plane-plus-hepta-durability-doc-integrity-"
    "full-suite-lint-receipt-recovery-and-trnm-production-config-hardening"
)
GATES = tuple(f"V12-X{index}" for index in range(1, 9))
ROLES = {
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
FINAL_FIELDS = {
    "decision", "uri", "sha256", "decided_at", "actor_id", "organization",
    "role", "scope", "candidate_commit_sha", "candidate_tree_sha",
}


def run_json(arguments: list[str]) -> tuple[int, dict[str, Any] | None, str]:
    completed = subprocess.run(
        arguments,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    raw = completed.stdout.strip()
    try:
        value = json.loads(raw)
    except json.JSONDecodeError:
        value = None
    return completed.returncode, value if isinstance(value, dict) else None, raw or "<no output>"


def parse_utc(value: object, label: str, problems: list[str]) -> datetime | None:
    if not isinstance(value, str) or not value.endswith("Z"):
        problems.append(f"{label} must be a UTC RFC3339 timestamp ending in Z")
        return None
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        problems.append(f"{label} is not a valid RFC3339 timestamp")
        return None
    if parsed.tzinfo is None or parsed.utcoffset() != timezone.utc.utcoffset(parsed):
        problems.append(f"{label} must resolve to UTC")
        return None
    return parsed


def is_within(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except (OSError, ValueError):
        return False


def validate_manifest_file(
    path: Path, expected_digest: object, problems: list[str]
) -> tuple[bytes, dict[str, Any], bool]:
    if not isinstance(expected_digest, str) or not DIGEST_RE.fullmatch(expected_digest):
        problems.append("repository_candidate_manifest.sha256 must be sha256:<64 lowercase hex>")
    try:
        if path.is_symlink():
            problems.append("candidate manifest path may not be a symlink")
            return b"", {}, False
        resolved = path.resolve(strict=True)
    except OSError as error:
        problems.append(f"cannot resolve candidate manifest: {error}")
        return b"", {}, False
    if not resolved.is_file():
        problems.append("candidate manifest path must reference a regular file")
        return b"", {}, False
    if is_within(resolved, ROOT):
        problems.append("candidate manifest supplied for external intake must be outside the source tree")
    try:
        raw = resolved.read_bytes()
    except OSError as error:
        problems.append(f"cannot read candidate manifest bytes: {error}")
        return b"", {}, False
    if "sha256:" + hashlib.sha256(raw).hexdigest() != expected_digest:
        problems.append(
            "candidate manifest byte digest does not match "
            "repository_candidate_manifest.sha256"
        )
    try:
        parsed = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        problems.append(f"candidate manifest is not valid UTF-8 JSON: {error}")
        return raw, {}, False
    if not isinstance(parsed, dict):
        problems.append("candidate manifest root must be an object")
        return raw, {}, False
    code, result, output = run_json(
        [sys.executable, str(MANIFEST_CHECKER), str(resolved)]
    )
    validator_ok = (
        code == 0
        and isinstance(result, dict)
        and result.get("status") == "ok"
        and result.get("production_authorization") == "not_granted"
    )
    if not validator_ok:
        problems.append("candidate manifest failed the authoritative validator: " + output)
    return raw, parsed, validator_ok


def validate_binding(
    bundle: dict[str, Any],
    manifest_raw: bytes,
    manifest: dict[str, Any],
    *,
    validator_ok: bool,
    bundle_path: Path | None = None,
) -> tuple[list[str], bool]:
    problems: list[str] = []
    if bundle_path is not None:
        try:
            if bundle_path.is_symlink():
                problems.append("external evidence bundle path may not be a symlink")
            resolved = bundle_path.resolve(strict=True)
            if not resolved.is_file():
                problems.append("external evidence bundle path must reference a regular file")
            if is_within(resolved, ROOT):
                problems.append("live external evidence bundle must remain outside the source tree")
        except OSError as error:
            problems.append(f"cannot resolve external evidence bundle: {error}")
    if not validator_ok:
        problems.append("candidate manifest validator result is not an authoritative pass")

    candidate = bundle.get("candidate")
    if not isinstance(candidate, dict):
        return problems + ["candidate must be an object"], False
    commit = candidate.get("commit_sha")
    tree = candidate.get("tree_sha")
    scope = candidate.get("artifact_scope")
    if candidate.get("repository") != EXPECTED_REPOSITORY:
        problems.append("bundle candidate repository is invalid")
    if not isinstance(commit, str) or not SHA_RE.fullmatch(commit):
        problems.append("bundle candidate commit SHA is invalid")
        commit = ""
    if not isinstance(tree, str) or not SHA_RE.fullmatch(tree):
        problems.append("bundle candidate tree SHA is invalid")
        tree = ""
    if candidate.get("migration_head") != EXPECTED_MIGRATION_HEAD:
        problems.append("bundle candidate migration head must equal " + EXPECTED_MIGRATION_HEAD)
    if scope != EXPECTED_SCOPE:
        problems.append("bundle candidate artifact_scope is not the active scope")

    reference = bundle.get("repository_candidate_manifest")
    expected_digest = reference.get("sha256") if isinstance(reference, dict) else None
    if not isinstance(reference, dict):
        problems.append("repository_candidate_manifest must be an object")
    if not isinstance(expected_digest, str) or not DIGEST_RE.fullmatch(expected_digest):
        problems.append("repository_candidate_manifest.sha256 must be sha256:<64 lowercase hex>")
    elif "sha256:" + hashlib.sha256(manifest_raw).hexdigest() != expected_digest:
        problems.append(
            "candidate manifest byte digest does not match "
            "repository_candidate_manifest.sha256"
        )

    if manifest.get("schema") != "cex.release-baseline-manifest.v1":
        problems.append("candidate manifest schema is invalid")
    if manifest.get("status") != "candidate":
        problems.append("candidate manifest must have status=candidate")
    if manifest.get("production_ready") is not False:
        problems.append("candidate manifest must keep production_ready=false")
    if manifest.get("production_authorization") != "not_granted":
        problems.append("candidate manifest must keep production_authorization=not_granted")
    if manifest.get("qualification_scope") != scope:
        problems.append("candidate manifest qualification_scope does not match artifact_scope")
    source = manifest.get("source")
    if not isinstance(source, dict):
        problems.append("candidate manifest source must be an object")
    else:
        for field, expected in (
            ("repository", EXPECTED_REPOSITORY),
            ("commit_sha", commit),
            ("tree_sha", tree),
        ):
            if source.get(field) != expected:
                problems.append(f"candidate manifest {field} does not match the bundle")
    database = manifest.get("database")
    if not isinstance(database, dict):
        problems.append("candidate manifest database must be an object")
    elif database.get("migration_head") != EXPECTED_MIGRATION_HEAD:
        problems.append("candidate manifest migration head does not match active authority")

    manifest_time = parse_utc(
        manifest.get("generated_at"), "candidate_manifest.generated_at", problems
    )
    bundle_time = parse_utc(bundle.get("generated_at"), "generated_at", problems)
    if manifest_time is not None and bundle_time is not None and manifest_time > bundle_time:
        problems.append("external evidence bundle predates its candidate manifest")

    gates = bundle.get("gates")
    if not isinstance(gates, list) or len(gates) != 8:
        return problems + [
            "external evidence bundle must contain exactly V12-X1 through V12-X8"
        ], False
    statuses: dict[str, str] = {}
    records: dict[str, list[dict[str, Any]]] = {}
    observed: list[str] = []
    seen_uris: set[str] = set()
    seen_digests: set[str] = set()
    for gate_index, gate in enumerate(gates):
        label = f"gates[{gate_index}]"
        if not isinstance(gate, dict):
            problems.append(f"{label} must be an object")
            continue
        gate_id = str(gate.get("id"))
        observed.append(gate_id)
        status = str(gate.get("status"))
        statuses[gate_id] = status
        evidence = gate.get("evidence")
        if not isinstance(evidence, list):
            problems.append(f"{label}.evidence must be an array")
            evidence = []
        gate_records: list[dict[str, Any]] = []
        for record_index, record in enumerate(evidence):
            record_label = f"{label}.evidence[{record_index}]"
            if not isinstance(record, dict):
                problems.append(f"{record_label} must be an object")
                continue
            uri = record.get("uri")
            digest = record.get("sha256")
            if isinstance(uri, str):
                if uri in seen_uris:
                    problems.append(f"{record_label}.uri reuses another gate's evidence object")
                seen_uris.add(uri)
            if isinstance(digest, str):
                if digest in seen_digests:
                    problems.append(f"{record_label}.sha256 reuses another gate's evidence object")
                seen_digests.add(digest)
            executed = parse_utc(
                record.get("executed_at"), f"{record_label}.executed_at", problems
            )
            if executed is not None and manifest_time is not None and executed < manifest_time:
                problems.append(f"{record_label} predates the candidate manifest")
            if executed is not None and bundle_time is not None and executed > bundle_time:
                problems.append(f"{record_label} is later than bundle.generated_at")
            issuer = record.get("issuer")
            gate_records.append(
                {
                    "uri": uri,
                    "sha256": digest,
                    "actor_id": issuer.get("actor_id") if isinstance(issuer, dict) else None,
                    "organization": issuer.get("organization") if isinstance(issuer, dict) else None,
                    "role": issuer.get("role") if isinstance(issuer, dict) else None,
                    "executed_at": executed,
                    "scope": record.get("scope"),
                    "candidate_commit_sha": record.get("candidate_commit_sha"),
                    "candidate_tree_sha": record.get("candidate_tree_sha"),
                }
            )
        records[gate_id] = gate_records
    if tuple(observed) != GATES:
        problems.append("external evidence bundle gate order/identity is invalid")

    final = bundle.get("final_human_decision")
    final_time: datetime | None = None
    if statuses.get("V12-X8") == "pass":
        if not all(statuses.get(gate_id) == "pass" for gate_id in GATES[:-1]):
            problems.append("V12-X8 cannot pass before V12-X1 through V12-X7 pass")
        x8_records = records.get("V12-X8", [])
        if len(x8_records) != 1:
            problems.append("V12-X8 pass requires exactly one canonical evidence record")
            x8 = None
        else:
            x8 = x8_records[0]
        if not isinstance(final, dict) or set(final) != FINAL_FIELDS:
            problems.append("final_human_decision field set is not canonical")
        else:
            final_time = parse_utc(
                final.get("decided_at"), "final_human_decision.decided_at", problems
            )
            if final.get("role") != ROLES["V12-X8"]:
                problems.append(
                    "final_human_decision.role is not the final human release authority"
                )
            if x8 is not None:
                for field in (
                    "uri", "sha256", "actor_id", "organization", "role", "scope",
                    "candidate_commit_sha", "candidate_tree_sha",
                ):
                    if final.get(field) != x8.get(field):
                        problems.append(
                            "final human decision must be the same immutable record "
                            f"as V12-X8: mismatch at {field}"
                        )
                if final_time != x8.get("executed_at"):
                    problems.append(
                        "final human decision must be the same immutable record "
                        "as V12-X8: decided_at must equal executed_at"
                    )
            if final_time is not None:
                for gate_id in GATES[:-1]:
                    for record in records.get(gate_id, []):
                        executed = record.get("executed_at")
                        if isinstance(executed, datetime) and executed > final_time:
                            problems.append(
                                "final human decision predates accepted evidence for " + gate_id
                            )
                if bundle_time is not None and final_time > bundle_time:
                    problems.append("final human decision is later than bundle.generated_at")
    elif final is not None:
        problems.append("final_human_decision must be null unless V12-X8 is pass")

    final_go = isinstance(final, dict) and final.get("decision") == "go"
    eligible = (
        not problems
        and all(statuses.get(gate_id) == "pass" for gate_id in GATES)
        and final_go
    )
    return problems, eligible


def base_manifest() -> dict[str, Any]:
    return {
        "schema": "cex.release-baseline-manifest.v1",
        "status": "candidate",
        "qualification_scope": EXPECTED_SCOPE,
        "source": {
            "repository": EXPECTED_REPOSITORY,
            "commit_sha": "1" * 40,
            "tree_sha": "2" * 40,
        },
        "database": {"migration_head": EXPECTED_MIGRATION_HEAD},
        "generated_at": "2026-09-02T00:00:00Z",
        "production_ready": False,
        "production_authorization": "not_granted",
    }


def base_bundle(manifest_raw: bytes) -> dict[str, Any]:
    gates: list[dict[str, Any]] = []
    for index, gate_id in enumerate(GATES, start=1):
        evidence = {
            "uri": f"artifact://external/{gate_id.lower()}-immutable",
            "sha256": f"sha256:{index:064x}",
            "issuer": {
                "actor_id": f"actor-{index}",
                "organization": f"organization-{index}",
                "role": ROLES[gate_id],
                "independent_of_repository_automation": True,
            },
            "executed_at": f"2026-09-02T00:{index:02d}:00Z",
            "decision": "pass",
            "scope": f"independently retained production evidence scope for {gate_id}",
            "candidate_commit_sha": "1" * 40,
            "candidate_tree_sha": "2" * 40,
            "waiver": None,
        }
        gates.append(
            {
                "id": gate_id,
                "classification": "external",
                "self_certifiable": False,
                "status": "pass",
                "required_issuer_role": ROLES[gate_id],
                "evidence": [evidence],
            }
        )
    x8 = gates[-1]["evidence"][0]
    issuer = x8["issuer"]
    return {
        "schema": "cex.external-production-evidence-bundle.v1",
        "status": "evidence_bundle",
        "template": False,
        "candidate": {
            "repository": EXPECTED_REPOSITORY,
            "commit_sha": "1" * 40,
            "tree_sha": "2" * 40,
            "migration_head": EXPECTED_MIGRATION_HEAD,
            "artifact_scope": EXPECTED_SCOPE,
        },
        "repository_candidate_manifest": {
            "uri": "artifact://candidate/exact-manifest",
            "sha256": "sha256:" + hashlib.sha256(manifest_raw).hexdigest(),
        },
        "generated_at": "2026-09-02T00:20:00Z",
        "retention_policy_id": "external-custody-v1",
        "production_authorization": "not_granted",
        "gates": gates,
        "final_human_decision": {
            "decision": "go",
            "uri": x8["uri"],
            "sha256": x8["sha256"],
            "decided_at": x8["executed_at"],
            "actor_id": issuer["actor_id"],
            "organization": issuer["organization"],
            "role": issuer["role"],
            "scope": x8["scope"],
            "candidate_commit_sha": "1" * 40,
            "candidate_tree_sha": "2" * 40,
        },
        "revocations": [],
    }


def run_self_tests() -> tuple[list[str], int]:
    manifest = base_manifest()
    raw = (json.dumps(manifest, sort_keys=True) + "\n").encode()
    original = base_bundle(raw)
    cases: list[tuple[str, dict[str, Any], dict[str, Any], bytes, str | None]] = []

    cases.append(("valid", copy.deepcopy(original), copy.deepcopy(manifest), raw, None))

    value = copy.deepcopy(original)
    value["candidate"]["migration_head"] = "0087_old.sql"
    cases.append(("migration", value, copy.deepcopy(manifest), raw, "migration head"))

    value = copy.deepcopy(original)
    value["repository_candidate_manifest"]["sha256"] = "sha256:" + "0" * 64
    cases.append(("digest", value, copy.deepcopy(manifest), raw, "byte digest"))

    changed_manifest = copy.deepcopy(manifest)
    changed_manifest["source"]["commit_sha"] = "3" * 40
    changed_raw = (json.dumps(changed_manifest, sort_keys=True) + "\n").encode()
    value = copy.deepcopy(original)
    value["repository_candidate_manifest"]["sha256"] = (
        "sha256:" + hashlib.sha256(changed_raw).hexdigest()
    )
    cases.append(("manifest identity", value, changed_manifest, changed_raw, "commit_sha"))

    late_manifest = copy.deepcopy(manifest)
    late_manifest["generated_at"] = "2026-09-02T00:10:00Z"
    late_raw = (json.dumps(late_manifest, sort_keys=True) + "\n").encode()
    value = copy.deepcopy(original)
    value["repository_candidate_manifest"]["sha256"] = (
        "sha256:" + hashlib.sha256(late_raw).hexdigest()
    )
    cases.append(
        ("manifest chronology", value, late_manifest, late_raw, "predates the candidate manifest")
    )

    value = copy.deepcopy(original)
    value["final_human_decision"]["uri"] = "artifact://external/different-final"
    cases.append(("final split", value, copy.deepcopy(manifest), raw, "same immutable"))

    value = copy.deepcopy(original)
    x8 = value["gates"][-1]["evidence"][0]
    x8["executed_at"] = "2026-09-02T00:03:00Z"
    value["final_human_decision"]["decided_at"] = x8["executed_at"]
    cases.append(("time order", value, copy.deepcopy(manifest), raw, "predates"))

    value = copy.deepcopy(original)
    value["generated_at"] = "2026-09-02T00:04:00Z"
    cases.append(("bundle time", value, copy.deepcopy(manifest), raw, "generated_at"))

    value = copy.deepcopy(original)
    first = value["gates"][0]["evidence"][0]
    second = value["gates"][1]["evidence"][0]
    second["uri"], second["sha256"] = first["uri"], first["sha256"]
    cases.append(("reuse", value, copy.deepcopy(manifest), raw, "reuses"))

    value = copy.deepcopy(original)
    extra = copy.deepcopy(value["gates"][-1]["evidence"][0])
    extra["uri"] = "artifact://external/v12-x8-second"
    extra["sha256"] = "sha256:" + "f" * 64
    value["gates"][-1]["evidence"].append(extra)
    cases.append(("multiple x8", value, copy.deepcopy(manifest), raw, "exactly one"))

    value = copy.deepcopy(original)
    cases.append(("validator failure", value, copy.deepcopy(manifest), raw, "validator result"))

    failures: list[str] = []
    for name, bundle, candidate_manifest, candidate_raw, expected in cases:
        validator_ok = name != "validator failure"
        problems, eligible = validate_binding(
            bundle, candidate_raw, candidate_manifest, validator_ok=validator_ok
        )
        if expected is None:
            if problems or not eligible:
                failures.append(f"{name}: expected success, got {problems!r}")
        elif eligible or not any(expected in item for item in problems):
            failures.append(
                f"{name}: expected rejection containing {expected!r}, got {problems!r}"
            )
    return failures, len(cases)


def main() -> int:
    parser = argparse.ArgumentParser()
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--contract-only", action="store_true")
    modes.add_argument("--bundle", type=Path)
    modes.add_argument("--self-test", action="store_true")
    parser.add_argument("--candidate-manifest", type=Path)
    args = parser.parse_args()

    if args.bundle is not None and args.candidate_manifest is None:
        parser.error("--candidate-manifest is required with --bundle")
    if args.bundle is None and args.candidate_manifest is not None:
        parser.error("--candidate-manifest is only valid with --bundle")

    if args.self_test:
        failures, count = run_self_tests()
        result = {
            "schema": "cex.external-production-evidence-binding-self-test.v1",
            "status": "failed" if failures else "ok",
            "cases": count,
            "production_authorization": "not_granted",
            "checker_may_grant_production_authorization": False,
            "problems": failures,
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 1 if failures else 0

    core_args = [sys.executable, str(CORE)]
    if args.contract_only:
        core_args.append("--contract-only")
    else:
        core_args.extend(["--bundle", str(args.bundle)])
    core_code, core, core_raw = run_json(core_args)
    problems: list[str] = []
    if core is None:
        problems.append("external evidence core did not emit JSON: " + core_raw)
        core_eligible = False
    else:
        core_problems = core.get("problems")
        if isinstance(core_problems, list):
            problems.extend("core: " + str(item) for item in core_problems)
        if core_code != 0 and not core_problems:
            problems.append("external evidence core failed without diagnostics")
        core_eligible = core.get("structurally_eligible_for_human_decision") is True

    hardening_eligible = False
    mode = "contract_only"
    if args.bundle is not None and args.candidate_manifest is not None:
        mode = "bundle"
        bundle_path = args.bundle
        try:
            bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
            problems.append(f"cannot read external evidence bundle: {error}")
            bundle = {}
        if not isinstance(bundle, dict):
            problems.append("external evidence bundle root must be an object")
            bundle = {}
        reference = bundle.get("repository_candidate_manifest")
        expected_digest = reference.get("sha256") if isinstance(reference, dict) else None
        manifest_raw, manifest, validator_ok = validate_manifest_file(
            args.candidate_manifest, expected_digest, problems
        )
        binding_problems, hardening_eligible = validate_binding(
            bundle,
            manifest_raw,
            manifest,
            validator_ok=validator_ok,
            bundle_path=bundle_path,
        )
        problems.extend(binding_problems)

    structurally_eligible = (
        mode == "bundle"
        and not problems
        and core_eligible
        and hardening_eligible
    )
    result = {
        "schema": "cex.external-production-evidence-contract-check.v1",
        "status": "failed" if problems else "ok",
        "mode": mode,
        "gate_ids": list(GATES),
        "structurally_eligible_for_human_decision": structurally_eligible,
        "production_authorization": "not_granted",
        "checker_may_grant_production_authorization": False,
        "problems": problems,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())

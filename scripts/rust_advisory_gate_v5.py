"""Authoritative CEX Rust advisory gate v5."""
from __future__ import annotations

import datetime as dt
import json
import re
import tempfile
import tomllib
from pathlib import Path
from types import SimpleNamespace
from typing import Any

from rust_advisory_common_v5 import (
    CODEOWNERS_PATH, DENY_PATH, EXPECTED_ADVISORIES, EXPECTED_LICENSE_EXCEPTIONS,
    LOCK_PATH, MINIMUM_CONTENT_MARKERS, MINIMUM_SURFACE_GLOBS, POLICY_PATH, ROOT,
    WORKFLOW_PATH, PolicyError, checked_result, read_json, require, run,
    surface_reasons, validate_spec,
)
from rust_advisory_packages_v5 import validate_local_package_closure, validate_release_surfaces

EXPECTED_OWNER = "ProfHepta"
EXPECTED_APPROVER = "Tomasrgbsf"
EXPECTED_REPOSITORY = "TrillionniumFoundation/CEX"
MAX_VALIDITY_DAYS = 30


def parse_date(value: Any, label: str) -> dt.date:
    require(isinstance(value, str), f"{label} must be an ISO date")
    try:
        return dt.date.fromisoformat(value)
    except ValueError as error:
        raise PolicyError(f"invalid {label}: {value!r}") from error


def validate_time(policy: dict[str, Any], today: dt.date) -> dict[str, Any]:
    created = parse_date(policy.get("created_on"), "created_on")
    expires = parse_date(policy.get("expires_on"), "expires_on")
    require(created <= today <= expires, "advisory policy is not active today")
    lifetime = (expires - created).days
    require(0 <= lifetime <= MAX_VALIDITY_DAYS, "advisory policy lifetime exceeds 30 days")
    require(policy.get("maximum_validity_days") == MAX_VALIDITY_DAYS, "maximum_validity_days drift")
    renewal = policy.get("renewal")
    require(isinstance(renewal, dict) and renewal.get("sequence") == 2, "renewal sequence drift")
    previous = renewal.get("previous_policy_blob")
    require(previous == "6c9e57045a8bf03299b2a44e7aa8fd674eab4e27", "previous policy blob drift")
    require(run("git", "cat-file", "-t", str(previous)) == "blob", "previous policy blob missing")
    require(parse_date(renewal.get("renewed_on"), "renewal.renewed_on") == created, "renewal date drift")
    require(renewal.get("approval_requirement") == "fresh_exact_head_github_review", "renewal approval drift")
    return {
        "created_on": created.isoformat(),
        "expires_on": expires.isoformat(),
        "lifetime_days": lifetime,
        "renewal_sequence": 2,
        "previous_policy_blob": previous,
    }


def validate_authority(policy: dict[str, Any]) -> dict[str, Any]:
    require(policy.get("accountable_owner") == EXPECTED_OWNER, "accountable owner drift")
    approval = policy.get("independent_security_approval")
    require(isinstance(approval, dict), "independent approval policy missing")
    require(approval.get("required_github_login") == EXPECTED_APPROVER, "security approver drift")
    require(approval.get("evidence_type") == "github_pull_request_review", "approval evidence type drift")
    require(approval.get("repository") == EXPECTED_REPOSITORY and approval.get("pull_request") == 34, "approval object drift")
    require(approval.get("head_binding") == "exact_final_head", "approval must bind exact final head")
    require(approval.get("status") == "required_external_not_embedded", "repository cannot self-approve")
    risk = policy.get("risk_register")
    require(isinstance(risk, dict) and risk.get("repository") == EXPECTED_REPOSITORY and risk.get("issue") == 35, "risk register drift")
    require(risk.get("state_required") == "open", "risk issue must remain open")
    require(risk.get("closure_condition") == "all_exceptions_removed_or_superseded_by_fresh_approval", "risk closure drift")
    codeowners = CODEOWNERS_PATH.read_text(encoding="utf-8")
    for path in (
        "/docs/security/rust-advisory-exceptions-v1.json",
        "/docs/security/rust-release-surfaces-v1.json",
        "/scripts/check-rust-advisory-exceptions.py",
        "/scripts/rust_advisory_common_v5.py",
        "/scripts/rust_advisory_packages_v5.py",
        "/scripts/rust_advisory_gate_v5.py",
        "/deny.toml", "/.github/workflows/trnm-economy-ci.yml",
    ):
        require(
            re.search(rf"(?m)^{re.escape(path)}\s+@{EXPECTED_APPROVER}\s*$", codeowners) is not None,
            f"CODEOWNERS missing independent owner for {path}",
        )
    return {
        "accountable_owner": EXPECTED_OWNER,
        "required_independent_security_approver": EXPECTED_APPROVER,
        "risk_register": f"{EXPECTED_REPOSITORY}#35",
        "approval_status": "required_external_not_embedded",
    }


def ignored_advisories(deny: dict[str, Any]) -> dict[str, str]:
    result: dict[str, str] = {}
    for entry in deny.get("advisories", {}).get("ignore", []):
        require(isinstance(entry, dict), "every advisory ignore requires a reason")
        advisory_id, reason = entry.get("id"), entry.get("reason")
        require(isinstance(advisory_id, str) and isinstance(reason, str) and reason.strip(), "invalid advisory ignore")
        require(advisory_id not in result, f"duplicate advisory ignore: {advisory_id}")
        result[advisory_id] = reason.strip()
    return result


def validate_tools_and_deny(policy: dict[str, Any]) -> tuple[dict[str, str], list[dict[str, Any]]]:
    deny = tomllib.loads(DENY_PATH.read_text(encoding="utf-8"))
    ignores = ignored_advisories(deny)
    require(set(ignores) == set(EXPECTED_ADVISORIES), f"advisory ignore drift: {sorted(ignores)}")
    require(deny.get("advisories", {}).get("yanked") == "deny", "yanked dependencies must be denied")
    require(deny.get("advisories", {}).get("unused-ignored-advisory") == "allow", "unused ignores must be owned by exact graph gate")
    require(deny.get("bans", {}).get("wildcards") == "deny", "wildcards must be denied")
    require(
        deny.get("sources", {}).get("unknown-git") == "deny" and not deny.get("sources", {}).get("allow-git"),
        "git dependency policy weakened",
    )
    require("hepta-research-league" in ignores["RUSTSEC-2024-0436"], "paste reason omits measured hepta exposure")
    require("absent from all" not in ignores["RUSTSEC-2024-0436"].lower(), "paste reason contradicts measured exposure")
    license_entries = deny.get("licenses", {}).get("exceptions", [])
    actual = {
        (entry.get("name") or entry.get("crate"), entry.get("version"), tuple(entry.get("allow", [])))
        for entry in license_entries if isinstance(entry, dict)
    }
    require(actual == EXPECTED_LICENSE_EXCEPTIONS, f"license exception drift: {sorted(actual)}")
    require("CDLA-Permissive-2.0" not in deny.get("licenses", {}).get("allow", []), "CDLA license must remain version scoped")
    workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
    require(
        set(re.findall(r"--ignore\s+(RUSTSEC-\d{4}-\d{4})", workflow)) == set(EXPECTED_ADVISORIES),
        "cargo-audit ignore drift",
    )
    require(
        "cargo-audit@0.22.2,cargo-deny@0.20.2" in workflow and "toolchain: 1.98.1" in workflow,
        "tool pins drift",
    )
    require(run("cargo-audit", "--version").split()[1] == policy.get("cargo_audit_version"), "cargo-audit version drift")
    require(run("cargo-deny", "--version").split()[1] == policy.get("cargo_deny_version"), "cargo-deny version drift")
    require(run("rustc", "--version").split()[1] == policy.get("rust_toolchain"), "Rust version drift")
    licenses = [
        {"package": name, "version": version.removeprefix("="), "licenses": list(values)}
        for name, version, values in sorted(actual)
    ]
    return ignores, licenses


def package_pattern(name: str, version: str) -> re.Pattern[str]:
    return re.compile(rf"(?m)(?:^|[\s│├└─]){re.escape(name)} v{re.escape(version)}(?:\s|$|\s\()")


def inverse_graph(spec: str, edges: str) -> str:
    return run("cargo", "tree", "--locked", "--target", "all", "-i", spec, "-e", edges)


def validate_advisories(
    policy: dict[str, Any], package_summary: dict[str, Any], ignores: dict[str, str]
) -> list[dict[str, Any]]:
    lock = tomllib.loads(LOCK_PATH.read_text(encoding="utf-8"))
    locked = {
        (item.get("name"), item.get("version"))
        for item in lock.get("package", []) if isinstance(item, dict)
    }
    exceptions = policy.get("exceptions")
    require(isinstance(exceptions, list), "exceptions must be a list")
    require(
        {item.get("advisory_id") for item in exceptions if isinstance(item, dict)} == set(EXPECTED_ADVISORIES),
        "advisory exception set drift",
    )
    require(len(exceptions) == len(EXPECTED_ADVISORIES), "duplicate advisory exception")
    evidence: list[dict[str, Any]] = []
    for entry in exceptions:
        require(isinstance(entry, dict), "exception entry must be an object")
        advisory_id = str(entry["advisory_id"])
        name, version = EXPECTED_ADVISORIES[advisory_id]
        require(entry.get("package") == name and entry.get("version") == version, f"{advisory_id} package drift")
        require((name, version) in locked, f"{name}@{version} left Cargo.lock; remove the exception")
        spec = f"{name}@{version}"
        all_graph = inverse_graph(spec, "all")
        execution_graph = inverse_graph(spec, "normal,build")
        if entry.get("all_target_graph_must_be_empty"):
            require(not all_graph, f"{advisory_id} became all-target reachable")
        if entry.get("execution_graph_must_be_empty"):
            require(not execution_graph, f"{advisory_id} became normal+build reachable")
        for marker in entry.get("required_all_target_markers", []):
            require(marker in all_graph, f"{advisory_id} all-target marker missing: {marker}")
        for marker in entry.get("required_execution_markers", []):
            require(marker in execution_graph, f"{advisory_id} execution marker missing: {marker}")
        reached: list[str] = []
        trees: dict[str, str] = {}
        pattern = package_pattern(name, version)
        for workspace_package in package_summary["workspace_packages"]:
            tree = run(
                "cargo", "tree", "--locked", "--target", "all", "-p", workspace_package,
                "-e", "normal,build",
            )
            if pattern.search(tree):
                reached.append(workspace_package)
                trees[workspace_package] = tree
        expected = entry.get("expected_workspace_execution_reachability")
        require(
            isinstance(expected, list) and reached == sorted(expected),
            f"{advisory_id} workspace reachability drift: {reached}",
        )
        for workspace_package, markers in entry.get("required_workspace_path_markers", {}).items():
            require(workspace_package in trees, f"{advisory_id} required workspace path absent: {workspace_package}")
            for marker in markers:
                require(marker in trees[workspace_package], f"{advisory_id}/{workspace_package} marker missing: {marker}")
        require(
            bool(str(entry.get("reason", "")).strip()) and bool(str(entry.get("removal_condition", "")).strip()),
            f"{advisory_id} risk text missing",
        )
        treatment = entry.get("risk_treatment")
        require(
            isinstance(treatment, dict)
            and all(str(treatment.get(key, "")).strip() for key in ("exposure", "mitigation", "rollback", "target_removal_date")),
            f"{advisory_id} risk treatment incomplete",
        )
        evidence.append({
            "advisory_id": advisory_id,
            "package_spec": spec,
            "classification": entry.get("classification"),
            "all_target_graph_empty": not all_graph,
            "normal_build_all_target_graph_empty": not execution_graph,
            "workspace_execution_reachability": reached,
            "deny_reason": ignores[advisory_id],
        })
    return evidence


def self_tests() -> dict[str, bool]:
    result: dict[str, bool] = {}
    try:
        checked_result(SimpleNamespace(returncode=19, stdout="", stderr=""), "empty hostile command")
    except PolicyError:
        result["nonzero_empty_output_rejected"] = True
    else:
        raise PolicyError("nonzero empty output was mistaken for graph absence")
    hostile_policy = {
        "surface_detection": {
            "path_globs": sorted(MINIMUM_SURFACE_GLOBS),
            "content_markers": sorted(MINIMUM_CONTENT_MARKERS),
        }
    }
    require(
        surface_reasons("neutral/path/Dockerfile.supply", "FROM scratch\n", hostile_policy),
        "nested Dockerfile escaped",
    )
    result["nested_dockerfile_detected"] = True
    require(
        surface_reasons("neutral/path/ship-now.sh", "cargo build --release -p hidden-carrier\n", hostile_policy),
        "neutral release script escaped",
    )
    result["neutral_release_script_detected"] = True
    with tempfile.TemporaryDirectory(prefix="cex-gate-") as temporary:
        root = Path(temporary)
        try:
            validate_spec(
                owner="fixture", section="dependencies", name="bad", spec="*",
                manifest_dir=root, root_document={"workspace": {"dependencies": {}}},
            )
        except PolicyError:
            result["wildcard_dependency_rejected"] = True
        else:
            raise PolicyError("wildcard dependency fixture passed")
    return result


def main() -> int:
    try:
        policy = read_json(POLICY_PATH)
        require(policy.get("schema") == "cex.rust-advisory-exceptions.v3", "policy schema drift")
        require(policy.get("status") == "active_bounded_exceptions", "policy must be active")
        require(policy.get("production_authorization") == "not_granted", "policy cannot authorize production")
        coverage = policy.get("coverage")
        require(isinstance(coverage, dict), "coverage policy missing")
        require(coverage.get("release_candidate_set") == "all_workspace_packages", "release candidate coverage narrowed")
        require(coverage.get("execution_edges") == "normal_and_build_all_targets", "build execution coverage narrowed")
        require(coverage.get("inverse_command_failure") == "fatal_including_empty_output", "command failure policy weakened")
        require(
            coverage.get("surface_policy") == "docs/security/rust-release-surfaces-v1.json",
            "surface policy path drift",
        )
        today = dt.datetime.now(dt.timezone.utc).date()
        time_policy = validate_time(policy, today)
        authority = validate_authority(policy)
        ignores, licenses = validate_tools_and_deny(policy)
        package_records, package_summary = validate_local_package_closure()
        surfaces = validate_release_surfaces(package_records, package_summary)
        advisories = validate_advisories(policy, package_summary, ignores)
        output = {
            "schema": "cex.rust-advisory-exception-check.v5",
            "status": "fail_closed_workspace_release_and_advisory_policy_valid",
            "checked_on": today.isoformat(),
            "time_policy": time_policy,
            "authority": authority,
            "rust_toolchain": policy["rust_toolchain"],
            "cargo_audit_version": policy["cargo_audit_version"],
            "cargo_deny_version": policy["cargo_deny_version"],
            "local_package_closure": package_summary,
            "release_surface_closure": surfaces,
            "license_exceptions": licenses,
            "advisory_exceptions": advisories,
            "hostile_self_tests": self_tests(),
            "production_authorization": "not_granted",
        }
        print(json.dumps(output, indent=2, sort_keys=True))
        return 0
    except (
        PolicyError, OSError, UnicodeDecodeError, ValueError,
        json.JSONDecodeError, tomllib.TOMLDecodeError,
    ) as error:
        print(json.dumps({
            "schema": "cex.rust-advisory-exception-check.v5",
            "status": "failed",
            "problems": [str(error)],
            "production_authorization": "not_granted",
        }, indent=2, sort_keys=True))
        return 1

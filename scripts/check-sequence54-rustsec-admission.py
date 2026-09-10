#!/usr/bin/env python3
"""Static fail-closed binding for the Sequence 54 RustSec admission surface."""
from __future__ import annotations

import json
import sys
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PROBLEMS: list[str] = []

EXPECTED_ADVISORIES = {
    "RUSTSEC-2023-0071": ("rsa", "0.9.10"),
    "RUSTSEC-2026-0214": ("gumdrop", "0.8.1"),
    "RUSTSEC-2024-0436": ("paste", "1.0.15"),
}
EXPECTED_FEATURE_REACHABILITY = {
    "RUSTSEC-2023-0071": [],
    "RUSTSEC-2026-0214": [],
    "RUSTSEC-2024-0436": ["hepta-research-league", "trnm-finality-verifier"],
}
PROTECTED_PATHS = (
    "/docs/security/rust-advisory-exceptions-v1.json",
    "/docs/security/rust-release-surfaces-v1.json",
    "/docs/security/rust-feature-closure-v1.json",
    "/scripts/check-rust-advisory-exceptions.py",
    "/scripts/check-sequence54-rustsec-admission.py",
    "/scripts/rust_advisory_common_v5.py",
    "/scripts/rust_advisory_packages_v5.py",
    "/scripts/rust_advisory_gate_v5.py",
    "/scripts/rust_advisory_feature_closure_v6.py",
    "/scripts/rust_advisory_policy_v6.py",
    "/deny.toml",
    "/.github/workflows/trnm-economy-ci.yml",
    "/.github/workflows/p0-sequence54-integration.yml",
)
WORKFLOW_MARKERS = (
    "cargo-audit@0.22.2,cargo-deny@0.20.2",
    "toolchain: 1.98.1",
    "python3 scripts/check-sequence54-rustsec-admission.py",
    "python3 scripts/check-rust-advisory-exceptions.py",
    "--ignore RUSTSEC-2023-0071",
    "--ignore RUSTSEC-2026-0214",
    "--ignore RUSTSEC-2024-0436",
    "docs/security/rust-advisory-exceptions-v1.json",
    "docs/security/rust-release-surfaces-v1.json",
    "docs/security/rust-feature-closure-v1.json",
    "scripts/rust_advisory_policy_v6.py",
)


def problem(message: str) -> None:
    PROBLEMS.append(message)


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        problem(f"missing required RustSec admission file: {relative}")
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        problem(f"required RustSec admission file is not UTF-8: {relative}: {error}")
        return ""


def load_json(relative: str) -> dict[str, Any]:
    raw = read(relative)
    if not raw:
        return {}
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        problem(f"invalid JSON: {relative}: {error}")
        return {}
    if not isinstance(value, dict):
        problem(f"JSON root must be an object: {relative}")
        return {}
    return value


def require(condition: bool, message: str) -> None:
    if not condition:
        problem(message)


def validate_policy() -> None:
    policy = load_json("docs/security/rust-advisory-exceptions-v1.json")
    require(policy.get("schema") == "cex.rust-advisory-exceptions.v3", "RustSec policy schema drift")
    require(policy.get("status") == "active_bounded_exceptions", "RustSec policy is not active")
    require(policy.get("production_authorization") == "not_granted", "RustSec policy grants production")
    require(policy.get("rust_toolchain") == "1.98.1", "RustSec policy toolchain drift")
    require(policy.get("cargo_audit_version") == "0.22.2", "cargo-audit policy pin drift")
    require(policy.get("cargo_deny_version") == "0.20.2", "cargo-deny policy pin drift")
    require(policy.get("created_on") == "2026-09-08", "RustSec policy renewal date drift")
    require(policy.get("expires_on") == "2026-10-08", "RustSec policy expiry drift")
    require(policy.get("maximum_validity_days") == 30, "RustSec policy validity widened")

    renewal = policy.get("renewal")
    require(isinstance(renewal, dict), "RustSec renewal object missing")
    if isinstance(renewal, dict):
        require(renewal.get("sequence") == 3, "RustSec renewal sequence drift")
        require(
            renewal.get("previous_policy_blob") == "3c68853d4de6891140290e933f8495e82aeddad7",
            "RustSec prior-policy blob drift",
        )
        require(
            renewal.get("approval_requirement") == "fresh_exact_head_github_review",
            "RustSec renewal approval weakened",
        )

    approval = policy.get("independent_security_approval")
    require(isinstance(approval, dict), "independent security approval object missing")
    if isinstance(approval, dict):
        require(approval.get("repository") == "TrillionniumFoundation/CEX", "security approval repository drift")
        require(approval.get("pull_request") == 53, "security approval is not bound to PR #53")
        require(approval.get("required_github_login") == "Tomasrgbsf", "security approver drift")
        require(approval.get("head_binding") == "exact_final_head", "security approval head binding weakened")
        require(
            approval.get("status") == "required_external_not_embedded",
            "repository-authored security approval is forbidden",
        )

    risk = policy.get("risk_register")
    require(isinstance(risk, dict), "RustSec risk register binding missing")
    if isinstance(risk, dict):
        require(risk.get("repository") == "TrillionniumFoundation/CEX", "risk repository drift")
        require(risk.get("issue") == 35, "risk register is not bound to issue #35")
        require(risk.get("state_required") == "open", "risk issue no longer required open")

    exceptions = policy.get("exceptions")
    require(isinstance(exceptions, list), "RustSec exceptions must be a list")
    observed: dict[str, tuple[str, str]] = {}
    if isinstance(exceptions, list):
        for item in exceptions:
            if not isinstance(item, dict):
                problem("RustSec exception entry must be an object")
                continue
            advisory_id = item.get("advisory_id")
            package = item.get("package")
            version = item.get("version")
            if not all(isinstance(value, str) for value in (advisory_id, package, version)):
                problem(f"invalid RustSec exception identity: {item!r}")
                continue
            if advisory_id in observed:
                problem(f"duplicate RustSec exception: {advisory_id}")
            observed[advisory_id] = (package, version)
            require(bool(str(item.get("reason", "")).strip()), f"{advisory_id} lacks reason")
            require(bool(str(item.get("removal_condition", "")).strip()), f"{advisory_id} lacks removal condition")
            treatment = item.get("risk_treatment")
            require(
                isinstance(treatment, dict)
                and all(str(treatment.get(key, "")).strip() for key in ("exposure", "mitigation", "rollback", "target_removal_date")),
                f"{advisory_id} risk treatment is incomplete",
            )
    require(observed == EXPECTED_ADVISORIES, f"RustSec exception set drift: {observed!r}")


def validate_feature_and_surface_policies() -> None:
    feature = load_json("docs/security/rust-feature-closure-v1.json")
    require(feature.get("schema") == "cex.rust-feature-closure.v1", "feature-closure schema drift")
    require(feature.get("status") == "active_fail_closed", "feature closure is not fail closed")
    require(feature.get("target_scope") == "all", "feature closure target scope narrowed")
    require(feature.get("execution_edges") == "normal_and_build", "feature closure execution edges narrowed")
    require(
        feature.get("expected_workspace_all_features_execution_reachability") == EXPECTED_FEATURE_REACHABILITY,
        "all-feature advisory reachability drift",
    )
    required_fixtures = {
        "optional_dependency",
        "no_default_features_plus_explicit_feature",
        "build_dependency",
        "proc_macro_dependency",
        "target_specific_dependency",
    }
    observed_fixtures = feature.get("hostile_fixtures_required")
    require(
        isinstance(observed_fixtures, list) and set(observed_fixtures) == required_fixtures,
        "feature hostile-fixture set drift",
    )
    require(feature.get("command_failure") == "fatal_including_empty_output", "feature graph command failure weakened")
    require(feature.get("production_authorization") == "not_granted", "feature policy grants production")

    surfaces = load_json("docs/security/rust-release-surfaces-v1.json")
    require(surfaces.get("schema") == "cex.rust-release-surfaces.v1", "release-surface schema drift")
    require(
        surfaces.get("coverage_strategy") == "all_workspace_packages_are_release_candidates",
        "release candidate coverage narrowed",
    )
    require(surfaces.get("unreachable_local_package_manifests") == "forbidden", "disconnected Cargo packages allowed")
    require(surfaces.get("unknown_explicit_package_references") == "forbidden", "unknown release package allowed")
    require(surfaces.get("production_authorization") == "not_granted", "release-surface policy grants production")
    detection = surfaces.get("surface_detection")
    require(isinstance(detection, dict), "release-surface detection missing")
    if isinstance(detection, dict):
        markers = detection.get("content_markers")
        for marker in ("cargo build", "cargo install", "docker build", "target/release/", "ExecStart="):
            require(isinstance(markers, list) and marker in markers, f"release-surface marker missing: {marker}")


def validate_deny_policy() -> None:
    raw = read("deny.toml")
    if not raw:
        return
    try:
        deny = tomllib.loads(raw)
    except tomllib.TOMLDecodeError as error:
        problem(f"deny.toml is invalid: {error}")
        return

    advisories = deny.get("advisories", {})
    require(advisories.get("yanked") == "deny", "yanked crates are not denied")
    require(advisories.get("unused-ignored-advisory") == "allow", "custom graph gate no longer owns unused ignores")
    ignores = advisories.get("ignore")
    ids: set[str] = set()
    if isinstance(ignores, list):
        for item in ignores:
            if not isinstance(item, dict) or not isinstance(item.get("id"), str):
                problem("deny.toml advisory ignore entry is invalid")
                continue
            require(bool(str(item.get("reason", "")).strip()), f"deny.toml ignore lacks reason: {item.get('id')}")
            ids.add(item["id"])
    require(ids == set(EXPECTED_ADVISORIES), f"deny.toml advisory ignore drift: {sorted(ids)}")

    bans = deny.get("bans", {})
    require(bans.get("wildcards") == "deny", "Cargo wildcard dependencies are not denied")
    sources = deny.get("sources", {})
    require(sources.get("unknown-git") == "deny", "unknown Git sources are not denied")
    require(not sources.get("allow-git"), "Git sources are allowlisted in Sequence 54")


def validate_ownership_and_wiring() -> None:
    codeowners = read(".github/CODEOWNERS")
    for path in PROTECTED_PATHS:
        require(f"{path} @Tomasrgbsf" in codeowners, f"independent CODEOWNER missing: {path}")

    for workflow_path in (
        ".github/workflows/p0-sequence54-integration.yml",
        ".github/workflows/trnm-economy-ci.yml",
    ):
        workflow = read(workflow_path)
        for marker in WORKFLOW_MARKERS:
            require(marker in workflow, f"{workflow_path} lacks RustSec admission marker: {marker}")

    entrypoint = read("scripts/check-rust-advisory-exceptions.py")
    require(
        "from rust_advisory_feature_closure_v6 import validate_feature_closure" in entrypoint,
        "advisory entrypoint no longer runs all-feature closure",
    )
    require(
        "from rust_advisory_gate_v5 import main as baseline_main" in entrypoint,
        "advisory entrypoint no longer runs baseline gate",
    )
    authority = read("scripts/rust_advisory_policy_v6.py")
    for marker in (
        "EXPECTED_PULL_REQUEST = 53",
        "EXPECTED_RISK_ISSUE = 35",
        'EXPECTED_APPROVER = "Tomasrgbsf"',
        "EXPECTED_RENEWAL_SEQUENCE = 3",
        'EXPECTED_PREVIOUS_POLICY_BLOB = "3c68853d4de6891140290e933f8495e82aeddad7"',
    ):
        require(marker in authority, f"RustSec authority adapter drift: {marker}")


def main() -> int:
    validate_policy()
    validate_feature_and_surface_policies()
    validate_deny_policy()
    validate_ownership_and_wiring()
    result = {
        "schema": "cex.sequence54-rustsec-admission-check.v1",
        "status": "failed" if PROBLEMS else "ok",
        "pull_request": 53,
        "risk_issue": 35,
        "required_independent_security_approver": "Tomasrgbsf",
        "bounded_advisories": sorted(EXPECTED_ADVISORIES),
        "policy_expiry": "2026-10-08",
        "checker_may_grant_production_authorization": False,
        "production_authorization": "not_granted",
        "problems": PROBLEMS,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if PROBLEMS else 0


if __name__ == "__main__":
    sys.exit(main())

"""Authoritative CEX Rust advisory gate v6 bound to the final closure PR."""

from __future__ import annotations

import datetime as dt
import json
import re
import tomllib
from typing import Any

from rust_advisory_common_v5 import (
    CODEOWNERS_PATH,
    EXPECTED_ADVISORIES,
    POLICY_PATH,
    ROOT,
    PolicyError,
    read_json,
    require,
)
from rust_advisory_gate_v5 import (
    self_tests,
    validate_advisories,
    validate_time,
    validate_tools_and_deny,
)
from rust_advisory_packages_v5 import (
    validate_local_package_closure,
    validate_release_surfaces,
)

EXPECTED_OWNER = "ProfHepta"
EXPECTED_APPROVER = "Tomasrgbsf"
EXPECTED_REPOSITORY = "TrillionniumFoundation/CEX"
EXPECTED_PULL_REQUEST = 45


def validate_authority(policy: dict[str, Any]) -> dict[str, Any]:
    require(
        policy.get("accountable_owner") == EXPECTED_OWNER,
        "accountable owner drift",
    )
    approval = policy.get("independent_security_approval")
    require(isinstance(approval, dict), "independent approval policy missing")
    require(
        approval.get("required_github_login") == EXPECTED_APPROVER,
        "security approver drift",
    )
    require(
        approval.get("evidence_type") == "github_pull_request_review",
        "approval evidence type drift",
    )
    require(
        approval.get("repository") == EXPECTED_REPOSITORY
        and approval.get("pull_request") == EXPECTED_PULL_REQUEST,
        "approval object is not bound to the final closure pull request",
    )
    require(
        approval.get("head_binding") == "exact_final_head",
        "approval must bind exact final head",
    )
    require(
        approval.get("status") == "required_external_not_embedded",
        "repository cannot self-approve",
    )

    risk = policy.get("risk_register")
    require(
        isinstance(risk, dict)
        and risk.get("repository") == EXPECTED_REPOSITORY
        and risk.get("issue") == 35,
        "risk register drift",
    )
    require(risk.get("state_required") == "open", "risk issue must remain open")
    require(
        risk.get("closure_condition")
        == "all_exceptions_removed_or_superseded_by_fresh_approval",
        "risk closure drift",
    )

    codeowners = CODEOWNERS_PATH.read_text(encoding="utf-8")
    independently_owned = (
        "/docs/security/rust-advisory-exceptions-v1.json",
        "/docs/security/rust-release-surfaces-v1.json",
        "/docs/security/rust-feature-closure-v1.json",
        "/scripts/check-rust-advisory-exceptions.py",
        "/scripts/rust_advisory_common_v5.py",
        "/scripts/rust_advisory_packages_v5.py",
        "/scripts/rust_advisory_gate_v5.py",
        "/scripts/rust_advisory_gate_v6.py",
        "/scripts/rust_advisory_feature_closure_v6.py",
        "/deny.toml",
        "/.github/workflows/trnm-economy-ci.yml",
        "/.github/workflows/p0-supply-chain-gate.yml",
    )
    for path in independently_owned:
        require(
            re.search(
                rf"(?m)^{re.escape(path)}\s+@{EXPECTED_APPROVER}(?:\s|$)",
                codeowners,
            )
            is not None,
            f"CODEOWNERS missing independent owner for {path}",
        )

    return {
        "accountable_owner": EXPECTED_OWNER,
        "required_independent_security_approver": EXPECTED_APPROVER,
        "approval_pull_request": EXPECTED_PULL_REQUEST,
        "approval_head_binding": "exact_final_head",
        "risk_register": f"{EXPECTED_REPOSITORY}#35",
        "approval_status": "required_external_not_embedded",
    }


def main() -> int:
    try:
        policy = read_json(POLICY_PATH)
        require(
            policy.get("schema") == "cex.rust-advisory-exceptions.v3",
            "policy schema drift",
        )
        require(
            policy.get("status") == "active_bounded_exceptions",
            "policy must be active",
        )
        require(
            policy.get("production_authorization") == "not_granted",
            "policy cannot authorize production",
        )
        coverage = policy.get("coverage")
        require(isinstance(coverage, dict), "coverage policy missing")
        require(
            coverage.get("release_candidate_set") == "all_workspace_packages",
            "release candidate coverage narrowed",
        )
        require(
            coverage.get("execution_edges") == "normal_and_build_all_targets",
            "build execution coverage narrowed",
        )
        require(
            coverage.get("inverse_command_failure")
            == "fatal_including_empty_output",
            "command failure policy weakened",
        )
        require(
            coverage.get("surface_policy")
            == "docs/security/rust-release-surfaces-v1.json",
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
            "schema": "cex.rust-advisory-exception-check.v6",
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
        PolicyError,
        OSError,
        UnicodeDecodeError,
        ValueError,
        json.JSONDecodeError,
        tomllib.TOMLDecodeError,
    ) as error:
        print(
            json.dumps(
                {
                    "schema": "cex.rust-advisory-exception-check.v6",
                    "status": "failed",
                    "problems": [str(error)],
                    "expected_advisories": sorted(EXPECTED_ADVISORIES),
                    "approval_pull_request": EXPECTED_PULL_REQUEST,
                    "production_authorization": "not_granted",
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

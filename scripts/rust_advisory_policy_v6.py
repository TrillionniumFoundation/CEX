"""Sequence 54 authority and renewal binding for the Rust advisory gate."""
from __future__ import annotations

import datetime as dt
import re
from typing import Any

from rust_advisory_common_v5 import CODEOWNERS_PATH, PolicyError, require, run

EXPECTED_OWNER = "ProfHepta"
EXPECTED_APPROVER = "Tomasrgbsf"
EXPECTED_REPOSITORY = "TrillionniumFoundation/CEX"
EXPECTED_PULL_REQUEST = 53
EXPECTED_RISK_ISSUE = 35
EXPECTED_RENEWAL_SEQUENCE = 3
EXPECTED_PREVIOUS_POLICY_BLOB = "3c68853d4de6891140290e933f8495e82aeddad7"
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
    require(
        policy.get("maximum_validity_days") == MAX_VALIDITY_DAYS,
        "maximum_validity_days drift",
    )
    renewal = policy.get("renewal")
    require(
        isinstance(renewal, dict)
        and renewal.get("sequence") == EXPECTED_RENEWAL_SEQUENCE,
        "renewal sequence drift",
    )
    previous = renewal.get("previous_policy_blob")
    require(previous == EXPECTED_PREVIOUS_POLICY_BLOB, "previous policy blob drift")
    require(
        run("git", "cat-file", "-t", str(previous)) == "blob",
        "previous policy blob missing",
    )
    require(
        parse_date(renewal.get("renewed_on"), "renewal.renewed_on") == created,
        "renewal date drift",
    )
    require(
        renewal.get("approval_requirement") == "fresh_exact_head_github_review",
        "renewal approval drift",
    )
    return {
        "created_on": created.isoformat(),
        "expires_on": expires.isoformat(),
        "lifetime_days": lifetime,
        "renewal_sequence": EXPECTED_RENEWAL_SEQUENCE,
        "previous_policy_blob": previous,
    }


def validate_authority(policy: dict[str, Any]) -> dict[str, Any]:
    require(policy.get("accountable_owner") == EXPECTED_OWNER, "accountable owner drift")
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
        "approval object drift",
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
        and risk.get("issue") == EXPECTED_RISK_ISSUE,
        "risk register drift",
    )
    require(risk.get("state_required") == "open", "risk issue must remain open")
    require(
        risk.get("closure_condition")
        == "all_exceptions_removed_or_superseded_by_fresh_approval",
        "risk closure drift",
    )

    codeowners = CODEOWNERS_PATH.read_text(encoding="utf-8")
    protected_paths = (
        "/docs/security/rust-advisory-exceptions-v1.json",
        "/docs/security/rust-release-surfaces-v1.json",
        "/docs/security/rust-feature-closure-v1.json",
        "/scripts/check-rust-advisory-exceptions.py",
        "/scripts/rust_advisory_common_v5.py",
        "/scripts/rust_advisory_packages_v5.py",
        "/scripts/rust_advisory_gate_v5.py",
        "/scripts/rust_advisory_feature_closure_v6.py",
        "/scripts/rust_advisory_policy_v6.py",
        "/deny.toml",
        "/.github/workflows/trnm-economy-ci.yml",
        "/.github/workflows/p0-sequence54-integration.yml",
    )
    for path in protected_paths:
        require(
            re.search(
                rf"(?m)^{re.escape(path)}\s+@{EXPECTED_APPROVER}\s*$",
                codeowners,
            )
            is not None,
            f"CODEOWNERS missing independent owner for {path}",
        )

    return {
        "accountable_owner": EXPECTED_OWNER,
        "required_independent_security_approver": EXPECTED_APPROVER,
        "approval_pull_request": EXPECTED_PULL_REQUEST,
        "risk_register": f"{EXPECTED_REPOSITORY}#{EXPECTED_RISK_ISSUE}",
        "approval_status": "required_external_not_embedded",
    }

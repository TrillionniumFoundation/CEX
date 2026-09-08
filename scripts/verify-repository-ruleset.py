#!/usr/bin/env python3
"""Verify the live GitHub main-branch ruleset without inferring it from source."""

from __future__ import annotations

import argparse
import json
import os
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_POLICY = ROOT / "docs/governance/main-ruleset-v1.json"
API_VERSION = "2022-11-28"


class VerificationError(RuntimeError):
    pass


def load_policy(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise VerificationError(f"cannot read ruleset policy {path}: {error}") from error
    if not isinstance(value, dict):
        raise VerificationError("ruleset policy root must be an object")
    if value.get("schema") != "cex.github-main-ruleset.v1":
        raise VerificationError("ruleset policy schema drift")
    if value.get("status") != "active_required":
        raise VerificationError("ruleset policy must remain active_required")
    if value.get("production_authorization") != "not_granted":
        raise VerificationError("repository governance policy cannot grant production authorization")
    contexts = value.get("required_status_checks")
    if not isinstance(contexts, list) or not contexts or contexts != sorted(set(contexts), key=contexts.index):
        raise VerificationError("required_status_checks must be a non-empty duplicate-free list")
    return value


def request_json(url: str, token: str) -> tuple[int, Any]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "cex-ruleset-verifier",
            "X-GitHub-Api-Version": API_VERSION,
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = response.read()
            return int(response.status), json.loads(payload) if payload else None
    except urllib.error.HTTPError as error:
        payload = error.read().decode("utf-8", errors="replace")
        try:
            value: Any = json.loads(payload)
        except json.JSONDecodeError:
            value = {"message": payload}
        return int(error.code), value


def rule_map(detail: dict[str, Any]) -> dict[str, dict[str, Any]]:
    rules = detail.get("rules")
    if not isinstance(rules, list):
        return {}
    result: dict[str, dict[str, Any]] = {}
    for rule in rules:
        if not isinstance(rule, dict):
            continue
        rule_type = rule.get("type")
        if isinstance(rule_type, str):
            result[rule_type] = rule
    return result


def verify(policy: dict[str, Any], token: str) -> dict[str, Any]:
    repository = policy["repository"]
    base = f"https://api.github.com/repos/{repository}"
    problems: list[str] = []

    repo_status, repository_value = request_json(base, token)
    if repo_status != 200 or not isinstance(repository_value, dict):
        raise VerificationError(f"cannot read repository metadata: HTTP {repo_status}")
    default_branch = repository_value.get("default_branch")
    if default_branch != policy["target_branch"]:
        problems.append(
            f"default branch drift: expected={policy['target_branch']!r} actual={default_branch!r}"
        )

    branch_name = urllib.parse.quote(policy["target_branch"], safe="")
    branch_status, branch = request_json(f"{base}/branches/{branch_name}", token)
    if branch_status != 200 or not isinstance(branch, dict):
        problems.append(f"cannot read target branch: HTTP {branch_status}")
        branch = {}
    if branch.get("protected") is not True:
        problems.append("target branch is not protected by a live GitHub control")

    rulesets_status, summaries = request_json(f"{base}/rulesets?per_page=100", token)
    if rulesets_status != 200 or not isinstance(summaries, list):
        problems.append(f"cannot read repository rulesets: HTTP {rulesets_status}")
        summaries = []
    matching = [
        item
        for item in summaries
        if isinstance(item, dict) and item.get("name") == policy["ruleset_name"]
    ]
    if len(matching) != 1:
        problems.append(
            f"expected exactly one named ruleset, found {len(matching)}: {policy['ruleset_name']}"
        )
        detail: dict[str, Any] = {}
    else:
        summary = matching[0]
        if summary.get("enforcement") != "active":
            problems.append("named ruleset enforcement is not active")
        if summary.get("target") != "branch":
            problems.append("named ruleset target is not branch")
        ruleset_id = summary.get("id")
        detail_status, detail_value = request_json(f"{base}/rulesets/{ruleset_id}", token)
        if detail_status != 200 or not isinstance(detail_value, dict):
            problems.append(f"cannot read named ruleset detail: HTTP {detail_status}")
            detail = {}
        else:
            detail = detail_value

    if detail:
        if detail.get("enforcement") != policy["enforcement"]:
            problems.append("ruleset enforcement drift")
        bypass_actors = detail.get("bypass_actors")
        if bypass_actors not in ([], None):
            problems.append("ruleset contains bypass actors")

        conditions = detail.get("conditions")
        ref_name = conditions.get("ref_name") if isinstance(conditions, dict) else None
        include = ref_name.get("include") if isinstance(ref_name, dict) else None
        accepted_targets = {"~DEFAULT_BRANCH", f"refs/heads/{policy['target_branch']}"}
        if not isinstance(include, list) or not accepted_targets.intersection(include):
            problems.append("ruleset does not include the default/main branch")

        rules = rule_map(detail)
        for required_type, enabled in (
            ("deletion", policy["block_deletion"]),
            ("non_fast_forward", policy["block_non_fast_forward"]),
            ("required_signatures", policy["require_signed_commits"]),
        ):
            if enabled and required_type not in rules:
                problems.append(f"ruleset lacks required rule type: {required_type}")

        pull_rule = rules.get("pull_request")
        pull_parameters = pull_rule.get("parameters") if isinstance(pull_rule, dict) else None
        expected_pull = policy["pull_request"]
        if not isinstance(pull_parameters, dict):
            problems.append("ruleset lacks pull_request parameters")
        else:
            for key in (
                "required_approving_review_count",
                "dismiss_stale_reviews_on_push",
                "require_code_owner_review",
                "require_last_push_approval",
                "required_review_thread_resolution",
            ):
                if pull_parameters.get(key) != expected_pull[key]:
                    problems.append(
                        f"pull_request parameter drift: {key} expected={expected_pull[key]!r} "
                        f"actual={pull_parameters.get(key)!r}"
                    )
            allowed = pull_parameters.get("allowed_merge_methods")
            if set(allowed or []) != set(expected_pull["allowed_merge_methods"]):
                problems.append("allowed merge methods drift")

        status_rule = rules.get("required_status_checks")
        status_parameters = status_rule.get("parameters") if isinstance(status_rule, dict) else None
        observed_contexts: set[str] = set()
        if not isinstance(status_parameters, dict):
            problems.append("ruleset lacks required_status_checks parameters")
        else:
            if status_parameters.get("strict_required_status_checks_policy") is not True:
                problems.append("strict required-status policy is disabled")
            if status_parameters.get("do_not_enforce_on_create") is not False:
                problems.append("required checks are not enforced on branch creation")
            checks = status_parameters.get("required_status_checks")
            if isinstance(checks, list):
                observed_contexts = {
                    item.get("context")
                    for item in checks
                    if isinstance(item, dict) and isinstance(item.get("context"), str)
                }
            missing_contexts = sorted(set(policy["required_status_checks"]) - observed_contexts)
            extra_contexts = sorted(observed_contexts - set(policy["required_status_checks"]))
            if missing_contexts:
                problems.append("required status contexts missing: " + ", ".join(missing_contexts))
            if extra_contexts:
                problems.append("unreviewed required status contexts present: " + ", ".join(extra_contexts))

    return {
        "schema": "cex.github-main-ruleset-verification.v1",
        "status": "verified" if not problems else "blocked",
        "repository": repository,
        "target_branch": policy["target_branch"],
        "default_branch": default_branch,
        "branch_protected": branch.get("protected") is True,
        "ruleset_name": policy["ruleset_name"],
        "matching_ruleset_count": len(matching),
        "required_status_checks": policy["required_status_checks"],
        "problems": problems,
        "production_authorization": "not_granted",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    try:
        policy = load_policy(args.policy)
        token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or ""
        if not token:
            raise VerificationError("GITHUB_TOKEN or GH_TOKEN is required")
        result = verify(policy, token)
    except VerificationError as error:
        result = {
            "schema": "cex.github-main-ruleset-verification.v1",
            "status": "blocked",
            "problems": [str(error)],
            "production_authorization": "not_granted",
        }

    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if result.get("status") == "verified" else 1


if __name__ == "__main__":
    raise SystemExit(main())

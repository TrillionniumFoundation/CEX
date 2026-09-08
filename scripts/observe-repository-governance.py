#!/usr/bin/env python3
"""Record live GitHub enforcement against the authoritative CEX ruleset policy."""

from __future__ import annotations

import argparse
import json
import os
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_POLICY = ROOT / "docs/governance/main-ruleset-v1.json"
API_VERSION = "2022-11-28"


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def request_json(url: str, token: str) -> tuple[int, Any]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "cex-repository-governance-observer",
            "X-GitHub-Api-Version": API_VERSION,
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
            return int(response.status), json.loads(raw) if raw else None
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8", errors="replace")
        try:
            payload: Any = json.loads(raw)
        except json.JSONDecodeError:
            payload = {"message": raw}
        return int(error.code), payload


def load_policy(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise SystemExit("ruleset policy root must be an object")
    if value.get("schema") != "cex.github-main-ruleset.v1":
        raise SystemExit("ruleset policy schema drift")
    if value.get("status") != "active_required":
        raise SystemExit("ruleset policy must remain active_required")
    if value.get("production_authorization") != "not_granted":
        raise SystemExit("ruleset policy cannot grant production authorization")
    checks = value.get("required_status_checks")
    if not isinstance(checks, list) or not checks or len(checks) != len(set(checks)):
        raise SystemExit("ruleset policy required checks are invalid")
    return value


def rules_by_type(detail: dict[str, Any]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    rules = detail.get("rules")
    if not isinstance(rules, list):
        return result
    for rule in rules:
        if not isinstance(rule, dict):
            continue
        rule_type = rule.get("type")
        if isinstance(rule_type, str):
            result[rule_type] = rule
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--commit-sha", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    args = parser.parse_args()

    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or ""
    if not token:
        raise SystemExit("GITHUB_TOKEN or GH_TOKEN is required")

    policy = load_policy(args.policy)
    if policy.get("repository") != args.repository:
        raise SystemExit(
            f"repository policy drift: expected={policy.get('repository')!r} "
            f"actual={args.repository!r}"
        )

    base = f"https://api.github.com/repos/{args.repository}"
    problems: list[str] = []

    repo_status, repo = request_json(base, token)
    if repo_status != 200 or not isinstance(repo, dict):
        raise SystemExit(f"cannot read repository metadata: HTTP {repo_status}")
    default_branch = repo.get("default_branch")
    if not isinstance(default_branch, str) or not default_branch:
        raise SystemExit("repository metadata lacks default_branch")
    if default_branch != policy["target_branch"]:
        problems.append(
            f"default branch drift: expected={policy['target_branch']} actual={default_branch}"
        )

    branch_status, branch = request_json(
        f"{base}/branches/{urllib.parse.quote(default_branch, safe='')}", token
    )
    if branch_status != 200 or not isinstance(branch, dict):
        problems.append(f"cannot read default branch metadata: HTTP {branch_status}")
        branch = {}
    protected = branch.get("protected") is True
    if not protected:
        problems.append("default branch is not protected")

    rulesets_status, summaries = request_json(f"{base}/rulesets?per_page=100", token)
    rulesets_readable = rulesets_status == 200 and isinstance(summaries, list)
    if not rulesets_readable:
        problems.append(f"repository rulesets are not readable: HTTP {rulesets_status}")
        summaries = []

    matching = [
        item
        for item in summaries
        if isinstance(item, dict) and item.get("name") == policy["ruleset_name"]
    ]
    detail: dict[str, Any] = {}
    if len(matching) != 1:
        problems.append(
            f"expected one authoritative ruleset, found {len(matching)}"
        )
    else:
        summary = matching[0]
        if summary.get("enforcement") != policy["enforcement"]:
            problems.append("authoritative ruleset is not active")
        if summary.get("target") != policy["target"]:
            problems.append("authoritative ruleset target drift")
        detail_status, detail_value = request_json(
            f"{base}/rulesets/{summary.get('id')}", token
        )
        if detail_status != 200 or not isinstance(detail_value, dict):
            problems.append(f"cannot read authoritative ruleset detail: HTTP {detail_status}")
        else:
            detail = detail_value

    observed_contexts: set[str] = set()
    observed_rules: list[str] = []
    if detail:
        bypass_actors = detail.get("bypass_actors")
        if bypass_actors not in ([], None):
            problems.append("authoritative ruleset contains bypass actors")

        conditions = detail.get("conditions")
        ref_name = conditions.get("ref_name") if isinstance(conditions, dict) else None
        include = ref_name.get("include") if isinstance(ref_name, dict) else None
        accepted_targets = {
            "~DEFAULT_BRANCH",
            f"refs/heads/{policy['target_branch']}",
        }
        if not isinstance(include, list) or not accepted_targets.intersection(include):
            problems.append("authoritative ruleset does not target main/default branch")

        rules = rules_by_type(detail)
        observed_rules = sorted(rules)
        for rule_type, required in (
            ("deletion", policy["block_deletion"]),
            ("non_fast_forward", policy["block_non_fast_forward"]),
            ("required_signatures", policy["require_signed_commits"]),
        ):
            if required and rule_type not in rules:
                problems.append(f"required ruleset rule missing: {rule_type}")

        pull_rule = rules.get("pull_request")
        pull_parameters = pull_rule.get("parameters") if isinstance(pull_rule, dict) else None
        if not isinstance(pull_parameters, dict):
            problems.append("pull_request ruleset parameters are missing")
        else:
            for key, expected in policy["pull_request"].items():
                actual = pull_parameters.get(key)
                if key == "allowed_merge_methods":
                    if set(actual or []) != set(expected):
                        problems.append("allowed merge methods drift")
                elif actual != expected:
                    problems.append(
                        f"pull_request parameter drift: {key} expected={expected!r} actual={actual!r}"
                    )

        status_rule = rules.get("required_status_checks")
        status_parameters = (
            status_rule.get("parameters") if isinstance(status_rule, dict) else None
        )
        if not isinstance(status_parameters, dict):
            problems.append("required_status_checks parameters are missing")
        else:
            if status_parameters.get("strict_required_status_checks_policy") \
                    != policy["strict_required_status_checks_policy"]:
                problems.append("strict required-status policy drift")
            if status_parameters.get("do_not_enforce_on_create") \
                    != policy["do_not_enforce_on_create"]:
                problems.append("required-status create enforcement drift")
            checks = status_parameters.get("required_status_checks")
            if isinstance(checks, list):
                observed_contexts = {
                    item.get("context")
                    for item in checks
                    if isinstance(item, dict) and isinstance(item.get("context"), str)
                }
            expected_contexts = set(policy["required_status_checks"])
            missing = sorted(expected_contexts - observed_contexts)
            extra = sorted(observed_contexts - expected_contexts)
            if missing:
                problems.append("required status contexts missing: " + ", ".join(missing))
            if extra:
                problems.append("unreviewed required status contexts present: " + ", ".join(extra))

    enforcement = "enforced" if not problems else (
        "not_enforced" if rulesets_readable else "unverifiable"
    )
    result = {
        "schema": "cex.repository-governance-observation.v2",
        "ok": not problems,
        "repository": args.repository,
        "commit_sha": args.commit_sha,
        "observed_at": utc_now(),
        "policy_path": args.policy.relative_to(ROOT).as_posix()
            if args.policy.is_relative_to(ROOT) else str(args.policy),
        "policy_schema": policy["schema"],
        "default_branch": default_branch,
        "default_branch_protected": protected,
        "authoritative_ruleset_name": policy["ruleset_name"],
        "matching_authoritative_rulesets": len(matching),
        "expected_required_status_contexts": policy["required_status_checks"],
        "observed_required_status_contexts": sorted(observed_contexts),
        "observed_rule_types": observed_rules,
        "rulesets_http_status": rulesets_status,
        "rulesets_readable": rulesets_readable,
        "repository_candidate_enforcement": enforcement,
        "problems": problems,
        "production_authorization": "not_granted",
        "interpretation": (
            "This is a live observation of GitHub controls against the checked-in policy. "
            "Source files and CI prose do not create branch protection or ruleset enforcement."
        ),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not problems else 1


if __name__ == "__main__":
    raise SystemExit(main())

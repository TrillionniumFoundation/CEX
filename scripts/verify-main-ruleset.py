#!/usr/bin/env python3
"""Verify live CEX main ruleset enforcement and reject source-only substitutes."""
from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "docs/repository-ruleset-required-contexts-v1.json"
REPOSITORY = os.environ.get("GITHUB_REPOSITORY", "TrillionniumFoundation/CEX")
TOKEN = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
NAME = "CEX v12 main admission"

if not TOKEN:
    raise SystemExit("GITHUB_TOKEN or GH_TOKEN is required")
policy = json.loads(POLICY.read_text(encoding="utf-8"))
headers = {
    "Accept": "application/vnd.github+json",
    "Authorization": f"Bearer {TOKEN}",
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "cex-v12-ruleset-verifier",
}


def get(path: str) -> Any:
    request = urllib.request.Request(
        f"https://api.github.com/repos/{REPOSITORY}/{path}", headers=headers
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise SystemExit(f"GitHub API GET {path} failed: HTTP {error.code}: {detail}") from error


branch = get("branches/main")
summaries = get("rulesets?per_page=100")
summary = next((item for item in summaries if item.get("name") == NAME), None)
problems: list[str] = []
full: dict[str, Any] = {}
if summary is None:
    problems.append(f"required ruleset is absent: {NAME}")
else:
    full = get(f"rulesets/{summary['id']}")
    if full.get("enforcement") != "active":
        problems.append("required ruleset is not active")
    if full.get("target") != "branch":
        problems.append("required ruleset target is not branch")

    conditions = full.get("conditions", {}).get("ref_name", {})
    if policy["target"] not in conditions.get("include", []):
        problems.append("ruleset does not include refs/heads/main")

    rules = full.get("rules", [])
    rule_types = {rule.get("type") for rule in rules if isinstance(rule, dict)}
    for required_type in ("deletion", "non_fast_forward", "pull_request", "required_status_checks"):
        if required_type not in rule_types:
            problems.append(f"ruleset lacks rule: {required_type}")

    status_rule = next(
        (rule for rule in rules if rule.get("type") == "required_status_checks"), {}
    )
    status_parameters = status_rule.get("parameters", {})
    actual_contexts = sorted(
        value.get("context")
        for value in status_parameters.get("required_status_checks", [])
        if isinstance(value, dict) and isinstance(value.get("context"), str)
    )
    expected_contexts = sorted(policy["required_status_checks"])
    if actual_contexts != expected_contexts:
        problems.append(
            f"required context mismatch: expected={expected_contexts} actual={actual_contexts}"
        )
    if status_parameters.get("strict_required_status_checks_policy") is not True:
        problems.append("required status checks are not strict")

    pr_rule = next((rule for rule in rules if rule.get("type") == "pull_request"), {})
    pr_parameters = pr_rule.get("parameters", {})
    for key, expected in policy["pull_request"].items():
        if pr_parameters.get(key) != expected:
            problems.append(
                f"pull-request rule mismatch for {key}: expected={expected!r} actual={pr_parameters.get(key)!r}"
            )
    if full.get("bypass_actors"):
        problems.append("ruleset contains bypass actors")

if not branch.get("protected"):
    problems.append("main is not reported protected by GitHub")

result = {
    "schema": "cex.repository-ruleset-readback.v1",
    "status": "failed" if problems else "ok",
    "repository": REPOSITORY,
    "main_sha": branch.get("commit", {}).get("sha"),
    "main_protected": bool(branch.get("protected")),
    "ruleset": full or summary,
    "expected_required_status_checks": policy["required_status_checks"],
    "production_authorization": "not_granted",
    "problems": problems,
}
print(json.dumps(result, indent=2, sort_keys=True))
sys.exit(1 if problems else 0)

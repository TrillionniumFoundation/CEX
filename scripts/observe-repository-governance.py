#!/usr/bin/env python3
"""Record GitHub branch/ruleset enforcement without claiming unavailable controls."""

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

DESIRED_CHECKS = [
    "fresh-postgres-migrations",
    "service-local-gate-linux",
    "service-local-gate-windows",
    "gateway-exact-reserve",
    "execution-settlement",
    "provider-reconciliation",
    "repository-candidate-qualification",
]


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def request_json(url: str, token: str) -> tuple[int, Any]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "cex-repository-governance-observer",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return int(response.status), json.load(response)
    except urllib.error.HTTPError as error:
        try:
            payload: Any = json.loads(error.read().decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            payload = {"message": str(error)}
        return int(error.code), payload


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--commit-sha", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN is required")

    base = f"https://api.github.com/repos/{args.repository}"
    repo_status, repo = request_json(base, token)
    if repo_status != 200 or not isinstance(repo, dict):
        raise SystemExit(f"cannot read repository metadata: HTTP {repo_status}")
    default_branch = repo.get("default_branch")
    if not isinstance(default_branch, str) or not default_branch:
        raise SystemExit("repository metadata lacks default_branch")

    branch_status, branch = request_json(
        f"{base}/branches/{urllib.parse.quote(default_branch, safe='')}", token
    )
    if branch_status != 200 or not isinstance(branch, dict):
        raise SystemExit(f"cannot read default branch metadata: HTTP {branch_status}")

    rulesets_status, rulesets = request_json(f"{base}/rulesets?per_page=100", token)
    rulesets_readable = rulesets_status == 200 and isinstance(rulesets, list)
    if not rulesets_readable:
        rulesets = []

    protection = branch.get("protection")
    if not isinstance(protection, dict):
        protection = {}
    required_status = protection.get("required_status_checks")
    if not isinstance(required_status, dict):
        required_status = {}
    contexts = required_status.get("contexts")
    if not isinstance(contexts, list):
        contexts = []
    actual_contexts = sorted({value for value in contexts if isinstance(value, str)})

    protected = bool(branch.get("protected"))
    ruleset_summaries = []
    for ruleset in rulesets:
        if not isinstance(ruleset, dict):
            continue
        ruleset_summaries.append(
            {
                "id": ruleset.get("id"),
                "name": ruleset.get("name"),
                "target": ruleset.get("target"),
                "enforcement": ruleset.get("enforcement"),
            }
        )

    required_checks_enforced = protected and set(DESIRED_CHECKS).issubset(actual_contexts)
    if required_checks_enforced:
        enforcement = "enforced"
    elif protected or rulesets_readable:
        enforcement = "not_enforced"
    else:
        enforcement = "unverifiable"

    result = {
        "schema": "cex.repository-governance-observation.v1",
        "ok": True,
        "repository": args.repository,
        "commit_sha": args.commit_sha,
        "observed_at": utc_now(),
        "default_branch": default_branch,
        "default_branch_protected": protected,
        "branch_protection_enabled": bool(protection.get("enabled")),
        "actual_required_status_contexts": actual_contexts,
        "desired_required_status_contexts": DESIRED_CHECKS,
        "required_candidate_checks_enforced": required_checks_enforced,
        "rulesets_http_status": rulesets_status,
        "rulesets_readable": rulesets_readable,
        "ruleset_count": len(ruleset_summaries),
        "rulesets": ruleset_summaries,
        "repository_candidate_enforcement": enforcement,
        "production_authorization": "not_granted",
        "interpretation": (
            "This is an observation of GitHub controls. Source files and CI prose do not create "
            "branch protection or ruleset enforcement."
        ),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
